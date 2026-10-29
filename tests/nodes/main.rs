//! Integration tests for file, encoding, archive, database, chunking and utility nodes.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test nodes <module>::`.

#[path = "../test_ai_chunk_nodes.rs"]
mod test_ai_chunk_nodes;
#[path = "../test_base64_write_safety.rs"]
mod test_base64_write_safety;
#[path = "../test_copy_move_file_safety.rs"]
mod test_copy_move_file_safety;
#[path = "../test_date_node.rs"]
mod test_date_node;
#[path = "../test_db_nodes.rs"]
mod test_db_nodes;
#[path = "../test_db_value_types.rs"]
mod test_db_value_types;
#[path = "../test_encoding_nodes.rs"]
mod test_encoding_nodes;
#[path = "../test_file_alias_roots.rs"]
mod test_file_alias_roots;
#[path = "../test_file_nodes.rs"]
mod test_file_nodes;
#[path = "../test_html_sanitize_node.rs"]
mod test_html_sanitize_node;
#[path = "../test_nodes.rs"]
mod test_nodes;
#[path = "../test_semantic_chunk_boundaries.rs"]
mod test_semantic_chunk_boundaries;
#[path = "../test_xml_text_fidelity.rs"]
mod test_xml_text_fidelity;
#[path = "../test_xml_yaml_nodes.rs"]
mod test_xml_yaml_nodes;
#[path = "../test_yaml_nodes.rs"]
mod test_yaml_nodes;
#[path = "../test_zip_cancellation.rs"]
mod test_zip_cancellation;
#[path = "../test_zip_limits.rs"]
mod test_zip_limits;
#[path = "../test_zip_metadata.rs"]
mod test_zip_metadata;
#[path = "../test_zip_security.rs"]
mod test_zip_security;
