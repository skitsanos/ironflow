use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::nodes::file::RootedDir;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};
use crate::util::node_config::config_bool_or;

mod input;

use input::{WriteInput, parse_input};

pub struct WriteFileNode;

struct Request {
    destination: PathBuf,
    input: WriteInput,
    append: bool,
    max_bytes: u64,
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

        run_tracked_blocking_step(move |execution| {
            write_request(
                Request {
                    destination,
                    input,
                    append,
                    max_bytes,
                },
                &execution,
            )
        })
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
    execution.checkpoint()?;
    let parent = request
        .destination
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let leaf = request
        .destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("write_file: destination has no file name"))?;
    let root = RootedDir::prepare(parent, "write_file", execution)?;
    let existing = if request.append {
        open_existing(&request.destination)?
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
            "write_file: existing file is {existing_size} bytes, exceeds IRONFLOW_MAX_FILE_BYTES ({})",
            request.max_bytes
        );
    }
    let mut artifact_file = None;
    let incoming = match &request.input {
        WriteInput::Text(text) => text.len() as u64,
        WriteInput::Base64 { decoded, .. } => *decoded,
        WriteInput::Artifact(source) => {
            let (file, _) = source.open("write_file artifact", execution)?.into_parts();
            let size = file.metadata()?.len();
            artifact_file = Some(file);
            size
        }
    };
    let final_size = existing_size
        .checked_add(incoming)
        .ok_or_else(|| anyhow::anyhow!("write_file: final size overflow"))?;
    admit_size(final_size, request.max_bytes)?;

    let mut staged = root.stage_file(Path::new(leaf), true, execution)?;
    if let Some(file) = existing {
        copy_exact(
            file,
            staged.writer(),
            existing_size,
            execution,
            "existing destination",
        )?;
    }
    write_input(
        &request.input,
        artifact_file,
        staged.writer(),
        incoming,
        execution,
    )?;
    staged.writer().flush()?;
    staged.writer().sync_all()?;
    execution.checkpoint()?;
    staged.commit()
}

fn open_existing(path: &Path) -> Result<Option<std::fs::File>> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => crate::util::bounded_read::open_regular_file(path, "write_file append").map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_input(
    input: &WriteInput,
    artifact_file: Option<std::fs::File>,
    destination: &mut std::fs::File,
    expected: u64,
    execution: &ExecutionControl,
) -> Result<()> {
    match input {
        WriteInput::Text(text) => copy_exact(
            Cursor::new(text.as_bytes()),
            destination,
            expected,
            execution,
            "text input",
        ),
        WriteInput::Base64 { encoded, .. } => {
            let reader = base64::read::DecoderReader::new(encoded.as_bytes(), &STANDARD);
            copy_exact(reader, destination, expected, execution, "base64 input")
                .context("write_file: invalid base64 input")
        }
        WriteInput::Artifact(_) => copy_exact(
            artifact_file.expect("artifact input was opened during admission"),
            destination,
            expected,
            execution,
            "artifact input",
        ),
    }
}

fn copy_exact(
    mut source: impl Read,
    destination: &mut impl Write,
    expected: u64,
    execution: &ExecutionControl,
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
            anyhow::bail!("write_file: {label} changed or exceeded its admitted size");
        }
        destination.write_all(&chunk[..read])?;
    }
    if copied != expected {
        anyhow::bail!("write_file: {label} changed while being copied");
    }
    execution.checkpoint()
}

fn admit_size(size: u64, maximum: u64) -> Result<()> {
    if size > maximum {
        anyhow::bail!(
            "write_file: final payload is {size} bytes, exceeds IRONFLOW_MAX_FILE_BYTES ({maximum})"
        );
    }
    Ok(())
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
