use super::savgol::savgol_filter;

pub(crate) fn find_local_minima_interpolated(
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
    for index in 0..data.len() {
        if first_derivative[index].abs() < tolerance && second_derivative[index] > 0.0 {
            indices.push(index);
            values.push(data[index]);
        }
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

    let threshold_value = percentile(values, threshold);
    let mut result_indices = Vec::new();
    let mut result_values = Vec::new();
    let mut last_index = None;
    for (&index, &value) in indices.iter().zip(values) {
        let has_distance = last_index
            .map(|last: usize| index >= last.saturating_add(min_distance))
            .unwrap_or(true);
        if value <= threshold_value && has_distance {
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
        assert_eq!(indices, vec![1, 4]);
        assert_eq!(values, vec![0.1, 0.2]);
    }

    #[test]
    fn clamping_returns_an_odd_window_within_data() {
        assert_eq!(clamp_odd_window(10, 8), 7);
        assert_eq!(clamp_odd_window(3, 2), 2);
    }
}
