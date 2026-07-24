mod csv;
mod data_collection;
mod data_fields;
mod data_filter;
mod data_mapping;
mod json;
mod xml;
mod yaml;

pub use csv::{CsvParseNode, CsvStringifyNode};
pub use data_collection::{BatchNode, DeduplicateNode};
pub use data_fields::{RenameFieldsNode, SelectFieldsNode};
pub use data_filter::DataFilterNode;
pub use data_mapping::DataTransformNode;
pub use json::{JsonExtractPathNode, JsonParseNode, JsonStringifyNode};
pub use xml::{XmlParseNode, XmlStringifyNode};
pub use yaml::{YamlParseNode, YamlStringifyNode};

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(JsonParseNode));
    registry.register(Arc::new(JsonStringifyNode));
    registry.register(Arc::new(JsonExtractPathNode));
    registry.register(Arc::new(CsvParseNode));
    registry.register(Arc::new(CsvStringifyNode));
    registry.register(Arc::new(SelectFieldsNode));
    registry.register(Arc::new(RenameFieldsNode));
    registry.register(Arc::new(DataFilterNode));
    registry.register(Arc::new(DataTransformNode));
    registry.register(Arc::new(BatchNode));
    registry.register(Arc::new(DeduplicateNode));
    registry.register(Arc::new(XmlParseNode));
    registry.register(Arc::new(XmlStringifyNode));
    registry.register(Arc::new(YamlParseNode));
    registry.register(Arc::new(YamlStringifyNode));
}
