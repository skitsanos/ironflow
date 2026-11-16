//! What Markdown conversion has added beyond the document's own text, and
//! the padded tables it is inside (IF-215).

use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use markup5ever_rcdom::{Node, NodeData};

use super::tree::parent;

/// Bytes the padded form puts around each cell: a space on both sides and
/// one column separator.
const CELL_FRAME: u64 = 3;
/// Rows a padded table has besides its body rows: the separator line.
const EXTRA_ROWS: u64 = 1;
/// A `|` in a cell is written as `&#124;`.
const ESCAPED_PIPE_GROWTH: u64 = 5;

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
    /// Address of the row the last cell belonged to.
    row: usize,
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
        let row = self
            .widths
            .iter()
            .fold(1_u64, |sum, width| sum.saturating_add(width + CELL_FRAME));
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

    pub(super) fn enter_table(&self, table: &Rc<Node>) {
        self.state().tables.push(Table {
            padded: is_padded(table),
            ..Table::default()
        });
    }

    /// Record one converted cell of the innermost table. `row` is the
    /// address of the cell's parent.
    pub(super) fn cell(&self, row: usize, content: &str) -> bool {
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

fn tag_of(node: &Node) -> Option<&str> {
    match &node.data {
        NodeData::Element { name, .. } => Some(name.local.as_ref()),
        _ => None,
    }
}

/// The converter writes the padded pipe form for a table that has a header
/// cell or a `<thead>` of its own and is not inside another table's cell;
/// any other table is converted like ordinary blocks.
fn is_padded(table: &Rc<Node>) -> bool {
    has_header(table) && !inside_cell(table)
}

fn has_header(table: &Rc<Node>) -> bool {
    let mut pending: Vec<Rc<Node>> = table.children.borrow().iter().cloned().collect();
    while let Some(node) = pending.pop() {
        match tag_of(&node) {
            Some("th" | "thead") => return true,
            // A nested table's headers are its own.
            Some("table") => continue,
            _ => pending.extend(node.children.borrow().iter().cloned()),
        }
    }
    false
}

fn inside_cell(table: &Rc<Node>) -> bool {
    let mut current = parent(table);
    while let Some(node) = current {
        if matches!(tag_of(&node), Some("td" | "th")) {
            return true;
        }
        current = parent(&node);
    }
    false
}
