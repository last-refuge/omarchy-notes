//! Combined undo/redo history with a strict allocation-payload budget.
use super::{Selection, Snapshot};
use notes_core::document::{DocumentSnapshot, SpanSnapshot};
use std::mem::size_of;

pub const MAX_HISTORY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_HISTORY_STEPS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryUsage {
    pub allocated_bytes: usize,
    pub undo_steps: usize,
    pub redo_steps: usize,
    pub limit_reached: bool,
}

struct Entry {
    document: DocumentSnapshot,
    selection: Selection,
    typing: SpanSnapshot,
    bytes: usize,
}

impl From<Snapshot> for Entry {
    fn from(snapshot: Snapshot) -> Self {
        let document = DocumentSnapshot::from(snapshot.document);
        let typing = SpanSnapshot::from(snapshot.typing);
        let selection = snapshot.selection;
        let bytes = document.allocated_bytes()
            + typing.allocated_bytes()
            + selection.anchor.block.capacity()
            + selection.head.block.capacity();
        Self {
            document,
            selection,
            typing,
            bytes,
        }
    }
}

impl Entry {
    fn restore(self) -> Snapshot {
        Snapshot {
            document: self.document.into_document(),
            selection: self.selection,
            typing: self.typing.into_span(),
        }
    }
}

pub(super) struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    payload_bytes: usize,
    byte_limit: usize,
    step_limit: usize,
    limit_reached: bool,
}

impl Default for History {
    fn default() -> Self {
        Self::new(MAX_HISTORY_BYTES, MAX_HISTORY_STEPS)
    }
}

impl History {
    fn new(byte_limit: usize, step_limit: usize) -> Self {
        let history = Self {
            undo: Vec::with_capacity(step_limit),
            redo: Vec::with_capacity(step_limit),
            payload_bytes: 0,
            byte_limit,
            step_limit,
            limit_reached: false,
        };
        assert!(history.buffer_bytes() <= byte_limit);
        history
    }

    fn buffer_bytes(&self) -> usize {
        (self.undo.capacity() + self.redo.capacity()) * size_of::<Entry>()
    }

    pub(super) fn usage(&self) -> HistoryUsage {
        HistoryUsage {
            allocated_bytes: self.buffer_bytes() + self.payload_bytes,
            undo_steps: self.undo.len(),
            redo_steps: self.redo.len(),
            limit_reached: self.limit_reached,
        }
    }

    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(super) fn clear_redo(&mut self) {
        self.payload_bytes -= self.redo.iter().map(|entry| entry.bytes).sum::<usize>();
        self.redo.clear();
    }

    fn make_room(&mut self, bytes: usize) -> bool {
        if bytes > self.byte_limit - self.buffer_bytes() || self.step_limit == 0 {
            // Retaining earlier states across a missing step would make Undo
            // skip edits. Both chains end at this explicit boundary.
            self.undo.clear();
            self.redo.clear();
            self.payload_bytes = 0;
            self.limit_reached = true;
            return false;
        }
        while self.undo.len() + self.redo.len() >= self.step_limit
            || self.usage().allocated_bytes + bytes > self.byte_limit
        {
            // The oldest undo and most distant redo are the ends of the
            // navigable chain. Never remove an interior step.
            let entry = if !self.undo.is_empty() {
                self.undo.remove(0)
            } else {
                self.redo.remove(0)
            };
            self.payload_bytes -= entry.bytes;
            self.limit_reached = true;
        }
        true
    }

    pub(super) fn record(&mut self, before: Snapshot, coalesce: bool) -> bool {
        self.clear_redo();
        if coalesce && self.can_undo() {
            return true;
        }
        let entry = Entry::from(before);
        if !self.make_room(entry.bytes) {
            return false;
        }
        self.payload_bytes += entry.bytes;
        self.undo.push(entry);
        true
    }

    pub(super) fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let entry = self.undo.pop()?;
        self.payload_bytes -= entry.bytes;
        let current = Entry::from(current);
        if self.make_room(current.bytes) {
            self.payload_bytes += current.bytes;
            self.redo.push(current);
        }
        Some(entry.restore())
    }

    pub(super) fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let entry = self.redo.pop()?;
        self.payload_bytes -= entry.bytes;
        let current = Entry::from(current);
        if self.make_room(current.bytes) {
            self.payload_bytes += current.bytes;
            self.undo.push(current);
        }
        Some(entry.restore())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;
    use notes_core::document::{Block, Document};

    fn editor(byte_limit: usize, step_limit: usize) -> Editor {
        let mut editor = Editor::new(Document::from_blocks(vec![Block::paragraph("start")]));
        editor.history = History::new(byte_limit, step_limit);
        editor
    }

    fn bounded(editor: &Editor) {
        let usage = editor.history_usage();
        assert!(usage.allocated_bytes <= editor.history.byte_limit);
        assert!(usage.undo_steps + usage.redo_steps <= editor.history.step_limit);
        assert_eq!(
            editor.history.payload_bytes,
            editor
                .history
                .undo
                .iter()
                .chain(&editor.history.redo)
                .map(|entry| entry.bytes)
                .sum::<usize>()
        );
    }

    #[test]
    fn recent_history_and_redo_share_count_and_byte_budget() {
        let mut editor = editor(12_000, 8);
        for i in 0..20 {
            editor.insert("é", i * 1000).unwrap();
            bounded(&editor);
        }
        let final_text = editor.text();
        let steps = editor.history_usage().undo_steps;
        assert!((1..=8).contains(&steps));
        assert!(editor.history_usage().limit_reached);
        for _ in 0..steps {
            editor.undo();
            bounded(&editor);
        }
        assert!(!editor.can_undo());
        for _ in 0..steps {
            editor.redo();
            bounded(&editor);
        }
        assert_eq!(editor.text(), final_text);
        editor.undo();
        editor.insert("X", 30_000).unwrap();
        assert!(!editor.can_redo());
        bounded(&editor);
    }

    #[test]
    fn oversized_snapshot_ends_both_chains_without_skipping_edits() {
        let mut editor = editor(4096, 4);
        editor.insert("old", 0).unwrap();
        editor.insert(&"x".repeat(10_000), 1000).unwrap();
        assert!(editor.can_undo());
        // Undo itself can succeed although the oversized current document
        // cannot be retained for Redo.
        editor.undo();
        assert!(!editor.can_redo());
        assert!(!editor.can_undo());
        assert_eq!(editor.text(), "oldstart");
        bounded(&editor);
        editor.insert(&"x".repeat(10_000), 2000).unwrap();
        editor.insert("Y", 3000).unwrap();
        assert!(!editor.can_undo());
        assert!(editor.history_usage().limit_reached);
        let text = editor.text();
        editor.undo();
        assert_eq!(editor.text(), text);
        bounded(&editor);
    }

    #[test]
    fn composition_cancel_commit_and_typing_coalescing_preserve_bounds() {
        let mut editor = editor(8000, 8);
        for i in 0..100 {
            editor.insert("x", i * 5).unwrap();
            bounded(&editor);
        }
        assert_eq!(editor.history_usage().undo_steps, 1);
        let committed = editor.document.clone();
        editor.input(None, "日本", Some(Some(2..2))).unwrap();
        assert_eq!(editor.committed(), &committed);
        editor.undo(); // Cancels preedit rather than consuming retained undo.
        assert_eq!(editor.document, committed);
        assert_eq!(editor.history_usage().undo_steps, 1);
        editor.input(None, "日本", Some(Some(2..2))).unwrap();
        editor.input(None, "日本語", Some(Some(3..3))).unwrap();
        editor.commit_composition();
        assert_eq!(editor.history_usage().undo_steps, 2);
        editor.undo();
        assert_eq!(editor.document, committed);
        editor.redo();
        assert!(editor.text().contains("日本語"));
        bounded(&editor);
    }

    #[test]
    fn growing_coalesced_document_cannot_overflow_redo_budget() {
        let mut editor = editor(4096, 4);
        for i in 0..12_000 {
            editor.insert("x", i).unwrap();
        }
        assert_eq!(editor.history_usage().undo_steps, 1);
        editor.undo();
        assert_eq!(editor.text(), "start");
        assert!(!editor.can_redo());
        bounded(&editor);
    }
}
