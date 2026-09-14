use super::savgol::savgol_filter;

pub(crate) fn find_local_maxima(
    data: &[f64],
    window_size: usize,
    polynomial_order: usize,
    tolerance: f64,
) -> Option<(Vec<usize>, Vec<f64>)> {
    if data.is_empty() {
        return Some((Vec::new(), Vec::new()));
    }

    let first_derivative = savgol_filter(data, window_size, polynomial_order, 1)?;
    let second_derivative = savgol_filter(data, window_size, polynomial_order, 2)?;
    let mut indices = Vec::new();
    let mut values = Vec::new();
    const EPSILON: f64 = 1e-12;
    let mut start = 0;
    while start < data.len() {
        let mut end = start;
        while end + 1 < data.len() && (data[end + 1] - data[start]).abs() <= EPSILON {
            end += 1;
        }
        // Select the center of a genuine peak, not every low-slope sample or
        // floating-point curvature noise on a flat signal. Exclude endpoints.
        let index = start + (end - start) / 2;
        let is_peak = start > 0
            && end + 1 < data.len()
            && data[start] > data[start - 1] + EPSILON
            && data[end] > data[end + 1] + EPSILON;
        if is_peak
            && data[index] > EPSILON
            && first_derivative[index].abs() < tolerance
            && second_derivative[start..=end]
                .iter()
                .any(|value| *value < -EPSILON)
        {
            indices.push(index);
            values.push(data[index]);
        }
        start = end + 1;
    }
    Some((indices, values))
}

pub(crate) fn filter_split_indices(
    indices: &[usize],
    values: &[f64],
    threshold: f64,
    min_distance: usize,
) -> (Vec<usize>, Vec<f64>) {
    let threshold = if threshold.is_nan() {
        0.0
    } else {
        threshold.clamp(0.0, 1.0)
    };
    if indices.is_empty() || values.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // Keep the strongest distances; higher threshold continues to admit more.
    let threshold_value = percentile(values, 1.0 - threshold);
    let mut result_indices = Vec::new();
    let mut result_values = Vec::new();
    let mut last_index = None;
    for (&index, &value) in indices.iter().zip(values) {
        let has_distance = last_index
            .map(|last: usize| index >= last.saturating_add(min_distance))
            .unwrap_or(true);
        if value >= threshold_value && has_distance {
            result_indices.push(index);
            result_values.push(value);
            last_index = Some(index);
        }
    }
    (result_indices, result_values)
}

pub(crate) fn clamp_odd_window(window: usize, data_len: usize) -> usize {
    let window = window.min(data_len);
    let window = if window.is_multiple_of(2) {
        window.saturating_sub(1)
    } else {
        window
    };
    window.max(3).min(data_len)
}

fn percentile(data: &[f64], percentile: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = percentile * (sorted.len() - 1) as f64;
    let lower = index.floor() as usize;
    let upper = (lower + 1).min(sorted.len() - 1);
    let weight = index - lower as f64;
    sorted[lower] * (1.0 - weight) + sorted[upper] * weight
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_applies_percentile_and_minimum_distance() {
        let (indices, values) = filter_split_indices(&[1, 2, 4], &[0.1, 0.4, 0.2], 0.5, 2);
        assert_eq!(indices, vec![2, 4]);
        assert_eq!(values, vec![0.4, 0.2]);
    }

    #[test]
    fn increasing_threshold_admits_weaker_distance_peaks_not_stronger_valleys() {
        let indices = [1, 3, 5];
        let values = [0.1, 0.8, 0.3];
        assert_eq!(filter_split_indices(&indices, &values, 0.0, 0).0, vec![3]);
        assert_eq!(
            filter_split_indices(&indices, &values, 0.5, 0).0,
            vec![3, 5]
        );
        assert_eq!(
            filter_split_indices(&indices, &values, 1.0, 0).0,
            vec![1, 3, 5]
        );
    }

    #[test]
    fn clamping_returns_an_odd_window_within_data() {
        assert_eq!(clamp_odd_window(10, 8), 7);
        assert_eq!(clamp_odd_window(3, 2), 2);
    }

    #[test]
    fn selects_a_positive_peak_instead_of_a_valley_or_an_endpoint() {
        let (indices, values) = find_local_maxima(&[0.0, 0.2, 0.4, 0.2, 0.0], 5, 2, 0.1).unwrap();
        assert_eq!(indices, vec![2]);
        assert_eq!(values, vec![0.4]);
        assert!(
            find_local_maxima(&[0.4, 0.2, 0.0, 0.2, 0.4], 5, 2, 0.1)
                .unwrap()
                .0
                .is_empty()
        );
    }

    #[test]
    fn a_flat_topped_peak_has_one_center_boundary() {
        for width in [3, 4, 9] {
            let mut data = vec![0.0, 0.2];
            data.extend(vec![0.4; width]);
            data.extend([0.2, 0.0]);
            assert_eq!(
                find_local_maxima(&data, 5, 2, 0.1).unwrap().0,
                vec![2 + (width - 1) / 2]
            );
        }
    }

    #[test]
    fn flat_and_numerically_flat_distances_do_not_create_peaks() {
        for data in [
            vec![0.0; 11],
            vec![1.0; 11],
            vec![0.0, 1e-14, 2e-14, 1e-14, 0.0],
        ] {
            assert!(find_local_maxima(&data, 5, 2, 0.1).unwrap().0.is_empty());
        }
        assert!(find_local_maxima(&[], 5, 2, 0.1).unwrap().0.is_empty());
    }

    #[test]
    fn filtering_keeps_ties_and_preserves_saturating_spacing() {
        let indices = [1, 3, 5];
        let values = [0.8, 0.8, 0.1];
        assert_eq!(
            filter_split_indices(&indices, &values, 0.0, 2).0,
            vec![1, 3]
        );
        assert_eq!(
            filter_split_indices(&indices, &values, 1.0, usize::MAX).0,
            vec![1]
        );
        assert!(filter_split_indices(&[], &[], 0.5, 2).0.is_empty());
    }
}
