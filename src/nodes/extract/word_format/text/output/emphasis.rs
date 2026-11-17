//! CommonMark emphasis for a paragraph's runs.
//!
//! Word splits text into runs freely (spell-check, revisions, Office for the
//! web edits), so wrapping each run on its own produced `*a**b** *`. Runs
//! are instead treated as one stream: adjacent runs with the same emphasis
//! share one marker pair, leading and trailing whitespace stays outside the
//! markers, whitespace-only runs never open a marker, and a change of
//! emphasis closes only the markers that end there (`**a*b***`).

use anyhow::Result;

use super::TextOutput;
use crate::nodes::extract::docx_parser::{DocxRun, Flags};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Bold,
    Italic,
    Strike,
}

impl Mark {
    /// Opening order, outermost first; closing runs in reverse.
    const ORDER: [Mark; 3] = [Mark::Bold, Mark::Italic, Mark::Strike];

    fn marker(self) -> &'static str {
        match self {
            Mark::Bold => "**",
            Mark::Italic => "*",
            Mark::Strike => "~~",
        }
    }
}

type Wanted = [bool; 3];

fn wanted(run: &DocxRun, inherited: Flags) -> Wanted {
    let [no_bold, no_italic, no_strike] = inherited;
    [
        run.bold && !no_bold,
        run.italic && !no_italic,
        run.strikethrough && !no_strike,
    ]
}

fn is_wanted(mark: Mark, wanted: Wanted) -> bool {
    wanted[Mark::ORDER.iter().position(|m| *m == mark).unwrap_or(0)]
}

struct Spans<'r> {
    open: Vec<Mark>,
    /// Whitespace seen since the last visible text, not yet placed: it goes
    /// inside the markers if the emphasis continues, outside if it changes.
    pending: Vec<&'r str>,
    escape_pipe: bool,
}

impl TextOutput<'_, '_> {
    /// Append runs with their emphasis markers. `inherited` names the bold,
    /// italic and strikethrough that the surrounding construct already
    /// expresses (a heading's own style) and that get no marker.
    pub(in crate::nodes::extract::word_format::text) fn append_markdown_runs(
        &mut self,
        runs: &[DocxRun],
        escape_pipe: bool,
        inherited: Flags,
    ) -> Result<()> {
        let mut spans = Spans {
            open: Vec::with_capacity(Mark::ORDER.len()),
            pending: Vec::new(),
            escape_pipe,
        };
        for run in runs {
            self.checkpoint()?;
            let text = run.text.as_str();
            let core = text.trim_matches(char::is_whitespace);
            if core.is_empty() {
                if !text.is_empty() {
                    spans.pending.push(text);
                }
                continue;
            }
            let start = core.as_ptr() as usize - text.as_ptr() as usize;
            if start > 0 {
                spans.pending.push(&text[..start]);
            }
            self.switch_emphasis(&mut spans, wanted(run, inherited))?;
            self.append_run_text(core, escape_pipe)?;
            let trail = &text[start + core.len()..];
            if !trail.is_empty() {
                spans.pending.push(trail);
            }
        }
        self.switch_emphasis(&mut spans, [false; 3])
    }

    fn switch_emphasis<'r>(&mut self, spans: &mut Spans<'r>, wanted: Wanted) -> Result<()> {
        // Close from the innermost open marker down to the deepest one that
        // must end, so the markers that stay remain properly nested.
        if let Some(deepest) = spans.open.iter().position(|mark| !is_wanted(*mark, wanted)) {
            while spans.open.len() > deepest {
                self.checkpoint()?;
                if let Some(mark) = spans.open.pop() {
                    self.append(mark.marker(), "DOCX markdown run wrappers")?;
                }
            }
        }
        for text in spans.pending.drain(..) {
            self.checkpoint()?;
            self.append_run_text(text, spans.escape_pipe)?;
        }
        for mark in Mark::ORDER {
            if is_wanted(mark, wanted) && !spans.open.contains(&mark) {
                self.append(mark.marker(), "DOCX markdown run wrappers")?;
                spans.open.push(mark);
            }
        }
        Ok(())
    }

    fn append_run_text(&mut self, text: &str, escape_pipe: bool) -> Result<()> {
        if escape_pipe {
            self.append_escaped_cell_text(text)
        } else {
            self.append(text, "DOCX markdown text copies")
        }
    }
}
