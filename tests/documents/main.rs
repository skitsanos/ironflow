//! Integration tests for extraction, PDF, PPTX, Markdown and image document nodes.
//!
//! One binary per area keeps the number of linked test executables bounded
//! (IF-162). Every module is an unchanged test file under `tests/`; run one
//! with `cargo test --test documents <module>::`.

// Several of these files include the same shared support or fixture module,
// each as its own private copy, exactly as they did as separate crates.
#![allow(clippy::duplicate_mod)]

#[path = "../test_extract_config.rs"]
mod test_extract_config;
#[path = "../test_extract_html_content.rs"]
mod test_extract_html_content;
#[path = "../test_extract_xlsx.rs"]
mod test_extract_xlsx;
#[path = "../test_image_resize_safety.rs"]
mod test_image_resize_safety;
#[path = "../test_image_util_nodes.rs"]
mod test_image_util_nodes;
#[path = "../test_markdown_autolinks.rs"]
mod test_markdown_autolinks;
#[path = "../test_markdown_document_limits.rs"]
mod test_markdown_document_limits;
#[path = "../test_markdown_document_runtime.rs"]
mod test_markdown_document_runtime;
#[path = "../test_markdown_documents.rs"]
mod test_markdown_documents;
#[path = "../test_markdown_docx_alignment.rs"]
mod test_markdown_docx_alignment;
#[path = "../test_markdown_fonts.rs"]
mod test_markdown_fonts;
#[path = "../test_markdown_multilingual.rs"]
mod test_markdown_multilingual;
#[path = "../test_pdf_actual_text.rs"]
mod test_pdf_actual_text;
#[path = "../test_pdf_load_safety.rs"]
mod test_pdf_load_safety;
#[path = "../test_pdf_merge_split_nodes.rs"]
mod test_pdf_merge_split_nodes;
#[path = "../test_pdf_page_graph.rs"]
mod test_pdf_page_graph;
#[path = "../test_pdf_page_rendering.rs"]
mod test_pdf_page_rendering;
#[path = "../test_pdf_split_group_runtime.rs"]
mod test_pdf_split_group_runtime;
#[path = "../test_pdf_split_group_safety.rs"]
mod test_pdf_split_group_safety;
#[path = "../test_pdf_split_groups.rs"]
mod test_pdf_split_groups;
#[path = "../test_pptx_graph_validation.rs"]
mod test_pptx_graph_validation;
#[path = "../test_pptx_presentation.rs"]
mod test_pptx_presentation;
