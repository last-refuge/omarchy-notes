//! Single-line input model for search; not the future rich-document editor.
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default)]
pub struct TextField {
    pub text: String,
    pub anchor: usize,
    pub head: usize,
    pub marked: Option<Range<usize>>,
}

impl TextField {
    pub fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
    pub fn utf16(&self, byte: usize) -> usize {
        self.text[..self.clip(byte)].encode_utf16().count()
    }
    pub fn byte(&self, utf16: usize) -> usize {
        Self::byte_in(&self.text, utf16)
    }
    pub fn byte_in(text: &str, utf16: usize) -> usize {
        let mut count = 0;
        for (i, ch) in text.char_indices() {
            if count + ch.len_utf16() > utf16 {
                return i;
            }
            count += ch.len_utf16();
        }
        text.len()
    }
    pub fn clip(&self, byte: usize) -> usize {
        let mut i = byte.min(self.text.len());
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
    pub fn byte_range(&self, r: Range<usize>) -> Range<usize> {
        let start = self.byte(r.start);
        start..self.byte(r.end).max(start)
    }
    pub fn select(&mut self, byte: usize, extend: bool) {
        self.head = self.clip(byte);
        if !extend {
            self.anchor = self.head;
        }
    }
    pub fn move_cursor(&mut self, forward: bool, extend: bool) {
        let selection = self.selection();
        let offset = if !extend && !selection.is_empty() {
            if forward {
                selection.end
            } else {
                selection.start
            }
        } else if forward {
            self.text
                .grapheme_indices(true)
                .find(|(i, _)| *i > self.head)
                .map_or(self.text.len(), |(i, _)| i)
        } else {
            self.text
                .grapheme_indices(true)
                .rev()
                .find(|(i, _)| *i < self.head)
                .map_or(0, |(i, _)| i)
        };
        self.select(offset, extend);
    }
    pub fn replace(
        &mut self,
        utf16: Option<Range<usize>>,
        text: &str,
        marked_selection: Option<Option<Range<usize>>>,
    ) {
        let range = utf16
            .map(|r| self.byte_range(r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let text = text.replace(['\r', '\n'], " ");
        self.text.replace_range(range.clone(), &text);
        self.marked = marked_selection
            .is_some()
            .then_some(range.start..range.start + text.len())
            .filter(|r| !r.is_empty());
        if let Some(Some(selected)) = marked_selection {
            self.anchor = range.start + Self::byte_in(&text, selected.start);
            self.head = range.start + Self::byte_in(&text, selected.end);
        } else {
            self.head = range.start + text.len();
            self.anchor = self.head;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emoji_deletion_uses_graphemes_and_utf16_ranges_round_trip() {
        let mut m = TextField::default();
        m.replace(None, "A👩‍💻e\u{301}", None);
        for (byte, _) in m.text.char_indices() {
            assert_eq!(m.byte(m.utf16(byte)), byte);
        }
        m.move_cursor(false, true);
        m.replace(None, "", None);
        assert_eq!(m.text, "A👩‍💻");
        m.move_cursor(false, true);
        m.replace(None, "", None);
        assert_eq!(m.text, "A");
    }
    #[test]
    fn composition_selection_is_relative_to_inserted_text_and_commit_replaces_preedit() {
        let mut m = TextField::default();
        m.replace(None, "prefix ", None);
        m.replace(None, "日本", Some(Some(1..2)));
        assert_eq!(&m.text[m.selection()], "本");
        m.replace(None, "日本語", None);
        assert_eq!(m.text, "prefix 日本語");
        assert!(m.marked.is_none());
    }
    #[test]
    fn ranges_are_clipped_and_multiline_paste_is_single_line() {
        let mut m = TextField::default();
        m.replace(None, "🙂", None);
        assert_eq!(m.byte_range(1..999), 0..4);
        m.replace(Some(0..999), "one\ntwo", None);
        assert_eq!(m.text, "one two");
    }
}
