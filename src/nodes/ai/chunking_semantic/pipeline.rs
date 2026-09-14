use anyhow::Result;

use super::config::SemanticChunkParams;
use crate::nodes::ai::chunking_semantic_engine::{
    clamp_odd_window, filter_split_indices, find_local_maxima, group_sentences_at_boundaries,
    savgol_filter, windowed_cosine_distance,
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
    let Some(distances) =
        windowed_cosine_distance(&flattened, sentences.len(), dimension, params.sim_window)
    else {
        return Ok(None);
    };

    let smoothed = smooth_distances(&distances, params);
    let (peak_indices, peak_values) = find_peaks(&smoothed, params);
    let (split_indices, _) = filter_split_indices(
        &peak_indices,
        &peak_values,
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

fn smooth_distances(distances: &[f64], params: &SemanticChunkParams) -> Vec<f64> {
    let window = clamp_odd_window(params.sg_window, distances.len());
    let window = if window <= params.poly_order {
        0
    } else {
        window
    };
    if window >= 3 {
        savgol_filter(distances, window, params.poly_order, 0).unwrap_or_else(|| distances.to_vec())
    } else {
        distances.to_vec()
    }
}

fn find_peaks(smoothed: &[f64], params: &SemanticChunkParams) -> (Vec<usize>, Vec<f64>) {
    let window = clamp_odd_window(params.sg_window.max(5), smoothed.len());
    if window >= 3 && window > params.poly_order {
        find_local_maxima(smoothed, window, params.poly_order, 0.1)
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

    fn topics(counts: &[usize]) -> (Vec<String>, Vec<Vec<f64>>) {
        let mut sentences = Vec::new();
        let mut embeddings = Vec::new();
        for (topic, &count) in counts.iter().enumerate() {
            for index in 0..count {
                sentences.push(format!("Topic {topic} sentence {index}."));
                let mut vector = vec![0.0; counts.len()];
                vector[topic] = 1.0;
                embeddings.push(vector);
            }
        }
        (sentences, embeddings)
    }

    #[test]
    fn separates_orthogonal_topics_at_the_actual_transition() {
        let (sentences, embeddings) = topics(&[8, 8]);
        let chunks = build_chunks(&sentences, &embeddings, &params())
            .unwrap()
            .unwrap();
        assert_eq!(
            chunks,
            vec![sentences[..8].join(" "), sentences[8..].join(" ")]
        );
    }

    #[test]
    fn flat_distances_do_not_invent_topic_boundaries() {
        let (sentences, _) = topics(&[24]);
        for vectors in [
            vec![vec![1.0, 2.0]; 24],
            vec![vec![0.0, 0.0]; 24],
            (0..24)
                .map(|i| {
                    if i % 2 == 0 {
                        vec![1.0, 0.0]
                    } else {
                        vec![0.0, 1.0]
                    }
                })
                .collect(),
        ] {
            for threshold in [0.0, 0.5, 1.0] {
                let mut params = params();
                params.threshold = threshold;
                assert_eq!(
                    build_chunks(&sentences, &vectors, &params)
                        .unwrap()
                        .unwrap(),
                    vec![sentences.join(" ")]
                );
            }
        }
    }

    #[test]
    fn minimum_distance_filters_splits_without_losing_sentences() {
        let (sentences, embeddings) = topics(&[8, 8, 8]);
        let mut params = params();
        params.threshold = 1.0;
        for (gap, lengths) in [(0, vec![8, 8, 8]), (8, vec![8, 8, 8]), (9, vec![8, 16])] {
            params.min_distance = gap;
            let chunks = build_chunks(&sentences, &embeddings, &params)
                .unwrap()
                .unwrap();
            let actual: Vec<_> = chunks
                .iter()
                .map(|c| super::super::super::chunking_semantic_engine::split_sentences(c).len())
                .collect();
            assert_eq!(actual, lengths, "gap {gap}, chunks {chunks:?}");
            assert_eq!(chunks.join(" "), sentences.join(" "));
        }
    }

    #[test]
    fn short_inputs_and_unsupported_smoothing_keep_one_chunk() {
        for count in 2..=4 {
            let (sentences, embeddings) = topics(&[count]);
            assert_eq!(
                build_chunks(&sentences, &embeddings, &params())
                    .unwrap()
                    .unwrap(),
                vec![sentences.join(" ")]
            );
        }
        let (sentences, embeddings) = topics(&[8, 8]);
        let mut params = params();
        params.poly_order = 100;
        assert_eq!(
            build_chunks(&sentences, &embeddings, &params)
                .unwrap()
                .unwrap(),
            vec![sentences.join(" ")]
        );
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
