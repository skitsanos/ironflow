use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::nodes::file::RootedDir;
use crate::util::bounded_read::open_regular_file;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};
use crate::util::node_config::config_bool_or;

mod input;

use input::{WriteInput, admit_size, parse_input, preflight_base64};

pub struct WriteFileNode;

struct Request {
    /// Node name used as the prefix of every error raised for this request.
    operation: &'static str,
    destination: PathBuf,
    input: WriteInput,
    append: bool,
    max_bytes: u64,
}

impl Request {
    async fn run(self) -> Result<()> {
        run_tracked_blocking_step(move |execution| write_request(self, &execution)).await
    }
}

/// Copy a regular file into `destination` through the tracked atomic writer,
/// so the destination receives the same alias-root, no-follow leaf and
/// special-file policy as `write_file`. The source follows the `read_file`
/// policy and is admitted against `IRONFLOW_MAX_FILE_BYTES`.
pub(crate) async fn copy_file(destination: PathBuf, source: PathBuf) -> Result<()> {
    Request {
        operation: "copy_file",
        destination,
        input: WriteInput::File(source),
        append: false,
        max_bytes: crate::util::limits::max_file_bytes(),
    }
    .run()
    .await
}

/// Stream a Base64 payload into `destination`. The decoded length is admitted
/// before the tracked stage and decoding happens in bounded chunks directly
/// into the staged file.
pub(crate) async fn write_base64(
    destination: PathBuf,
    encoded: String,
    url_safe: bool,
) -> Result<()> {
    let max_bytes = crate::util::limits::max_file_bytes();
    let decoded = preflight_base64(&encoded, max_bytes, "base64_decode")?;
    let engine = if url_safe { &URL_SAFE } else { &STANDARD };
    Request {
        operation: "base64_decode",
        destination,
        input: WriteInput::Base64 {
            encoded,
            decoded,
            engine,
        },
        append: false,
        max_bytes,
    }
    .run()
    .await
}

#[async_trait]
impl Node for WriteFileNode {
    fn node_type(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Atomically write text, streamed base64, or an artifact to a file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let destination = required_string(config, "path")?;
        let destination = PathBuf::from(interpolate_ctx(destination, ctx));
        let max_bytes = crate::util::limits::max_file_bytes();
        let input = parse_input(config, ctx, max_bytes)?;
        let append = config_bool_or(config, "append", ctx, false)?;
        let output_path = destination.to_string_lossy().into_owned();

        Request {
            operation: "write_file",
            destination,
            input,
            append,
            max_bytes,
        }
        .run()
        .await?;

        Ok(NodeOutput::from([
            (
                "write_file_path".to_owned(),
                serde_json::Value::String(output_path),
            ),
            (
                "write_file_success".to_owned(),
                serde_json::Value::Bool(true),
            ),
        ]))
    }
}

fn write_request(request: Request, execution: &ExecutionControl) -> Result<()> {
    let operation = request.operation;
    execution.checkpoint()?;
    let parent = request
        .destination
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let leaf = request
        .destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{operation}: destination has no file name"))?;
    let root = RootedDir::prepare(parent, operation, execution)?;
    let existing = if request.append {
        root.open_existing(leaf, execution)?
    } else {
        None
    };
    let existing_size = existing
        .as_ref()
        .map(|file| file.metadata().map(|metadata| metadata.len()))
        .transpose()?
        .unwrap_or(0);
    if existing_size > request.max_bytes {
        anyhow::bail!(
            "{operation}: existing file is {existing_size} bytes, exceeds the IRONFLOW_MAX_FILE_BYTES limit ({})",
            request.max_bytes
        );
    }
    let mut source_file = None;
    let incoming = match &request.input {
        WriteInput::Text(text) => text.len() as u64,
        WriteInput::Base64 { decoded, .. } => *decoded,
        WriteInput::Artifact(source) => {
            let (file, _) = source
                .open(&format!("{operation} artifact"), execution)?
                .into_parts();
            let size = file.metadata()?.len();
            source_file = Some(file);
            size
        }
        WriteInput::File(source) => {
            let file = open_regular_file(source, operation)?;
            let size = file.metadata()?.len();
            source_file = Some(file);
            size
        }
    };
    let final_size = existing_size
        .checked_add(incoming)
        .ok_or_else(|| anyhow::anyhow!("{operation}: final size overflow"))?;
    admit_size(final_size, request.max_bytes, operation)?;

    let mut staged = root.stage_file(Path::new(leaf), true, execution)?;
    if let Some(file) = existing {
        copy_exact(
            file,
            staged.writer(),
            existing_size,
            execution,
            operation,
            "existing destination",
        )?;
    }
    write_input(
        &request.input,
        source_file,
        staged.writer(),
        incoming,
        execution,
        operation,
    )?;
    staged.writer().flush()?;
    staged.writer().sync_all()?;
    execution.checkpoint()?;
    staged.commit()
}

fn write_input(
    input: &WriteInput,
    source_file: Option<File>,
    destination: &mut File,
    expected: u64,
    execution: &ExecutionControl,
    operation: &str,
) -> Result<()> {
    match input {
        WriteInput::Text(text) => copy_exact(
            Cursor::new(text.as_bytes()),
            destination,
            expected,
            execution,
            operation,
            "text input",
        ),
        WriteInput::Base64 {
            encoded, engine, ..
        } => {
            let reader = base64::read::DecoderReader::new(encoded.as_bytes(), *engine);
            copy_exact(
                reader,
                destination,
                expected,
                execution,
                operation,
                "base64 input",
            )
            .with_context(|| format!("{operation}: invalid base64 input"))
        }
        WriteInput::Artifact(_) | WriteInput::File(_) => {
            let label = match input {
                WriteInput::Artifact(_) => "artifact input",
                _ => "source file",
            };
            copy_exact(
                source_file.expect("source input was opened during admission"),
                destination,
                expected,
                execution,
                operation,
                label,
            )
        }
    }
}

fn copy_exact(
    mut source: impl Read,
    destination: &mut impl Write,
    expected: u64,
    execution: &ExecutionControl,
    operation: &str,
    label: &str,
) -> Result<()> {
    let mut copied = 0_u64;
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        execution.checkpoint()?;
        let read = source.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        copied = copied.saturating_add(read as u64);
        if copied > expected {
            anyhow::bail!("{operation}: {label} changed or exceeded its admitted size");
        }
        destination.write_all(&chunk[..read])?;
    }
    if copied != expected {
        anyhow::bail!("{operation}: {label} changed while being copied");
    }
    execution.checkpoint()
}

fn required_string<'a>(config: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    optional_string(config, key)?
        .ok_or_else(|| anyhow::anyhow!("write_file requires '{key}' parameter"))
}

fn optional_string<'a>(config: &'a serde_json::Value, key: &str) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("write_file: '{key}' must be a string"),
    }
}

#[cfg(test)]
mod tests;
