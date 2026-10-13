use super::*;

#[test]
fn mixed_marks_become_consistent_and_undo_retains_other_metadata() {
    let mut plain = Span::text("Plain ");
    plain.0["href"] = json!("https://example.com");
    let mut bold = Span::text("Bold");
    bold.set_mark("bold", true);
    bold.set_mark("italic", true);
    let mut e = Editor::new(Document {
        blocks: vec![Block {
            spans: vec![plain, bold],
            ..Block::paragraph("")
        }],
        ..Default::default()
    });
    e.select_all();
    let before = e.document.clone();
    assert_eq!(e.mark_state("bold"), None);
    assert_eq!(e.mark_state("code"), Some(false));
    e.mark("bold").unwrap();
    assert_eq!(e.mark_state("bold"), Some(true));
    assert_eq!(
        e.document.blocks[0].spans[0].0["href"],
        json!("https://example.com")
    );
    assert!(e.document.blocks[0].spans[1].mark("italic"));
    e.mark("bold").unwrap();
    assert_eq!(e.mark_state("bold"), Some(false));
    e.undo();
    e.undo();
    assert_eq!(e.document, before);
}

#[test]
fn word_formats_keep_caret_and_boundaries_only_change_future_typing() {
    for mark in notes_core::document::MARKS {
        let mut e = Editor::new(Document {
            blocks: vec![Block::paragraph("A café next")],
            ..Default::default()
        });
        e.select(4, false);
        let before = e.snapshot();
        e.mark(mark).unwrap();
        assert_eq!(e.selection, before.selection);
        assert_eq!(e.document.blocks[0].spans[1].content(), "café");
        assert!(e.document.blocks[0].spans[1].mark(mark));
        assert!(!e.document.blocks[0].spans[0].mark(mark));
        let after = e.document.clone();
        e.undo();
        assert_eq!(e.document, before.document);
        e.redo();
        assert_eq!(e.document, after);
        e.select(0, false);
        let before = e.document.clone();
        let rev = e.revision;
        e.mark(mark).unwrap();
        assert_eq!(e.document, before);
        assert_eq!(e.revision, rev);
        e.insert("New ", 0).unwrap();
        assert!(e.document.blocks[0].spans[0].mark(mark));
        e.undo();
        assert_eq!(e.document, before);
    }
}
#[test]
fn paragraph_styles_report_whole_selection_and_preserve_existing_check_state() {
    let mut e = Editor::new(Document {
        blocks: vec![Block::paragraph("First"), Block::paragraph("Second")],
        ..Default::default()
    });
    e.style("h2").unwrap();
    assert!(e.style_active("h2"));
    assert!(!e.style_active("h1"));
    e.select_all();
    assert!(!e.style_active("h2"));
    e.style("h3").unwrap();
    assert!(e.style_active("h3"));
    e.style("check").unwrap();
    let id = e.document.blocks[0].id.clone();
    e.toggle_check(&id);
    e.select(0, false);
    e.indent(false).unwrap();
    e.select_all();
    let before = e.document.clone();
    let revision = e.revision;
    e.style("check").unwrap();
    assert_eq!(e.document, before);
    assert_eq!(e.revision, revision);
    e.style("ordered").unwrap();
    assert!(e.style_active("ordered"));
    e.undo();
    assert_eq!(e.document, before);
}
#[test]
fn formatting_rejects_unknown_commands_and_composition_without_mutation() {
    let mut e = Editor::new(Document {
        blocks: vec![Block::paragraph("Words")],
        ..Default::default()
    });
    let before = e.document.clone();
    assert!(e.mark("unknown").is_err());
    assert!(e.style("unknown").is_err());
    assert_eq!(e.document, before);
    e.input(None, "候補", Some(None)).unwrap();
    let draft = e.document.clone();
    assert!(e.mark("highlight").is_err());
    assert!(e.style("h3").is_err());
    assert_eq!(e.document, draft);
    assert!(e.marked.is_some());
}
