use anyhow::{Context as _, Result, ensure};

use super::batch_config::BatchOptions;
use super::transport::Endpoint;

pub(super) async fn embed_batches(
    endpoint: Endpoint<'_>,
    texts: &[String],
    options: &BatchOptions,
) -> Result<Vec<Vec<f64>>> {
    // Validate the complete input before paying for any provider requests.
    for (index, text) in texts.iter().enumerate() {
        ensure!(
            text.len() <= options.max_bytes,
            "embedding input {} exceeds batch_max_bytes ({})",
            index + 1,
            options.max_bytes
        );
        if index.is_multiple_of(512) {
            tokio::task::yield_now().await;
        }
    }
    let maximum = crate::util::limits::max_embedding_values();
    ensure!(
        texts.len() as u64 <= maximum,
        "embedding input count exceeds IRONFLOW_MAX_EMBEDDING_VALUES ({maximum})"
    );
    let mut output: Vec<Vec<f64>> = Vec::new();
    let mut start = 0;
    let mut batch = 0;
    while start < texts.len() {
        tokio::task::yield_now().await;
        let mut end = start;
        let mut bytes = 0;
        while end < texts.len() && end - start < options.size {
            let length = texts[end].len();
            if length > options.max_bytes - bytes {
                break;
            }
            bytes += length;
            end += 1;
        }
        batch += 1;
        let result = async {
            let vectors = endpoint
                .request(&texts[start..end], options.retries)
                .await?;
            let dimension = output.first().map(Vec::len);
            validate_vectors(&vectors, end - start, dimension, texts.len(), maximum)?;
            output
                .try_reserve(vectors.len())
                .context("cannot reserve embedding output")?;
            output.extend(vectors);
            Ok::<_, anyhow::Error>(())
        }
        .await;
        // Keep the batch position in the outer message used by node diagnostics.
        result.map_err(|error| {
            anyhow::anyhow!(
                "embedding batch {batch} (inputs {}-{end} of {}): {error:#}",
                start + 1,
                texts.len()
            )
        })?;
        start = end;
    }
    Ok(output)
}

fn validate_vectors(
    vectors: &[Vec<f64>],
    expected: usize,
    dimension: Option<usize>,
    total_inputs: usize,
    maximum: u64,
) -> Result<()> {
    ensure!(
        vectors.len() == expected,
        "provider returned {} embeddings for {expected} inputs",
        vectors.len()
    );
    let dimension = dimension.unwrap_or_else(|| vectors.first().map_or(0, Vec::len));
    ensure!(dimension > 0, "embedding dimension is 0");
    ensure!(
        (dimension as u64)
            .checked_mul(total_inputs as u64)
            .is_some_and(|size| size <= maximum),
        "embedding output exceeds IRONFLOW_MAX_EMBEDDING_VALUES ({maximum})"
    );
    for (index, vector) in vectors.iter().enumerate() {
        ensure!(
            vector.len() == dimension,
            "embedding {index} has dimension {}, expected {dimension}",
            vector.len()
        );
        ensure!(
            vector.iter().all(|value| value.is_finite()),
            "embedding {index} contains non-finite values"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_count_dimensions_and_total_retained_values() {
        assert!(validate_vectors(&[vec![1.0, 2.0]], 1, None, 3, 6).is_ok());
        assert!(validate_vectors(&[vec![1.0, 2.0]], 1, None, 4, 6).is_err());
        assert!(validate_vectors(&[], 1, None, 1, 10).is_err());
        assert!(validate_vectors(&[vec![]], 1, None, 1, 10).is_err());
        assert!(validate_vectors(&[vec![1.0]], 1, Some(2), 1, 10).is_err());
        assert!(validate_vectors(&[vec![f64::NAN]], 1, None, 1, 10).is_err());
        assert!(validate_vectors(&[vec![1.0, 2.0]], 1, None, usize::MAX, 10).is_err());
    }
}
