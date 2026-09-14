#![cfg(feature = "postgres")]

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::{Node, database::DbQueryNode};
use serde_json::{Value, json};

// No dotenv discovery: live tests must opt into an explicitly supplied service.
fn connection() -> Option<String> {
    let url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"));
    if url.is_none() {
        let required = std::env::var("IRONFLOW_POSTGRES_TEST_REQUIRED")
            .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"));
        assert!(
            !required,
            "PostgreSQL query tests require a PostgreSQL DATABASE_URL"
        );
        eprintln!("Skipping PostgreSQL query test: DATABASE_URL is not configured for PostgreSQL");
    }
    url
}

async fn query(url: &str, sql: &str, params: Value) -> anyhow::Result<NodeOutput> {
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        DbQueryNode.execute(
            &json!({"connection": url, "query": sql, "params": params}),
            &Context::new(),
        ),
    )
    .await
    .expect("PostgreSQL query timed out")
}

#[tokio::test]
async fn postgres_supported_scalars_preserve_values_and_nulls() {
    let Some(url) = connection() else { return };
    let output = query(
        &url,
        "WITH samples(id, small, integer, big, single, double, flag, label, bytes) AS (VALUES \
         (1, (-32768)::smallint, (-2147483648)::integer, 9223372036854775807::bigint, \
         1.25::real, 2.5::double precision, true, '123'::varchar, decode('00ff8041', 'hex')), \
         (2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)) SELECT * FROM samples ORDER BY id",
        json!([]),
    )
    .await
    .unwrap();
    assert_eq!(
        output["rows"],
        json!([
            {"id": 1, "small": -32768, "integer": i32::MIN, "big": i64::MAX, "single": 1.25,
             "double": 2.5, "flag": true, "label": "123", "bytes": [0, 255, 128, 65]},
            {"id": 2, "small": null, "integer": null, "big": null, "single": null,
             "double": null, "flag": null, "label": null, "bytes": null}
        ])
    );
    assert!(output["rows"][0]["single"].is_f64());
    assert_eq!(output["rows_success"], true);
    assert_eq!(output["rows_count"], 2);
}

#[tokio::test]
async fn postgres_aggregates_and_bound_parameters_preserve_types() {
    let Some(url) = connection() else { return };
    let output = query(&url,
        "WITH samples(amount) AS (VALUES (10), (20)) SELECT COUNT(*) AS count, SUM(amount) AS total, \
         AVG(amount::double precision) AS average FROM samples", json!([])).await.unwrap();
    assert_eq!(
        output["rows"],
        json!([{"count": 2, "total": 30, "average": 15.0}])
    );
    let output = query(
        &url,
        "SELECT $1::bigint AS integer, $2::double precision AS fractional, $3::boolean AS flag, \
         $4::text AS label, $5::text AS missing, decode('', 'hex') AS empty_bytes",
        json!([9_007_199_254_740_993_i64, 1.25, false, "123", null]),
    )
    .await
    .unwrap();
    assert_eq!(
        output["rows"],
        json!([{"integer": 9_007_199_254_740_993_i64,
        "fractional": 1.25, "flag": false, "label": "123", "missing": null, "empty_bytes": []}])
    );
}

#[tokio::test]
async fn postgres_unsupported_generic_driver_types_fail_instead_of_becoming_null() {
    let Some(url) = connection() else { return };
    for sql in [
        "SELECT 1.25::numeric AS value",
        "SELECT NULL::numeric AS value",
        "SELECT AVG(value) FROM (VALUES (10), (20)) AS samples(value)",
        "SELECT '{}'::jsonb AS value",
        "SELECT DATE '2026-09-13' AS value",
        "SELECT ARRAY[1, 2] AS value",
    ] {
        let error = query(&url, sql, json!([])).await.unwrap_err();
        assert!(
            error.to_string().contains("Any driver does not support"),
            "{error}"
        );
    }
    let output = query(&url, "SELECT 1.25::numeric::text AS decimal", json!([]))
        .await
        .unwrap();
    assert_eq!(output["rows"], json!([{"decimal": "1.25"}]));
}

#[tokio::test]
async fn postgres_non_finite_values_fail_without_partial_success() {
    let Some(url) = connection() else { return };
    for kind in ["real", "double precision"] {
        for value in ["NaN", "Infinity", "-Infinity"] {
            let sql = format!("SELECT 1::{kind} AS value UNION ALL SELECT '{value}'::{kind}");
            let error = query(&url, &sql, json!([])).await.unwrap_err();
            assert!(error.to_string().contains("non-finite"), "{error}");
        }
    }
}
