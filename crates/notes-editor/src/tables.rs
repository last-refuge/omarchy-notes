//! Table transactions preserve cell contents, stable block IDs and schema-1 metadata.
use super::*;
use notes_core::document::Cell;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableAction {
    RowBefore,
    RowAfter,
    ColumnBefore,
    ColumnAfter,
    DeleteRow,
    DeleteColumn,
    SplitCell,
    Remove,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableContext {
    pub id: String,
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    pub rows: usize,
    pub columns: usize,
}
impl TableContext {
    pub fn allows(&self, action: TableAction) -> bool {
        match action {
            TableAction::RowBefore | TableAction::RowAfter => self.rows < 1000,
            TableAction::ColumnBefore | TableAction::ColumnAfter => self.columns < 64,
            TableAction::DeleteRow => self.rows > 1,
            TableAction::DeleteColumn => self.columns > 1,
            TableAction::SplitCell => self.row_span > 1 || self.column_span > 1,
            TableAction::Remove => true,
        }
    }
}
fn span(cell: &Cell, row: bool) -> usize {
    cell.attrs
        .get(if row { "rowSpan" } else { "colSpan" })
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1) as usize
}
fn set_span(cell: &mut Cell, row: bool, value: usize) {
    let key = if row { "rowSpan" } else { "colSpan" };
    if value == 1 {
        cell.attrs.remove(key);
    } else {
        cell.attrs.insert(key.into(), json!(value));
    }
}
fn coordinate(cell: &Cell, row: bool) -> usize {
    if row { cell.row } else { cell.col }
}
fn set_coordinate(cell: &mut Cell, row: bool, value: usize) {
    if row {
        cell.row = value;
    } else {
        cell.col = value;
    }
}
fn empty_cell(row: usize, col: usize) -> Cell {
    Cell {
        row,
        col,
        blocks: vec![Block::paragraph("")],
        attrs: BTreeMap::new(),
    }
}
/// The innermost table containing a leaf. Table identity survives insertion/deletion.
fn locate<'a>(blocks: &'a [Block], id: &str, depth: usize) -> Option<(&'a Block, usize, usize)> {
    for table in blocks.iter().filter(|b| b.kind == "table") {
        for (index, cell) in table.cells.iter().enumerate() {
            if let Some(found) = locate(&cell.blocks, id, depth + 1) {
                return Some(found);
            }
            if cell.blocks.iter().any(|b| b.id == id) {
                return Some((table, index, depth));
            }
        }
    }
    None
}
fn contains_leaf(blocks: &[Block], id: &str) -> bool {
    blocks
        .iter()
        .any(|b| b.id == id || b.cells.iter().any(|c| contains_leaf(&c.blocks, id)))
}
fn shared_cell<'a>(blocks: &'a [Block], anchor: &str, head: &str) -> Option<(&'a Block, usize)> {
    for table in blocks.iter().filter(|b| b.kind == "table") {
        for (index, cell) in table.cells.iter().enumerate() {
            if contains_leaf(&cell.blocks, anchor) && contains_leaf(&cell.blocks, head) {
                return shared_cell(&cell.blocks, anchor, head).or(Some((table, index)));
            }
        }
    }
    None
}
fn table_by_id<'a>(blocks: &'a [Block], id: &str) -> Option<&'a Block> {
    blocks.iter().find_map(|b| {
        if b.id == id && b.kind == "table" {
            Some(b)
        } else {
            b.cells.iter().find_map(|c| table_by_id(&c.blocks, id))
        }
    })
}
fn first_leaf(blocks: &[Block]) -> Option<&Block> {
    blocks.iter().find_map(|b| {
        if b.kind == "table" {
            b.cells.iter().find_map(|c| first_leaf(&c.blocks))
        } else {
            Some(b)
        }
    })
}
fn last_leaf(blocks: &[Block]) -> Option<&Block> {
    blocks.iter().rev().find_map(|b| {
        if b.kind == "table" {
            b.cells.iter().rev().find_map(|c| last_leaf(&c.blocks))
        } else {
            Some(b)
        }
    })
}
/// Qt permits omitted cells; materialize their implicit empty paragraphs before editing.
/// Overlap is ambiguous and is rejected before the live document is changed.
fn normalize(table: &mut Block) -> Result<()> {
    let rows = table.number("rows", 0);
    let columns = table.number("columns", 0);
    ensure!(
        (1..=1000).contains(&rows) && (1..=64).contains(&columns),
        "Invalid table dimensions."
    );
    let mut occupied = vec![false; rows * columns];
    for cell in &mut table.cells {
        let (rs, cs) = (span(cell, true), span(cell, false));
        ensure!(
            rs > 0
                && cs > 0
                && cell.row.checked_add(rs).is_some_and(|n| n <= rows)
                && cell.col.checked_add(cs).is_some_and(|n| n <= columns),
            "Invalid table cell bounds."
        );
        for row in cell.row..cell.row + rs {
            for col in cell.col..cell.col + cs {
                ensure!(
                    !occupied[row * columns + col],
                    "This table has overlapping cells. Restore or repair it before changing its structure."
                );
                occupied[row * columns + col] = true;
            }
        }
        if first_leaf(&cell.blocks).is_none() {
            cell.blocks.push(Block::paragraph(""));
        }
    }
    for row in 0..rows {
        for col in 0..columns {
            if !occupied[row * columns + col] {
                table.cells.push(empty_cell(row, col));
            }
        }
    }
    table.cells.sort_by_key(|c| (c.row, c.col));
    Ok(())
}
fn insert_axis(table: &mut Block, row: bool, at: usize) -> Result<()> {
    let key = if row { "rows" } else { "columns" };
    let count = table.number(key, 0);
    ensure!(
        count < if row { 1000 } else { 64 },
        "This table has reached the {} limit.",
        if row { "1,000-row" } else { "64-column" }
    );
    ensure!(at <= count, "Invalid table insertion position.");
    for cell in &mut table.cells {
        let start = coordinate(cell, row);
        let length = span(cell, row);
        if start >= at {
            set_coordinate(cell, row, start + 1);
        } else if start + length > at {
            set_span(cell, row, length + 1);
        }
    }
    table.attrs.insert(key.into(), json!(count + 1));
    normalize(table)
}
fn delete_axis(table: &mut Block, row: bool, at: usize) -> Result<()> {
    let key = if row { "rows" } else { "columns" };
    let count = table.number(key, 0);
    ensure!(
        count > 1,
        "Keep at least one {} or remove the table.",
        if row { "row" } else { "column" }
    );
    table.cells.retain_mut(|cell| {
        let start = coordinate(cell, row);
        let length = span(cell, row);
        if start > at {
            set_coordinate(cell, row, start - 1);
        } else if start + length > at {
            if length == 1 {
                return false;
            }
            set_span(cell, row, length - 1);
        }
        true
    });
    table.attrs.insert(key.into(), json!(count - 1));
    normalize(table)
}
impl Editor {
    pub fn in_table(&self) -> bool {
        locate(&self.document.blocks, &self.selection.head.block, 0).is_some()
    }
    pub fn table_context(&self) -> Option<TableContext> {
        let (table, index) = shared_cell(
            &self.document.blocks,
            &self.selection.anchor.block,
            &self.selection.head.block,
        )?;
        let cell = &table.cells[index];
        Some(TableContext {
            id: table.id.clone(),
            row: cell.row,
            column: cell.col,
            row_span: span(cell, true),
            column_span: span(cell, false),
            rows: table.number("rows", 0),
            columns: table.number("columns", 0),
        })
    }
    fn table_transaction(&mut self, change: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        let before = self.snapshot();
        if let Err(error) = change(self).and_then(|_| self.document.validate()) {
            self.restore(before);
            return Err(error);
        }
        self.last_typing = None;
        self.record(before, false);
        Ok(())
    }
    fn install_table(&mut self, table: Block) {
        let target = self
            .document
            .block_mut(&table.id)
            .expect("table transaction keeps its container");
        *target = table;
    }
    fn select_cell(&mut self, cell: &Cell, all: bool) {
        let first = first_leaf(&cell.blocks).expect("normalized cells have content");
        let last = if all {
            last_leaf(&cell.blocks).unwrap()
        } else {
            first
        };
        let anchor = Position {
            block: first.id.clone(),
            offset: 0,
        };
        let head = Position {
            block: last.id.clone(),
            offset: if all { last.text().len() } else { 0 },
        };
        self.select(self.global(&anchor), false);
        self.select(self.global(&head), true);
    }
    /// Insert after the current paragraph, matching the Qt shortcut; never discard selection text.
    pub fn insert_table(&mut self, rows: usize, columns: usize) -> Result<()> {
        ensure!(
            (1..=1000).contains(&rows) && (1..=64).contains(&columns),
            "Choose 1–1,000 rows and 1–64 columns."
        );
        if let Some((_, _, depth)) = locate(&self.document.blocks, &self.selection.head.block, 0) {
            ensure!(depth < 4, "Tables cannot be nested more deeply.");
        }
        self.table_transaction(|editor| {
            let mut table = Block {
                id: new_id(),
                kind: "table".into(),
                spans: vec![],
                cells: vec![],
                attrs: BTreeMap::from([
                    ("rows".into(), json!(rows)),
                    ("columns".into(), json!(columns)),
                ]),
            };
            normalize(&mut table)?;
            let first = table.cells[0].clone();
            let head = editor.selection.head.block.clone();
            let container = editor.document.container_mut(&head).unwrap();
            let index = container.iter().position(|b| b.id == head).unwrap();
            // Retain the original block and all of its metadata, even when empty.
            container.insert(index + 1, table);
            if container.get(index + 2).is_none_or(|b| b.kind == "table") {
                container.insert(index + 2, Block::paragraph(""));
            }
            editor.select_cell(&first, false);
            Ok(())
        })
    }
    pub fn table_action(&mut self, action: TableAction) -> Result<()> {
        let context = self
            .table_context()
            .context("Place the caret in one table cell first.")?;
        ensure!(
            context.allows(action),
            "That table action is unavailable for this cell."
        );
        self.table_transaction(|editor| {
            if action == TableAction::Remove {
                let container = editor.document.container_mut(&context.id).unwrap();
                let index = container.iter().position(|b| b.id == context.id).unwrap();
                container.remove(index);
                if container.get(index).is_none_or(|b| b.kind == "table") {
                    container.insert(index, Block::paragraph(""));
                }
                let position = Position {
                    block: container[index].id.clone(),
                    offset: 0,
                };
                editor.select(editor.global(&position), false);
                return Ok(());
            }
            let mut table = table_by_id(&editor.document.blocks, &context.id)
                .unwrap()
                .clone();
            normalize(&mut table)?;
            match action {
                TableAction::RowBefore => insert_axis(&mut table, true, context.row)?,
                TableAction::RowAfter => {
                    insert_axis(&mut table, true, context.row + context.row_span)?
                }
                TableAction::ColumnBefore => insert_axis(&mut table, false, context.column)?,
                TableAction::ColumnAfter => {
                    insert_axis(&mut table, false, context.column + context.column_span)?
                }
                TableAction::DeleteRow => delete_axis(&mut table, true, context.row)?,
                TableAction::DeleteColumn => delete_axis(&mut table, false, context.column)?,
                TableAction::SplitCell => {
                    let cell = table
                        .cells
                        .iter_mut()
                        .find(|c| c.row == context.row && c.col == context.column)
                        .unwrap();
                    set_span(cell, true, 1);
                    set_span(cell, false, 1);
                    normalize(&mut table)?;
                }
                TableAction::Remove => unreachable!(),
            }
            // Preserve the caret when its block survives; otherwise use the nearest remaining cell.
            let row = context.row.min(table.number("rows", 1) - 1);
            let col = context.column.min(table.number("columns", 1) - 1);
            let target = table
                .cells
                .iter()
                .find(|c| {
                    c.row <= row
                        && row < c.row + span(c, true)
                        && c.col <= col
                        && col < c.col + span(c, false)
                })
                .unwrap()
                .clone();
            editor.install_table(table);
            if editor
                .document
                .block(&editor.selection.head.block)
                .is_none()
                || editor
                    .document
                    .block(&editor.selection.anchor.block)
                    .is_none()
            {
                editor.select_cell(&target, false);
            }
            Ok(())
        })
    }
    /// True consumes Tab. Body paragraphs return false so the native focus route remains available.
    pub fn tab(&mut self, backward: bool) -> Result<bool> {
        if self.marked.is_some() {
            return Ok(true);
        }
        if let Some(context) = self.table_context() {
            self.table_transaction(|editor| {
                let mut table = table_by_id(&editor.document.blocks, &context.id)
                    .unwrap()
                    .clone();
                normalize(&mut table)?;
                let index = table
                    .cells
                    .iter()
                    .position(|c| c.row == context.row && c.col == context.column)
                    .unwrap();
                if backward && index == 0 {
                    editor.install_table(table);
                    return Ok(());
                }
                let target = if backward {
                    index.saturating_sub(1)
                } else {
                    index + 1
                };
                if !backward && target == table.cells.len() {
                    let rows = table.number("rows", 0);
                    insert_axis(&mut table, true, rows)?;
                }
                let cell = table.cells[target].clone();
                editor.install_table(table);
                editor.select_cell(&cell, true);
                Ok(())
            })?;
            return Ok(true);
        }
        // Do not navigate or indent an ambiguous selection spanning different cells.
        if locate(&self.document.blocks, &self.selection.head.block, 0).is_some() {
            bail!("Select content within one cell before moving between cells.");
        }
        if self
            .document
            .block(&self.selection.head.block)
            .is_some_and(|b| b.kind == "list")
        {
            self.indent(backward)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn leave_table(&mut self) -> Result<()> {
        let context = self
            .table_context()
            .context("Place the caret in one table cell first.")?;
        self.table_transaction(|editor| {
            let container = editor.document.container_mut(&context.id).unwrap();
            let after = container.iter().position(|b| b.id == context.id).unwrap() + 1;
            if container.get(after).is_none_or(|b| b.kind == "table") {
                container.insert(after, Block::paragraph(""));
            }
            let target = Position {
                block: container[after].id.clone(),
                offset: 0,
            };
            editor.select(editor.global(&target), false);
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
