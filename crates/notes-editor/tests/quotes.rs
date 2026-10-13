use notes_core::document::{Block, Cell, Document, Span};
use notes_editor::Editor;
use serde_json::json;

#[test]
fn pasted_quote_preserves_prose_on_both_sides() {
    let original = Document::from_blocks(vec![Block::paragraph("Before. After.")]);
    let mut editor = Editor::new(original.clone());
    editor.select(8, false);
    let mut quoted = Block::paragraph("A separate quotation.");
    quoted.set_kind("quote");
    editor.paste_fragment(vec![quoted]).unwrap();
    assert_eq!(
        editor
            .document
            .blocks
            .iter()
            .map(|b| (b.kind.as_str(), b.text()))
            .collect::<Vec<_>>(),
        vec![
            ("paragraph", "Before. ".into()),
            ("quote", "A separate quotation.".into()),
            ("paragraph", "After.".into())
        ]
    );
    assert_eq!(editor.selection.head.block, editor.document.blocks[1].id);
    editor.document.validate().unwrap();
    editor.undo();
    assert_eq!(editor.document, original);
}

#[test]
fn quote_schema_conversion_typing_and_history_preserve_identity_and_marks() {
    let mut block = Block::paragraph("A thoughtful 👩‍💻 sentence.");
    block.set_kind("check");
    block.attrs.insert("custom".into(), json!("kept"));
    block.spans[0].set_mark("italic", true);
    block.spans[0].0["href"] = json!("https://example.com");
    let old = Document::from_blocks(vec![block]);
    let id = old.blocks[0].id.clone();
    let mut e = Editor::new(old.clone());
    e.style("quote").unwrap();
    assert_eq!(e.document.schema, 2);
    assert!(e.style_active("quote"));
    let quote = e.document.clone();
    assert_eq!(quote.blocks[0].id, id);
    assert_eq!(quote.blocks[0].spans, old.blocks[0].spans);
    assert_eq!(quote.blocks[0].string("custom"), "kept");
    assert!(!quote.blocks[0].attrs.contains_key("checked"));
    e.undo();
    assert_eq!(e.document, old);
    e.redo();
    assert_eq!(e.document, quote);
    let end = e.text().len();
    e.select(end, false);
    e.enter(false).unwrap();
    assert_eq!(e.document.blocks[1].kind, "quote");
    assert_ne!(e.document.blocks[1].id, id);
    e.enter(false).unwrap();
    assert_eq!(e.document.blocks[1].kind, "paragraph");
    e.undo();
    assert_eq!(e.document.blocks[1].kind, "quote");
    e.insert("second", 1).unwrap();
    e.enter(true).unwrap();
    e.insert("line", 2).unwrap();
    assert_eq!(e.document.blocks[1].text(), "second\nline");
    assert_eq!(e.document.blocks.len(), 2);
    e.select_all();
    e.style("paragraph").unwrap();
    assert!(e.document.blocks.iter().all(|b| b.kind == "paragraph"));
    e.undo();
    assert!(e.document.blocks.iter().all(|b| b.kind == "quote"));
    e.document.validate().unwrap();
}

#[test]
fn quote_fragments_tables_composition_and_future_versions_stay_structured() {
    let mut quote = Block::paragraph("Marked quotation");
    quote.set_kind("quote");
    quote.spans[0].set_mark("bold", true);
    let mut table = Block::paragraph("");
    table.kind = "table".into();
    table.attrs.insert("rows".into(), json!(1));
    table.attrs.insert("columns".into(), json!(1));
    table.cells = vec![Cell {
        row: 0,
        col: 0,
        blocks: vec![quote.clone()],
        attrs: Default::default(),
    }];
    let doc = Document::from_blocks(vec![table]);
    assert_eq!(doc.schema, 2);
    doc.validate().unwrap();
    let mut source = Editor::new(doc);
    source.select_all();
    let mut target = Editor::new(Document::default());
    target.paste_fragment(source.fragment().unwrap()).unwrap();
    assert_eq!(target.document.schema, 2);
    target.document.validate().unwrap();
    assert_eq!(target.document.blocks[0].cells[0].blocks[0].kind, "quote");
    target.undo();
    assert_eq!(target.document.schema, 1);
    let mut e = Editor::new(Document::from_blocks(vec![quote]));
    let before = e.document.clone();
    e.input(None, "日本", Some(Some(2..2))).unwrap();
    assert!(e.style("paragraph").is_err());
    e.enter(false).unwrap();
    assert_eq!(e.document.blocks.len(), 1);
    e.cancel_composition();
    assert_eq!(e.document, before);
    let mut invalid = before.clone();
    invalid.schema = 1;
    assert!(invalid.validate().is_err());
    invalid.schema = 3;
    assert!(invalid.validate().is_err());
    assert_eq!(
        Document::from_json(&serde_json::to_string(&before).unwrap()).unwrap(),
        before
    );
    let italic = Document::from_blocks(vec![Block {
        spans: vec![Span(json!({"text":"ordinary italic","marks":["italic"]}))],
        ..Block::paragraph("")
    }]);
    assert_eq!(italic.schema, 1);
    assert_eq!(italic.blocks[0].kind, "paragraph");
}
