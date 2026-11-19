//! IF-223 (b): `csv_stringify` writes text that a spreadsheet would read as
//! a formula with a leading apostrophe, unless the flow turns that off.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

async fn run(node: &str, config: Value, ctx: &Context) -> anyhow::Result<Context> {
    let registry = NodeRegistry::with_builtins();
    let output = registry.get(node).unwrap().execute(&config, ctx).await?;
    Ok(output.into_iter().collect())
}

async fn stringify(source: Value, extra: Value) -> anyhow::Result<String> {
    let mut config = json!({"source_key": "rows", "output_key": "csv"});
    config
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let ctx: Context = [("rows".to_string(), source)].into_iter().collect();
    let output = run("csv_stringify", config, &ctx).await?;
    Ok(output["csv"].as_str().unwrap().to_string())
}

async fn parse(csv: &str, extra: Value) -> Value {
    let mut config = json!({"source_key": "csv", "output_key": "rows"});
    config
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let ctx: Context = [("csv".to_string(), json!(csv))].into_iter().collect();
    run("csv_parse", config, &ctx).await.unwrap()["rows"].clone()
}

#[tokio::test]
async fn text_that_starts_a_formula_is_prefixed_by_default() {
    let rows = json!([
        ["=1+1"],
        ["+cmd|' /C calc'!A0"],
        ["-2+3"],
        ["@SUM(A1:A2)"],
        ["\tindented"],
        ["\rreturn"],
        ["\nline"],
        ["＝1+1"],
        ["＋1"],
        ["－1"],
        ["＠x"],
        ["=HYPERLINK(\"http://example.invalid\",\"x\"),1"],
        ["-"],
        ["+1 (555) 0100"]
    ]);
    let csv = stringify(rows, json!({"include_headers": false}))
        .await
        .unwrap();
    assert_eq!(
        csv,
        "'=1+1\n'+cmd|' /C calc'!A0\n'-2+3\n'@SUM(A1:A2)\n'\tindented\n\"'\rreturn\"\n\
         \"'\nline\"\n'＝1+1\n'＋1\n'－1\n'＠x\n\
         \"'=HYPERLINK(\"\"http://example.invalid\"\",\"\"x\"\"),1\"\n'-\n'+1 (555) 0100\n"
    );
}

#[tokio::test]
async fn numbers_and_numeric_text_are_written_unchanged() {
    let rows = json!([
        [-5, -1.5, 1e30, -0.0, 7],
        ["-5", "+1.5e3", "-0.25", "-.5", "+15551234567"]
    ]);
    let csv = stringify(rows, json!({"include_headers": false}))
        .await
        .unwrap();
    assert_eq!(
        csv,
        "-5,-1.5,1e+30,-0.0,7\n-5,+1.5e3,-0.25,-.5,+15551234567\n"
    );
}

#[tokio::test]
async fn sign_followed_by_anything_but_a_number_is_prefixed() {
    let rows = json!([[
        "-5+1", "+1e", "-1.2.3", "--5", "+ 5", "-5 ", "-0x1F", "-1e5e5"
    ]]);
    let csv = stringify(rows, json!({"include_headers": false}))
        .await
        .unwrap();
    assert_eq!(csv, "'-5+1,'+1e,'-1.2.3,'--5,'+ 5,'-5 ,'-0x1F,'-1e5e5\n");
}

#[tokio::test]
async fn ordinary_text_is_written_unchanged() {
    let rows = json!([[
        "plain", "a=b", " =1+1", "x@example.com", "", "'quoted", "1+1", true, null,
        {"k": "=1"}
    ]]);
    let csv = stringify(rows, json!({"include_headers": false}))
        .await
        .unwrap();
    assert_eq!(
        csv,
        "plain,a=b, =1+1,x@example.com,,'quoted,1+1,true,,\"{\"\"k\"\":\"\"=1\"\"}\"\n"
    );
}

#[tokio::test]
async fn headers_and_every_input_shape_are_covered() {
    let objects = json!([{"=cmd": "@a", "name": "ok"}]);
    assert_eq!(
        stringify(objects, json!({})).await.unwrap(),
        "'=cmd,name\n'@a,ok\n"
    );
    let object = json!({"+total": "=SUM(A1)", "qty": -3});
    assert_eq!(
        stringify(object, json!({})).await.unwrap(),
        "'+total,qty\n'=SUM(A1),-3\n"
    );
    let scalars = json!(["=1", -2, "text"]);
    assert_eq!(
        stringify(scalars, json!({})).await.unwrap(),
        "value\n'=1\n-2\ntext\n"
    );
    let quoted = json!([["=1", -2]]);
    assert_eq!(
        stringify(quoted, json!({"quote_all": true, "include_headers": false}))
            .await
            .unwrap(),
        "\"'=1\",\"-2\"\n"
    );
}

#[tokio::test]
async fn escape_formulas_false_writes_raw_cells() {
    let rows = json!([{"=cmd": "=1+1", "n": "-2+3"}]);
    for off in [json!(false), json!("false")] {
        assert_eq!(
            stringify(rows.clone(), json!({"escape_formulas": off}))
                .await
                .unwrap(),
            "=cmd,n\n=1+1,-2+3\n"
        );
    }
    let error = stringify(rows, json!({"escape_formulas": "sometimes"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("escape_formulas"), "{error}");
}

#[tokio::test]
async fn csv_parse_returns_the_prefix_as_part_of_the_text() {
    let rows = json!([{"formula": "=1+1", "amount": -5, "note": "plain"}]);
    let escaped = stringify(rows.clone(), json!({})).await.unwrap();
    assert_eq!(
        parse(&escaped, json!({"infer_types": true})).await,
        json!([{"formula": "'=1+1", "amount": -5, "note": "plain"}])
    );
    let raw = stringify(rows.clone(), json!({"escape_formulas": false}))
        .await
        .unwrap();
    assert_eq!(parse(&raw, json!({"infer_types": true})).await, rows);
}

async fn assert_header_collision(source: Value, options: Value, columns: &str) {
    let error = stringify(source, options).await.unwrap_err().to_string();
    assert!(error.contains("header collision"), "{error}");
    assert!(error.contains(columns), "{error}");
}

#[tokio::test]
async fn single_object_rejects_colliding_escaped_headers() {
    assert_header_collision(
        json!({"@region":"first", "'@region":"second"}),
        json!({}),
        "columns 1 and 2",
    )
    .await;
}

#[tokio::test]
async fn object_array_checks_escaped_headers_across_rows_even_when_quoted() {
    assert_header_collision(
        json!([{"@region":"first", "0":"between"}, {"'@region":"second"}]),
        json!({"quote_all":true}),
        "columns 1 and 3",
    )
    .await;
}

#[tokio::test]
async fn headerless_and_raw_exports_preserve_both_columns() {
    let row = json!({"@region":"@east", "'@region":"west"});
    for source in [row.clone(), json!([row.clone()])] {
        let headerless = stringify(source.clone(), json!({"include_headers":false}))
            .await
            .unwrap();
        assert_eq!(
            parse(&headerless, json!({"has_header":false})).await,
            json!([["west", "'@east"]])
        );
        let raw = stringify(source, json!({"escape_formulas":false}))
            .await
            .unwrap();
        assert_eq!(parse(&raw, json!({})).await, json!([row.clone()]));
    }
}

#[tokio::test]
async fn repeated_source_keys_and_distinct_escaped_headers_still_round_trip() {
    let source = json!([{"@region":"east", "'note":"1"}, {"@region":"west", "'note":"2"}]);
    let csv = stringify(source, json!({})).await.unwrap();
    assert_eq!(
        parse(&csv, json!({})).await,
        json!([
            {"'@region":"east", "'note":"1"}, {"'@region":"west", "'note":"2"}
        ])
    );
}
