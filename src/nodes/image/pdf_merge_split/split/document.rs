use std::collections::BTreeMap;

use anyhow::Result;
use lopdf::{Document, Object, ObjectId, dictionary};

use crate::util::execution::ExecutionControl;

use super::super::{page_graph::collect_page_graph, remap_references};

pub(super) fn selected_document(
    source: &Document,
    page_ids: &[ObjectId],
    maximum_objects: Option<u64>,
    execution: &ExecutionControl,
) -> Result<Document> {
    let count = u32::try_from(page_ids.len())
        .map_err(|_| anyhow::anyhow!("pdf_split: page count exceeds PDF u32 range"))?;
    let mut document = Document::new();
    let pages_id = document.new_object_id();
    // Account for the new Pages root and Catalog before cloning the page graph.
    let limit =
        maximum_objects.map(|maximum| (maximum.saturating_sub(2), "IRONFLOW_MAX_PDF_OBJECTS"));
    let objects = collect_page_graph(source, page_ids, limit, "pdf_split", execution)?;
    let mut remap = BTreeMap::new();
    for (old_id, object) in objects {
        execution.checkpoint()?;
        remap.insert(old_id, document.add_object(object));
    }
    for new_id in remap.values() {
        execution.checkpoint()?;
        remap_references(document.get_object_mut(*new_id)?, &remap);
    }
    let mut kids = Vec::new();
    kids.try_reserve_exact(page_ids.len())?;
    for page_id in page_ids {
        execution.checkpoint()?;
        let new_id = remap[page_id];
        document.get_dictionary_mut(new_id)?.set("Parent", pages_id);
        kids.push(Object::Reference(new_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    document.max_id = document.objects.keys().map(|id| id.0).max().unwrap_or(0);
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::execution::run_tracked_blocking_step;

    #[tokio::test]
    async fn group_graph_budget_includes_new_tree_and_catalog() {
        run_tracked_blocking_step(|execution| {
            let mut source = Document::new();
            let root = source.new_object_id();
            let content = source.add_object(lopdf::Stream::new(dictionary! {}, b"BT ET".to_vec()));
            let page = source.add_object(dictionary! {
                "Type" => "Page", "Parent" => root, "Contents" => content,
                "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            });
            source.objects.insert(
                root,
                Object::Dictionary(dictionary! {
                    "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1,
                }),
            );
            let error = selected_document(&source, &[page], Some(3), &execution).unwrap_err();
            assert!(error.to_string().contains("IRONFLOW_MAX_PDF_OBJECTS"));
            let document = selected_document(&source, &[page], Some(4), &execution)?;
            assert_eq!(document.objects.len(), 4);
            Ok(())
        })
        .await
        .unwrap();
    }
}
