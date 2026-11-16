//! Where text is shown on the page, to decide what separates two shown
//! strings: nothing, a space or a line break.
//!
//! The text and line matrices and the page transformation are tracked the
//! way the content stream sets them. Text that starts off the baseline of
//! the last string, or left of where it ended, begins a new line; a gap on
//! the same baseline is a space. When the end of the last string is unknown
//! (its font has no usable widths) or the geometry is degenerate, the older
//! rule applies: a new `BT … ET` block starts a new line.

use lopdf::Object;

use super::widths::Measure;

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
/// Saved page transformations kept; deeper `q` nesting restores nothing.
const MAX_SAVED: usize = 64;
/// Text further than this share of its size off the last baseline is a new
/// line. Superscripts and subscripts stay within it.
const LINE_TOLERANCE: f64 = 0.5;
/// A start this far left of the last end is a new line, not kerning.
const BACK_TOLERANCE: f64 = 0.1;
/// A gap wider than this share of the text size is a word space.
const SPACE_GAP: f64 = 0.2;

/// `a` applied first, then `b`.
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn numbers<const N: usize>(operands: &[Object]) -> Option<[f64; N]> {
    let mut values = [0.0; N];
    for (value, operand) in values.iter_mut().zip(operands.get(..N)?) {
        *value = match operand {
            Object::Integer(number) => *number as f64,
            Object::Real(number) => f64::from(*number),
            _ => return None,
        };
    }
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum Join {
    None,
    Space,
    Line,
}

/// The glyphs of one show operator and its `TJ` adjustments.
pub(super) struct Run {
    measure: Measure,
    known: bool,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            measure: Measure::default(),
            known: true,
        }
    }
}

impl Run {
    pub(super) fn glyphs(&mut self, measure: Option<Measure>) {
        match measure {
            Some(measure) => {
                self.measure.thousandths += measure.thousandths;
                self.measure.glyphs += measure.glyphs;
                self.measure.spaces += measure.spaces;
            }
            None => self.known = false,
        }
    }

    /// A `TJ` number moves the next glyph back by that many thousandths.
    pub(super) fn adjust(&mut self, thousandths: f64) {
        self.measure.thousandths -= thousandths;
    }
}

pub(super) struct Layout {
    page: Matrix,
    saved: Vec<Matrix>,
    unsaved: usize,
    text: Matrix,
    line: Matrix,
    leading: f64,
    size: f64,
    char_spacing: f64,
    word_spacing: f64,
    scale: f64,
    /// Where the last string ended, or started when its advance is unknown.
    last: Option<(f64, f64, bool)>,
    block_ended: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            page: IDENTITY,
            saved: Vec::new(),
            unsaved: 0,
            text: IDENTITY,
            line: IDENTITY,
            leading: 0.0,
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 1.0,
            last: None,
            block_ended: false,
        }
    }
}

impl Layout {
    /// Apply an operator that moves text or changes its metrics. Operands
    /// that are not finite numbers leave the state as it was.
    pub(super) fn operation(&mut self, operator: &str, operands: &[Object]) {
        match operator {
            "q" if self.saved.len() < MAX_SAVED => self.saved.push(self.page),
            "q" => self.unsaved += 1,
            "Q" if self.unsaved > 0 => self.unsaved -= 1,
            "Q" => self.page = self.saved.pop().unwrap_or(self.page),
            "cm" => {
                if let Some(matrix) = numbers::<6>(operands) {
                    self.page = multiply(&matrix, &self.page);
                }
            }
            "BT" => {
                self.text = IDENTITY;
                self.line = IDENTITY;
            }
            "Tm" => {
                if let Some(matrix) = numbers::<6>(operands) {
                    self.text = matrix;
                    self.line = matrix;
                }
            }
            "Td" | "TD" => {
                if let Some([x, y]) = numbers::<2>(operands) {
                    if operator == "TD" {
                        self.leading = -y;
                    }
                    self.move_line(x, y);
                }
            }
            "TL" => self.set(operands, |layout, value| layout.leading = value),
            "Tc" => self.set(operands, |layout, value| layout.char_spacing = value),
            "Tw" => self.set(operands, |layout, value| layout.word_spacing = value),
            "Tz" => self.set(operands, |layout, value| layout.scale = value / 100.0),
            _ => {}
        }
    }

    fn set(&mut self, operands: &[Object], apply: impl FnOnce(&mut Self, f64)) {
        if let Some([value]) = numbers::<1>(operands) {
            apply(self, value);
        }
    }

    fn move_line(&mut self, x: f64, y: f64) {
        self.line = multiply(&[1.0, 0.0, 0.0, 1.0, x, y], &self.line);
        self.text = self.line;
    }

    /// `Tf`: the size operand.
    pub(super) fn font_size(&mut self, operands: &[Object]) {
        if let Some([size]) = operands.get(1..).and_then(numbers::<1>) {
            self.size = size;
        }
    }

    /// `T*`, and the move of `'` and `"`.
    pub(super) fn next_line(&mut self) {
        self.move_line(0.0, -self.leading);
    }

    /// The word and character spacing operands of `"`.
    pub(super) fn spacing(&mut self, operands: &[Object]) {
        if let Some([word, character]) = numbers::<2>(operands) {
            self.word_spacing = word;
            self.char_spacing = character;
        }
    }

    pub(super) fn end_block(&mut self) {
        self.block_ended = true;
    }

    /// Whether a block ended since the last visible text; clears the mark.
    pub(super) fn take_block_end(&mut self) -> bool {
        std::mem::take(&mut self.block_ended)
    }

    /// What separates the text about to be shown from the last string.
    pub(super) fn join(&self) -> Join {
        let legacy = if self.block_ended {
            Join::Line
        } else {
            Join::None
        };
        let Some((last_x, last_y, end_known)) = self.last else {
            return legacy;
        };
        let matrix = multiply(&self.text, &self.page);
        let length = matrix[0].hypot(matrix[1]);
        let size = self.size.abs() * matrix[2].hypot(matrix[3]);
        let (dx, dy) = (matrix[4] - last_x, matrix[5] - last_y);
        let along = (dx * matrix[0] + dy * matrix[1]) / length;
        let across = (dy * matrix[0] - dx * matrix[1]) / length;
        let usable = [length, size, along, across]
            .iter()
            .all(|value| value.is_finite())
            && length > 0.0
            && size > 0.0;
        if !usable {
            legacy
        } else if across.abs() > LINE_TOLERANCE * size {
            Join::Line
        } else if !end_known {
            legacy
        } else if along < -BACK_TOLERANCE * size {
            Join::Line
        } else if along > SPACE_GAP * size {
            Join::Space
        } else {
            Join::None
        }
    }

    /// A string was shown: move past it. `visible` text ends the wait for a
    /// block break.
    pub(super) fn shown(&mut self, run: &Run, visible: bool) {
        let measure = run.measure;
        let advance = ((measure.thousandths / 1000.0) * self.size
            + measure.glyphs as f64 * self.char_spacing
            + measure.spaces as f64 * self.word_spacing)
            * self.scale;
        let known = run.known && advance.is_finite();
        if known {
            self.text = multiply(&[1.0, 0.0, 0.0, 1.0, advance, 0.0], &self.text);
        }
        let matrix = multiply(&self.text, &self.page);
        self.last = Some((matrix[4], matrix[5], known));
        if visible {
            self.block_ended = false;
        }
    }
}
