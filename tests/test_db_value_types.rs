use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::{Node, database::DbQueryNode};
use serde_json::{Value, json};

async fn query(connection: &str, sql: &str) -> anyhow::Result<NodeOutput> {
    DbQueryNode
        .execute(
            &json!({"connection": connection, "query": sql}),
            &Context::new(),
        )
        .await
}

#[tokio::test]
async fn sqlite_aggregates_and_expressions_preserve_numeric_types() {
    let output = query(
        "sqlite::memory:",
        "WITH samples(amount) AS (VALUES (10), (20)) SELECT COUNT(*) AS count, \
         SUM(amount) AS total, AVG(amount) AS average, 1 AS literal, \
         'text' AS label, SUM(amount) / 2.0 AS calculated FROM samples",
    )
    .await
    .unwrap();
    assert_eq!(output["rows_success"], true);
    assert_eq!(output["rows_count"], 1);
    assert_eq!(
        output["rows"],
        json!([{"count": 2, "total": 30, "average": 15.0, "literal": 1,
                "label": "text", "calculated": 15.0}])
    );
    assert!(output["rows"][0]["count"].is_i64());
    assert!(output["rows"][0]["average"].is_f64());
}

#[tokio::test]
async fn sqlite_literals_preserve_null_text_binary_and_integer_boundaries() {
    let output = query(
        "sqlite::memory:",
        "SELECT -9223372036854775808 AS minimum, 9223372036854775807 AS maximum, \
         9007199254740993 AS above_float_precision, 1.25 AS fractional, \
         NULL AS missing, '' AS empty_text, '123' AS numeric_text, TRUE AS flag, \
         X'00FF8041' AS bytes, X'' AS empty_bytes",
    )
    .await
    .unwrap();
    assert_eq!(
        output["rows"],
        json!([{
            "minimum": i64::MIN, "maximum": i64::MAX,
            "above_float_precision": 9_007_199_254_740_993_i64, "fractional": 1.25,
            "missing": null, "empty_text": "", "numeric_text": "123", "flag": 1,
            "bytes": [0, 255, 128, 65], "empty_bytes": []
        }])
    );
}

#[tokio::test]
async fn sqlite_declared_columns_follow_each_rows_runtime_storage_class() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("types.db").display()
    );
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    sqlx::raw_sql(
        "CREATE TABLE samples (id INTEGER PRIMARY KEY, value INTEGER, label TEXT, \
         measurement REAL, payload BLOB); \
         INSERT INTO samples VALUES (1, NULL, NULL, NULL, NULL), \
         (2, 10, 'ten', 1.25, X'00FF'), (3, 2.5, 'fraction', 2.5, X''), \
         (4, 'not numeric', '', 0, NULL), (5, X'80', 'binary', -1.25, X'80');",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
    let output = query(&url, "SELECT * FROM samples ORDER BY id")
        .await
        .unwrap();
    assert_eq!(
        output["rows"],
        json!([
            {"id": 1, "value": null, "label": null, "measurement": null, "payload": null},
            {"id": 2, "value": 10, "label": "ten", "measurement": 1.25, "payload": [0, 255]},
            {"id": 3, "value": 2.5, "label": "fraction", "measurement": 2.5, "payload": []},
            {"id": 4, "value": "not numeric", "label": "", "measurement": 0.0, "payload": null},
            {"id": 5, "value": [128], "label": "binary", "measurement": -1.25, "payload": [128]}
        ])
    );
}

#[tokio::test]
async fn sqlite_empty_aggregates_preserve_actual_nulls() {
    let output = query(
        "sqlite::memory:",
        "WITH empty(value) AS (SELECT 1 WHERE 0) \
         SELECT COUNT(*) AS count, SUM(value) AS total, AVG(value) AS average FROM empty",
    )
    .await
    .unwrap();
    assert_eq!(
        output["rows"],
        json!([{"count": 0, "total": null, "average": null}])
    );
}

#[tokio::test]
async fn sqlite_non_finite_values_fail_without_partial_success() {
    for value in ["1e999", "-1e999"] {
        let error = query(
            "sqlite::memory:",
            &format!("SELECT 1.0 AS value UNION ALL SELECT {value}"),
        )
        .await
        .expect_err("non-finite JSON numbers must fail, even after valid rows");
        assert!(error.to_string().contains("non-finite"), "{error}");
    }
}

#[tokio::test]
async fn sqlite_driver_decode_and_aggregate_errors_are_not_nulls() {
    for sql in [
        "SELECT CAST(X'FF' AS TEXT) AS invalid_utf8",
        "WITH samples(value) AS (VALUES (9223372036854775807), (1)) SELECT SUM(value) FROM samples",
    ] {
        let error = query("sqlite::memory:", sql).await.unwrap_err();
        assert!(error.to_string().contains("db_query failed"), "{error}");
    }
}

#[tokio::test]
async fn sqlite_binary_output_obeys_serialized_result_limit() {
    let rows = json!([{"bytes": [0, 255, 128, 65]}]);
    let length = serde_json::to_vec(&rows).unwrap().len();
    for limit in [length, length - 1] {
        let result = DbQueryNode
            .execute(
                &json!({"connection": "sqlite::memory:", "query": "SELECT X'00FF8041' AS bytes",
                        "max_result_bytes": limit}),
                &Context::new(),
            )
            .await;
        if limit == length {
            assert_eq!(result.unwrap()["rows"], rows);
        } else {
            assert!(result.unwrap_err().to_string().contains("max_result_bytes"));
        }
    }
    assert_ne!(rows[0]["bytes"], Value::Null);
}
