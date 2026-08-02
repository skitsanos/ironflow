#[path = "extraction_benchmark_worker/fixtures.rs"]
mod fixtures;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(about = "Content-safe worker for the opt-in extraction benchmark")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run {
        #[arg(long)]
        node: String,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        label: String,
        #[arg(long)]
        cancel_after_ms: Option<u64>,
    },
    GenerateFixtures {
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Serialize)]
struct WorkerResult {
    schema_version: u8,
    label: String,
    node: String,
    input_sha256: Option<String>,
    raw_bytes: u64,
    declared_bytes: u64,
    status: &'static str,
    limit: Option<String>,
    serialized_output_bytes: u64,
    persisted_bytes: u64,
    cancellation_requested_ms: Option<f64>,
    post_cancellation_drain_ms: Option<f64>,
}

enum Outcome {
    Completed(anyhow::Result<NodeOutput>),
    Cancelled { requested_ms: f64 },
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::GenerateFixtures { output } => {
            let manifest = fixtures::generate(&output)?;
            serde_json::to_writer(io::stdout().lock(), &manifest)?;
            println!();
            Ok(())
        }
        Command::Run {
            node,
            input,
            label,
            cancel_after_ms,
        } => run(node, input, label, cancel_after_ms),
    }
}

fn run(
    node_name: String,
    input: Option<PathBuf>,
    label: String,
    cancel_after_ms: Option<u64>,
) -> Result<()> {
    let (input_sha256, raw_bytes, declared_bytes) = match input.as_deref() {
        Some(path) => (
            Some(checksum(path)?),
            std::fs::metadata(path)?.len(),
            declared_size(path)?,
        ),
        None => (None, 0, 0),
    };
    let artifact_dir = std::env::var_os("IRONFLOW_ARTIFACT_DIR").map(PathBuf::from);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let started = Instant::now();
    let outcome = if node_name == "baseline" {
        runtime.block_on(async { tokio::task::yield_now().await });
        Outcome::Completed(Ok(NodeOutput::new()))
    } else {
        let path = input
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("non-baseline worker requires --input"))?;
        let registry = NodeRegistry::with_builtins();
        let node = registry
            .get(&node_name)
            .with_context(|| format!("unknown extraction node '{node_name}'"))?;
        let config = node_config(&node_name, path)?;
        runtime.block_on(async {
            let context = Context::new();
            let execution = node.execute(&config, &context);
            match cancel_after_ms {
                Some(milliseconds) => {
                    match tokio::time::timeout(Duration::from_millis(milliseconds), execution).await
                    {
                        Ok(result) => Outcome::Completed(result),
                        Err(_) => Outcome::Cancelled {
                            requested_ms: started.elapsed().as_secs_f64() * 1_000.0,
                        },
                    }
                }
                None => Outcome::Completed(execution.await),
            }
        })
    };
    let drain_started = Instant::now();
    drop(runtime);
    let drain_ms = drain_started.elapsed().as_secs_f64() * 1_000.0;

    let (status, limit, serialized_output_bytes, cancellation_requested_ms, drain) = match outcome {
        Outcome::Completed(Ok(output)) => ("success", None, serialized_size(&output)?, None, None),
        Outcome::Completed(Err(error)) => {
            let message = error.to_string();
            let limit = extract_limit(&message);
            (
                if limit.is_some() { "limit" } else { "error" },
                limit,
                0,
                None,
                None,
            )
        }
        Outcome::Cancelled { requested_ms } => {
            ("cancelled", None, 0, Some(requested_ms), Some(drain_ms))
        }
    };
    let result = WorkerResult {
        schema_version: 1,
        label,
        node: node_name,
        input_sha256,
        raw_bytes,
        declared_bytes,
        status,
        limit,
        serialized_output_bytes,
        persisted_bytes: artifact_dir
            .as_deref()
            .map(directory_size)
            .transpose()?
            .unwrap_or(0),
        cancellation_requested_ms,
        post_cancellation_drain_ms: drain,
    };
    serde_json::to_writer(io::stdout().lock(), &result)?;
    println!();
    Ok(())
}

fn node_config(node: &str, path: &Path) -> Result<serde_json::Value> {
    let path = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("benchmark input path is not UTF-8"))?;
    Ok(match node {
        "extract_word" => serde_json::json!({
            "path": path, "format": "json", "metadata_key": "metadata", "comments_key": "comments"
        }),
        "extract_pptx" => serde_json::json!({
            "path": path, "format": "json", "metadata_key": "metadata",
            "comments_key": "comments", "media_mode": "artifact"
        }),
        "extract_pdf" | "extract_html" => serde_json::json!({
            "path": path, "format": "text", "metadata_key": "metadata"
        }),
        "extract_srt" | "extract_vtt" => serde_json::json!({
            "path": path, "format": "text", "metadata_key": "metadata", "include_cues": true
        }),
        "extract_xlsx" => serde_json::json!({ "path": path, "output_key": "workbook" }),
        _ => anyhow::bail!("unsupported benchmark node '{node}'"),
    })
}

fn checksum(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut HashWriter(&mut hasher))?;
    Ok(hex::encode(hasher.finalize()))
}

struct HashWriter<'a>(&'a mut Sha256);

impl Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn declared_size(path: &Path) -> Result<u64> {
    if matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("docx" | "pptx" | "xlsx")
    ) {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path)?)?;
        let mut total = 0_u64;
        for index in 0..archive.len() {
            total = total
                .checked_add(archive.by_index_raw(index)?.size())
                .ok_or_else(|| anyhow::anyhow!("declared ZIP byte count overflow"))?;
        }
        Ok(total)
    } else {
        Ok(std::fs::metadata(path)?.len())
    }
}

fn serialized_size(output: &NodeOutput) -> Result<u64> {
    let mut writer = ByteCounter(0);
    serde_json::to_writer(&mut writer, output)?;
    Ok(writer.0)
}

struct ByteCounter(u64);

impl Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len() as u64);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn extract_limit(message: &str) -> Option<String> {
    message
        .split(|character: char| {
            !(character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_')
        })
        .find(|word| word.starts_with("IRONFLOW_MAX_"))
        .map(str::to_owned)
}

fn directory_size(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_size_is_counted_without_returning_content() {
        let output = NodeOutput::from([(
            "content".to_owned(),
            serde_json::Value::String("private document text".to_owned()),
        )]);
        assert_eq!(
            serialized_size(&output).unwrap(),
            serde_json::to_vec(&output).unwrap().len() as u64
        );
    }

    #[test]
    fn limit_classification_retains_only_the_variable_name() {
        assert_eq!(
            extract_limit("path /private/input hit IRONFLOW_MAX_XLSX_CELLS (33000)"),
            Some("IRONFLOW_MAX_XLSX_CELLS".to_owned())
        );
        assert_eq!(extract_limit("parser rejected private content"), None);
    }
}
