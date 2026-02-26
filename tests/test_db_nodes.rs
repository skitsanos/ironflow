use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    std::collections::HashMap::new()
}

fn sqlite_url(path: &std::path::Path) -> String {
    format!("sqlite://{}?mode=rwc", path.to_string_lossy())
}

#[tokio::test]
async fn db_exec_and_query_round_trip_with_typed_params() {
    let reg = NodeRegistry::with_builtins();
    let db_query = reg.get("db_query").unwrap();
    let db_exec = reg.get("db_exec").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let connection = sqlite_url(&db_path);

    let create = serde_json::json!({
        "connection": connection,
        "query": "CREATE TABLE IF NOT EXISTS people (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, age INTEGER, active INTEGER)"
    });
    let created = db_exec.execute(&create, empty_ctx()).await.unwrap();
    assert_eq!(created.get("db_exec_success").unwrap(), true);

    let insert = serde_json::json!({
        "connection": connection,
        "query": "INSERT INTO people(name, age, active) VALUES(?, ?, ?)",
        "params": ["Alice", 42, true]
    });
    let inserted = db_exec.execute(&insert, empty_ctx()).await.unwrap();
    assert_eq!(inserted.get("rows_affected").unwrap(), 1);

    let query = serde_json::json!({
        "connection": connection,
        "query": "SELECT name, age, active FROM people WHERE age > ?",
        "params": [40],
        "output_key": "people"
    });

    let rows = db_query.execute(&query, empty_ctx()).await.unwrap();
    assert_eq!(rows.get("people_success").unwrap(), true);
    assert_eq!(rows.get("people_count").unwrap(), 1);

    let result = rows.get("people").unwrap().as_array().unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].get("name").unwrap(), "Alice");
    assert_eq!(result[0].get("age").unwrap(), 42);
    assert_eq!(result[0].get("active").unwrap(), 1);
}

#[tokio::test]
async fn db_query_returns_empty_when_no_rows() {
    let reg = NodeRegistry::with_builtins();
    let db_query = reg.get("db_query").unwrap();
    let db_exec = reg.get("db_exec").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("empty.db");
    let connection = sqlite_url(&db_path);

    let create = serde_json::json!({
        "connection": connection,
        "query": "CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, value TEXT)"
    });
    db_exec.execute(&create, empty_ctx()).await.unwrap();

    let query = serde_json::json!({
        "connection": connection,
        "query": "SELECT * FROM t WHERE value = ?",
        "params": ["missing"],
        "output_key": "rows"
    });
    let rows = db_query.execute(&query, empty_ctx()).await.unwrap();
    assert_eq!(rows.get("rows_count").unwrap(), 0);
    assert!(rows.get("rows").unwrap().as_array().unwrap().is_empty());
}
