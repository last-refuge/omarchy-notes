use super::*;
use notes_core::links::LinkTarget;

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub href: String,
    pub range: Range<usize>,
}
impl Editor {
    /// Exact text hit; objects and separators are never part of a link.
    pub fn link_at(&self, byte: usize) -> Option<Link> {
        let mut base = 0;
        for block in self.document.leaves() {
            let mut runs: Vec<Link> = vec![];
            let mut offset = base;
            for span in &block.spans {
                let end = offset + span.content().len();
                if span.is_text()
                    && end > offset
                    && let Some(href) = span.0.get("href").and_then(|v| v.as_str())
                {
                    if let Some(last) = runs.last_mut()
                        && last.range.end == offset
                        && last.href == href
                    {
                        last.range.end = end;
                    } else {
                        runs.push(Link {
                            href: href.into(),
                            range: offset..end,
                        });
                    }
                }
                offset = end;
            }
            if let Some(link) = runs.into_iter().find(|l| l.range.contains(&byte)) {
                return Some(link);
            }
            base = offset + 1;
        }
        None
    }
    /// A caret immediately after a link may edit it, but a pointer hit may not.
    pub fn current_link(&self) -> Option<Link> {
        let range = self.range();
        let link = self.link_at(range.start).or_else(|| {
            if range.is_empty() {
                range
                    .start
                    .checked_sub(1)
                    .and_then(|i| self.link_at(i))
                    .filter(|l| l.range.end == range.start)
            } else {
                None
            }
        })?;
        (range.end <= link.range.end).then_some(link)
    }
    pub fn set_link(&mut self, href: &str, label: &str) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        let href = LinkTarget::parse(href)?.href();
        let before = self.snapshot();
        let mut range = self.range();
        if range.is_empty()
            && let Some(link) = self.current_link()
        {
            range = link.range;
        }
        let result = if range.is_empty() {
            let label = if label.is_empty() { &href } else { label };
            if label.contains(['\n', '\r', '\u{fffc}']) || label.len() > 8192 {
                bail!("Use a short, single-line link label.");
            }
            self.typing
                .0
                .as_object_mut()
                .unwrap()
                .insert("href".into(), json!(href));
            self.replace(range, label)
        } else {
            self.change_href(range, Some(&href))
        }
        .and_then(|_| self.document.validate());
        if let Err(error) = result {
            self.restore(before);
            return Err(error);
        }
        self.typing.0.as_object_mut().unwrap().remove("href");
        self.last_typing = None;
        self.record(before, false);
        Ok(())
    }
    pub fn remove_link(&mut self) -> Result<()> {
        ensure!(self.marked.is_none(), "Finish text composition first.");
        let mut range = self.range();
        if range.is_empty() {
            let Some(link) = self.current_link() else {
                return Ok(());
            };
            range = link.range;
        }
        let before = self.snapshot();
        self.change_href(range, None)?;
        self.typing.0.as_object_mut().unwrap().remove("href");
        self.last_typing = None;
        self.record(before, false);
        Ok(())
    }
    fn change_href(&mut self, range: Range<usize>, href: Option<&str>) -> Result<()> {
        let mut offset = 0;
        let leaves: Vec<_> = self
            .document
            .leaves()
            .iter()
            .map(|b| (b.id.clone(), b.text().len()))
            .collect();
        let mut found = false;
        for (id, len) in leaves {
            let end = offset + len;
            if range.start < end && range.end > offset {
                let block = self.document.block_mut(&id).unwrap();
                let selected = range.start.saturating_sub(offset)..range.end.min(end) - offset;
                let mut spans = slice_spans(&block.spans, 0..selected.start);
                for mut span in slice_spans(&block.spans, selected.clone()) {
                    if span.is_text() && !span.content().is_empty() {
                        found = true;
                        if let Some(href) = href {
                            span.0
                                .as_object_mut()
                                .unwrap()
                                .insert("href".into(), json!(href));
                        } else {
                            span.0.as_object_mut().unwrap().remove("href");
                        }
                    }
                    spans.push(span);
                }
                spans.extend(slice_spans(&block.spans, selected.end..len));
                block.spans = spans;
            }
            offset = end + 1;
        }
        ensure!(found || href.is_none(), "Select some text for the link.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn editor() -> Editor {
        let mut b = Block::paragraph("Café 👩‍👩‍👧‍👦 and words");
        b.spans[0]
            .0
            .as_object_mut()
            .unwrap()
            .insert("marks".into(), json!(["bold"]));
        b.spans[0]
            .0
            .as_object_mut()
            .unwrap()
            .insert("future".into(), json!({"keep":true}));
        Editor::new(Document {
            blocks: vec![b, Block::paragraph("next")],
            ..Default::default()
        })
    }
    #[test]
    fn linking_preserves_text_metadata_identity_and_one_step_undo() {
        let mut e = editor();
        let before = e.document.clone();
        e.select(0, false);
        e.select(5, true);
        let selection = e.selection.clone();
        e.set_link("https://example.com", "ignored").unwrap();
        assert_eq!(e.selection, selection);
        assert_eq!(e.text(), Editor::new(before.clone()).text());
        let span = &e.document.blocks[0].spans[0];
        assert!(span.mark("bold"));
        assert_eq!(span.0["future"], json!({"keep":true}));
        assert_eq!(e.link_at(0).unwrap().range, 0..5);
        let after = e.document.clone();
        e.undo();
        assert_eq!(e.document, before);
        e.redo();
        assert_eq!(e.document, after);
        e.select(3, false);
        e.set_link("note://AbC", "").unwrap();
        assert_eq!(e.current_link().unwrap().range, 0..5);
        e.remove_link().unwrap();
        assert_eq!(e.document, before);
    }
    #[test]
    fn insertion_ends_link_typing_and_invalid_inputs_are_atomic() {
        let mut e = editor();
        e.select(e.text().len(), false);
        e.set_link("note://AbC", "A thought").unwrap();
        e.insert(" tail", 1).unwrap();
        assert!(e.link_at(e.text().len() - 1).is_none());
        let before = e.document.clone();
        let rev = e.revision;
        assert!(e.set_link("file:///tmp/no", "").is_err());
        assert_eq!(e.document, before);
        assert_eq!(e.revision, rev);
        e.input(None, "候補", Some(None)).unwrap();
        assert!(e.set_link("https://example.com", "").is_err());
        assert!(e.remove_link().is_err());
        e.cancel_composition();
        assert_eq!(e.document, before);
    }
    #[test]
    fn selected_links_split_without_replacing_other_text_or_paragraphs() {
        let mut e = editor();
        let before = e.document.clone();
        e.select_all();
        e.set_link("https://example.com", "").unwrap();
        assert_eq!(e.document.blocks.len(), 2);
        assert!(e.link_at(e.document.blocks[0].text().len()).is_none());
        e.select(0, false);
        e.select(5, true);
        e.remove_link().unwrap();
        assert!(e.link_at(0).is_none());
        assert!(e.link_at(6).is_some());
        e.undo();
        e.undo();
        assert_eq!(e.document, before);
    }
    #[test]
    fn unsafe_legacy_link_can_be_removed_and_adjacent_marks_form_one_link() {
        let mut e = editor();
        e.select(0, false);
        e.select(5, true);
        e.set_link("https://example.com", "").unwrap();
        e.select(0, false);
        e.select(3, true);
        e.mark("italic").unwrap();
        e.select(3, false);
        assert_eq!(e.current_link().unwrap().range, 0..5);
        for span in &mut e.document.blocks[0].spans[..2] {
            span.0
                .as_object_mut()
                .unwrap()
                .insert("href".into(), json!("javascript:bad"));
        }
        e.remove_link().unwrap();
        assert!(e.link_at(0).is_none());
    }
    #[test]
    fn table_cells_and_objects_survive_linking_a_broad_selection() {
        let mut e = Editor::new(Document::default());
        e.insert_table(1, 2).unwrap();
        let table = e
            .document
            .blocks
            .iter_mut()
            .find(|b| b.kind == "table")
            .unwrap();
        table.cells[0].blocks[0].spans = vec![
            Span::text("Left"),
            Span(json!({"image":"a".repeat(64), "alt":"Keep me"})),
        ];
        table.cells[1].blocks[0].spans = vec![Span::text("Right")];
        let before = e.document.clone();
        e.select_all();
        e.set_link("https://example.com", "").unwrap();
        e.document.validate().unwrap();
        let table = e
            .document
            .blocks
            .iter()
            .find(|b| b.kind == "table")
            .unwrap();
        assert_eq!(
            table.cells[0].blocks[0].spans[1],
            before
                .blocks
                .iter()
                .find(|b| b.kind == "table")
                .unwrap()
                .cells[0]
                .blocks[0]
                .spans[1]
        );
        assert_eq!(
            table.cells[1].blocks[0].spans[0].0["href"],
            "https://example.com"
        );
        e.undo();
        assert_eq!(e.document, before);
        let image = e.text().find('\u{fffc}').unwrap();
        e.select(image, false);
        e.select(image + 3, true);
        assert!(e.set_link("https://example.com", "").is_err());
        assert_eq!(e.document, before);
    }
}
