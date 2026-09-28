use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use anyhow::{Result, anyhow};
use html2md::{Handle, NodeData, StructuredPrinter, TagHandler, TagHandlerFactory};

use crate::nodes::extract::resource::Budget;
use crate::util::execution::ExecutionControl;

pub(super) fn extract(
    html: &str,
    format: &str,
    execution: &ExecutionControl,
    budget: &Budget<'_>,
) -> Result<String> {
    let root = Rc::new(RefCell::new(None));
    let filter = BodyFilter {
        root: root.clone(),
        execution: execution.clone(),
        skip: format == "text",
    };
    let custom: HashMap<String, Box<dyn TagHandlerFactory>> = HashMap::from([(
        "html".to_string(),
        Box::new(filter) as Box<dyn TagHandlerFactory>,
    )]);
    budget.checkpoint()?;
    let markdown = html2md::parse_html_custom(html, &custom);
    budget.checkpoint()?;
    let root = root
        .borrow_mut()
        .take()
        .ok_or_else(|| anyhow!("extract_html: HTML parser did not produce a document"))??;
    if format == "text" {
        plain_text(root, budget)
    } else {
        Ok(markdown)
    }
}

#[derive(Clone)]
struct BodyFilter {
    root: Rc<RefCell<Option<Result<Handle>>>>,
    execution: ExecutionControl,
    skip: bool,
}

impl TagHandlerFactory for BodyFilter {
    fn instantiate(&self) -> Box<dyn TagHandler> {
        Box::new(self.clone())
    }
}

impl TagHandler for BodyFilter {
    fn handle(&mut self, tag: &Handle, _printer: &mut StructuredPrinter) {
        // Filter the whole parsed tree before conversion: table handlers start
        // their own walks, and preformatted descendants bypass custom handlers.
        // RcDom's document destructor clears descendant child lists even when
        // those nodes still have handles. Retain the owner, not just <html>,
        // until plain-text traversal finishes after parse_html_custom returns.
        let parent = tag.parent.take();
        let document = parent.as_ref().and_then(std::rc::Weak::upgrade);
        tag.parent.set(parent);
        let result = prune(tag, &self.execution).and_then(|()| {
            document.ok_or_else(|| anyhow!("extract_html: HTML document owner is missing"))
        });
        self.skip |= result.is_err();
        *self.root.borrow_mut() = Some(result);
    }

    fn after_handle(&mut self, _printer: &mut StructuredPrinter) {}

    fn skip_descendants(&self) -> bool {
        self.skip
    }
}

fn prune(root: &Handle, execution: &ExecutionControl) -> Result<()> {
    let mut pending = vec![root.clone()];
    while let Some(node) = pending.pop() {
        execution.checkpoint()?;
        let mut children = node.children.borrow_mut();
        children.retain(|child| {
            !matches!(&child.data, NodeData::Element { name, .. }
                if matches!(name.local.as_ref(),
                    "head" | "title" | "script" | "style" | "noscript" | "template"))
        });
        pending.extend(children.iter().cloned());
    }
    Ok(())
}

/// How an element's text is separated from surrounding plain-text output.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Separator {
    /// Paragraph-level block: a blank line on both entry and exit.
    Paragraph,
    /// Line-level block such as a list item, table row, or layout `div`.
    Line,
    /// `<br>`: one newline that never widens an existing blank line.
    Break,
    None,
}

fn separator(tag: &str) -> Separator {
    match tag {
        "address" | "article" | "aside" | "blockquote" | "dl" | "fieldset" | "figcaption"
        | "figure" | "footer" | "form" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "header"
        | "hr" | "main" | "nav" | "ol" | "p" | "pre" | "section" | "table" | "ul" => {
            Separator::Paragraph
        }
        "dd" | "div" | "dt" | "li" | "tr" => Separator::Line,
        "br" => Separator::Break,
        _ => Separator::None,
    }
}

/// Make non-empty output end with `newlines` line feeds, adding only the missing ones.
fn separate(output: &mut String, newlines: usize) {
    if output.is_empty() {
        return;
    }
    let trailing = output
        .chars()
        .rev()
        .take(newlines)
        .take_while(|ch| *ch == '\n')
        .count();
    for _ in trailing..newlines {
        output.push('\n');
    }
}

fn plain_text(root: Handle, budget: &Budget<'_>) -> Result<String> {
    let mut output = String::new();
    let mut pending_space = false;
    // `root` owns the document for the whole walk: RcDom's node destructor
    // clears descendant child lists even while handles to them remain, and
    // the root frame is popped first.
    let mut pending = vec![(root.clone(), false, false)];
    while let Some((node, exiting, in_pre)) = pending.pop() {
        budget.checkpoint()?;
        let tag = match &node.data {
            NodeData::Element { name, .. } => name.local.as_ref(),
            _ => "",
        };
        let separator = separator(tag);
        let cell = matches!(tag, "td" | "th");
        match separator {
            Separator::Paragraph => separate(&mut output, 2),
            Separator::Line => separate(&mut output, 1),
            Separator::Break if !output.is_empty() && !output.ends_with("\n\n") => {
                output.push('\n');
            }
            Separator::Break | Separator::None => {}
        }
        if separator != Separator::None {
            pending_space = false;
        } else if exiting && cell {
            pending_space = true;
        }
        if exiting {
            continue;
        }
        if let NodeData::Text { contents } = &node.data {
            let contents = contents.borrow();
            for (index, ch) in contents.chars().enumerate() {
                if index % 4096 == 0 {
                    budget.admit_output(output.len() as u64, "HTML content")?;
                }
                if !in_pre && matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c') {
                    pending_space = true;
                } else {
                    if pending_space && !output.is_empty() && !output.ends_with('\n') {
                        output.push(' ');
                    }
                    pending_space = false;
                    output.push(ch);
                }
            }
        } else if tag == "img"
            && let Some(alt) = html2md::common::get_tag_attr(&node, "alt")
        {
            budget.admit_output((output.len() + alt.len()) as u64, "HTML content")?;
            if pending_space && !output.is_empty() && !output.ends_with('\n') {
                output.push(' ');
            }
            pending_space = false;
            output.push_str(&alt);
        }
        // Only frames whose exit changes spacing are revisited; text, inline
        // markup, `<br>`, and `<img>` need no second pass.
        if matches!(separator, Separator::Paragraph | Separator::Line) || cell {
            pending.push((node.clone(), true, in_pre));
        }
        pending.extend(
            node.children
                .borrow()
                .iter()
                .rev()
                .map(|child| (child.clone(), false, in_pre || tag == "pre")),
        );
    }
    Ok(output.trim_matches('\n').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::extract::resource::Limits;

    async fn text(html: &'static str) -> String {
        crate::util::execution::run_blocking_step(move |execution| {
            let budget = Budget::new("extract_html", Limits::current(), &execution);
            extract(html, "text", &execution, &budget)
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn paragraphs_and_double_breaks_are_separated_by_blank_lines() {
        assert_eq!(
            text("<p>a<br><br>b</p><p>c</p>\n<br>\n<p>d</p>").await,
            "a\n\nb\n\nc\n\nd"
        );
    }

    #[tokio::test]
    async fn list_items_are_separated_by_one_newline() {
        assert_eq!(text("<ul><li>x</li><li>y</li></ul>").await, "x\ny");
    }

    #[tokio::test]
    async fn heading_and_paragraph_are_separated_by_a_blank_line() {
        assert_eq!(text("<h1>T</h1><p>a</p>").await, "T\n\na");
    }

    #[tokio::test]
    async fn single_breaks_rules_and_table_rows_keep_narrow_separation() {
        assert_eq!(
            text("<p>a<br>b</p><br><p>c</p><hr><div>d</div>e").await,
            "a\nb\n\nc\n\nd\ne"
        );
        assert_eq!(
            text("<table><tr><th>N</th><th>C</th></tr><tr><td>S</td><td>4</td></tr></table>").await,
            "N C\nS 4"
        );
    }
}
