use std::io;
use std::time::Duration;

use rmcp::RoleClient;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::nodes::child_process::ChildProcessGuard;

const CHANNEL_CAPACITY: usize = 32;
const EOF_GRACE_PERIOD: Duration = Duration::from_secs(1);
const TERMINATE_GRACE_PERIOD: Duration = Duration::from_secs(1);

#[derive(Debug, Error)]
pub(super) enum StdioTransportError {
    #[error("MCP stdio transport closed")]
    Closed,
    #[error("MCP stdio I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("MCP stdio worker failed: {0}")]
    Worker(String),
}

pub(super) struct StrictStdioTransport {
    outgoing: mpsc::Sender<OutgoingFrame>,
    incoming: mpsc::Receiver<RxJsonRpcMessage<RoleClient>>,
    close: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<Result<(), StdioTransportError>>>,
}

struct OutgoingFrame {
    message: TxJsonRpcMessage<RoleClient>,
    written: oneshot::Sender<Result<(), String>>,
}

impl StrictStdioTransport {
    pub(super) fn new(
        mut child: Child,
        process_guard: ChildProcessGuard,
    ) -> Result<Self, StdioTransportError> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| StdioTransportError::Io(io::Error::other("child stdin is not piped")))?;
        let stdout = child.stdout.take().ok_or_else(|| {
            StdioTransportError::Io(io::Error::other("child stdout is not piped"))
        })?;
        let (outgoing, outgoing_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (incoming_tx, incoming) = mpsc::channel(CHANNEL_CAPACITY);
        let (close, close_rx) = oneshot::channel();
        let worker = tokio::spawn(run_worker(
            child,
            stdin,
            stdout,
            process_guard,
            outgoing_rx,
            incoming_tx,
            close_rx,
        ));

        Ok(Self {
            outgoing,
            incoming,
            close: Some(close),
            worker: Some(worker),
        })
    }
}

impl Transport<RoleClient> for StrictStdioTransport {
    type Error = StdioTransportError;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let outgoing = self.outgoing.clone();
        async move {
            let (written, written_rx) = oneshot::channel();
            outgoing
                .send(OutgoingFrame {
                    message: item,
                    written,
                })
                .await
                .map_err(|_| StdioTransportError::Closed)?;
            written_rx
                .await
                .map_err(|_| StdioTransportError::Closed)?
                .map_err(StdioTransportError::Worker)
        }
    }

    fn receive(&mut self) -> impl Future<Output = Option<RxJsonRpcMessage<RoleClient>>> + Send {
        self.incoming.recv()
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let close = self.close.take();
        let worker = self.worker.take();
        async move {
            if let Some(close) = close {
                let _ = close.send(());
            }
            if let Some(worker) = worker {
                worker
                    .await
                    .map_err(|error| StdioTransportError::Worker(error.to_string()))??;
            }
            Ok(())
        }
    }
}

async fn run_worker(
    mut child: Child,
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    process_guard: ChildProcessGuard,
    mut outgoing: mpsc::Receiver<OutgoingFrame>,
    incoming: mpsc::Sender<RxJsonRpcMessage<RoleClient>>,
    mut close: oneshot::Receiver<()>,
) -> Result<(), StdioTransportError> {
    let mut stdout = BufReader::new(stdout);
    let max_frame_bytes =
        crate::util::limits::max_shell_output_bytes().min((usize::MAX - 1) as u64) as usize;

    loop {
        tokio::select! {
            _ = &mut close => break,
            outgoing = outgoing.recv() => {
                let Some(outgoing) = outgoing else { break };
                match write_frame(&mut stdin, &outgoing.message, max_frame_bytes).await {
                    Ok(()) => {
                        let _ = outgoing.written.send(Ok(()));
                    }
                    Err(error) => {
                        let message = error.to_string();
                        let _ = outgoing.written.send(Err(message));
                        return Err(error);
                    }
                }
            }
            frame = read_frame(&mut stdout, max_frame_bytes) => {
                let Some(frame) = frame? else { break };
                let value = serde_json::from_slice::<serde_json::Value>(&frame)
                    .map_err(invalid_frame)?;
                validate_json_rpc_frame(&value)?;
                let message = serde_json::from_value::<RxJsonRpcMessage<RoleClient>>(value)
                    .map_err(invalid_frame)?;
                if incoming.send(message).await.is_err() {
                    break;
                }
            }
        }
    }

    shutdown_child(&mut child, stdin, &process_guard).await;
    Ok(())
}

fn validate_json_rpc_frame(value: &serde_json::Value) -> Result<(), StdioTransportError> {
    let object = value.as_object().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "JSON-RPC frame must be an object",
        )
    })?;
    if object.get("jsonrpc").and_then(serde_json::Value::as_str) != Some("2.0") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "JSON-RPC frame must declare version 2.0",
        )
        .into());
    }

    let has_method = object.contains_key("method");
    let has_result = object.contains_key("result");
    let has_error = object.contains_key("error");
    if has_method {
        if object
            .get("method")
            .and_then(serde_json::Value::as_str)
            .is_none()
            || has_result
            || has_error
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "JSON-RPC request/notification has an invalid envelope",
            )
            .into());
        }
    } else if has_result == has_error {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "JSON-RPC response must contain exactly one of result or error",
        )
        .into());
    }

    if let Some(id) = object.get("id")
        && !is_valid_request_id(id)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "JSON-RPC id must be a string or integer",
        )
        .into());
    }
    if !has_method && !object.contains_key("id") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "JSON-RPC response is missing its id",
        )
        .into());
    }
    if let Some(error) = object.get("error") {
        let error = error.as_object().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "JSON-RPC error must be an object",
            )
        })?;
        if error
            .get("code")
            .and_then(serde_json::Value::as_i64)
            .is_none()
            || error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .is_none()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "JSON-RPC error requires an integer code and string message",
            )
            .into());
        }
    }
    Ok(())
}

fn is_valid_request_id(value: &serde_json::Value) -> bool {
    value.is_string() || value.as_i64().is_some() || value.as_u64().is_some()
}

fn invalid_frame(error: serde_json::Error) -> StdioTransportError {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid MCP JSON-RPC frame: {error}"),
    )
    .into()
}

async fn write_frame(
    stdin: &mut ChildStdin,
    message: &TxJsonRpcMessage<RoleClient>,
    max_frame_bytes: usize,
) -> Result<(), StdioTransportError> {
    let frame = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if frame.len() > max_frame_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("outgoing MCP frame exceeds {max_frame_bytes} bytes"),
        )
        .into());
    }
    stdin.write_all(&frame).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await?;
    Ok(())
}

async fn read_frame(
    reader: &mut BufReader<ChildStdout>,
    max_frame_bytes: usize,
) -> Result<Option<Vec<u8>>, StdioTransportError> {
    let mut frame = Vec::new();
    let bytes_read = reader
        .take((max_frame_bytes + 1) as u64)
        .read_until(b'\n', &mut frame)
        .await?;
    if bytes_read == 0 {
        return Ok(None);
    }
    if bytes_read > max_frame_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("incoming MCP frame exceeds {max_frame_bytes} bytes"),
        )
        .into());
    }
    if frame.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "MCP stdio frame is not newline-delimited",
        )
        .into());
    }
    frame.pop();
    if frame.last() == Some(&b'\r') {
        frame.pop();
    }
    Ok(Some(frame))
}

async fn shutdown_child(child: &mut Child, stdin: ChildStdin, process_guard: &ChildProcessGuard) {
    drop(stdin);
    if tokio::time::timeout(EOF_GRACE_PERIOD, child.wait())
        .await
        .is_ok()
    {
        process_guard.terminate_process_tree();
        return;
    }

    process_guard.request_termination();
    #[cfg(not(unix))]
    let _ = child.start_kill();
    if tokio::time::timeout(TERMINATE_GRACE_PERIOD, child.wait())
        .await
        .is_ok()
    {
        process_guard.terminate_process_tree();
        return;
    }
    process_guard.terminate(child).await;
}
