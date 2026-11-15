//! Comment anchors and `mc:AlternateContent` (IF-222 b). A text box is
//! written twice, as an `mc:Choice` drawing and an `mc:Fallback` VML copy;
//! comment ranges and anchored text inside the fallback repeat the choice
//! and must not be collected again. The parts are hand-built.

use serde_json::json;

#[path = "support/docx.rs"]
mod fixture;
use fixture::{document, extract, part};

fn run(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

/// A run holding a DrawingML text box and its VML fallback, both with `content`.
fn boxed_run(content: &str) -> String {
    format!(
        "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:anchor><a:graphic>\
         <a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
         <wps:wsp><wps:txbx><w:txbxContent>{content}</w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp>\
         </a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice>\
         <mc:Fallback><w:pict><v:shape><v:textbox><w:txbxContent>{content}</w:txbxContent>\
         </v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    )
}

fn comments(ids: &[&str]) -> String {
    let comments = ids
        .iter()
        .map(|id| {
            format!(
                "<w:comment w:id=\"{id}\" w:author=\"Reviewer\"><w:p><w:r><w:t>Note {id}</w:t></w:r>\
                 </w:p></w:comment>"
            )
        })
        .collect::<String>();
    part("w:comments", &comments)
}

async fn anchors(body: &str, ids: &[&str]) -> Vec<(String, String)> {
    let output = extract(
        &[
            ("word/document.xml", document(body)),
            ("word/comments.xml", comments(ids)),
        ],
        json!({"comments_key": "comments"}),
    )
    .await;
    output["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|comment| {
            (
                comment["id"].as_str().unwrap().to_string(),
                comment["anchored_text"]
                    .as_str()
                    .unwrap_or("<none>")
                    .to_string(),
            )
        })
        .collect()
}

fn pair(id: &str, text: &str) -> (String, String) {
    (id.to_string(), text.to_string())
}

#[tokio::test]
async fn a_range_around_a_text_box_collects_the_box_text_once() {
    let body = format!(
        "<w:p><w:commentRangeStart w:id=\"0\"/>{}{}{}<w:commentRangeEnd w:id=\"0\"/></w:p>",
        run("before "),
        boxed_run(&format!("<w:p>{}</w:p>", run("INSIDE-BOX"))),
        run("after")
    );
    assert_eq!(
        anchors(&body, &["0"]).await,
        [pair("0", "before INSIDE-BOX\nafter")]
    );
}

#[tokio::test]
async fn a_range_inside_a_text_box_is_opened_and_collected_once() {
    let content = format!(
        "<w:p>{}<w:commentRangeStart w:id=\"7\"/>{}<w:commentRangeEnd w:id=\"7\"/>{}</w:p>",
        run("lead "),
        run("ANCHORED"),
        run(" tail")
    );
    let body = format!(
        "<w:p>{}</w:p><w:p>{}</w:p>",
        boxed_run(&content),
        run("Tail")
    );
    assert_eq!(anchors(&body, &["7"]).await, [pair("7", "ANCHORED")]);
}

/// A fallback that differs from its choice is still not live content: a range
/// that only the fallback declares anchors nothing and cannot fail the walk.
#[tokio::test]
async fn ranges_declared_only_in_a_fallback_are_ignored() {
    let body = format!(
        "<w:p>{}<mc:AlternateContent><mc:Choice Requires=\"wps\">{}</mc:Choice><mc:Fallback>\
         <w:commentRangeEnd w:id=\"1\"/><w:commentRangeStart w:id=\"2\"/>{}</mc:Fallback>\
         </mc:AlternateContent></w:p>",
        run("Host"),
        run(" choice"),
        run(" fallback")
    );
    assert_eq!(
        anchors(&body, &["1", "2"]).await,
        [pair("1", "<none>"), pair("2", "<none>")]
    );
}
