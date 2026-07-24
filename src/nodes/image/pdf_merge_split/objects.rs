use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};

pub(crate) fn collect_objects_recursive(
    document: &Document,
    object_id: ObjectId,
    collected: &mut BTreeMap<ObjectId, Object>,
) {
    if collected.contains_key(&object_id) {
        return;
    }
    if let Ok(object) = document.get_object(object_id) {
        collected.insert(object_id, object.clone());
        for reference in extract_references(object) {
            collect_objects_recursive(document, reference, collected);
        }
    }
}

pub(crate) fn extract_references(object: &Object) -> Vec<ObjectId> {
    let mut references = Vec::new();
    match object {
        Object::Reference(id) => references.push(*id),
        Object::Array(values) => {
            for value in values {
                references.extend(extract_references(value));
            }
        }
        Object::Dictionary(dictionary) => {
            for (_, value) in dictionary.iter() {
                references.extend(extract_references(value));
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter() {
                references.extend(extract_references(value));
            }
        }
        _ => {}
    }
    references
}

pub(crate) fn remap_references(object: &mut Object, map: &BTreeMap<ObjectId, ObjectId>) {
    match object {
        Object::Reference(id) => {
            if let Some(new_id) = map.get(id) {
                *id = *new_id;
            }
        }
        Object::Array(values) => {
            for value in values {
                remap_references(value, map);
            }
        }
        Object::Dictionary(dictionary) => {
            for (_, value) in dictionary.iter_mut() {
                remap_references(value, map);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter_mut() {
                remap_references(value, map);
            }
        }
        _ => {}
    }
}
