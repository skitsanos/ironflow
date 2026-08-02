use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use rmcp::model::{
    CallToolRequest, CallToolRequestParams, CallToolResult, ClientInfo, ClientRequest,
    ListToolsRequest, ListToolsResult, ProtocolVersion, ServerPeerInfo, ServerResult,
};
use rmcp::service::{PeerRequestOptions, RoleClient, RunningService};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::nodes::child_process::ChildProcessGuard;

use super::config::McpTransport;

const DEFAULT_SESSION_CAPACITY: usize = 1024;
const DEFAULT_SESSION_TTL_SECS: u64 = 3600;
const BACKGROUND_CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) type ClientService = RunningService<RoleClient, ClientInfo>;

pub(super) struct McpSession {
    service: ClientService,
    transport: McpTransport,
    _process_guard: Option<ChildProcessGuard>,
}

impl McpSession {
    pub(super) fn new(
        service: ClientService,
        transport: McpTransport,
        process_guard: Option<ChildProcessGuard>,
    ) -> Self {
        Self {
            service,
            transport,
            _process_guard: process_guard,
        }
    }

    pub(super) fn transport(&self) -> McpTransport {
        self.transport
    }

    pub(super) fn server_info(&self) -> Result<ServerPeerInfo> {
        self.service
            .peer_info()
            .map(|info| info.as_ref().clone())
            .ok_or_else(|| anyhow!("mcp_client: initialized session has no server information"))
    }

    pub(super) fn validate_protocol(&self) -> Result<()> {
        let selected = self.server_info()?.protocol_version;
        if selected != ProtocolVersion::V_2025_11_25 {
            bail!(
                "mcp_client: server selected unsupported protocol version '{selected}'; expected 2025-11-25"
            );
        }
        Ok(())
    }

    fn require_tools_capability(&self) -> Result<()> {
        if self.server_info()?.capabilities.tools.is_none() {
            bail!("mcp_client: server did not negotiate the tools capability");
        }
        Ok(())
    }

    pub(super) async fn list_tools(&self, timeout: Duration) -> Result<ListToolsResult> {
        self.require_tools_capability()?;
        let request = ClientRequest::ListToolsRequest(ListToolsRequest::default());
        let response = self
            .service
            .peer()
            .send_cancellable_request(request, PeerRequestOptions::with_timeout(timeout))
            .await
            .map_err(|error| anyhow!("mcp_client: failed to send tools/list: {error}"))?
            .await_response()
            .await
            .map_err(|error| anyhow!("mcp_client: tools/list failed: {error}"))?;

        match response {
            ServerResult::ListToolsResult(result) => Ok(result),
            _ => bail!("mcp_client: tools/list returned an unexpected response type"),
        }
    }

    pub(super) async fn call_tool(
        &self,
        name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        timeout: Duration,
    ) -> Result<CallToolResult> {
        self.require_tools_capability()?;
        let params = CallToolRequestParams::new(name).with_arguments(arguments);
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(params));
        let response = self
            .service
            .peer()
            .send_cancellable_request(request, PeerRequestOptions::with_timeout(timeout))
            .await
            .map_err(|error| anyhow!("mcp_client: failed to send tools/call: {error}"))?
            .await_response()
            .await
            .map_err(|error| anyhow!("mcp_client: tools/call failed: {error}"))?;

        match response {
            ServerResult::CallToolResult(result) => Ok(result),
            _ => bail!("mcp_client: tools/call returned an unexpected response type"),
        }
    }

    pub(super) async fn close(&mut self, timeout: Duration) -> Result<()> {
        self.service
            .close_with_timeout(timeout)
            .await
            .map_err(|error| anyhow!("mcp_client: session shutdown task failed: {error}"))?;
        Ok(())
    }
}

struct StoredSession {
    session: Arc<AsyncMutex<McpSession>>,
    last_used: Instant,
}

pub(super) struct SessionManager {
    sessions: Mutex<HashMap<String, StoredSession>>,
    capacity: usize,
    ttl: Duration,
}

impl Default for SessionManager {
    fn default() -> Self {
        let capacity = env_usize("IRONFLOW_MCP_SESSION_CACHE_SIZE", DEFAULT_SESSION_CAPACITY);
        let ttl_secs = env_u64("IRONFLOW_MCP_SESSION_TTL_SECS", DEFAULT_SESSION_TTL_SECS);
        Self {
            sessions: Mutex::new(HashMap::new()),
            capacity,
            ttl: Duration::from_secs(ttl_secs),
        }
    }
}

impl SessionManager {
    pub(super) fn insert(&self, session: McpSession) -> Result<String> {
        let now = Instant::now();
        let mut sessions = self.lock_sessions()?;
        let expired = self.prune_expired(&mut sessions, now);
        let mut evicted = Vec::new();
        while sessions.len() >= self.capacity {
            let Some(oldest) = sessions
                .iter()
                .min_by_key(|(_, stored)| stored.last_used)
                .map(|(handle, _)| handle.clone())
            else {
                break;
            };
            if let Some(stored) = sessions.remove(&oldest) {
                evicted.push(stored.session);
            }
        }

        let handle = loop {
            let candidate = format!("mcp_{}", Uuid::new_v4().simple());
            if !sessions.contains_key(&candidate) {
                break candidate;
            }
        };
        sessions.insert(
            handle.clone(),
            StoredSession {
                session: Arc::new(AsyncMutex::new(session)),
                last_used: now,
            },
        );
        drop(sessions);
        schedule_close(expired.into_iter().chain(evicted));
        Ok(handle)
    }

    pub(super) fn lease(self: &Arc<Self>, handle: &str) -> Result<SessionLease> {
        let now = Instant::now();
        let mut sessions = self.lock_sessions()?;
        let expired = self.prune_expired(&mut sessions, now);
        let stored = sessions
            .get_mut(handle)
            .ok_or_else(|| anyhow!("mcp_client: unknown or expired session handle '{handle}'"))?;
        stored.last_used = now;
        let session = Arc::clone(&stored.session);
        drop(sessions);
        schedule_close(expired);

        Ok(SessionLease {
            manager: Arc::clone(self),
            handle: handle.to_string(),
            session,
            armed: true,
        })
    }

    pub(super) async fn close(&self, handle: &str, timeout: Duration) -> Result<McpTransport> {
        let stored = self
            .lock_sessions()?
            .remove(handle)
            .ok_or_else(|| anyhow!("mcp_client: unknown or expired session handle '{handle}'"))?;
        let mut session = stored.session.lock().await;
        let transport = session.transport();
        session.close(timeout).await?;
        Ok(transport)
    }

    fn remove(&self, handle: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(handle);
        }
    }

    fn lock_sessions(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, StoredSession>>> {
        self.sessions
            .lock()
            .map_err(|_| anyhow!("mcp_client: session registry lock poisoned"))
    }

    fn prune_expired(
        &self,
        sessions: &mut HashMap<String, StoredSession>,
        now: Instant,
    ) -> Vec<Arc<AsyncMutex<McpSession>>> {
        let expired = sessions
            .iter()
            .filter(|(_, stored)| now.duration_since(stored.last_used) >= self.ttl)
            .map(|(handle, _)| handle.clone())
            .collect::<Vec<_>>();
        expired
            .into_iter()
            .filter_map(|handle| sessions.remove(&handle).map(|stored| stored.session))
            .collect()
    }
}

pub(super) struct SessionLease {
    manager: Arc<SessionManager>,
    handle: String,
    session: Arc<AsyncMutex<McpSession>>,
    armed: bool,
}

impl SessionLease {
    pub(super) fn session(&self) -> Arc<AsyncMutex<McpSession>> {
        Arc::clone(&self.session)
    }

    pub(super) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        if self.armed {
            self.manager.remove(&self.handle);
        }
    }
}

fn schedule_close(sessions: impl IntoIterator<Item = Arc<AsyncMutex<McpSession>>>) {
    for session in sessions {
        let _cleanup_task = tokio::spawn(async move {
            let mut session = session.lock().await;
            let _ = session.close(BACKGROUND_CLOSE_TIMEOUT).await;
        });
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
