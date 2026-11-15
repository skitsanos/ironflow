//! The ceiling on what one `mcp_client` action puts into workflow context
//! (IF-215).
//!
//! The transports bound a reply before it is held in memory (16 MiB for a
//! Streamable HTTP JSON response, `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` for a
//! stdio frame). Those are wire limits. A result is written to context under
//! several keys, so it is measured in its JSON form against a smaller limit
//! before any of them exists.

use std::io::{self, Write};

use anyhow::{Result, bail};
use serde::Serialize;

use super::config::McpAction;

/// Refuse `result` when its JSON form is larger than
/// `IRONFLOW_MAX_MCP_RESULT_BYTES`.
pub(super) fn admit<T: Serialize>(action: McpAction, result: &T) -> Result<()> {
    admit_within(action, result, crate::util::limits::max_mcp_result_bytes())
}

fn admit_within<T: Serialize>(action: McpAction, result: &T, limit: u64) -> Result<()> {
    let mut counter = Counter {
        remaining: limit,
        exceeded: false,
    };
    match serde_json::to_writer(&mut counter, result) {
        Ok(()) => Ok(()),
        Err(_) if counter.exceeded => {
            bail!("mcp_client: {action} result exceeds IRONFLOW_MAX_MCP_RESULT_BYTES ({limit})")
        }
        Err(error) => Err(error.into()),
    }
}

/// Counts serialized bytes and stops the serializer at the limit, so the
/// measurement never holds the encoded result.
struct Counter {
    remaining: u64,
    exceeded: bool,
}

impl Write for Counter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self.remaining.checked_sub(buffer.len() as u64) {
            Some(remaining) => {
                self.remaining = remaining;
                Ok(buffer.len())
            }
            None => {
                self.exceeded = true;
                Err(io::Error::other("MCP result limit exceeded"))
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_result_is_measured_in_its_json_form() {
        // `{"text":"abcd"}` is 15 bytes.
        let result = json!({ "text": "abcd" });

        admit_within(McpAction::CallTool, &result, 15).unwrap();
        let error = admit_within(McpAction::CallTool, &result, 14).unwrap_err();

        assert_eq!(
            error.to_string(),
            "mcp_client: call_tool result exceeds IRONFLOW_MAX_MCP_RESULT_BYTES (14)"
        );
    }
}
