//! What Markdown conversion has added beyond the document's own text, and
//! the padded tables it is inside (IF-215).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Bytes the padded form puts around each cell: a space on both sides and
/// one column separator.
const CELL_FRAME: u64 = 3;
/// Rows a padded table can have besides the rows its cells came from: the
/// separator line, and an empty header row when the table has a `<thead>`
/// without cells.
const EXTRA_ROWS: u64 = 2;
/// A `|` in a cell is written as `\|`.
const ESCAPED_PIPE_GROWTH: u64 = 1;
/// The converter writes an empty column one byte wide.
const MIN_COLUMN_WIDTH: u64 = 1;

pub(super) struct Meter {
    limit: u64,
    exceeded: AtomicBool,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Bytes added by prefixes and by finished padded tables.
    added: u64,
    /// The tables being converted, innermost last.
    tables: Vec<Table>,
}

/// One table being converted. Only a table the converter pads is measured.
#[derive(Default)]
struct Table {
    padded: bool,
    /// Identity of the row the last cell belonged to.
    row: u64,
    rows: u64,
    column: usize,
    /// Widest cell seen in each column, in bytes.
    widths: Vec<u64>,
    /// Bytes of cell content, which are not padding.
    content: u64,
}

impl Table {
    /// Upper bound on the padded table: every row repeats every column's
    /// width.
    fn projected(&self) -> u64 {
        let row = self.widths.iter().fold(1_u64, |sum, width| {
            sum.saturating_add(width.max(&MIN_COLUMN_WIDTH) + CELL_FRAME)
        });
        row.saturating_mul(self.rows + EXTRA_ROWS)
    }

    fn padding(&self) -> u64 {
        self.projected().saturating_sub(self.content)
    }
}

impl Meter {
    pub(super) fn new(limit: u64) -> Self {
        Self {
            limit,
            exceeded: AtomicBool::new(false),
            state: Mutex::new(State::default()),
        }
    }

    pub(super) fn exceeded(&self) -> bool {
        self.exceeded.load(Ordering::Relaxed)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// `true` while the conversion is within the limit.
    fn settle(&self, within: bool) -> bool {
        if !within {
            self.exceeded.store(true, Ordering::Relaxed);
        }
        within
    }

    /// Charge `added` bytes for an element whose result is `length` bytes.
    pub(super) fn charge(&self, added: u64, length: usize) -> bool {
        let mut state = self.state();
        state.added = state.added.saturating_add(added);
        let within = state.added <= self.limit && length as u64 <= self.limit;
        self.settle(within)
    }

    /// Open a table. Only a `padded` one, which the converter writes in the
    /// pipe form, is measured.
    pub(super) fn enter_table(&self, padded: bool) {
        self.state().tables.push(Table {
            padded,
            ..Table::default()
        });
    }

    /// Record one converted cell of the innermost table. `row` identifies
    /// the cell's parent.
    pub(super) fn cell(&self, row: u64, content: &str) -> bool {
        let mut state = self.state();
        let added = state.added;
        let Some(table) = state.tables.last_mut().filter(|table| table.padded) else {
            return true;
        };
        if table.rows == 0 || table.row != row {
            table.row = row;
            table.rows += 1;
            table.column = 0;
        } else {
            table.column += 1;
        }
        let pipes = content.matches('|').count() as u64;
        let width = (content.len() as u64).saturating_add(pipes * ESCAPED_PIPE_GROWTH);
        if table.widths.len() <= table.column {
            table.widths.resize(table.column + 1, 0);
        }
        let column = table.column;
        table.widths[column] = table.widths[column].max(width);
        table.content = table.content.saturating_add(content.len() as u64);
        let within = added.saturating_add(table.padding()) <= self.limit;
        self.settle(within)
    }

    /// Close the innermost table, whose result is `length` bytes, and keep
    /// what its padding added.
    pub(super) fn leave_table(&self, length: usize) -> bool {
        let mut state = self.state();
        let padding = state
            .tables
            .pop()
            .filter(|table| table.padded)
            .map_or(0, |table| table.padding().min(length as u64));
        state.added = state.added.saturating_add(padding);
        let within = state.added <= self.limit && length as u64 <= self.limit;
        self.settle(within)
    }
}
