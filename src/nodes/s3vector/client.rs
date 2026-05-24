use anyhow::Result;
use aws_sdk_s3vectors::Client;
use aws_sdk_s3vectors::config::Region;

use crate::engine::types::Context;

use super::config::{resolve_endpoint_url, resolve_region};

pub(super) async fn build_s3vector_client(
    config: &serde_json::Value,
    ctx: &Context,
) -> Result<Client> {
    let region = resolve_region(config, ctx);
    let endpoint_url = resolve_endpoint_url(config, ctx);

    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(region) = region {
        loader = loader.region(Region::new(region));
    }

    let base_config = loader.load().await;
    let mut builder = aws_sdk_s3vectors::config::Builder::from(&base_config);
    if let Some(endpoint_url) = endpoint_url {
        builder = builder.endpoint_url(endpoint_url);
    }
    Ok(Client::from_conf(builder.build()))
}
