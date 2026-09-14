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

fn plain_text(root: Handle, budget: &Budget<'_>) -> Result<String> {
    let mut output = String::new();
    let mut pending_space = false;
    let mut pending = vec![(root, false, false)];
    while let Some((node, exiting, in_pre)) = pending.pop() {
        budget.checkpoint()?;
        let tag = match &node.data {
            NodeData::Element { name, .. } => name.local.as_ref(),
            _ => "",
        };
        let block = matches!(
            tag,
            "address"
                | "article"
                | "aside"
                | "blockquote"
                | "div"
                | "dl"
                | "dt"
                | "dd"
                | "fieldset"
                | "figcaption"
                | "figure"
                | "footer"
                | "form"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "header"
                | "hr"
                | "li"
                | "main"
                | "nav"
                | "ol"
                | "p"
                | "pre"
                | "section"
                | "table"
                | "tr"
                | "ul"
        );
        if block || tag == "br" {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            pending_space = false;
        } else if exiting && matches!(tag, "td" | "th") {
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
            if pending_space && !output.is_empty() && !output.ends_with('\n') {
                output.push(' ');
            }
            pending_space = false;
            output.push_str(&alt);
        }
        budget.admit_output(output.len() as u64, "HTML content")?;
        pending.push((node.clone(), true, in_pre));
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
