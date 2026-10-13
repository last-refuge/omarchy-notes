//! Presentation-only literal search. Byte ranges address the same text as Editor.
use notes_core::document::Document;
use regex::{Regex, RegexBuilder};
use std::{ops::Range, sync::Arc};

const MAX_MATCHES: usize = 100_000;
#[derive(Default)]
pub struct Find {
    pub query: String,
    pub matches: Arc<[Range<usize>]>,
    pub current: Option<usize>,
    pub truncated: bool,
    pub error: Option<String>,
    pattern: Option<Regex>,
}
impl Find {
    pub fn set_query(&mut self, document: &Document, query: String, caret: usize) {
        self.query = query;
        self.current = None;
        self.pattern = None;
        self.error = None;
        if self.query.len() > 4096 {
            self.error = Some("Use a shorter search (up to 4 KiB).".into());
        } else if !self.query.is_empty() {
            match RegexBuilder::new(&regex::escape(&self.query))
                .case_insensitive(true)
                .build()
            {
                Ok(pattern) => self.pattern = Some(pattern),
                Err(_) => {
                    self.error = Some("This search is too complex. Use a shorter phrase.".into())
                }
            }
        }
        self.refresh(document);
        self.current = self
            .matches
            .iter()
            .position(|r| r.start >= caret)
            .or_else(|| (!self.matches.is_empty()).then_some(0));
    }
    /// Editing recomputes hits but never moves the user's caret.
    pub fn refresh(&mut self, document: &Document) {
        let previous = self.selected().cloned();
        let mut matches = Vec::new();
        self.truncated = false;
        let mut offset = 0;
        if let Some(pattern) = &self.pattern {
            for block in document.leaves() {
                let text = block.text();
                for hit in pattern.find_iter(&text) {
                    if matches.len() == MAX_MATCHES {
                        self.truncated = true;
                        break;
                    }
                    matches.push(offset + hit.start()..offset + hit.end());
                }
                if self.truncated {
                    break;
                }
                offset += text.len() + 1;
            }
        }
        self.matches = matches.into();
        self.current = previous.and_then(|r| self.matches.iter().position(|hit| *hit == r));
    }
    pub fn selected(&self) -> Option<&Range<usize>> {
        self.current.and_then(|i| self.matches.get(i))
    }
    pub fn step(&mut self, backward: bool, caret: usize) -> Option<Range<usize>> {
        let count = self.matches.len();
        if count == 0 {
            return None;
        }
        self.current = Some(match self.current {
            Some(i) if backward => (i + count - 1) % count,
            Some(i) => (i + 1) % count,
            None if backward => self
                .matches
                .iter()
                .rposition(|r| r.start < caret)
                .unwrap_or(count - 1),
            None => self
                .matches
                .iter()
                .position(|r| r.start >= caret)
                .unwrap_or(0),
        });
        self.selected().cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;
    use notes_core::document::Block;
    fn document(text: &str) -> Document {
        Document {
            blocks: text.split('\n').map(Block::paragraph).collect(),
            ..Default::default()
        }
    }
    #[test]
    fn literal_unicode_case_insensitive_ranges_and_wrapping() {
        let d = document("One ONE one. [x] é 👩‍👩‍👧‍👦\nΣ ς σ 中文");
        let text = Editor::new(d.clone()).text();
        let mut f = Find::default();
        f.set_query(&d, "one".into(), 5);
        assert_eq!(f.selected(), Some(&(8..11)));
        assert_eq!(f.step(false, 0), Some(0..3));
        assert_eq!(f.step(true, 0), Some(8..11));
        for (query, count) in [("[x]", 1), ("é", 1), ("👩‍👩‍👧‍👦", 1), ("Σ", 3), ("中文", 1)]
        {
            f.set_query(&d, query.into(), 0);
            assert_eq!(f.matches.len(), count);
            for r in f.matches.iter() {
                assert!(text.is_char_boundary(r.start) && text.is_char_boundary(r.end));
            }
        }
        f.set_query(&d, "family".into(), 0);
        assert!(f.step(false, 0).is_none());
        f.set_query(&d, String::new(), 0);
        assert!(f.matches.is_empty());
    }
    #[test]
    fn spans_tables_soft_breaks_and_refresh_never_edit_document_or_selection() {
        let mut e = Editor::new(document("Alpha"));
        e.insert_table(1, 2).unwrap();
        e.insert("ALPHA", 0).unwrap();
        e.enter(true).unwrap();
        e.insert("alpha", 0).unwrap();
        e.tab(false).unwrap();
        e.insert("alphabet", 0).unwrap();
        let before = e.snapshot();
        let mut f = Find::default();
        f.set_query(&e.document, "alpha".into(), 0);
        assert_eq!(f.matches.len(), 4);
        assert_eq!(e.document, before.document);
        assert_eq!(e.selection, before.selection);
        e.select(0, false);
        e.select(5, true);
        e.insert("Changed", 0).unwrap();
        let selection = e.selection.clone();
        f.refresh(&e.document);
        assert_eq!(f.matches.len(), 3);
        assert!(f.current.is_none());
        assert_eq!(e.selection, selection);
        e.undo();
        f.refresh(&e.document);
        assert_eq!(f.matches.len(), 4);
        f.set_query(&e.document, "alpha\nalphabet".into(), 0);
        assert!(
            f.matches.is_empty(),
            "find never crosses paragraph/cell boundaries"
        );
    }
    #[test]
    fn resource_bounds_are_explicit() {
        let d = document(&"a".repeat(MAX_MATCHES + 1));
        let mut f = Find::default();
        f.set_query(&d, "a".into(), 0);
        assert_eq!(f.matches.len(), MAX_MATCHES);
        assert!(f.truncated);
        f.set_query(&d, "b".repeat(4097), 0);
        assert!(f.error.is_some());
        assert!(f.matches.is_empty());
        f.set_query(&d, "b".into(), 0);
        assert!(f.error.is_none());
        assert!(!f.truncated);
    }
}
