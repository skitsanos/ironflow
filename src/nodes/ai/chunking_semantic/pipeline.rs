use anyhow::Result;

use super::config::SemanticChunkParams;
use crate::nodes::ai::chunking_semantic_engine::{
    clamp_odd_window, filter_split_indices, find_local_minima_interpolated,
    group_sentences_at_boundaries, savgol_filter, windowed_cross_similarity,
};

pub(super) fn build_chunks(
    sentences: &[String],
    embeddings: &[Vec<f64>],
    params: &SemanticChunkParams,
) -> Result<Option<Vec<String>>> {
    validate_embeddings(sentences, embeddings)?;
    let dimension = embeddings.first().map(Vec::len).unwrap_or(0);
    let flattened = embeddings
        .iter()
        .flat_map(|embedding| embedding.iter().copied())
        .collect::<Vec<_>>();
    let Some(similarities) =
        windowed_cross_similarity(&flattened, sentences.len(), dimension, params.sim_window)
    else {
        return Ok(None);
    };

    let smoothed = smooth_similarities(&similarities, params);
    let (minima_indices, minima_values) = find_minima(&smoothed, params);
    let (split_indices, _) = filter_split_indices(
        &minima_indices,
        &minima_values,
        params.threshold,
        params.min_distance,
    );

    Ok(Some(group_sentences_at_boundaries(
        sentences,
        &split_indices,
    )))
}

fn validate_embeddings(sentences: &[String], embeddings: &[Vec<f64>]) -> Result<()> {
    if embeddings.len() != sentences.len() {
        anyhow::bail!(
            "ai_chunk_semantic: provider returned {} embeddings for {} sentences",
            embeddings.len(),
            sentences.len()
        );
    }
    let dimension = embeddings.first().map(Vec::len).unwrap_or(0);
    if dimension == 0 {
        anyhow::bail!("ai_chunk_semantic: embedding dimension is 0");
    }
    if let Some((index, actual)) = embeddings
        .iter()
        .enumerate()
        .find_map(|(index, embedding)| {
            (embedding.len() != dimension).then_some((index, embedding.len()))
        })
    {
        anyhow::bail!(
            "ai_chunk_semantic: embedding {} has dimension {}, expected {}",
            index,
            actual,
            dimension
        );
    }
    Ok(())
}

fn smooth_similarities(similarities: &[f64], params: &SemanticChunkParams) -> Vec<f64> {
    let window = clamp_odd_window(params.sg_window, similarities.len());
    let window = if window <= params.poly_order {
        0
    } else {
        window
    };
    if window >= 3 {
        savgol_filter(similarities, window, params.poly_order, 0)
            .unwrap_or_else(|| similarities.to_vec())
    } else {
        similarities.to_vec()
    }
}

fn find_minima(smoothed: &[f64], params: &SemanticChunkParams) -> (Vec<usize>, Vec<f64>) {
    let window = clamp_odd_window(params.sg_window.max(5), smoothed.len());
    if window >= 3 && window > params.poly_order {
        find_local_minima_interpolated(smoothed, window, params.poly_order, 0.1)
            .unwrap_or_else(|| (Vec::new(), Vec::new()))
    } else {
        (Vec::new(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> SemanticChunkParams {
        SemanticChunkParams {
            timeout_s: 120.0,
            sim_window: 3,
            sg_window: 11,
            poly_order: 3,
            threshold: 0.5,
            min_distance: 2,
        }
    }

    #[test]
    fn rejects_embedding_count_mismatch() {
        let error =
            build_chunks(&["One.".into(), "Two.".into()], &[vec![1.0]], &params()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "ai_chunk_semantic: provider returned 1 embeddings for 2 sentences"
        );
    }

    #[test]
    fn rejects_zero_dimension_embeddings() {
        let error = build_chunks(
            &["One.".into(), "Two.".into()],
            &[Vec::new(), Vec::new()],
            &params(),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "ai_chunk_semantic: embedding dimension is 0"
        );
    }

    #[test]
    fn rejects_inconsistent_embedding_dimensions_without_panicking() {
        let error = build_chunks(
            &["One.".into(), "Two.".into()],
            &[vec![1.0, 2.0], vec![3.0]],
            &params(),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "ai_chunk_semantic: embedding 1 has dimension 1, expected 2"
        );
    }
}
