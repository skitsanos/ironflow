//! Which paragraphs of a shape are list items.
//!
//! A bullet is declared by `a:buChar`, `a:buAutoNum` or `a:buBlip` and
//! switched off by `a:buNone`, in the paragraph's `a:pPr` or, per level, in
//! the `a:lstStyle` of the shape's text body. A first-level paragraph has no
//! `lvl` attribute. A placeholder paragraph that declares nothing inherits
//! from its slide layout and slide master; a shape that is not a placeholder
//! inherits no bullet.

use super::super::inherited::{Inherited, Resolved};
use super::super::list_style::{self, ListStyle, declared_bullet};

#[derive(Default)]
pub(super) struct Bullets {
    placeholder: bool,
    /// `type` and `idx` of the shape's `p:ph`.
    pub(super) kind: Option<String>,
    pub(super) idx: Option<String>,
    in_list_style: bool,
    /// What the list style of the open text body declares for each level.
    shape: ListStyle,
    /// What the open paragraph declares itself.
    paragraph: Option<bool>,
    level: Option<u32>,
}

impl Bullets {
    /// A shape or graphic frame opens: nothing carries over from the last.
    pub(super) fn start_shape(&mut self) {
        *self = Self::default();
    }

    pub(super) fn placeholder(&mut self) {
        self.placeholder = true;
    }

    pub(super) fn start_text_body(&mut self) {
        self.shape = ListStyle::default();
    }

    pub(super) fn start_paragraph(&mut self) {
        self.paragraph = None;
        self.level = None;
    }

    pub(super) fn level(&mut self, level: u32) {
        self.level = Some(level);
    }

    /// An element opens inside the text body.
    pub(super) fn open(&mut self, name: &[u8], in_paragraph: bool) {
        if name == b"lstStyle" {
            self.in_list_style = true;
        } else if in_paragraph {
            if let Some(declared) = declared_bullet(name) {
                self.paragraph = Some(declared);
            }
        } else if self.in_list_style {
            self.shape.open(name);
        }
    }

    pub(super) fn close(&mut self, name: &[u8]) {
        if name == b"lstStyle" {
            self.in_list_style = false;
        }
        self.shape.close(name);
    }

    /// The list level of the open paragraph, if it is a list item.
    pub(super) fn list_level(&self, inherited: &Inherited) -> Option<u32> {
        let level = self.level.unwrap_or(0);
        let declared = self
            .paragraph
            .or_else(|| list_style::at(&self.shape.levels, level));
        let bulleted = match declared {
            Some(declared) => declared,
            None if !self.placeholder => false,
            None => match inherited.bullet(self.kind.as_deref(), self.idx.as_deref(), level) {
                Resolved::Bullet => true,
                Resolved::Plain => false,
                // No layout or master to ask: a level marks the item.
                Resolved::Unknown => self.level.is_some(),
            },
        };
        bulleted.then_some(level)
    }
}
