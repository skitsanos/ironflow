//! Link text on one line (IF-237).
//!
//! A link is an inline, so outside literal contexts the converter writes line endings inside
//! link text as a character reference: `<a href="/c"><div>Card</div></a>`
//! becomes `[&#10;&#10;Card&#10;&#10;](/c)`, and a `<br>` inside a link
//! becomes `&#10;`. Anchors around block content are common, and the
//! references are noise to a reader of the Markdown source, so the text of
//! such a link is joined with spaces instead. Literal anchors keep IronFlow's
//! original encoded line endings rather than losing their breaks.

use std::sync::atomic::{AtomicUsize, Ordering};

use htmd::element_handler::{
    Context, ContextKind, Element, ElementHandler, HandlerResult, Handlers,
};

/// Wraps the converter's `<a>` handler.
#[derive(Default)]
pub(super) struct Links {
    /// Anchors being converted. Only the outermost rewrites its text, so a
    /// subtree is walked at most twice however anchors nest inside tables.
    open: AtomicUsize,
}

impl ElementHandler for Links {
    fn handle(&self, handlers: &dyn Handlers, element: &Element) -> Option<HandlerResult> {
        let outermost = self.open.fetch_add(1, Ordering::Relaxed) == 0;
        let result = handlers.fallback(element);
        let result = match result {
            Some(mut result)
                if outermost
                    && element.context.literal
                    && result.markdown_translated
                    && result.content.starts_with('[')
                    && result.content.contains(['\r', '\n']) =>
            {
                // htmd 0.6.1 leaves breaks literal inside preformatted links.
                result.content = encode_line_endings(&result.content);
                Some(result)
            }
            Some(result) if outermost && !element.context.literal && has_line_ending(&result) => {
                Some(on_one_line(handlers, element, result))
            }
            other => other,
        };
        self.open.fetch_sub(1, Ordering::Relaxed);
        result
    }
}

fn has_line_ending(result: &HandlerResult) -> bool {
    result.content.contains("&#10;") || result.content.contains("&#13;")
}

fn encode_line_endings(text: &str) -> String {
    text.replace('\r', "&#13;").replace('\n', "&#10;")
}

/// Rewrite `[text](destination)` so `text` holds no line ending.
///
/// The text is walked again to find where it ends in `link`, which the
/// converter does not report. If the two walks disagree, the converter's
/// own result is kept.
fn on_one_line(handlers: &dyn Handlers, element: &Element, link: HandlerResult) -> HandlerResult {
    let context = Context {
        kind: ContextKind::Inline,
        ..element.context
    };
    let text = handlers.walk_children_content(element.node, context);
    let encoded = encode_line_endings(&text);
    let Some(rest) = link
        .content
        .strip_prefix('[')
        .and_then(|content| content.strip_prefix(encoded.as_str()))
    else {
        return link;
    };
    let joined = text
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    // An anchor around blocks stands where those blocks stood.
    let content = if element.context.is_block() && text.starts_with(['\r', '\n']) {
        format!("\n\n[{joined}{rest}\n\n")
    } else {
        format!("[{joined}{rest}")
    };
    HandlerResult {
        content,
        markdown_translated: link.markdown_translated,
    }
}
