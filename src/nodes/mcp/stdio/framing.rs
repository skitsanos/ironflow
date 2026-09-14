use std::io;

use rmcp::RoleClient;
use rmcp::service::TxJsonRpcMessage;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::StdioTransportError;

pub(super) struct FrameReader<R> {
    reader: R,
    frame: Vec<u8>,
    max_frame_bytes: usize,
}

impl<R: AsyncBufRead + Unpin> FrameReader<R> {
    pub(super) fn new(reader: R, max_frame_bytes: usize) -> Self {
        Self {
            reader,
            frame: Vec::new(),
            max_frame_bytes: max_frame_bytes.min(usize::MAX - 1),
        }
    }

    pub(super) async fn read(&mut self) -> Result<Option<Vec<u8>>, StdioTransportError> {
        let max_frame_bytes = self.max_frame_bytes;
        // read_until retains bytes on cancellation, but its per-call count resets.
        // Keep the buffer here and limit each retry to the remaining frame budget.
        if self.frame.len() <= max_frame_bytes {
            let remaining = max_frame_bytes + 1 - self.frame.len();
            (&mut self.reader)
                .take(remaining as u64)
                .read_until(b'\n', &mut self.frame)
                .await?;
        }
        if self.frame.is_empty() {
            return Ok(None);
        }
        if self.frame.len() > max_frame_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("incoming MCP frame exceeds {max_frame_bytes} bytes"),
            )
            .into());
        }
        if self.frame.last() != Some(&b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "MCP stdio frame is not newline-delimited",
            )
            .into());
        }
        self.frame.pop();
        if self.frame.last() == Some(&b'\r') {
            self.frame.pop();
        }
        Ok(Some(std::mem::take(&mut self.frame)))
    }
}

pub(super) async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
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
    writer.write_all(&frame).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests;
