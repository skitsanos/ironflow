use std::collections::BTreeMap;

use lopdf::{Object, ObjectId};

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
            // An omitted page/tree reference cannot retain its old, possibly reused ID.
            *object = map
                .get(id)
                .copied()
                .map(Object::Reference)
                .unwrap_or(Object::Null);
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
