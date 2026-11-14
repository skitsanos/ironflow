//! IF-196: `xml_stringify` names and scalar values, `csv_parse` type
//! inference, and numeric equality in `data_filter` and `deduplicate`.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn ctx_with(pairs: Vec<(&str, Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

async fn run(node: &str, config: Value, ctx: &Context) -> anyhow::Result<Context> {
    let registry = NodeRegistry::with_builtins();
    let output = registry.get(node).unwrap().execute(&config, ctx).await?;
    Ok(output.into_iter().collect())
}

async fn stringify(source: Value, extra: Value) -> anyhow::Result<String> {
    let mut config = json!({"source_key": "doc"});
    config
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let output = run("xml_stringify", config, &ctx_with(vec![("doc", source)])).await?;
    Ok(output["xml"].as_str().unwrap().to_string())
}

async fn parse_xml(xml: &str) -> Value {
    let ctx = ctx_with(vec![("xml", json!(xml))]);
    let output = run("xml_parse", json!({"source_key": "xml"}), &ctx)
        .await
        .unwrap();
    output["xml_data"].clone()
}

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";

// --- xml_stringify ---

#[tokio::test]
async fn xml_stringify_supported_document_is_unchanged() {
    let source = json!({
        "book": {"@id": "1", "title": "Rust & <XML>", "tag": ["a", "b"], "none": null},
        "count": 3,
        "ok": {"@unit": "kg", "#text": "3"},
        "ready": true
    });
    let xml = stringify(source, json!({})).await.unwrap();
    assert_eq!(
        xml,
        format!(
            "{DECLARATION}<root><book id=\"1\"><none/><tag>a</tag><tag>b</tag>\
             <title>Rust &amp; &lt;XML&gt;</title></book><count>3</count>\
             <ok unit=\"kg\">3</ok><ready>true</ready></root>"
        )
    );
}

#[tokio::test]
async fn xml_stringify_rejects_keys_that_are_not_xml_names() {
    for key in [
        "first name",
        "a/><admin>true</admin><b",
        "",
        "1st",
        "-a",
        "a:b:c",
        ":a",
        "a:",
        "a\"b",
        "#comment",
        "xmlns",
        "xmlns:a",
    ] {
        let error = stringify(json!({ key: "v" }), json!({})).await.unwrap_err();
        assert!(
            error.to_string().contains("not a valid XML element name"),
            "{key:?}: {error}"
        );
    }
    for key in ["@", "@unit price", "@a=\"1\" b", "@xmlns:xmlns", "@a:b:c"] {
        let error = stringify(json!({"item": { key: "v" }}), json!({}))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("not a valid XML attribute name"),
            "{key:?}: {error}"
        );
    }
    let error = stringify(json!({"a": 1}), json!({"root_tag": "my root"}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("root_tag"), "{error}");
}

#[tokio::test]
async fn xml_stringify_accepts_prefixed_and_non_ascii_names() {
    let source = json!({
        "@xmlns:soap": "http://www.w3.org/2003/05/soap-envelope",
        "@xml:lang": "en",
        "soap:Body": {"naïve_名前": "x", "xml_data": "kept", "a.b-c": "y"}
    });
    let xml = stringify(source.clone(), json!({"root_tag": "soap:Envelope"}))
        .await
        .unwrap();
    assert_eq!(parse_xml(&xml).await, json!({"soap:Envelope": source}));
}

#[tokio::test]
async fn xml_stringify_writes_scalar_attribute_and_text_values() {
    let source = json!({
        "qty": {"@unit": 5, "#text": 3},
        "flag": {"@on": true, "@ratio": 2.5, "@gone": null, "#text": false},
        "blank": {"@unit": "kg", "#text": null}
    });
    let xml = stringify(source, json!({})).await.unwrap();
    assert_eq!(
        xml,
        format!(
            "{DECLARATION}<root><blank unit=\"kg\"/>\
             <flag on=\"true\" ratio=\"2.5\">false</flag>\
             <qty unit=\"5\">3</qty></root>"
        )
    );
}

#[tokio::test]
async fn xml_stringify_rejects_structured_attribute_and_text_values() {
    for value in [json!({"@unit": {"a": 1}}), json!({"@unit": [1, 2]})] {
        let error = stringify(json!({ "qty": value }), json!({}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("attribute"), "{error}");
    }
    for value in [json!({"#text": {"a": 1}}), json!({"#text": ["a"]})] {
        let error = stringify(json!({ "qty": value }), json!({}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("#text"), "{error}");
    }
}

#[tokio::test]
async fn xml_stringify_round_trips_through_xml_parse() {
    let source = json!({
        "qty": {"@unit": 5, "#text": 3},
        "ok": {"@unit": "kg", "#text": "3"},
        "note": {"@title": "line one\nline two\ttabbed", "#text": "a < b & \"c\""},
        "item": ["x", "y"],
        "empty": null
    });
    let expected = json!({"root": {
        "qty": {"@unit": "5", "#text": "3"},
        "ok": {"@unit": "kg", "#text": "3"},
        "note": {"@title": "line one\nline two\ttabbed", "#text": "a < b & \"c\""},
        "item": ["x", "y"],
        "empty": null
    }});
    for pretty in [false, true] {
        let xml = stringify(source.clone(), json!({ "pretty": pretty }))
            .await
            .unwrap();
        assert_eq!(parse_xml(&xml).await, expected, "pretty={pretty}: {xml}");
    }
}

#[tokio::test]
async fn xml_stringify_rejects_characters_xml_cannot_carry() {
    for source in [
        json!({"a": "nul\u{0}"}),
        json!({"a": {"@b": "bell\u{7}"}}),
        json!({"a": {"#text": "\u{ffff}"}}),
    ] {
        let error = stringify(source, json!({})).await.unwrap_err();
        assert!(error.to_string().contains("character"), "{error}");
    }
}

#[tokio::test]
async fn xml_stringify_rejects_array_roots_unless_item_tag_wraps_them() {
    for source in [json!(["a", "b"]), json!([]), json!(["only"])] {
        let error = stringify(source, json!({})).await.unwrap_err();
        assert!(error.to_string().contains("item_tag"), "{error}");
    }
    let xml = stringify(json!([{"id": 1}, {"id": 2}]), json!({"item_tag": "row"}))
        .await
        .unwrap();
    assert_eq!(
        xml,
        format!("{DECLARATION}<root><row><id>1</id></row><row><id>2</id></row></root>")
    );
    assert_eq!(
        parse_xml(&xml).await,
        json!({"root": {"row": [{"id": "1"}, {"id": "2"}]}})
    );
    let empty = stringify(json!([]), json!({"item_tag": "row"}))
        .await
        .unwrap();
    assert_eq!(empty, format!("{DECLARATION}<root/>"));
    assert_eq!(parse_xml(&empty).await, json!({"root": null}));
    let error = stringify(json!([1]), json!({"item_tag": "bad tag"}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("item_tag"), "{error}");
}

#[tokio::test]
async fn xml_parse_rejects_a_second_root_element() {
    for xml in ["<root>a</root><root>b</root>", "<a/><b/>", "<a>1</a><b/>"] {
        let ctx = ctx_with(vec![("xml", json!(xml))]);
        let error = run("xml_parse", json!({"source_key": "xml"}), &ctx)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("more than one root"), "{error}");
    }
    assert_eq!(
        parse_xml("<?xml version=\"1.0\"?><!-- c --><a>1</a><!-- d -->\n").await,
        json!({"a": "1"})
    );
}

// --- csv_parse ---

async fn infer(fields: &[&str]) -> Vec<Value> {
    let ctx = ctx_with(vec![("raw", json!(fields.join(",")))]);
    let config = json!({
        "source_key": "raw", "output_key": "rows", "has_header": false, "infer_types": true
    });
    let output = run("csv_parse", config, &ctx).await.unwrap();
    output["rows"][0].as_array().unwrap().clone()
}

#[tokio::test]
async fn csv_inference_keeps_identifiers_and_non_finite_words_as_text() {
    let fields = [
        "NAN",
        "02134",
        "+15551234567",
        "12345678901234567890",
        "inf",
    ];
    assert_eq!(
        infer(&fields).await,
        json!(fields).as_array().unwrap().clone()
    );

    let fields = [
        "nan",
        "-inf",
        "Infinity",
        "+infinity",
        "007",
        "00",
        "-012",
        "0123.5",
        "+1",
        "+1.5",
        "-0",
        "1234567890123456",
        "-9223372036854775808",
        "0.1234567890123456",
        "1e999",
    ];
    assert_eq!(
        infer(&fields).await,
        json!(fields).as_array().unwrap().clone()
    );
}

#[tokio::test]
async fn csv_inference_still_converts_plain_numbers_and_booleans() {
    let row = infer(&[
        "30",
        "-5",
        "0",
        "1.5",
        "-0.25",
        "0.5",
        ".5",
        "5.",
        "1e3",
        "2.5E-1",
        "123456789012345",
        "0.000123456789012345",
        " 42 ",
        "true",
        "FALSE",
        "abc",
        " padded ",
    ])
    .await;
    assert_eq!(
        row,
        json!([
            30,
            -5,
            0,
            1.5,
            -0.25,
            0.5,
            0.5,
            5.0,
            1000.0,
            0.25,
            123_456_789_012_345_i64,
            0.000_123_456_789_012_345,
            42,
            true,
            false,
            "abc",
            "padded"
        ])
        .as_array()
        .unwrap()
        .clone()
    );
    assert!(row[0].is_i64() && row[7].is_f64() && row[8].is_f64());
}

#[tokio::test]
async fn csv_inference_keeps_empty_fields_and_is_off_by_default() {
    let ctx = ctx_with(vec![("raw", json!("id,zip,n\n1,02134,\n"))]);
    let config = json!({"source_key": "raw", "output_key": "rows", "infer_types": true});
    let output = run("csv_parse", config, &ctx).await.unwrap();
    assert_eq!(output["rows"], json!([{"id": 1, "zip": "02134", "n": ""}]));

    let config = json!({"source_key": "raw", "output_key": "rows"});
    let output = run("csv_parse", config, &ctx).await.unwrap();
    assert_eq!(
        output["rows"],
        json!([{"id": "1", "zip": "02134", "n": ""}])
    );
}

// --- data_filter ---

async fn filter(items: Value, op: &str, value: Value) -> anyhow::Result<Value> {
    let config = json!({
        "source_key": "items", "output_key": "out", "field": "qty", "op": op, "value": value
    });
    let output = run("data_filter", config, &ctx_with(vec![("items", items)])).await?;
    assert_eq!(output["out_count"], output["out"].as_array().unwrap().len());
    Ok(output["out"].clone())
}

#[tokio::test]
async fn data_filter_equality_compares_numbers_by_value() {
    let items = json!([{"qty": 5}, {"qty": 7}]);
    let five = json!(5.0);
    assert!(five.is_f64());
    assert_eq!(
        filter(items.clone(), "eq", five.clone()).await.unwrap(),
        json!([{"qty": 5}])
    );
    assert_eq!(
        filter(items.clone(), "neq", five.clone()).await.unwrap(),
        json!([{"qty": 7}])
    );
    assert_eq!(filter(items.clone(), "gte", five).await.unwrap(), items);

    let floats = json!([{"qty": 5.0}, {"qty": 5.5}, {"qty": {"n": [1.0, 2]}}]);
    assert_eq!(
        filter(floats.clone(), "eq", json!(5)).await.unwrap(),
        json!([{"qty": 5.0}])
    );
    assert_eq!(
        filter(floats, "eq", json!({"n": [1, 2.0]})).await.unwrap(),
        json!([{"qty": {"n": [1.0, 2]}}])
    );
}

#[tokio::test]
async fn data_filter_equality_keeps_types_and_large_integers_apart() {
    let items = json!([
        {"qty": "5"}, {"qty": 5}, {"qty": true}, {"qty": 9007199254740993_i64}, {"qty": null}, {}
    ]);
    assert_eq!(
        filter(items.clone(), "eq", json!("5")).await.unwrap(),
        json!([{"qty": "5"}])
    );
    assert_eq!(
        filter(items.clone(), "eq", json!(true)).await.unwrap(),
        json!([{"qty": true}])
    );
    assert_eq!(
        filter(items.clone(), "eq", json!(9007199254740992_i64))
            .await
            .unwrap(),
        json!([])
    );
    assert_eq!(
        filter(items.clone(), "eq", json!(9007199254740992.0))
            .await
            .unwrap(),
        json!([])
    );
    // An item without the field matches neither `eq` nor `neq`.
    assert_eq!(
        filter(items, "neq", json!(5)).await.unwrap(),
        json!([{"qty": "5"}, {"qty": true}, {"qty": 9007199254740993_i64}, {"qty": null}])
    );
}

#[tokio::test]
async fn data_filter_rejects_an_unknown_operator() {
    let error = filter(json!([{"qty": 5}]), "equals", json!(5))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("unknown op 'equals'"), "{error}");
}

// --- deduplicate ---

#[tokio::test]
async fn deduplicate_treats_equal_numbers_as_duplicates() {
    let items = json!([{"id": 5, "v": 1}, {"id": 5.0, "v": 2}, {"id": "5", "v": 3}]);
    let ctx = ctx_with(vec![
        ("items", items),
        ("plain", json!([1, 1.0, 2, [3.0], [3]])),
    ]);
    let config = json!({"source_key": "items", "output_key": "out", "key": "id"});
    let output = run("deduplicate", config, &ctx).await.unwrap();
    assert_eq!(
        output["out"],
        json!([{"id": 5, "v": 1}, {"id": "5", "v": 3}])
    );
    assert_eq!(output["out_removed"], 1);

    let config = json!({"source_key": "plain", "output_key": "out"});
    let output = run("deduplicate", config, &ctx).await.unwrap();
    assert_eq!(output["out"], json!([1, 2, [3.0]]));
}
