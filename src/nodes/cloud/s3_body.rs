//! Bounded collection for S3 download streams.

use anyhow::Result;
use aws_sdk_s3::primitives::ByteStream;

/// Collect an object body without trusting `Content-Length`.
///
/// The caller may reject an oversized declared length before entering this
/// function, but that header can be absent or incorrect on S3-compatible
/// services. The streaming counter is therefore the authoritative bound.
pub(super) async fn collect_capped(mut body: ByteStream, max_bytes: u64) -> Result<Vec<u8>> {
    let capacity = body
        .size_hint()
        .1
        .unwrap_or(0)
        .min(max_bytes)
        .min(64 * 1024)
        .try_into()
        .unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);

    while let Some(chunk) = body
        .try_next()
        .await
        .map_err(|error| anyhow::anyhow!("s3_get_object: failed to read object body: {error}"))?
    {
        let remaining = max_bytes.saturating_sub(bytes.len() as u64);
        if chunk.len() as u64 > remaining {
            anyhow::bail!(
                "s3_get_object: object body exceeded the {max_bytes} byte limit while streaming \
                 (raise IRONFLOW_MAX_FILE_BYTES)"
            );
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::collect_capped;
    use aws_sdk_s3::primitives::ByteStream;

    #[tokio::test]
    async fn exact_limit_is_accepted() {
        let bytes = collect_capped(ByteStream::from_static(b"12345678"), 8)
            .await
            .unwrap();
        assert_eq!(bytes, b"12345678");
    }

    #[tokio::test]
    async fn streaming_bound_rejects_max_plus_one_without_header_input() {
        // No content-length value is passed to the collector. This is the
        // authoritative path for services that omit or misstate the header.
        let error = collect_capped(ByteStream::from_static(b"123456789"), 8)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("8 byte limit while streaming"), "{error}");
        assert!(error.contains("IRONFLOW_MAX_FILE_BYTES"), "{error}");
    }
}
