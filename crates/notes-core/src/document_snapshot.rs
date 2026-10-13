//! Owned history representation with directly countable heap allocations.
//!
//! Maps become sorted vectors. This avoids guessing the private node allocation
//! layout of `BTreeMap`, and preserves every document property without encoding
//! or decoding JSON on each edit.
use super::{Block, Cell, Document, Span};
use serde_json::Value;
use std::{collections::BTreeMap, mem::size_of};

pub struct DocumentSnapshot {
    schema: u32,
    blocks: Vec<SavedBlock>,
    extra: Vec<(String, SavedValue)>,
}

pub struct SpanSnapshot(SavedValue);

struct SavedBlock {
    id: String,
    kind: String,
    spans: Vec<SpanSnapshot>,
    cells: Vec<SavedCell>,
    attrs: Vec<(String, SavedValue)>,
}

struct SavedCell {
    row: usize,
    col: usize,
    blocks: Vec<SavedBlock>,
    attrs: Vec<(String, SavedValue)>,
}

enum SavedValue {
    Null,
    Bool(bool),
    // A decimal string also remains countable if a downstream crate enables
    // serde_json's arbitrary_precision representation.
    Number(String),
    String(String),
    Array(Vec<SavedValue>),
    Object(Vec<(String, SavedValue)>),
}

fn vector_bytes<T>(items: &Vec<T>, nested: impl Fn(&T) -> usize) -> usize {
    items.capacity() * size_of::<T>() + items.iter().map(nested).sum::<usize>()
}

fn map_bytes(items: &Vec<(String, SavedValue)>) -> usize {
    vector_bytes(items, |(key, value)| {
        key.capacity() + value.allocated_bytes()
    })
}

fn save_map(map: BTreeMap<String, Value>) -> Vec<(String, SavedValue)> {
    map.into_iter()
        .map(|(key, value)| (key, value.into()))
        .collect()
}

fn restore_map(map: Vec<(String, SavedValue)>) -> BTreeMap<String, Value> {
    map.into_iter()
        .map(|(key, value)| (key, value.into()))
        .collect()
}

impl From<Value> for SavedValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(value),
            Value::Number(value) => Self::Number(value.to_string()),
            Value::String(value) => Self::String(value),
            Value::Array(items) => Self::Array(items.into_iter().map(Into::into).collect()),
            Value::Object(items) => Self::Object(
                items
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect(),
            ),
        }
    }
}

impl From<SavedValue> for Value {
    fn from(value: SavedValue) -> Self {
        match value {
            SavedValue::Null => Self::Null,
            SavedValue::Bool(value) => Self::Bool(value),
            SavedValue::Number(value) => {
                Self::Number(value.parse().expect("saved valid JSON number"))
            }
            SavedValue::String(value) => Self::String(value),
            SavedValue::Array(items) => Self::Array(items.into_iter().map(Into::into).collect()),
            SavedValue::Object(items) => Self::Object(
                items
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect(),
            ),
        }
    }
}

impl SavedValue {
    fn allocated_bytes(&self) -> usize {
        match self {
            Self::Null | Self::Bool(_) => 0,
            Self::String(text) | Self::Number(text) => text.capacity(),
            Self::Array(items) => vector_bytes(items, Self::allocated_bytes),
            Self::Object(items) => map_bytes(items),
        }
    }
}

impl From<Span> for SpanSnapshot {
    fn from(span: Span) -> Self {
        Self(span.0.into())
    }
}

impl SpanSnapshot {
    pub fn into_span(self) -> Span {
        Span(self.0.into())
    }

    pub fn allocated_bytes(&self) -> usize {
        self.0.allocated_bytes()
    }
}

impl From<Block> for SavedBlock {
    fn from(block: Block) -> Self {
        Self {
            id: block.id,
            kind: block.kind,
            spans: block.spans.into_iter().map(Into::into).collect(),
            cells: block.cells.into_iter().map(Into::into).collect(),
            attrs: save_map(block.attrs),
        }
    }
}

impl From<Cell> for SavedCell {
    fn from(cell: Cell) -> Self {
        Self {
            row: cell.row,
            col: cell.col,
            blocks: cell.blocks.into_iter().map(Into::into).collect(),
            attrs: save_map(cell.attrs),
        }
    }
}

impl From<SavedBlock> for Block {
    fn from(block: SavedBlock) -> Self {
        Self {
            id: block.id,
            kind: block.kind,
            spans: block
                .spans
                .into_iter()
                .map(SpanSnapshot::into_span)
                .collect(),
            cells: block.cells.into_iter().map(Into::into).collect(),
            attrs: restore_map(block.attrs),
        }
    }
}

impl From<SavedCell> for Cell {
    fn from(cell: SavedCell) -> Self {
        Self {
            row: cell.row,
            col: cell.col,
            blocks: cell.blocks.into_iter().map(Into::into).collect(),
            attrs: restore_map(cell.attrs),
        }
    }
}

impl SavedBlock {
    fn allocated_bytes(&self) -> usize {
        self.id.capacity()
            + self.kind.capacity()
            + vector_bytes(&self.spans, SpanSnapshot::allocated_bytes)
            + vector_bytes(&self.cells, SavedCell::allocated_bytes)
            + map_bytes(&self.attrs)
    }
}

impl SavedCell {
    fn allocated_bytes(&self) -> usize {
        vector_bytes(&self.blocks, SavedBlock::allocated_bytes) + map_bytes(&self.attrs)
    }
}

impl From<Document> for DocumentSnapshot {
    fn from(document: Document) -> Self {
        Self {
            schema: document.schema,
            blocks: document.blocks.into_iter().map(Into::into).collect(),
            extra: save_map(document.extra),
        }
    }
}

impl DocumentSnapshot {
    pub fn into_document(self) -> Document {
        Document {
            schema: self.schema,
            blocks: self.blocks.into_iter().map(Into::into).collect(),
            extra: restore_map(self.extra),
        }
    }

    /// Heap payload owned by this snapshot, including spare vector/string
    /// capacity. Inline fields, allocator metadata and the allocator's retained
    /// pages are separate from these allocation sizes.
    pub fn allocated_bytes(&self) -> usize {
        vector_bytes(&self.blocks, SavedBlock::allocated_bytes) + map_bytes(&self.extra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_nested_unknown_properties_and_capacity() {
        let mut document = Document {
            schema: 2,
            blocks: vec![Block::paragraph("👩‍👩‍👧‍👦 e\u{301}")],
            extra: BTreeMap::from([("future".into(), json!({"array": [null, true, 5, 2.5, "é"]}))]),
        };
        document.blocks[0]
            .attrs
            .insert("future".into(), json!({"kind": "unchanged"}));
        document.blocks[0].cells.push(Cell {
            row: 1,
            col: 2,
            blocks: vec![Block::paragraph("nested")],
            attrs: BTreeMap::from([("colSpan".into(), json!(2))]),
        });
        document.blocks[0].id.reserve(512);
        let expected = document.clone();
        let snapshot = DocumentSnapshot::from(document);
        assert!(snapshot.allocated_bytes() >= 512);
        assert_eq!(snapshot.into_document(), expected);
    }
}
