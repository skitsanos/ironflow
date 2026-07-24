fn matrix_multiply(a: &[f64], b: &[f64], m: usize, n: usize, p: usize) -> Vec<f64> {
    let mut result = vec![0.0; m * p];
    for row in 0..m {
        for column in 0..p {
            let mut sum = 0.0;
            for offset in 0..n {
                sum += a[row * n + offset] * b[offset * p + column];
            }
            result[row * p + column] = sum;
        }
    }
    result
}

fn matrix_transpose(matrix: &[f64], rows: usize, columns: usize) -> Vec<f64> {
    let mut transposed = vec![0.0; columns * rows];
    for row in 0..rows {
        for column in 0..columns {
            transposed[column * rows + row] = matrix[row * columns + column];
        }
    }
    transposed
}

fn matrix_inverse(matrix: &[f64], size: usize) -> Option<Vec<f64>> {
    let mut inverse = vec![0.0; size * size];
    for index in 0..size {
        inverse[index * size + index] = 1.0;
    }
    let mut work = matrix.to_vec();

    for pivot_index in 0..size {
        let mut max_row = pivot_index;
        let mut max_value = work[pivot_index * size + pivot_index].abs();
        for row in (pivot_index + 1)..size {
            let value = work[row * size + pivot_index].abs();
            if value > max_value {
                max_value = value;
                max_row = row;
            }
        }

        if max_row != pivot_index {
            for column in 0..size {
                work.swap(pivot_index * size + column, max_row * size + column);
                inverse.swap(pivot_index * size + column, max_row * size + column);
            }
        }

        let pivot = work[pivot_index * size + pivot_index];
        if pivot.abs() < 1e-10 {
            return None;
        }
        for column in 0..size {
            work[pivot_index * size + column] /= pivot;
            inverse[pivot_index * size + column] /= pivot;
        }

        for row in 0..size {
            if row == pivot_index {
                continue;
            }
            let factor = work[row * size + pivot_index];
            for column in 0..size {
                work[row * size + column] -= factor * work[pivot_index * size + column];
                inverse[row * size + column] -= factor * inverse[pivot_index * size + column];
            }
        }
    }
    Some(inverse)
}

fn compute_coefficients(
    window_size: usize,
    polynomial_order: usize,
    derivative: usize,
) -> Option<Vec<f64>> {
    let half_window = (window_size - 1) / 2;
    let polynomial_columns = polynomial_order + 1;
    let mut design = vec![0.0; window_size * polynomial_columns];
    for row in 0..window_size {
        let x = row as f64 - half_window as f64;
        for column in 0..polynomial_columns {
            design[row * polynomial_columns + column] = x.powi(column as i32);
        }
    }

    let transpose = matrix_transpose(&design, window_size, polynomial_columns);
    let product = matrix_multiply(
        &transpose,
        &design,
        polynomial_columns,
        window_size,
        polynomial_columns,
    );
    let inverse = matrix_inverse(&product, polynomial_columns)?;
    let factorial = (1..=derivative)
        .map(|value| value as f64)
        .product::<f64>()
        .max(1.0);
    let mut coefficients = vec![0.0; window_size];

    if derivative < polynomial_columns {
        for row in 0..window_size {
            let mut sum = 0.0;
            for column in 0..polynomial_columns {
                sum += inverse[derivative * polynomial_columns + column]
                    * design[row * polynomial_columns + column];
            }
            coefficients[row] = factorial * sum;
        }
    }
    Some(coefficients)
}

fn apply_convolution(data: &[f64], kernel: &[f64]) -> Vec<f64> {
    let data_len = data.len();
    let half_kernel = kernel.len() / 2;
    let mut output = vec![0.0; data_len];

    for (index, value) in output.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (offset, &coefficient) in kernel.iter().enumerate() {
            let mut source = index as isize - half_kernel as isize + offset as isize;
            if source < 0 {
                source = -source;
            } else if source >= data_len as isize {
                source = 2 * data_len as isize - source - 2;
            }
            source = source.clamp(0, data_len as isize - 1);
            sum += data[source as usize] * coefficient;
        }
        *value = sum;
    }
    output
}

pub(crate) fn savgol_filter(
    data: &[f64],
    window_length: usize,
    polynomial_order: usize,
    derivative: usize,
) -> Option<Vec<f64>> {
    if window_length.is_multiple_of(2) || window_length <= polynomial_order || data.is_empty() {
        return None;
    }

    let coefficients = compute_coefficients(window_length, polynomial_order, derivative)?;
    Some(apply_convolution(data, &coefficients))
}

#[cfg(test)]
mod tests {
    use super::savgol_filter;

    #[test]
    fn rejects_even_windows() {
        assert!(savgol_filter(&[1.0, 2.0, 3.0], 2, 1, 0).is_none());
    }
}
