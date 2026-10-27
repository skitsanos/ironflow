use ironflow::{engine::types::Context, nodes::NodeRegistry};
use quick_xml::{Reader, events::Event};
use serde_json::json;
use std::{collections::BTreeMap, fs, io::Read, path::Path};

#[derive(Default, Debug)]
struct Paragraph {
    text: String,
    style: String,
    rtl: bool,
    direct: String,
    choice: String,
    fallback: String,
}

fn part(path: &Path, name: &str) -> String {
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn attributes(element: &quick_xml::events::BytesStart<'_>) -> BTreeMap<String, String> {
    element
        .attributes()
        .map(|attribute| {
            let attribute = attribute.unwrap();
            (
                attribute.key.0.to_owned(),
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .unwrap()
                    .into_owned(),
            )
        })
        .collect()
}

fn paragraphs(xml: &str) -> Vec<Paragraph> {
    let mut reader = Reader::from_str(xml);
    let mut result = Vec::new();
    let mut current = None::<Paragraph>;
    let mut branch = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event().unwrap() {
            Event::Start(e) => match e.name().as_ref() {
                "w:p" => current = Some(Paragraph::default()),
                "w:t" => in_text = true,
                "mc:Choice" => {
                    assert_eq!(attributes(&e)["Requires"], "w14");
                    branch = "choice".to_owned();
                }
                "mc:Fallback" => branch = "fallback".to_owned(),
                _ => {}
            },
            Event::Empty(e) => {
                if let Some(p) = current.as_mut() {
                    let attrs = attributes(&e);
                    match e.name().as_ref() {
                        "w:pStyle" => p.style = attrs["w:val"].clone(),
                        "w:bidi" => p.rtl = attrs["w:val"] == "1",
                        "w:jc" => {
                            let target = match branch.as_str() {
                                "choice" => &mut p.choice,
                                "fallback" => &mut p.fallback,
                                _ => &mut p.direct,
                            };
                            assert!(target.is_empty(), "duplicate paragraph alignment");
                            *target = attrs["w:val"].clone();
                        }
                        _ => {}
                    }
                }
            }
            Event::Text(e) if in_text => {
                current.as_mut().unwrap().text.push_str(&e.xml10_content());
            }
            Event::End(e) => match e.name().as_ref() {
                "w:p" => result.push(current.take().unwrap()),
                "w:t" => in_text = false,
                "mc:Choice" | "mc:Fallback" => branch.clear(),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    result
}

fn styles(xml: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    let mut styles = BTreeMap::<String, BTreeMap<String, String>>::new();
    let mut id = String::new();
    loop {
        match reader.read_event().unwrap() {
            Event::Start(e) if e.name().as_ref() == "w:style" => {
                let attrs = attributes(&e);
                id = attrs["w:styleId"].clone();
                styles.insert(id.clone(), attrs);
            }
            Event::Empty(e) if !id.is_empty() => {
                if let Some(value) = attributes(&e).get("w:val") {
                    styles
                        .get_mut(&id)
                        .unwrap()
                        .insert(e.name().as_ref().to_owned(), value.clone());
                }
            }
            Event::End(e) if e.name().as_ref() == "w:style" => id.clear(),
            Event::Eof => break,
            _ => {}
        }
    }
    styles
}

#[tokio::test]
async fn docx_has_physical_style_fallbacks_and_logical_rtl_alignment() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("alignment.docx");
    NodeRegistry::with_builtins().get("markdown_to_docx").unwrap().execute(
        &json!({"input":include_str!("../examples/fixtures/markdown-multilingual.md"),"output_path":path}),
        &Context::new()).await.unwrap();
    let xml = part(&path, "word/document.xml");
    assert!(xml.contains("xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\""));
    assert!(xml.contains("mc:Ignorable=\"w14\""));
    let definitions = styles(&part(&path, "word/styles.xml"));
    assert_eq!(definitions["Normal"]["w:default"], "1");
    assert!(!definitions["Normal"].contains_key("w:b"));
    let mut rtl_count = 0;
    let mut ltr_count = 0;
    for p in paragraphs(&xml).iter().filter(|p| !p.text.is_empty()) {
        let style = &definitions[&p.style];
        if p.rtl {
            rtl_count += 1;
            assert_eq!(p.choice, "start", "{p:?}");
            assert_eq!(p.fallback, "right", "{p:?}");
            assert_eq!(style["w:jc"], "right", "{p:?}");
            assert_eq!(style["w:bidi"], "1");
            assert!(p.direct.is_empty());
        } else {
            ltr_count += 1;
            assert_eq!(p.direct, "left", "{p:?}");
            assert_eq!(style["w:jc"], "left", "{p:?}");
            assert!(p.choice.is_empty() && p.fallback.is_empty());
        }
    }
    assert!(rtl_count >= 10, "headings, body, list and table paragraphs");
    assert!(ltr_count >= 10);
    for base in [
        "Normal", "Heading1", "Heading2", "Heading3", "Heading4", "Heading5", "Heading6",
    ] {
        assert_eq!(definitions[&format!("{base}Rtl")]["w:basedOn"], base);
    }
}

#[tokio::test]
async fn forced_direction_retains_left_aligned_code_and_rtl_fallbacks() {
    let temp = tempfile::tempdir().unwrap();
    for direction in ["ltr", "rtl"] {
        let path = temp.path().join(format!("{direction}.docx"));
        NodeRegistry::with_builtins().get("markdown_to_docx").unwrap().execute(
            &json!({"input":"# English heading\n\nالعربية text 123\n\n```text\ncode العربية\n```", "direction":direction,"output_path":path}),
            &Context::new()).await.unwrap();
        let ps = paragraphs(&part(&path, "word/document.xml"));
        assert_eq!(ps.len(), 3);
        for p in &ps[..2] {
            assert_eq!(p.rtl, direction == "rtl");
            if direction == "rtl" {
                assert_eq!(p.fallback, "right");
            } else {
                assert_eq!(p.direct, "left");
            }
        }
        assert!(!ps[2].rtl);
        assert_eq!(ps[2].style, "Normal");
        assert_eq!(ps[2].direct, "left");
    }
}

#[tokio::test]
async fn docx_table_grid_and_cells_fill_the_available_width() {
    let temp = tempfile::tempdir().unwrap();
    for (page, margin, columns) in [("a4", 54, 2), ("letter", 144, 3)] {
        let path = temp.path().join(format!("{page}.docx"));
        let row = format!("|{}|", vec![" cell "; columns].join("|"));
        let separator = format!("|{}|", vec![" --- "; columns].join("|"));
        NodeRegistry::with_builtins().get("markdown_to_docx").unwrap().execute(
            &json!({"input":format!("{row}\n{separator}\n{row}"),"output_path":path,"page_size":page,"margin":margin}),
            &Context::new()).await.unwrap();
        let xml = part(&path, "word/document.xml");
        let mut reader = Reader::from_str(&xml);
        let mut grid = Vec::<usize>::new();
        let mut cells = Vec::<usize>::new();
        let mut width = 0;
        let mut page_width = 0;
        loop {
            match reader.read_event().unwrap() {
                Event::Empty(e) => {
                    let attrs = attributes(&e);
                    match e.name().as_ref() {
                        "w:gridCol" => grid.push(attrs["w:w"].parse().unwrap()),
                        "w:tcW" => {
                            assert_eq!(attrs["w:type"], "dxa");
                            cells.push(attrs["w:w"].parse().unwrap());
                        }
                        "w:tblW" => {
                            assert_eq!(attrs["w:type"], "dxa");
                            width = attrs["w:w"].parse().unwrap();
                        }
                        "w:tblLayout" => assert_eq!(attrs["w:type"], "fixed"),
                        "w:pgSz" => page_width = attrs["w:w"].parse::<usize>().unwrap(),
                        _ => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        assert_eq!(grid.len(), columns);
        assert_eq!(grid.iter().sum::<usize>(), width);
        assert_eq!(width, page_width - 2 * margin * 20);
        assert!(grid.iter().all(|w| *w > 100));
        assert_eq!(cells, grid.repeat(2));
    }
}
