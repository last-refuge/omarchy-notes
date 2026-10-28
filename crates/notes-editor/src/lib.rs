//! Framework-free rich editing. Selection is addressed by stable block identity.
use anyhow::{Context, Result, bail, ensure};
mod find;
mod history;
mod links;
pub use history::{HistoryUsage, MAX_HISTORY_BYTES, MAX_HISTORY_STEPS};
pub use links::Link;
#[cfg(test)]
mod autoformat_tests;
#[cfg(test)]
mod format_tests;
mod tables;
pub use find::Find;
use notes_core::document::{Block, Document, Span, new_id};
use notes_core::text_field::TextField;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ops::Range;
pub use tables::{TableAction, TableContext};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub block: String,
    pub offset: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}
/// Exact formatting context for subsequent typing, including explicit caret
/// choices and style properties unknown to this editor.
#[derive(Clone, Debug, PartialEq)]
pub struct TypingState(Span);
#[derive(Clone)]
struct Snapshot {
    document: Document,
    selection: Selection,
    typing: Span,
}

pub struct Editor {
    pub document: Document,
    pub selection: Selection,
    pub revision: u64,
    pub marked: Option<Range<usize>>,
    /// Whether the library can store quotes. Until it can, a typed `>` stays
    /// text rather than making a note that cannot be saved.
    pub quotes_enabled: bool,
    /// Revision right after a typed marker was applied. Backspace takes the
    /// formatting back while nothing has changed since; moving the caret ends it.
    marker_undo: Option<u64>,
    typing: Span,
    history: history::History,
    composition: Option<Snapshot>,
    last_typing: Option<(u64, usize)>,
}

impl Editor {
    pub fn new(mut document: Document) -> Self {
        if document.leaves().is_empty() {
            document.blocks.push(Block::paragraph(""));
        }
        let position = Position {
            block: document.leaves()[0].id.clone(),
            offset: 0,
        };
        Self {
            document,
            selection: Selection {
                anchor: position.clone(),
                head: position,
            },
            revision: 0,
            marked: None,
            quotes_enabled: false,
            marker_undo: None,
            typing: Span::text(""),
            history: history::History::default(),
            composition: None,
            last_typing: None,
        }
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            document: self.document.clone(),
            selection: self.selection.clone(),
            typing: self.typing.clone(),
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.document = s.document;
        self.selection = s.selection;
        self.typing = s.typing;
        self.marked = None;
    }
    fn record(&mut self, before: Snapshot, coalesce: bool) {
        fn normalize(blocks: &mut [Block]) {
            fn same_style(a: &Span, b: &Span) -> bool {
                let (Some(a), Some(b)) = (a.0.as_object(), b.0.as_object()) else {
                    return false;
                };
                a.len() == b.len()
                    && a.iter()
                        .filter(|(key, _)| key.as_str() != "text")
                        .eq(b.iter().filter(|(key, _)| key.as_str() != "text"))
            }
            for block in blocks {
                if block.kind == "table" {
                    for cell in &mut block.cells {
                        normalize(&mut cell.blocks);
                    }
                    continue;
                }
                let mut merged: Vec<Span> = vec![];
                for span in std::mem::take(&mut block.spans) {
                    if let Some(last) = merged.last_mut()
                        && span.is_text()
                        && last.is_text()
                        && same_style(&span, last)
                    {
                        if let Some(serde_json::Value::String(text)) = last.0.get_mut("text") {
                            text.push_str(span.content());
                        } else {
                            *last =
                                last.with_text(&format!("{}{}", last.content(), span.content()));
                        }
                    } else {
                        merged.push(span);
                    }
                }
                block.spans = merged;
            }
        }
        normalize(&mut self.document.blocks);
        if self.document == before.document {
            return;
        }
        if !self.history.record(before, coalesce) {
            self.last_typing = None;
        }
        self.revision += 1;
    }
    pub fn committed(&self) -> &Document {
        self.composition
            .as_ref()
            .map_or(&self.document, |s| &s.document)
    }
    pub fn text(&self) -> String {
        self.document
            .leaves()
            .iter()
            .map(|b| b.text())
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn global(&self, position: &Position) -> usize {
        let mut start = 0;
        for block in self.document.leaves() {
            if block.id == position.block {
                return start + clip(&block.text(), position.offset);
            }
            start += block.spans.iter().map(|s| s.content().len()).sum::<usize>() + 1;
        }
        0
    }
    pub fn position(&self, mut offset: usize) -> Position {
        let leaves = self.document.leaves();
        for (i, b) in leaves.iter().enumerate() {
            let len = b.spans.iter().map(|s| s.content().len()).sum::<usize>();
            if offset <= len || i == leaves.len() - 1 {
                return Position {
                    block: b.id.clone(),
                    offset: clip(&b.text(), offset),
                };
            }
            offset -= len + 1;
        }
        unreachable!("editor always has a leaf")
    }
    pub fn range(&self) -> Range<usize> {
        if self.selection.anchor == self.selection.head {
            let at = self.global(&self.selection.head);
            return at..at;
        }
        let (a, b) = (
            self.global(&self.selection.anchor),
            self.global(&self.selection.head),
        );
        a.min(b)..a.max(b)
    }
    pub fn select(&mut self, byte: usize, extend: bool) {
        self.marker_undo = None;
        self.selection.head = self.position(byte);
        if !extend {
            self.selection.anchor = self.selection.head.clone();
        }
        self.last_typing = None;
        let b = self.document.block(&self.selection.head.block).unwrap();
        let mut at = 0;
        self.typing = Span::text("");
        for s in &b.spans {
            at += s.content().len();
            if at >= self.selection.head.offset && s.is_text() {
                self.typing = s.with_text("");
                break;
            }
        }
    }
    pub fn select_all(&mut self) {
        self.select(0, false);
        self.select(self.text().len(), true);
    }
    pub fn move_horizontal(&mut self, forward: bool, extend: bool, word: bool) {
        let text = self.text();
        let caret = self.global(&self.selection.head);
        let r = self.range();
        let next = if !extend && !r.is_empty() {
            if forward { r.end } else { r.start }
        } else if word {
            if forward {
                text.split_word_bound_indices()
                    .map(|(i, s)| i + s.len())
                    .find(|i| *i > caret)
                    .unwrap_or(text.len())
            } else {
                text.split_word_bound_indices()
                    .map(|(i, _)| i)
                    .rfind(|i| *i < caret)
                    .unwrap_or(0)
            }
        } else if forward {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i > caret)
                .unwrap_or(text.len())
        } else {
            text.grapheme_indices(true)
                .rev()
                .map(|(i, _)| i)
                .find(|i| *i < caret)
                .unwrap_or(0)
        };
        self.select(next, extend);
    }
    fn replace(&mut self, range: Range<usize>, text: &str) -> Result<()> {
        if !range.is_empty()
            && range == (0..self.text().len())
            && (self
                .document
                .blocks
                .first()
                .is_some_and(|b| b.kind == "table")
                || self
                    .document
                    .blocks
                    .last()
                    .is_some_and(|b| b.kind == "table"))
        {
            self.document.blocks = vec![Block::paragraph("")];
            self.select(0, false);
            return self.replace(0..0, text);
        }
        let start = self.position(range.start);
        let end = self.position(range.end);
        let container = self.document.container_mut(&start.block).unwrap();
        let a = container.iter().position(|b| b.id == start.block).unwrap();
        let Some(b) = container.iter().position(|b| b.id == end.block) else {
            bail!("Edit one table cell at a time. Cross-cell replacement is not supported yet.");
        };
        let left = slice_spans(&container[a].spans, 0..start.offset);
        let right = slice_spans(&container[b].spans, end.offset..container[b].text().len());
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let parts: Vec<_> = text.split('\n').collect();
        let mut blocks = vec![];
        for (i, part) in parts.iter().enumerate() {
            let mut block = if i == 0 {
                container[a].clone()
            } else {
                Block::paragraph("")
            };
            if i > 0 && container[a].kind == "list" {
                block.kind = "list".into();
                block.attrs = container[a].attrs.clone();
                if block.string("list") == "check" {
                    block.attrs.insert("checked".into(), json!(false));
                }
            }
            if i > 0 && container[a].kind == "quote" {
                block.kind = "quote".into();
            }
            block.spans = if i == 0 { left.clone() } else { vec![] };
            if !part.is_empty() {
                block.spans.push(self.typing.with_text(part));
            }
            if i + 1 == parts.len() {
                block.spans.extend(right.clone());
            }
            blocks.push(block);
        }
        let last = blocks.last().unwrap();
        let caret = Position {
            block: last.id.clone(),
            offset: if parts.len() == 1 {
                start.offset + text.len()
            } else {
                parts.last().unwrap().len()
            },
        };
        container.splice(a..=b, blocks);
        self.selection = Selection {
            anchor: caret.clone(),
            head: caret,
        };
        Ok(())
    }
    pub fn insert(&mut self, text: &str, time_ms: u64) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        let before = self.snapshot();
        let range = self.range();
        let typing =
            range.is_empty() && !text.contains(['\n', '\r']) && text.graphemes(true).count() == 1;
        let coalesce = typing
            && self.last_typing.is_some_and(|(last, end)| {
                time_ms.saturating_sub(last) < 650 && end == range.start
            });
        self.replace(range, text)?;
        self.last_typing = typing.then_some((time_ms, self.global(&self.selection.head)));
        self.record(before, coalesce);
        if typing && text == " " {
            self.apply_typed_marker();
        }
        Ok(())
    }
    /// A typed space after a marker such as `*`, `1.`, `[]` or `##` at the start of
    /// a paragraph applies the matching block style and removes the marker. The
    /// conversion is its own history step, so Undo first restores the marker.
    fn apply_typed_marker(&mut self) {
        let head = self.selection.head.clone();
        let Some(block) = self.document.block(&head.block) else {
            return;
        };
        let text = block.text();
        let Some(&(marker, style)) = TYPED_MARKERS
            .iter()
            .find(|(marker, _)| head.offset == marker.len() + 1 && text.starts_with(marker))
        else {
            return;
        };
        if text.as_bytes()[marker.len()] != b' ' || (style == "quote" && !self.quotes_enabled) {
            return;
        }
        // Brackets also turn a list item into a checklist item; nothing else
        // converts out of a styled block.
        let convertible = match block.kind.as_str() {
            "paragraph" => true,
            "list" => style == "check" && block.string("list") != "check",
            _ => false,
        };
        if !convertible {
            return;
        }
        let indent = block.number("indent", 0);
        let before = self.snapshot();
        let block = self.document.block_mut(&head.block).unwrap();
        block.spans = slice_spans(&block.spans, marker.len() + 1..text.len());
        let was_list = block.kind == "list";
        block.set_kind(style);
        if was_list && indent > 0 {
            block.attrs.insert("indent".into(), json!(indent));
        }
        if style == "quote" {
            self.document.schema = 2;
        }
        // Set the caret directly, as `replace` does, to keep the typing format.
        let start = Position {
            block: head.block,
            offset: 0,
        };
        self.selection = Selection {
            anchor: start.clone(),
            head: start,
        };
        self.last_typing = None;
        self.record(before, false);
        self.marker_undo = Some(self.revision);
    }
    pub fn enter(&mut self, soft: bool) -> Result<()> {
        if self.marked.is_some() {
            return Ok(());
        }
        let before = self.snapshot();
        let id = self.selection.head.block.clone();
        let collapsed = self.range().is_empty();
        let b = self.document.block_mut(&id).unwrap();
        if !soft && b.kind == "quote" && b.text().is_empty() && collapsed {
            b.set_kind("paragraph");
        } else if !soft && b.kind == "list" && b.text().is_empty() {
            leave_block(b);
        } else if soft {
            let range = self.range();
            let start = self.position(range.start);
            let end = self.position(range.end);
            ensure!(
                start.block == end.block,
                "Soft breaks require a selection within one paragraph."
            );
            let b = self.document.block_mut(&start.block).unwrap();
            let mut spans = slice_spans(&b.spans, 0..start.offset);
            spans.push(self.typing.with_text("\n"));
            spans.extend(slice_spans(&b.spans, end.offset..b.text().len()));
            b.spans = spans;
            self.select(range.start + 1, false);
        } else {
            self.replace(self.range(), "\n")?;
        }
        self.last_typing = None;
        self.record(before, false);
        Ok(())
    }
    pub fn delete(&mut self, forward: bool) -> Result<()> {
        if self.marked.is_some() {
            return Ok(());
        }
        if !forward
            && self.range().is_empty()
            && self.marker_undo == Some(self.revision)
            && self.history.can_undo()
        {
            // Backspace right after a typed marker was applied takes it back and
            // restores the typed text, as in Google Docs and ProseMirror editors.
            self.marker_undo = None;
            self.undo();
            return Ok(());
        }
        let before = self.snapshot();
        if !forward && self.range().is_empty() {
            // Backspace at the start of a list item or quote leaves it instead
            // of joining the previous block, as in other rich text editors.
            let head = &self.selection.head;
            if head.offset == 0
                && let Some(block) = self.document.block_mut(&head.block)
                && matches!(block.kind.as_str(), "list" | "quote")
            {
                leave_block(block);
                self.last_typing = None;
                self.record(before, false);
                return Ok(());
            }
        }
        if self.range().is_empty() {
            self.move_horizontal(forward, true, false);
        }
        if let Err(error) = self.replace(self.range(), "") {
            self.restore(before);
            return Err(error);
        }
        self.last_typing = None;
        self.record(before, false);
        Ok(())
    }
    pub fn mark(&mut self, mark: &str) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        ensure!(
            notes_core::document::MARKS.contains(&mark),
            "Unknown text format."
        );
        let mut range = self.range();
        // Match the established editor: inside a word format the word, while a
        // boundary changes the format for subsequent typing. Keep the caret.
        if range.is_empty() {
            let block = self.document.block(&self.selection.head.block).unwrap();
            let local = self.selection.head.offset;
            let text = block.text();
            if let Some((start, word)) = text
                .unicode_word_indices()
                .find(|(start, word)| *start < local && local < *start + word.len())
            {
                let base = range.start - local;
                range = base + start..base + start + word.len();
            }
        }
        if range.is_empty() {
            let enabled = !self.typing.mark(mark);
            self.typing.set_mark(mark, enabled);
            self.last_typing = None;
            return Ok(());
        }
        let before = self.snapshot();
        let mut offset = 0;
        let mut selections = vec![];
        let mut all = true;
        for b in self.document.leaves() {
            let end = offset + b.text().len();
            if range.start < end && range.end > offset {
                let r = range.start.saturating_sub(offset)..range.end.min(end) - offset;
                all &= slice_spans(&b.spans, r.clone())
                    .iter()
                    .filter(|s| s.is_text())
                    .all(|s| s.mark(mark));
                selections.push((b.id.clone(), r));
            }
            offset = end + 1;
        }
        for (id, r) in selections {
            let b = self.document.block_mut(&id).unwrap();
            let mut spans = slice_spans(&b.spans, 0..r.start);
            spans.extend(slice_spans(&b.spans, r.clone()).into_iter().map(|mut s| {
                s.set_mark(mark, !all);
                s
            }));
            spans.extend(slice_spans(&b.spans, r.end..b.text().len()));
            b.spans = spans;
        }
        if self.range().is_empty() {
            self.typing.set_mark(mark, !all);
        }
        self.last_typing = None;
        self.record(before, false);
        Ok(())
    }
    pub fn mark_active(&self, mark: &str) -> bool {
        self.mark_state(mark) == Some(true)
    }
    /// None means a mixed selection; objects do not contribute a text mark.
    pub fn mark_state(&self, mark: &str) -> Option<bool> {
        let range = self.range();
        if range.is_empty() {
            return Some(self.typing.mark(mark));
        }
        let mut offset = 0;
        let mut state = None;
        for block in self.document.leaves() {
            let end = offset + block.text().len();
            if range.start < end && range.end > offset {
                for span in slice_spans(
                    &block.spans,
                    range.start.saturating_sub(offset)..range.end.min(end) - offset,
                ) {
                    if span.is_text() {
                        let marked = span.mark(mark);
                        if state.is_some_and(|old| old != marked) {
                            return None;
                        }
                        state = Some(marked);
                    }
                }
            }
            offset = end + 1;
        }
        Some(state.unwrap_or(false))
    }
    pub fn style_active(&self, style: &str) -> bool {
        if self.selection.anchor == self.selection.head {
            return self
                .document
                .block(&self.selection.head.block)
                .is_some_and(|b| block_style(b, style));
        }
        let range = self.range();
        let mut offset = 0;
        let mut found = false;
        for block in self.document.leaves() {
            let end = offset + block.text().len();
            if (range.is_empty() && block.id == self.selection.head.block)
                || (range.start <= end && range.end > offset)
            {
                found = true;
                if !block_style(block, style) {
                    return false;
                }
            }
            offset = end + 1;
        }
        found
    }
    pub fn style(&mut self, kind: &str) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        ensure!(
            matches!(
                kind,
                "paragraph" | "h1" | "h2" | "h3" | "bullet" | "ordered" | "check" | "quote"
            ),
            "Unknown paragraph style."
        );
        let before = self.snapshot();
        let range = self.range();
        let mut offset = 0;
        let mut ids = vec![];
        for b in self.document.leaves() {
            let end = offset + b.text().len();
            if (range.is_empty() && b.id == self.selection.head.block)
                || (range.start <= end && range.end > offset)
            {
                ids.push(b.id.clone());
            }
            offset = end + 1;
        }
        for id in ids {
            let block = self.document.block_mut(&id).unwrap();
            if !block_style(block, kind) {
                block.set_kind(kind);
            }
        }
        if kind == "quote" {
            self.document.schema = 2;
        }
        self.last_typing = None;
        self.record(before, false);
        Ok(())
    }
    pub fn toggle_check(&mut self, id: &str) {
        if self.marked.is_some() {
            return;
        }
        let before = self.snapshot();
        if let Some(b) = self.document.block_mut(id)
            && b.string("list") == "check"
        {
            b.attrs.insert("checked".into(), json!(!b.checked()));
        }
        self.record(before, false);
    }
    pub fn indent(&mut self, outdent: bool) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        let before = self.snapshot();
        let b = self.document.block_mut(&self.selection.head.block).unwrap();
        if b.kind == "list" {
            let n = b.number("indent", 0);
            b.attrs.insert(
                "indent".into(),
                json!(if outdent {
                    n.saturating_sub(1)
                } else {
                    (n + 1).min(16)
                }),
            );
            self.record(before, false);
        } else {
            let leaves = self.document.leaves();
            let i = leaves
                .iter()
                .position(|b| b.id == self.selection.head.block)
                .unwrap();
            let target = if outdent {
                i.saturating_sub(1)
            } else {
                (i + 1).min(leaves.len() - 1)
            };
            let p = Position {
                block: leaves[target].id.clone(),
                offset: 0,
            };
            self.select(self.global(&p), false);
        }
        Ok(())
    }
    pub fn undo(&mut self) {
        if self.composition.is_some() {
            self.cancel_composition();
            return;
        }
        if self.history.can_undo()
            && let Some(s) = self.history.undo(self.snapshot())
        {
            self.restore(s);
            self.revision += 1;
            self.last_typing = None;
        }
    }
    pub fn redo(&mut self) {
        if self.marked.is_some() {
            return;
        }
        if self.history.can_redo()
            && let Some(s) = self.history.redo(self.snapshot())
        {
            self.restore(s);
            self.revision += 1;
            self.last_typing = None;
        }
    }
    pub fn can_undo(&self) -> bool {
        self.composition.is_some() || self.history.can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.marked.is_none() && self.history.can_redo()
    }
    pub fn history_usage(&self) -> HistoryUsage {
        self.history.usage()
    }
    pub fn typing_state(&self) -> TypingState {
        TypingState(self.typing.clone())
    }
    /// Restore after restoring the matching document selection. This ends the
    /// previous typing group without changing the document, history or preedit.
    pub fn restore_typing_state(&mut self, state: TypingState) {
        self.typing = state.0;
        self.last_typing = None;
    }
    pub fn input(
        &mut self,
        utf16: Option<Range<usize>>,
        text: &str,
        marked_selection: Option<Option<Range<usize>>>,
    ) -> Result<()> {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = normalized.as_str();
        let range = utf16
            .map(|r| {
                let plain = self.text();
                TextField::byte_in(&plain, r.start)..TextField::byte_in(&plain, r.end)
            })
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.range());
        let before = self.composition.clone().unwrap_or_else(|| self.snapshot());
        self.replace(range.clone(), text)?;
        if let Some(selected) = marked_selection {
            if self.composition.is_none() {
                self.composition = Some(before);
            }
            self.marked = Some(range.start..range.start + text.len());
            if let Some(r) = selected {
                self.select(range.start + TextField::byte_in(text, r.start), false);
                self.select(range.start + TextField::byte_in(text, r.end), true);
            }
        } else {
            self.marked = None;
            self.composition = None;
            self.last_typing = None;
            self.record(before, false);
        }
        Ok(())
    }
    pub fn commit_composition(&mut self) {
        if let Some(before) = self.composition.take() {
            self.marked = None;
            self.record(before, false);
        }
    }
    pub fn cancel_composition(&mut self) {
        if let Some(s) = self.composition.take() {
            self.restore(s);
        }
    }
    pub fn fragment(&self) -> Result<Vec<Block>> {
        let range = self.range();
        if range.is_empty() {
            return Ok(vec![]);
        }
        if range == (0..self.text().len()) {
            return Ok(self.document.blocks.clone());
        }
        fn selected(blocks: &[Block], range: &Range<usize>, offset: &mut usize) -> Vec<Block> {
            let mut out = vec![];
            for block in blocks {
                let mut copy = block.clone();
                if block.kind == "table" {
                    let mut included = false;
                    for cell in &mut copy.cells {
                        cell.blocks = selected(&cell.blocks, range, offset);
                        included |= !cell.blocks.is_empty();
                        if cell.blocks.is_empty() {
                            cell.blocks.push(Block::paragraph(""));
                        }
                    }
                    if included {
                        out.push(copy);
                    }
                } else {
                    let end = *offset + block.text().len();
                    if range.start <= end && range.end > *offset {
                        copy.spans = slice_spans(
                            &block.spans,
                            range.start.saturating_sub(*offset)..range.end.min(end) - *offset,
                        );
                        out.push(copy);
                    }
                    *offset = end + 1;
                }
            }
            out
        }
        // A selection wholly inside one cell copies its paragraphs, not the containing table.
        let start = self.position(range.start);
        let end = self.position(range.end);
        let mut doc = self.document.clone();
        let container = doc.container_mut(&start.block).unwrap();
        if container.iter().any(|b| b.id == end.block) {
            let document = Document {
                blocks: container.clone(),
                ..Default::default()
            };
            if let Some(first) = document.leaves().first() {
                let mut offset = self.global(&Position {
                    block: first.id.clone(),
                    offset: 0,
                });
                return Ok(selected(container, &range, &mut offset));
            }
        }
        Ok(selected(&self.document.blocks, &range, &mut 0))
    }

    pub fn paste_fragment(&mut self, mut blocks: Vec<Block>) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        if blocks.is_empty() {
            return Ok(());
        }
        let doc = Document::from_blocks(blocks.clone());
        doc.validate()?;
        let before = self.snapshot();
        self.replace(self.range(), "")?;
        let p = self.selection.head.clone();
        let container = self.document.container_mut(&p.block).unwrap();
        let index = container.iter().position(|b| b.id == p.block).unwrap();
        let original = &container[index];
        let mut suffix = original.clone();
        suffix.id = new_id();
        let left = slice_spans(&original.spans, 0..p.offset);
        let right = slice_spans(&original.spans, p.offset..original.text().len());
        fn renew(blocks: &mut [Block]) {
            for b in blocks {
                b.id = new_id();
                for c in &mut b.cells {
                    renew(&mut c.blocks);
                }
            }
        }
        renew(&mut blocks);
        if blocks[0].kind == "table"
            || (blocks[0].kind == "quote" && original.kind != "quote" && !left.is_empty())
        {
            if !left.is_empty() {
                let mut prefix = original.clone();
                prefix.spans = left;
                blocks.insert(0, prefix);
            }
        } else {
            blocks[0].id = p.block.clone();
            if !left.is_empty() || (blocks.len() == 1 && blocks[0].kind == "paragraph") {
                blocks[0].kind = original.kind.clone();
                blocks[0].attrs = original.attrs.clone();
            }
            let mut spans = left;
            spans.append(&mut blocks[0].spans);
            blocks[0].spans = spans;
        }
        if blocks.last().unwrap().kind == "table" {
            blocks.push(Block::paragraph(""));
        }
        let last = blocks.last_mut().unwrap();
        let caret = Position {
            block: last.id.clone(),
            offset: last.text().len(),
        };
        if last.kind == "quote" && original.kind != "quote" && !right.is_empty() {
            suffix.spans = right;
            blocks.push(suffix);
        } else {
            last.spans.extend(right);
        }
        container.splice(index..=index, blocks);
        self.document.schema = self.document.schema.max(doc.schema);
        self.selection = Selection {
            anchor: caret.clone(),
            head: caret,
        };
        if let Err(error) = self.document.validate() {
            self.restore(before);
            return Err(error);
        }
        self.record(before, false);
        Ok(())
    }
}

pub fn clip(s: &str, byte: usize) -> usize {
    let mut i = byte.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
pub fn slice_spans(spans: &[Span], range: Range<usize>) -> Vec<Span> {
    let mut offset = 0;
    let mut out = vec![];
    for s in spans {
        let text = s.content();
        let end = offset + text.len();
        if range.start < end && range.end > offset {
            if s.is_text() {
                let a = clip(text, range.start.saturating_sub(offset));
                let b = clip(text, range.end.min(end) - offset);
                if a < b {
                    out.push(s.with_text(&text[a..b]));
                }
            } else {
                out.push(s.clone());
            }
        }
        offset = end;
    }
    out
}

/// Markers that start a block style when followed by a typed space. Numbered
/// lists have no start number, so only `1.` and `1)` qualify. A quote also needs
/// `Editor::quotes_enabled`.
const TYPED_MARKERS: [(&str, &str); 10] = [
    ("*", "bullet"),
    ("-", "bullet"),
    ("1.", "ordered"),
    ("1)", "ordered"),
    ("[]", "check"),
    ("[ ]", "check"),
    ("#", "h1"),
    ("##", "h2"),
    ("###", "h3"),
    (">", "quote"),
];

/// One step out of a list or quote: outdent a nested list item, otherwise make
/// the block a paragraph.
fn leave_block(block: &mut Block) {
    let indent = block.number("indent", 0);
    if block.kind == "list" && indent > 0 {
        block.attrs.insert("indent".into(), json!(indent - 1));
    } else {
        block.set_kind("paragraph");
    }
}

fn block_style(block: &Block, style: &str) -> bool {
    match style {
        "h1" | "h2" | "h3" => {
            block.kind == "heading"
                && block.number("level", 0) == style[1..].parse::<usize>().unwrap()
        }
        "bullet" | "ordered" | "check" => block.kind == "list" && block.string("list") == style,
        _ => block.kind == style,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typing_state_recreation_keeps_inherited_and_explicit_boundary_styles() {
        let span = Span(json!({
            "text": "Styled words",
            "marks": ["bold", "italic"],
            "future-style": {"color": "warm", "version": 3}
        }));
        let mut block = Block::paragraph("");
        block.spans.push(span.clone());
        let document = Document {
            blocks: vec![block],
            ..Default::default()
        };
        for boundary in [false, true] {
            let mut original = Editor::new(document.clone());
            original.select(if boundary { original.text().len() } else { 3 }, false);
            if boundary {
                original.mark("bold").unwrap();
                assert_eq!(
                    original.document, document,
                    "a boundary toggle is not a body edit"
                );
                assert!(!original.mark_active("bold"));
            }
            let saved = original.typing_state();
            let mut reopened = Editor::new(document.clone());
            reopened.selection = original.selection.clone();
            let history = reopened.history_usage();
            reopened.restore_typing_state(saved.clone());
            assert_eq!(reopened.typing_state(), saved);
            assert_eq!(reopened.document, document);
            assert_eq!(reopened.revision, 0);
            assert_eq!(reopened.history_usage(), history);
            assert!(reopened.marked.is_none());
            reopened.insert("X", 0).unwrap();
            let inserted = reopened.document.blocks[0]
                .spans
                .iter()
                .find(|s| s.content().contains('X'))
                .unwrap();
            assert_eq!(inserted.mark("bold"), !boundary);
            assert!(inserted.mark("italic"));
            assert_eq!(inserted.0["future-style"], span.0["future-style"]);
            reopened.undo();
            assert_eq!(reopened.document, document);
        }
    }
    #[test]
    fn typing_state_recreation_keeps_empty_caret_choices() {
        let mut original = Editor::new(Document::default());
        original.mark("bold").unwrap();
        original.mark("italic").unwrap();
        let mut reopened = Editor::new(original.document.clone());
        reopened.selection = original.selection.clone();
        reopened.restore_typing_state(original.typing_state());
        assert_eq!(reopened.revision, 0);
        assert!(!reopened.can_undo());
        reopened.input(None, "First words", None).unwrap();
        let inserted = &reopened.document.blocks[0].spans[0];
        assert!(inserted.mark("bold") && inserted.mark("italic"));
    }
    #[test]
    fn typing_restore_ends_coalescence_without_changing_history_or_composition() {
        let mut e = Editor::new(Document::default());
        e.insert("a", 100).unwrap();
        let state = e.typing_state();
        let before = (
            e.document.clone(),
            e.selection.clone(),
            e.revision,
            e.history_usage(),
        );
        e.restore_typing_state(state.clone());
        assert_eq!(
            (
                e.document.clone(),
                e.selection.clone(),
                e.revision,
                e.history_usage()
            ),
            before
        );
        e.insert("b", 101).unwrap();
        e.undo();
        assert_eq!(e.text(), "a", "restoration begins a new typing group");
        e.input(None, "に", Some(None)).unwrap();
        let before = (
            e.document.clone(),
            e.selection.clone(),
            e.revision,
            e.history_usage(),
            e.marked.clone(),
            e.committed().clone(),
        );
        e.restore_typing_state(state);
        assert_eq!(
            (
                e.document.clone(),
                e.selection.clone(),
                e.revision,
                e.history_usage(),
                e.marked.clone(),
                e.committed().clone()
            ),
            before
        );
        e.cancel_composition();
        assert_eq!(
            e.text(),
            "a",
            "restoration leaves the composition rollback intact"
        );
    }
    #[test]
    fn rich_cross_block_edit_undo_and_stable_ids() {
        let mut e = Editor::new(Document::default());
        let id = e.selection.head.block.clone();
        e.insert("Title", 0).unwrap();
        e.style("h1").unwrap();
        e.enter(false).unwrap();
        e.insert("body 👩‍💻e\u{301}", 1).unwrap();
        assert_eq!(e.document.blocks[1].kind, "paragraph");
        e.delete(false).unwrap();
        assert!(e.text().ends_with("👩‍💻"));
        e.undo();
        assert!(e.text().ends_with("e\u{301}"));
        e.select(2, false);
        e.select(8, true);
        e.mark("bold").unwrap();
        assert!(e.document.blocks[0].spans.iter().any(|s| s.mark("bold")));
        e.insert("X", 2).unwrap();
        assert_eq!(e.document.blocks.len(), 1);
        assert_eq!(e.document.blocks[0].id, id);
        e.undo();
        assert_eq!(e.document.blocks.len(), 2);
        e.document.validate().unwrap();
    }
    #[test]
    fn checklists_continue_unchecked_and_empty_item_exits() {
        let mut e = Editor::new(Document::default());
        e.style("check").unwrap();
        e.insert("Task", 0).unwrap();
        let id = e.selection.head.block.clone();
        e.toggle_check(&id);
        e.enter(false).unwrap();
        assert!(!e.document.blocks[1].checked());
        e.enter(false).unwrap();
        assert_eq!(e.document.blocks[1].kind, "paragraph");
    }
    #[test]
    fn composition_is_not_committed_and_is_one_undo_group() {
        let mut e = Editor::new(Document::default());
        e.insert("Hello ", 0).unwrap();
        let rev = e.revision;
        e.input(None, "に", Some(None)).unwrap();
        e.input(None, "日本", Some(Some(1..2))).unwrap();
        assert_eq!(e.committed().plain_text(), "Hello ");
        assert_eq!(e.revision, rev);
        e.enter(false).unwrap();
        assert_eq!(e.document.blocks.len(), 1);
        e.input(None, "日本語", None).unwrap();
        assert_eq!(e.text(), "Hello 日本語");
        e.undo();
        assert_eq!(e.text(), "Hello ");
        e.redo();
        assert_eq!(e.text(), "Hello 日本語");
        e.input(None, "x", Some(None)).unwrap();
        e.cancel_composition();
        assert_eq!(e.text(), "Hello 日本語");
    }
    #[test]
    fn rich_paste_keeps_marks_and_refreshes_ids() {
        let mut e = Editor::new(Document::default());
        e.insert("hello\nworld", 0).unwrap();
        e.select_all();
        e.mark("italic").unwrap();
        let fragment = e.fragment().unwrap();
        let old = fragment[1].id.clone();
        e.select(e.text().len(), false);
        e.paste_fragment(fragment).unwrap();
        assert!(e.document.blocks.last().unwrap().spans[0].mark("italic"));
        assert_ne!(e.document.blocks.last().unwrap().id, old);
        e.document.validate().unwrap();
    }
    #[test]
    fn table_edits_preserve_objects_and_reject_destructive_cross_cell_ranges() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/qt-v1/documents.json"))
                .unwrap();
        let document = Document::from_json(&fixtures[1]["body"].to_string()).unwrap();
        let mut e = Editor::new(document.clone());
        let table = document.blocks.iter().find(|b| b.kind == "table").unwrap();
        let cell = &table.cells[3].blocks[0];
        let start = e.global(&Position {
            block: cell.id.clone(),
            offset: 0,
        });
        e.select(start, false);
        e.insert("Dr. ", 0).unwrap();
        assert_eq!(e.document.block(&cell.id).unwrap().text(), "Dr. Dana");
        assert_eq!(e.document.references("image"), document.references("image"));
        assert_eq!(
            e.document.references("attachment"),
            document.references("attachment")
        );
        let before = e.document.clone();
        let other = e.global(&Position {
            block: table.cells[4].blocks[0].id.clone(),
            offset: 1,
        });
        e.select(other, true);
        assert!(e.insert("replacement", 0).is_err());
        assert_eq!(e.document, before);
        assert!(e.input(None, "日本", Some(None)).is_err());
        assert!(e.composition.is_none());
        e.undo();
        assert_eq!(e.document, document);
    }
    #[test]
    fn mixed_document_copy_paste_cut_and_undo_preserve_tables_and_objects() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/qt-v1/documents.json"))
                .unwrap();
        let doc = Document::from_json(&fixtures[1]["body"].to_string()).unwrap();
        let mut source = Editor::new(doc.clone());
        source.select_all();
        assert_eq!(source.fragment().unwrap(), doc.blocks);
        let mut target = Editor::new(Document::default());
        target.paste_fragment(source.fragment().unwrap()).unwrap();
        target.document.validate().unwrap();
        assert_eq!(target.document.references("image"), doc.references("image"));
        assert_eq!(
            target.document.references("attachment"),
            doc.references("attachment")
        );
        assert_eq!(target.text(), source.text());
        let before = target.document.clone();
        target.select_all();
        target.insert("", 0).unwrap();
        assert!(target.text().is_empty());
        target.undo();
        assert_eq!(target.document, before);
        target.undo();
        assert!(target.text().is_empty());
        target.redo();
        assert_eq!(target.document, before);
        // A leading table must not shift the global range used to clip later paragraphs.
        let mut leading = doc.clone();
        leading.blocks.retain(|b| b.kind == "table");
        leading.blocks.push(Block::paragraph("after the table"));
        let mut e = Editor::new(leading);
        let start = e.text().len() - "after the table".len();
        e.select(start, false);
        e.select(start + 5, true);
        assert_eq!(e.fragment().unwrap()[0].text(), "after");
        e.select_all();
        e.insert("replacement", 0).unwrap();
        assert_eq!(e.text(), "replacement");
        e.undo();
        assert_eq!(e.document.blocks[0].kind, "table");
    }
    #[test]
    fn inline_object_is_atomic_and_undoable() {
        let mut block = Block::paragraph("before");
        block
            .spans
            .push(Span(json!({"image":"a".repeat(64),"alt":"Picture"})));
        block.spans.push(Span::text("after"));
        let doc = Document {
            blocks: vec![block],
            ..Default::default()
        };
        let mut e = Editor::new(doc.clone());
        e.select(6, false);
        e.move_horizontal(true, true, false);
        assert_eq!(e.range(), 6..9);
        assert_eq!(e.fragment().unwrap()[0].spans.len(), 1);
        e.insert("", 0).unwrap();
        assert_eq!(e.text(), "beforeafter");
        e.undo();
        assert_eq!(e.document, doc);
    }
}
