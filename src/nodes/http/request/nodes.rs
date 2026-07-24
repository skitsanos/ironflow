use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::do_http_request;

pub struct HttpRequestNode;

#[async_trait]
impl Node for HttpRequestNode {
    fn node_type(&self) -> &str {
        "http_request"
    }

    fn description(&self) -> &str {
        "Generic HTTP request with configurable method"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let method = config
            .get("method")
            .and_then(|value| value.as_str())
            .unwrap_or("GET");
        do_http_request(method, config, ctx).await
    }
}

macro_rules! fixed_method_node {
    ($name:ident, $node_type:literal, $description:literal, $method:literal) => {
        pub struct $name;

        #[async_trait]
        impl Node for $name {
            fn node_type(&self) -> &str {
                $node_type
            }

            fn description(&self) -> &str {
                $description
            }

            async fn execute(
                &self,
                config: &serde_json::Value,
                ctx: &Context,
            ) -> Result<NodeOutput> {
                do_http_request($method, config, ctx).await
            }
        }
    };
}

fixed_method_node!(HttpGetNode, "http_get", "HTTP GET request", "GET");
fixed_method_node!(HttpPostNode, "http_post", "HTTP POST request", "POST");
fixed_method_node!(HttpPutNode, "http_put", "HTTP PUT request", "PUT");
fixed_method_node!(
    HttpDeleteNode,
    "http_delete",
    "HTTP DELETE request",
    "DELETE"
);
