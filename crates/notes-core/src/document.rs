//! Rich-document schemas 1 (Qt-compatible) and 2 (editable quotes).
//! Unknown versions or block/span kinds are rejected, never flattened on load.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
#[path = "document_snapshot.rs"]
mod snapshot;
pub use snapshot::{DocumentSnapshot, SpanSnapshot};

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub const MARKS: [&str; 6] = [
    "bold",
    "italic",
    "underline",
    "strikethrough",
    "highlight",
    "code",
];

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub schema: u32,
    pub blocks: Vec<Block>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Clone for Document {
    fn clone(&self) -> Self {
        Self {
            schema: self.schema,
            blocks: self.blocks.clone(),
            extra: self.extra.clone(),
        }
    }

    /// Refresh an owned draft mirror without reallocating unchanged roots.
    /// Compare complete blocks, including nested cells and unknown properties;
    /// topology changes still copy every differing position exactly.
    fn clone_from(&mut self, source: &Self) {
        self.schema = source.schema;
        if self.extra != source.extra {
            self.extra.clone_from(&source.extra);
        }
        for (destination, source) in self.blocks.iter_mut().zip(&source.blocks) {
            if destination != source {
                *destination = source.clone();
            }
        }
        self.blocks.truncate(source.blocks.len());
        let copied = self.blocks.len();
        self.blocks.extend_from_slice(&source.blocks[copied..]);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<Cell>,
    #[serde(flatten)]
    pub attrs: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub blocks: Vec<Block>,
    #[serde(flatten)]
    pub attrs: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Span(pub Value);

impl Span {
    pub fn text(text: impl Into<String>) -> Self {
        Self(json!({"text": text.into()}))
    }
    pub fn content(&self) -> &str {
        self.0
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("\u{fffc}")
    }
    pub fn is_text(&self) -> bool {
        self.0.get("text").is_some()
    }
    pub fn mark(&self, mark: &str) -> bool {
        self.0
            .get("marks")
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|m| m == mark))
    }
    pub fn set_mark(&mut self, mark: &str, enabled: bool) {
        if !self.is_text() {
            return;
        }
        let mut marks: Vec<Value> = self
            .0
            .get("marks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        marks.retain(|m| m != mark);
        if enabled {
            marks.push(json!(mark));
        }
        if marks.is_empty() {
            self.0.as_object_mut().unwrap().remove("marks");
        } else {
            self.0["marks"] = json!(marks);
        }
    }
    pub fn with_text(&self, text: &str) -> Self {
        let mut span = self.clone();
        span.0["text"] = json!(text);
        span
    }
}

impl Block {
    pub fn paragraph(text: &str) -> Self {
        Self {
            id: new_id(),
            kind: "paragraph".into(),
            spans: if text.is_empty() {
                vec![]
            } else {
                vec![Span::text(text)]
            },
            cells: vec![],
            attrs: BTreeMap::new(),
        }
    }
    pub fn number(&self, key: &str, default: usize) -> usize {
        self.attrs
            .get(key)
            .and_then(Value::as_u64)
            .map_or(default, |n| n as usize)
    }
    pub fn string(&self, key: &str) -> &str {
        self.attrs.get(key).and_then(Value::as_str).unwrap_or("")
    }
    pub fn checked(&self) -> bool {
        self.attrs
            .get("checked")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
    pub fn text(&self) -> String {
        self.spans.iter().map(Span::content).collect()
    }
    pub fn plain_text(&self) -> String {
        self.spans
            .iter()
            .filter(|s| s.is_text())
            .map(Span::content)
            .collect()
    }
    pub fn set_kind(&mut self, kind: &str) {
        for key in ["level", "list", "indent", "checked"] {
            self.attrs.remove(key);
        }
        match kind {
            "h1" | "h2" | "h3" => {
                self.kind = "heading".into();
                self.attrs
                    .insert("level".into(), json!(kind[1..].parse::<u8>().unwrap()));
            }
            "bullet" | "ordered" | "check" => {
                self.kind = "list".into();
                self.attrs.insert("list".into(), json!(kind));
                self.attrs.insert("indent".into(), json!(0));
                if kind == "check" {
                    self.attrs.insert("checked".into(), json!(false));
                }
            }
            "quote" => self.kind = "quote".into(),
            _ => self.kind = "paragraph".into(),
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self {
            schema: 1,
            blocks: vec![Block::paragraph("")],
            extra: BTreeMap::new(),
        }
    }
}

impl Document {
    /// Construct a fragment using the oldest schema that can express its blocks.
    /// Validation remains explicit; unknown block kinds never become paragraphs.
    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        fn quoted(blocks: &[Block]) -> bool {
            blocks
                .iter()
                .any(|b| b.kind == "quote" || b.cells.iter().any(|c| quoted(&c.blocks)))
        }
        Self {
            schema: if quoted(&blocks) { 2 } else { 1 },
            blocks,
            extra: BTreeMap::new(),
        }
    }
    pub fn from_json(json: &str) -> Result<Self> {
        ensure!(
            json.len() <= 32 * 1024 * 1024,
            "This document exceeds the 32 MiB editing limit."
        );
        let document: Self = serde_json::from_str(json)?;
        document.validate()?;
        Ok(document)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            serde_json::to_vec(self)?.len() <= 32 * 1024 * 1024,
            "This document exceeds the 32 MiB storage limit."
        );
        ensure!(
            (1..=2).contains(&self.schema),
            "Unsupported document schema {}. The original document is unchanged.",
            self.schema
        );
        fn blocks(
            values: &[Block],
            depth: usize,
            seen: &mut HashSet<String>,
            schema: u32,
        ) -> Result<()> {
            ensure!(depth <= 5, "Tables are nested too deeply.");
            for b in values {
                ensure!(
                    !b.id.is_empty() && seen.insert(b.id.clone()),
                    "Missing or duplicate block ID."
                );
                ensure!(
                    matches!(b.kind.as_str(), "paragraph" | "heading" | "list" | "table")
                        || (schema == 2 && b.kind == "quote"),
                    "Unsupported block type: {}",
                    b.kind
                );
                if b.kind == "heading" {
                    ensure!(
                        (1..=3).contains(&b.number("level", 0)),
                        "Invalid heading level."
                    );
                }
                if b.kind == "list" {
                    ensure!(
                        matches!(b.string("list"), "bullet" | "ordered" | "check"),
                        "Invalid list type."
                    );
                    ensure!(b.number("indent", 0) <= 16, "Invalid list indentation.");
                }
                if b.kind == "table" {
                    let (rows, cols) = (b.number("rows", 0), b.number("columns", 0));
                    ensure!(
                        (1..=1000).contains(&rows) && (1..=64).contains(&cols),
                        "Invalid table dimensions."
                    );
                    for cell in &b.cells {
                        let rs = cell
                            .attrs
                            .get("rowSpan")
                            .and_then(Value::as_u64)
                            .unwrap_or(1) as usize;
                        let cs = cell
                            .attrs
                            .get("colSpan")
                            .and_then(Value::as_u64)
                            .unwrap_or(1) as usize;
                        ensure!(
                            rs > 0
                                && cs > 0
                                && cell.row.saturating_add(rs) <= rows
                                && cell.col.saturating_add(cs) <= cols,
                            "Invalid table cell."
                        );
                        blocks(&cell.blocks, depth + 1, seen, schema)?;
                    }
                }
                for s in &b.spans {
                    ensure!(
                        ["text", "image", "attachment"]
                            .iter()
                            .filter(|k| s.0.get(**k).is_some())
                            .count()
                            == 1,
                        "Invalid span content."
                    );
                    if s.is_text() {
                        ensure!(s.0["text"].is_string(), "Invalid text span.");
                        if let Some(marks) = s.0.get("marks") {
                            ensure!(
                                marks.as_array().is_some_and(|a| a
                                    .iter()
                                    .all(|v| v.as_str().is_some_and(|s| MARKS.contains(&s)))),
                                "Unsupported text mark."
                            );
                        }
                    } else if let Some(hash) = s.0.get("image") {
                        ensure!(
                            hash.as_str().is_some_and(|s| s.len() == 64
                                && s.bytes()
                                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))),
                            "Invalid image reference."
                        );
                    } else {
                        ensure!(
                            s.0["attachment"]
                                .as_str()
                                .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok()),
                            "Invalid attachment reference."
                        );
                    }
                }
            }
            Ok(())
        }
        blocks(&self.blocks, 0, &mut HashSet::new(), self.schema)
    }
    pub fn leaves(&self) -> Vec<&Block> {
        fn visit<'a>(blocks: &'a [Block], out: &mut Vec<&'a Block>) {
            for b in blocks {
                if b.kind == "table" {
                    for c in &b.cells {
                        visit(&c.blocks, out);
                    }
                } else {
                    out.push(b);
                }
            }
        }
        let mut out = vec![];
        visit(&self.blocks, &mut out);
        out
    }
    pub fn block(&self, id: &str) -> Option<&Block> {
        self.leaves().into_iter().find(|b| b.id == id)
    }
    pub fn container_mut(&mut self, id: &str) -> Option<&mut Vec<Block>> {
        fn find<'a>(blocks: &'a mut Vec<Block>, id: &str) -> Option<&'a mut Vec<Block>> {
            if blocks.iter().any(|b| b.id == id) {
                return Some(blocks);
            }
            for b in blocks {
                for cell in &mut b.cells {
                    if let Some(found) = find(&mut cell.blocks, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        find(&mut self.blocks, id)
    }
    pub fn block_mut(&mut self, id: &str) -> Option<&mut Block> {
        self.container_mut(id)?.iter_mut().find(|b| b.id == id)
    }
    pub fn plain_text(&self) -> String {
        self.leaves()
            .iter()
            .map(|b| b.plain_text())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn title(&self) -> String {
        for block in self.leaves() {
            let text = block.plain_text();
            if let Some(line) = text.lines().map(str::trim).find(|s| !s.is_empty()) {
                return take_utf16(line, 200);
            }
        }
        String::new()
    }
    pub fn snippet(&self) -> String {
        let mut skipped_title = false;
        let mut value = String::with_capacity(164);
        let mut units = 0;
        for block in self.leaves() {
            let text = block.plain_text();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                if !skipped_title {
                    skipped_title = true;
                    continue;
                }
                for word in line.split_whitespace() {
                    if !value.is_empty() {
                        value.push(' ');
                        units += 1;
                    }
                    for ch in word.chars() {
                        value.push(ch);
                        units += ch.len_utf16();
                        if units > 160 {
                            return format!("{}…", take_utf16(&value, 159));
                        }
                    }
                }
            }
        }
        value
    }
    pub fn references(&self, key: &str) -> Vec<String> {
        let mut out = vec![];
        for block in self.leaves() {
            for span in &block.spans {
                if let Some(value) = span.0.get(key).and_then(Value::as_str)
                    && !out.iter().any(|s| s == value)
                {
                    out.push(value.to_owned());
                }
            }
        }
        out
    }
    pub fn tags(&self) -> Vec<String> {
        let mut tags = vec![];
        for b in self.leaves() {
            for s in &b.spans {
                if !s.is_text()
                    || s.mark("code")
                    || s.0
                        .get("href")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty())
                {
                    continue;
                }
                let chars: Vec<_> = s.content().chars().collect();
                for (i, c) in chars.iter().enumerate() {
                    if *c != '#'
                        || (i > 0
                            && !chars[i - 1].is_whitespace()
                            && !"([{\"'“‘".contains(chars[i - 1]))
                    {
                        continue;
                    }
                    let value: String = chars[i + 1..]
                        .iter()
                        .take_while(|c| c.is_alphanumeric() || "_-/".contains(**c))
                        .collect();
                    let value = value.trim_end_matches(['-', '/']);
                    if value.starts_with(['-', '/'])
                        || !value.chars().any(char::is_alphabetic)
                        || value.encode_utf16().count() > 64
                    {
                        continue;
                    }
                    let tag = value.to_lowercase();
                    if !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
            }
        }
        tags
    }
}

fn take_utf16(s: &str, max: usize) -> String {
    let mut count = 0;
    s.chars()
        .take_while(|c| {
            count += c.len_utf16();
            count <= max
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_mirror_preserves_nested_properties_topology_and_independent_ownership() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/qt-v1/documents.json"))
                .unwrap();
        let mut source = fixture
            .as_array()
            .unwrap()
            .iter()
            .map(|note| Document::from_json(&note["body"].to_string()).unwrap())
            .find(|document| document.blocks.iter().any(|block| block.kind == "table"))
            .unwrap();
        let mut mirror = source.clone();
        source.schema = 2;
        source
            .extra
            .insert("future".into(), json!({"unknown": [null, true, "😀"]}));
        let table = source
            .blocks
            .iter_mut()
            .find(|block| block.kind == "table")
            .unwrap();
        table.attrs.insert("future".into(), json!({"keep": 3}));
        let cell = &mut table.cells[0];
        cell.attrs.insert("future".into(), json!(["preserve", 2]));
        cell.blocks[0]
            .spans
            .push(Span(json!({"text": "é", "future": {"value": 7}})));
        mirror.clone_from(&source);
        assert_eq!(
            serde_json::to_value(&mirror).unwrap(),
            serde_json::to_value(&source).unwrap()
        );

        source.blocks.insert(0, Block::paragraph("Inserted"));
        mirror.clone_from(&source);
        assert_eq!(mirror, source);
        source.blocks.rotate_left(1);
        mirror.clone_from(&source);
        assert_eq!(mirror, source);
        source.blocks.pop();
        mirror.clone_from(&source);
        assert_eq!(mirror, source);
        source.extra.clear();
        mirror.clone_from(&source);
        assert_eq!(mirror, source);

        let before = source.clone();
        mirror.blocks[0]
            .attrs
            .insert("mirror-only".into(), json!(true));
        assert_eq!(source, before);
        source.blocks[0]
            .attrs
            .insert("source-only".into(), json!(true));
        assert!(!mirror.blocks[0].attrs.contains_key("source-only"));
    }
    #[test]
    fn summaries_preserve_line_and_utf16_limits() {
        let d = Document::from_blocks(vec![
            Block::paragraph(" \n"),
            Block::paragraph("  Hello \nworld"),
            Block::paragraph(" \tnext\n line "),
        ]);
        assert_eq!(d.title(), "Hello");
        assert_eq!(d.snippet(), "world next line");
        let mut d = Document::from_blocks(vec![
            Block::paragraph(&"😀".repeat(101)),
            Block::paragraph(&"😀".repeat(80)),
        ]);
        assert_eq!(d.title(), "😀".repeat(100));
        assert_eq!(d.snippet(), "😀".repeat(80));
        d.blocks.push(Block::paragraph(" \n \t"));
        assert_eq!(d.snippet(), "😀".repeat(80));
        d.blocks.push(Block::paragraph("more"));
        assert_eq!(d.snippet(), format!("{}…", "😀".repeat(79)));
    }
    #[test]
    fn qt_fixture_round_trips_without_losing_structure_or_properties() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/qt-v1/documents.json"))
                .unwrap();
        for note in fixture.as_array().unwrap() {
            let d = Document::from_json(&note["body"].to_string()).unwrap();
            assert_eq!(serde_json::to_value(&d).unwrap(), note["body"]);
            assert_eq!(d.title(), note["title"].as_str().unwrap());
        }
    }
    #[test]
    fn rejects_future_schema_and_unknown_blocks() {
        assert!(Document::from_json(r#"{"schema":9,"blocks":[]}"#).is_err());
        assert!(
            Document::from_json(r#"{"schema":1,"blocks":[{"id":"x","type":"future"}]}"#).is_err()
        );
    }
    #[test]
    fn derived_text_and_tags_follow_qt_rules() {
        let mut d = Document::default();
        d.blocks[0].spans = vec![Span::text(
            "  Title\nbody #Tödo- (#work/sub) url#no #123 #_ok\nnext",
        )];
        assert_eq!(d.title(), "Title");
        assert_eq!(d.snippet(), "body #Tödo- (#work/sub) url#no #123 #_ok next");
        assert_eq!(d.tags(), ["tödo", "work/sub", "_ok"]);
    }
}
