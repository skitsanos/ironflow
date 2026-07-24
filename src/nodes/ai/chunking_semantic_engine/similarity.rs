pub(crate) fn windowed_cross_similarity(
    embeddings: &[f64],
    count: usize,
    dimension: usize,
    window_size: usize,
) -> Option<Vec<f64>> {
    if window_size.is_multiple_of(2) || window_size < 3 || count < 2 || dimension == 0 {
        return None;
    }

    let half_window = window_size / 2;
    let mut result = vec![0.0; count - 1];
    for (index, slot) in result.iter_mut().enumerate() {
        let start = index.saturating_sub(half_window);
        let end = index
            .saturating_add(half_window)
            .saturating_add(2)
            .min(count);
        let mut total_similarity = 0.0;
        let mut comparisons = 0;

        for pair_start in start..(end - 1) {
            let left_start = pair_start * dimension;
            let right_start = (pair_start + 1) * dimension;
            let mut dot_product = 0.0;
            let mut left_norm = 0.0;
            let mut right_norm = 0.0;

            for offset in 0..dimension {
                let left = embeddings[left_start + offset];
                let right = embeddings[right_start + offset];
                dot_product += left * right;
                left_norm += left * left;
                right_norm += right * right;
            }

            if left_norm > 0.0 && right_norm > 0.0 {
                total_similarity += dot_product / (left_norm.sqrt() * right_norm.sqrt());
                comparisons += 1;
            }
        }

        *slot = if comparisons > 0 {
            1.0 - total_similarity / comparisons as f64
        } else {
            0.0
        };
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::windowed_cross_similarity;

    #[test]
    fn identical_vectors_have_zero_distance() {
        let embeddings = [1.0, 0.0, 1.0, 0.0];
        assert_eq!(
            windowed_cross_similarity(&embeddings, 2, 2, 3),
            Some(vec![0.0])
        );
    }
}
