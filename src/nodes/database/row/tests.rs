use super::*;

#[tokio::test]
async fn incompatible_decoding_fails_without_disclosing_cell_contents() {
    sqlx::any::install_default_drivers();
    let pool = sqlx::AnyPool::connect("sqlite::memory:").await.unwrap();
    let row = sqlx::query("SELECT 'cell-secret-sentinel' AS value")
        .fetch_one(&pool)
        .await
        .unwrap();
    let error = decode::<i64>(&row, 0).unwrap_err();
    assert!(error.to_string().contains("column at index 0"));
    assert!(!format!("{error:#}").contains("cell-secret-sentinel"));
    assert!(error.source().is_none());
    assert_eq!(
        row_to_json(&row).unwrap(),
        json!({"value": "cell-secret-sentinel"})
    );
    pool.close().await;
}

#[test]
fn non_finite_numbers_are_errors_not_json_null() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = finite_number(value, 3).unwrap_err();
        assert!(error.to_string().contains("non-finite"));
        assert!(error.to_string().contains("index 3"));
    }
    for value in [0.0, -1.25, f64::MIN, f64::MAX] {
        assert_eq!(finite_number(value, 0).unwrap().as_f64(), Some(value));
    }
}
