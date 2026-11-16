//! Bullets declared per level in an `a:lstStyle` or in one of the master's
//! `p:txStyles` containers.
//!
//! Each `a:lvl1pPr` to `a:lvl9pPr` child may declare a bullet (`a:buChar`,
//! `a:buAutoNum`, `a:buBlip`) or switch it off (`a:buNone`). A level that
//! declares neither leaves the decision to the next style in the chain.

/// Levels `a:lvl1pPr` to `a:lvl9pPr`.
pub(super) const LEVELS: usize = 9;

/// What a list style declares for each level: a bullet, none, or nothing.
pub(super) type Levels = [Option<bool>; LEVELS];

/// Reads the levels of the list style that is open.
#[derive(Default)]
pub(super) struct ListStyle {
    /// The open `a:lvlNpPr`, 0-based.
    level: Option<usize>,
    pub(super) levels: Levels,
}

impl ListStyle {
    /// An element opens inside the list style.
    pub(super) fn open(&mut self, name: &[u8]) {
        if let Some(level) = style_level(name) {
            self.level = Some(level);
        } else if let Some(declared) = declared_bullet(name)
            && let Some(slot) = self.level.and_then(|level| self.levels.get_mut(level))
        {
            *slot = Some(declared);
        }
    }

    pub(super) fn close(&mut self, name: &[u8]) {
        if style_level(name).is_some() {
            self.level = None;
        }
    }
}

/// Whether a paragraph-property child declares a bullet (`true`) or
/// switches it off (`false`).
pub(super) fn declared_bullet(name: &[u8]) -> Option<bool> {
    match name {
        b"buChar" | b"buAutoNum" | b"buBlip" => Some(true),
        b"buNone" => Some(false),
        _ => None,
    }
}

/// The declaration for a paragraph level, if the level exists.
pub(super) fn at(levels: &Levels, level: u32) -> Option<bool> {
    let level = usize::try_from(level).ok()?;
    levels.get(level).copied().flatten()
}

/// The 0-based level of an `a:lvlNpPr` element name.
fn style_level(name: &[u8]) -> Option<usize> {
    let digit = name.strip_prefix(b"lvl")?.strip_suffix(b"pPr")?;
    match digit {
        [digit @ b'1'..=b'9'] => Some(usize::from(digit - b'1')),
        _ => None,
    }
}
