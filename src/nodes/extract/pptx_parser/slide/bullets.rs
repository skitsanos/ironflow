//! Which paragraphs of a shape are list items.
//!
//! A bullet is declared by `a:buChar`, `a:buAutoNum` or `a:buBlip` and
//! switched off by `a:buNone`, in the paragraph's `a:pPr` or, per level, in
//! the `a:lstStyle` of the shape's text body. A first-level paragraph has no
//! `lvl` attribute. Layouts and masters are not read: a placeholder
//! paragraph that declares nothing is a list item when it carries a level,
//! and a shape that is not a placeholder inherits no bullet.

/// Levels `a:lvl1pPr` to `a:lvl9pPr`.
const LEVELS: usize = 9;

#[derive(Default)]
pub(super) struct Bullets {
    placeholder: bool,
    in_list_style: bool,
    /// The open `a:lvlNpPr` of the list style, 0-based.
    style_level: Option<usize>,
    /// What the list style of the open text body declares for each level.
    shape: [Option<bool>; LEVELS],
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
        self.shape = [None; LEVELS];
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
        let declared = match name {
            b"lstStyle" => {
                self.in_list_style = true;
                return;
            }
            b"buChar" | b"buAutoNum" | b"buBlip" => true,
            b"buNone" => false,
            _ => {
                if self.in_list_style
                    && let Some(level) = style_level(name)
                {
                    self.style_level = Some(level);
                }
                return;
            }
        };
        if in_paragraph {
            self.paragraph = Some(declared);
        } else if let Some(slot) = self.style_level.and_then(|level| self.shape.get_mut(level)) {
            *slot = Some(declared);
        }
    }

    pub(super) fn close(&mut self, name: &[u8]) {
        if name == b"lstStyle" {
            self.in_list_style = false;
            self.style_level = None;
        } else if style_level(name).is_some() {
            self.style_level = None;
        }
    }

    /// The list level of the open paragraph, if it is a list item.
    pub(super) fn list_level(&self) -> Option<u32> {
        let level = self.level.unwrap_or(0);
        let styled = usize::try_from(level)
            .ok()
            .and_then(|level| self.shape.get(level).copied())
            .flatten();
        match self.paragraph.or(styled) {
            Some(true) => Some(level),
            Some(false) => None,
            None => self.level.filter(|_| self.placeholder),
        }
    }
}

/// The 0-based level of an `a:lvlNpPr` element name.
fn style_level(name: &[u8]) -> Option<usize> {
    let digit = name.strip_prefix(b"lvl")?.strip_suffix(b"pPr")?;
    match digit {
        [digit @ b'1'..=b'9'] => Some(usize::from(digit - b'1')),
        _ => None,
    }
}
