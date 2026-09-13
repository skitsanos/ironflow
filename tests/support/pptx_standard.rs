use std::path::Path;

#[allow(dead_code)]
#[path = "pptx.rs"]
mod fixture;

/// Author the manifest/links for conventional synthetic decks. Negative graph
/// fixtures use the raw writer instead, so missing manifests are never repaired.
pub fn complete(path: &Path) {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let archive = zip::ZipArchive::new(file).unwrap();
    let names: std::collections::HashSet<String> =
        archive.file_names().map(str::to_owned).collect();
    if names.contains("ppt/presentation.xml") {
        return;
    }
    let mut slides = names
        .iter()
        .filter_map(|name| {
            name.strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?
                .parse::<u32>()
                .ok()
        })
        .collect::<Vec<_>>();
    if slides.is_empty() {
        return;
    }
    slides.sort_unstable();
    let mut zip = zip::ZipWriter::new_append(archive.into_inner()).unwrap();
    let ids = slides
        .iter()
        .map(|n| (n.to_string(), format!("s{n}")))
        .collect::<Vec<_>>();
    let manifest = fixture::presentation(
        &ids.iter()
            .map(|(id, rid)| (id.as_str(), rid.as_str()))
            .collect::<Vec<_>>(),
    );
    fixture::add(&mut zip, "ppt/presentation.xml", manifest.as_bytes());
    let mut entries = slides
        .iter()
        .map(|n| (format!("s{n}"), "slide", format!("slides/slide{n}.xml")))
        .collect::<Vec<_>>();
    if names.contains("ppt/commentAuthors.xml") {
        entries.push((
            "authors".into(),
            "commentAuthors",
            "commentAuthors.xml".into(),
        ));
    }
    let rels = fixture::relationships(
        &entries
            .iter()
            .map(|(id, kind, target)| (id.as_str(), *kind, target.as_str()))
            .collect::<Vec<_>>(),
    );
    fixture::add(&mut zip, "ppt/_rels/presentation.xml.rels", rels.as_bytes());
    for index in slides {
        let rels_name = format!("ppt/slides/_rels/slide{index}.xml.rels");
        if names.contains(&rels_name) {
            continue;
        }
        let mut entries = Vec::new();
        for (id, kind, part) in [
            (
                "notes",
                "notesSlide",
                format!("notesSlides/notesSlide{index}.xml"),
            ),
            (
                "comments",
                "comments",
                format!("comments/comment{index}.xml"),
            ),
        ] {
            if names.contains(&format!("ppt/{part}")) {
                entries.push((id, kind, format!("../{part}")));
            }
        }
        if !entries.is_empty() {
            let rels = fixture::relationships(
                &entries
                    .iter()
                    .map(|(id, kind, target)| (*id, *kind, target.as_str()))
                    .collect::<Vec<_>>(),
            );
            fixture::add(&mut zip, &rels_name, rels.as_bytes());
        }
    }
    zip.finish().unwrap();
}
