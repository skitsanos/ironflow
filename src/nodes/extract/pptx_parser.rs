mod comments;
mod content_types;
mod metadata;
mod notes;
mod package;
mod relationships;
mod slide;

pub(super) use comments::extract_pptx_comments;
pub(super) use metadata::extract_pptx_metadata;
pub(super) use package::extract_pptx_slides;

pub(super) struct PptxSlide {
    pub(super) slide_index: u32,
    pub(super) title: Option<String>,
    pub(super) elements: Vec<PptxElement>,
    pub(super) speaker_notes: Option<String>,
    pub(super) comments: Vec<PptxComment>,
}

pub(super) enum PptxElement {
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
        artifact: Option<crate::artifacts::ArtifactRef>,
    },
}

#[derive(Default)]
pub(super) struct PptxTextPara {
    pub(super) text: String,
    pub(super) list_level: Option<u32>,
}

#[derive(serde::Serialize, Default)]
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
