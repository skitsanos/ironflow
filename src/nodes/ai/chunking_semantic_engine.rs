// =============================================================================
// Sentence Splitting
// =============================================================================

pub(super) fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    while i < len {
        if (bytes[i] == b'.' || bytes[i] == b'!' || bytes[i] == b'?')
            && (i + 1 >= len || bytes[i + 1].is_ascii_whitespace())
        {
            // End of sentence at delimiter
            let end = i + 1;
            let sentence = &text[start..end];
            if !sentence.trim().is_empty() {
                sentences.push(sentence.to_string());
            }
            // Skip trailing whitespace
            i = end;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }

    // Remaining text
    if start < len {
        let remainder = &text[start..];
        if !remainder.trim().is_empty() {
            sentences.push(remainder.to_string());
        }
    }

    sentences
}

// =============================================================================
// Matrix Operations (ported from cognigraph-chunker savgol.rs)
// =============================================================================

fn matrix_multiply(a: &[f64], b: &[f64], m: usize, n: usize, p: usize) -> Vec<f64> {
    let mut c = vec![0.0; m * p];
    for i in 0..m {
        for j in 0..p {
            let mut sum = 0.0;
            for k in 0..n {
                sum += a[i * n + k] * b[k * p + j];
            }
            c[i * p + j] = sum;
        }
    }
    c
}

fn matrix_transpose(a: &[f64], m: usize, n: usize) -> Vec<f64> {
    let mut at = vec![0.0; n * m];
    for i in 0..m {
        for j in 0..n {
            at[j * m + i] = a[i * n + j];
        }
    }
    at
}

fn matrix_inverse(a: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut a_inv = vec![0.0; n * n];
    for i in 0..n {
        a_inv[i * n + i] = 1.0;
    }

    let mut work = a.to_vec();

    for i in 0..n {
        let mut max_row = i;
        let mut max_val = work[i * n + i].abs();
        for k in (i + 1)..n {
            let val = work[k * n + i].abs();
            if val > max_val {
                max_val = val;
                max_row = k;
            }
        }

        if max_row != i {
            for j in 0..n {
                work.swap(i * n + j, max_row * n + j);
                a_inv.swap(i * n + j, max_row * n + j);
            }
        }

        let pivot = work[i * n + i];
        if pivot.abs() < 1e-10 {
            return None;
        }

        for j in 0..n {
            work[i * n + j] /= pivot;
            a_inv[i * n + j] /= pivot;
        }

        for k in 0..n {
            if k != i {
                let factor = work[k * n + i];
                for j in 0..n {
                    work[k * n + j] -= factor * work[i * n + j];
                    a_inv[k * n + j] -= factor * a_inv[i * n + j];
                }
            }
        }
    }

    Some(a_inv)
}

// =============================================================================
// Savitzky-Golay Filter
// =============================================================================

fn compute_savgol_coeffs(window_size: usize, poly_order: usize, deriv: usize) -> Option<Vec<f64>> {
    let half_window = (window_size - 1) / 2;
    let poly_cols = poly_order + 1;

    let mut a = vec![0.0; window_size * poly_cols];
    for i in 0..window_size {
        let x = i as f64 - half_window as f64;
        for j in 0..poly_cols {
            a[i * poly_cols + j] = x.powi(j as i32);
        }
    }

    let at = matrix_transpose(&a, window_size, poly_cols);
    let ata = matrix_multiply(&at, &a, poly_cols, window_size, poly_cols);
    let ata_inv = matrix_inverse(&ata, poly_cols)?;

    let factorial: f64 = (1..=deriv).map(|i| i as f64).product::<f64>().max(1.0);

    let mut coeffs = vec![0.0; window_size];
    for i in 0..window_size {
        if deriv < poly_cols {
            let mut sum = 0.0;
            for k in 0..poly_cols {
                sum += ata_inv[deriv * poly_cols + k] * a[i * poly_cols + k];
            }
            coeffs[i] = factorial * sum;
        }
    }

    Some(coeffs)
}

fn apply_convolution(data: &[f64], kernel: &[f64]) -> Vec<f64> {
    let n = data.len();
    let kernel_size = kernel.len();
    let half = kernel_size / 2;
    let mut output = vec![0.0; n];

    for (i, out) in output.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (j, &k) in kernel.iter().enumerate() {
            let mut idx = i as isize - half as isize + j as isize;
            if idx < 0 {
                idx = -idx;
            } else if idx >= n as isize {
                idx = 2 * n as isize - idx - 2;
            }
            idx = idx.clamp(0, n as isize - 1);
            sum += data[idx as usize] * k;
        }
        *out = sum;
    }

    output
}

pub(super) fn savgol_filter(
    data: &[f64],
    window_length: usize,
    poly_order: usize,
    deriv: usize,
) -> Option<Vec<f64>> {
    if window_length.is_multiple_of(2) || window_length <= poly_order || data.is_empty() {
        return None;
    }

    let coeffs = compute_savgol_coeffs(window_length, poly_order, deriv)?;
    Some(apply_convolution(data, &coeffs))
}

// =============================================================================
// Windowed Cross-Similarity
// =============================================================================

pub(super) fn windowed_cross_similarity(
    embeddings: &[f64],
    n: usize,
    d: usize,
    window_size: usize,
) -> Option<Vec<f64>> {
    if window_size.is_multiple_of(2) || window_size < 3 || n < 2 || d == 0 {
        return None;
    }

    let half_window = window_size / 2;
    let mut result = vec![0.0; n - 1];

    for (i, slot) in result.iter_mut().enumerate() {
        let start = i.saturating_sub(half_window);
        let end = (i + half_window + 2).min(n);

        let mut total_sim = 0.0;
        let mut count = 0;

        for j in start..(end - 1) {
            let emb1_start = j * d;
            let emb2_start = (j + 1) * d;

            let mut dot = 0.0;
            let mut norm1 = 0.0;
            let mut norm2 = 0.0;

            for k in 0..d {
                let v1 = embeddings[emb1_start + k];
                let v2 = embeddings[emb2_start + k];
                dot += v1 * v2;
                norm1 += v1 * v1;
                norm2 += v2 * v2;
            }

            if norm1 > 0.0 && norm2 > 0.0 {
                total_sim += dot / (norm1.sqrt() * norm2.sqrt());
                count += 1;
            }
        }

        *slot = if count > 0 {
            1.0 - (total_sim / count as f64)
        } else {
            0.0
        };
    }

    Some(result)
}

// =============================================================================
// Local Minima Detection
// =============================================================================

pub(super) fn find_local_minima_interpolated(
    data: &[f64],
    window_size: usize,
    poly_order: usize,
    tolerance: f64,
) -> Option<(Vec<usize>, Vec<f64>)> {
    if data.is_empty() {
        return Some((vec![], vec![]));
    }

    let first_deriv = savgol_filter(data, window_size, poly_order, 1)?;
    let second_deriv = savgol_filter(data, window_size, poly_order, 2)?;

    let mut indices = Vec::new();
    let mut values = Vec::new();

    for i in 0..data.len() {
        if first_deriv[i].abs() < tolerance && second_deriv[i] > 0.0 {
            indices.push(i);
            values.push(data[i]);
        }
    }

    Some((indices, values))
}

// =============================================================================
// Split Index Filtering
// =============================================================================

fn percentile(data: &[f64], p: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let idx = p * (sorted.len() - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = (lower + 1).min(sorted.len() - 1);
    let weight = idx - lower as f64;

    sorted[lower] * (1.0 - weight) + sorted[upper] * weight
}

pub(super) fn filter_split_indices(
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
        return (vec![], vec![]);
    }

    let threshold_val = percentile(values, threshold);

    let mut result_indices = Vec::new();
    let mut result_values = Vec::new();
    let mut last_idx: Option<usize> = None;

    for (&idx, &val) in indices.iter().zip(values.iter()) {
        let distance_ok = match last_idx {
            Some(last) => idx >= last + min_distance,
            None => true,
        };

        if val <= threshold_val && distance_ok {
            result_indices.push(idx);
            result_values.push(val);
            last_idx = Some(idx);
        }
    }

    (result_indices, result_values)
}

// =============================================================================
// Helpers
// =============================================================================

pub(super) fn clamp_odd_window(window: usize, data_len: usize) -> usize {
    let w = window.min(data_len);
    let w = if w.is_multiple_of(2) {
        w.saturating_sub(1)
    } else {
        w
    };
    w.max(3).min(data_len)
}

pub(super) fn group_sentences_at_boundaries(
    sentences: &[String],
    split_indices: &[usize],
) -> Vec<String> {
    if sentences.is_empty() {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut chunk_start = 0;

    for &split_idx in split_indices {
        let chunk_end = split_idx + 1;
        if chunk_end > chunk_start && chunk_end <= sentences.len() {
            let chunk_text: String = sentences[chunk_start..chunk_end].join(" ");
            chunks.push(chunk_text);
            chunk_start = chunk_end;
        }
    }

    // Remaining sentences form the last chunk
    if chunk_start < sentences.len() {
        let chunk_text: String = sentences[chunk_start..].join(" ");
        chunks.push(chunk_text);
    }

    chunks
}
