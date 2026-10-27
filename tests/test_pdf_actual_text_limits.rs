#[path = "pdf_actual_text/fixture.rs"]
mod fixture;

use fixture::{document, extract, marked, operation, show};
use ironflow::util::execution::with_execution_deadline;
use lopdf::{Object, dictionary, text_string};
use std::{ffi::OsString, time::Duration};
use tokio::time::Instant;

struct Environment(Vec<(&'static str, Option<OsString>)>);
impl Environment {
    fn capture(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }
    fn set(key: &str, value: &str) {
        // This binary contains one sequential environment-mutating test.
        unsafe {
            std::env::set_var(key, value);
        }
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

#[tokio::test]
async fn replacement_text_obeys_output_work_stream_and_deadline_limits() {
    const OUTPUT: &str = "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES";
    const ITEMS: &str = "IRONFLOW_MAX_EXTRACT_ITEMS";
    const STREAM: &str = "IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES";
    let _environment = Environment::capture(&[OUTPUT, ITEMS, STREAM]);
    Environment::set(OUTPUT, "256");
    Environment::set(ITEMS, "250000");
    Environment::set(STREAM, "67108864");

    // An indirect text string is not limited by the page's small content stream.
    let (mut doc, _, resources) = document(vec![
        operation(
            "BDC",
            vec![Object::Name(b"Span".to_vec()), Object::Name(b"P".to_vec())],
        ),
        show("wrong"),
        operation("EMC", vec![]),
    ]);
    let value = doc.add_object(text_string(&"א".repeat(2048)));
    let prop = doc.add_object(dictionary! {"ActualText" => value});
    doc.get_dictionary_mut(resources)
        .unwrap()
        .set("Properties", dictionary! {"P"=>prop});
    let error = extract(&mut doc).await.unwrap_err().to_string();
    assert!(error.contains(OUTPUT), "{error}");
    assert!(error.contains("page 1"), "{error}");

    Environment::set(OUTPUT, "600");
    let (mut doc, page, resources) = document(vec![
        operation(
            "BDC",
            vec![Object::Name(b"Span".to_vec()), Object::Name(b"P".to_vec())],
        ),
        operation("EMC", vec![]),
    ]);
    // Both small page streams fit the shrinking budget; their logical text does not.
    let replacement = doc.add_object(dictionary! {"ActualText"=>text_string(&"a".repeat(350))});
    doc.get_dictionary_mut(resources)
        .unwrap()
        .set("Properties", dictionary! {"P"=>replacement});
    let second = doc.add_object(doc.get_dictionary(page).unwrap().clone());
    let parent = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    let parent = doc.get_dictionary_mut(parent).unwrap();
    parent.set(
        "Kids",
        vec![Object::Reference(page), Object::Reference(second)],
    );
    parent.set("Count", 2);
    let error = extract(&mut doc).await.unwrap_err().to_string();
    assert!(
        error.contains(OUTPUT) && error.contains("page 2"),
        "{error}"
    );

    Environment::set(OUTPUT, "1048576");
    Environment::set(ITEMS, "4");
    let (mut doc, _, _) = document(vec![
        marked(text_string("logical")),
        show("wrong"),
        operation("EMC", vec![]),
    ]);
    let error = extract(&mut doc).await.unwrap_err().to_string();
    assert!(error.contains(ITEMS), "{error}");

    Environment::set(ITEMS, "250000");
    Environment::set(STREAM, "16");
    let error = extract(&mut doc).await.unwrap_err().to_string();
    // Shared loading can reject the xref stream before page extraction starts.
    assert!(error.contains(STREAM), "{error}");
    assert!(error.contains("16"), "{error}");

    Environment::set(STREAM, "67108864");
    let result = with_execution_deadline(
        Some(Instant::now() - Duration::from_secs(1)),
        extract(&mut doc),
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("deadline"));
}
