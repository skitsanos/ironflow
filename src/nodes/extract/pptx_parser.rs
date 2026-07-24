mod comments;
mod notes;
mod relationships;
mod slide;

pub(super) use comments::extract_pptx_comments;
pub(super) use notes::parse_pptx_notes;
pub(super) use relationships::{normalize_pptx_path, parse_pptx_rels, read_pptx_media};
pub(super) use slide::parse_pptx_slide;

#[derive(Clone)]
pub(super) struct PptxSlide {
    pub(super) slide_index: u32,
    pub(super) title: Option<String>,
    pub(super) elements: Vec<PptxElement>,
    pub(super) speaker_notes: Option<String>,
    pub(super) comments: Vec<PptxComment>,
}

#[derive(Clone)]
pub(super) enum PptxElement {
    /// Top-level text block (could be the title placeholder or a content placeholder).
    /// `paragraphs` is a list of text paragraphs; each may be a bullet point with a level.
    TextBlock {
        placeholder: Option<String>,
        paragraphs: Vec<PptxTextPara>,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
    Image {
        alt_text: Option<String>,
        embed_id: Option<String>,
        embedded_path: Option<String>,
        media_b64: Option<String>,
        mime_type: Option<String>,
    },
}

#[derive(Clone, Default)]
pub(super) struct PptxTextPara {
    pub(super) text: String,
    pub(super) list_level: Option<u32>,
}

#[derive(Clone, serde::Serialize, Default)]
pub(super) struct PptxComment {
    pub(super) slide_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) idx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) author_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) date: Option<String>,
    pub(super) text: String,
}
