use anyhow::{Result, ensure};
use std::sync::Arc;

pub(super) const MAX_INPUT: usize = 256 * 1024;
pub(super) const MAX_OUTPUT: usize = 32 * 1024 * 1024;
pub(super) const MAX_NODES: usize = 20_000;
pub(super) const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, Default)]
pub(super) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: Option<Arc<str>>,
}

#[derive(Clone, Debug)]
pub(super) struct Run {
    pub text: String,
    pub style: Style,
}

#[derive(Debug)]
pub(super) enum Block {
    Paragraph {
        runs: Vec<Run>,
        depth: usize,
        label: String,
    },
    Heading(u8, Vec<Run>),
    Code(String),
    Table(Vec<Vec<Vec<Run>>>),
    Image {
        key: String,
        alt: String,
    },
    Rule,
}

pub(super) fn safe_link(target: &str) -> Result<()> {
    ensure!(target.len() <= 2048, "link exceeds 2048 URL bytes");
    let url = url::Url::parse(target)?;
    ensure!(
        matches!(url.scheme(), "https" | "http" | "mailto"),
        "only http, https and mailto links are supported"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "links must not contain credentials"
    );
    Ok(())
}

pub(super) fn valid_text(text: &str) -> Result<()> {
    ensure!(
        text.chars()
            .all(|c| matches!(c, '\n' | '\r' | '\t')
                || c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}'),
        "Markdown contains characters not permitted in document XML"
    );
    Ok(())
}
