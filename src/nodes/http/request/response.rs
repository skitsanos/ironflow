use std::io::{self, Read};

use anyhow::Result;

use crate::artifacts::{ArtifactRef, ArtifactStore};
use crate::engine::types::NodeOutput;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::sensitive_url::redact_sensitive_text;

const BODY_CHANNEL_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ResponseMode {
    Inline,
    Artifact,
}

impl ResponseMode {
    pub(super) fn parse(config: &serde_json::Value) -> Result<Self> {
        match config.get("response_mode") {
            None => Ok(Self::Inline),
            Some(serde_json::Value::String(value)) => match value.as_str() {
                "inline" => Ok(Self::Inline),
                "artifact" => Ok(Self::Artifact),
                _ => anyhow::bail!("HTTP response_mode must be 'inline' or 'artifact'"),
            },
            Some(_) => anyhow::bail!("HTTP response_mode must be a string"),
        }
    }
}

pub(super) struct ResponseMetadata {
    pub(super) status: u16,
    pub(super) success: bool,
    pub(super) retry_after_secs: Option<f64>,
    headers: serde_json::Map<String, serde_json::Value>,
    content_length: Option<u64>,
    mime_type: Option<String>,
}

impl ResponseMetadata {
    pub(super) fn from_response(response: &reqwest::Response) -> Self {
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    serde_json::Value::String(value.to_str().unwrap_or("").to_owned()),
                )
            })
            .collect();
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .filter(|value| crate::artifacts::validate_mime_type(Some(value)).is_ok())
            .map(str::to_owned);

        Self {
            status: response.status().as_u16(),
            success: response.status().is_success(),
            retry_after_secs: response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| *value >= 0.0),
            content_length: response.content_length(),
            headers,
            mime_type,
        }
    }
}

pub(super) async fn response_to_output(
    response: reqwest::Response,
    output_key: &str,
    mode: ResponseMode,
    metadata: ResponseMetadata,
) -> Result<NodeOutput> {
    let maximum = crate::util::limits::max_http_body_bytes();
    if let Some(length) = metadata.content_length
        && length > maximum
    {
        anyhow::bail!(
            "HTTP response body {length} bytes exceeds limit {maximum} (set IRONFLOW_MAX_HTTP_BODY_BYTES to raise)"
        );
    }

    let mut output = metadata_output(output_key, &metadata);
    match mode {
        ResponseMode::Inline => {
            let bytes = collect_inline(response, maximum).await?;
            let body = String::from_utf8_lossy(&bytes).into_owned();
            let data = serde_json::from_str(&body).unwrap_or(serde_json::Value::String(body));
            output.insert(format!("{output_key}_data"), data);
        }
        ResponseMode::Artifact => {
            let artifact = store_artifact(response, maximum, metadata.mime_type).await?;
            output.insert(
                format!("{output_key}_artifact"),
                serde_json::to_value(artifact)?,
            );
        }
    }
    Ok(output)
}

fn metadata_output(output_key: &str, metadata: &ResponseMetadata) -> NodeOutput {
    let mut output = NodeOutput::new();
    output.insert(
        format!("{output_key}_status"),
        serde_json::Value::Number(metadata.status.into()),
    );
    output.insert(
        format!("{output_key}_headers"),
        serde_json::Value::Object(metadata.headers.clone()),
    );
    output.insert(
        format!("{output_key}_success"),
        serde_json::Value::Bool(metadata.success),
    );
    output
}

async fn collect_inline(mut response: reqwest::Response, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(response_read_error)? {
        if bytes.len() as u64 + chunk.len() as u64 > maximum {
            anyhow::bail!(
                "HTTP response body exceeds limit {maximum} bytes mid-stream (set IRONFLOW_MAX_HTTP_BODY_BYTES to raise)"
            );
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn store_artifact(
    response: reqwest::Response,
    maximum: u64,
    mime_type: Option<String>,
) -> Result<ArtifactRef> {
    let (sender, receiver) = tokio::sync::mpsc::channel(BODY_CHANNEL_CAPACITY);
    let producer = pump_response(response, sender);
    let consumer = run_tracked_blocking_step(move |execution| {
        ArtifactStore::from_env()?.put_reader(
            ChannelReader::new(receiver),
            maximum,
            mime_type,
            &execution,
        )
    });
    let (producer_result, artifact_result) = tokio::join!(producer, consumer);
    producer_result?;
    artifact_result.map_err(|error| anyhow::anyhow!("Failed to store HTTP response: {error:#}"))
}

async fn pump_response(
    mut response: reqwest::Response,
    sender: tokio::sync::mpsc::Sender<BodyMessage>,
) -> Result<()> {
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if sender
                    .send(BodyMessage::Chunk(chunk.to_vec()))
                    .await
                    .is_err()
                {
                    return Ok(());
                }
            }
            Ok(None) => {
                let _ = sender.send(BodyMessage::Complete).await;
                return Ok(());
            }
            Err(error) => {
                let error = response_read_error(error).to_string();
                let _ = sender.send(BodyMessage::Failed(error.clone())).await;
                anyhow::bail!(error);
            }
        }
    }
}

fn response_read_error(error: reqwest::Error) -> anyhow::Error {
    anyhow::anyhow!(
        "Failed to read HTTP response: {}",
        redact_sensitive_text(&error.to_string())
    )
}

enum BodyMessage {
    Chunk(Vec<u8>),
    Complete,
    Failed(String),
}

struct ChannelReader {
    receiver: tokio::sync::mpsc::Receiver<BodyMessage>,
    current: Vec<u8>,
    offset: usize,
    complete: bool,
}

impl ChannelReader {
    fn new(receiver: tokio::sync::mpsc::Receiver<BodyMessage>) -> Self {
        Self {
            receiver,
            current: Vec::new(),
            offset: 0,
            complete: false,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.offset < self.current.len() {
                let available = &self.current[self.offset..];
                let copied = available.len().min(output.len());
                output[..copied].copy_from_slice(&available[..copied]);
                self.offset += copied;
                return Ok(copied);
            }
            if self.complete {
                return Ok(0);
            }
            match self.receiver.blocking_recv() {
                Some(BodyMessage::Chunk(chunk)) => {
                    self.current = chunk;
                    self.offset = 0;
                }
                Some(BodyMessage::Complete) => self.complete = true,
                Some(BodyMessage::Failed(error)) => return Err(io::Error::other(error)),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "HTTP response stream ended before completion",
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_mode_is_strict() {
        assert_eq!(
            ResponseMode::parse(&serde_json::json!({})).unwrap(),
            ResponseMode::Inline
        );
        assert_eq!(
            ResponseMode::parse(&serde_json::json!({"response_mode": "artifact"})).unwrap(),
            ResponseMode::Artifact
        );
        assert!(ResponseMode::parse(&serde_json::json!({"response_mode": false})).is_err());
        assert!(ResponseMode::parse(&serde_json::json!({"response_mode": "bytes"})).is_err());
    }
}
