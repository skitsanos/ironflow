//! Integration tests for HTTP, S3, MCP, provider and notification nodes against local servers.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test nodes_network <module>::`.

// Several of these files include the same shared support or fixture module,
// each as its own private copy, exactly as they did as separate crates.
#![allow(clippy::duplicate_mod)]

#[path = "../test_arangodb_cursor.rs"]
mod test_arangodb_cursor;
#[path = "../test_arangodb_node.rs"]
mod test_arangodb_node;
#[path = "../test_embedding_batches.rs"]
mod test_embedding_batches;
#[path = "../test_http_redirect_security.rs"]
mod test_http_redirect_security;
#[path = "../test_http_retry_limits.rs"]
mod test_http_retry_limits;
#[path = "../test_llm_secret_errors.rs"]
mod test_llm_secret_errors;
#[path = "../test_mcp_nodes.rs"]
mod test_mcp_nodes;
#[path = "../test_mcp_streamable_http.rs"]
mod test_mcp_streamable_http;
#[path = "../test_mcp_streamable_http_sse.rs"]
mod test_mcp_streamable_http_sse;
#[path = "../test_notification_response_limits.rs"]
mod test_notification_response_limits;
#[path = "../test_provider_redirect_security.rs"]
mod test_provider_redirect_security;
#[path = "../test_s3_copy.rs"]
mod test_s3_copy;
#[path = "../test_s3_header_signing.rs"]
mod test_s3_header_signing;
#[path = "../test_s3_nodes.rs"]
mod test_s3_nodes;
#[path = "../test_s3_signing_serve.rs"]
mod test_s3_signing_serve;
#[path = "../test_s3vector_nodes.rs"]
mod test_s3vector_nodes;
#[path = "../test_transcribe_node.rs"]
mod test_transcribe_node;
#[path = "../test_transcribe_redirect_security.rs"]
mod test_transcribe_redirect_security;
