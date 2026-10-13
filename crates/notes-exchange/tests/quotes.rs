use notes_core::document::{Block, Cell, Document, Span};
use notes_exchange::{Fragment, html, markdown};
use serde_json::json;

#[test]
fn quotes_keep_structure_marks_links_and_objects_in_rich_fragments() {
    let mut quote = Block::paragraph("Be curious — **literally**.");
    quote.set_kind("quote");
    quote.spans[0].set_mark("italic", true);
    quote.spans[0].0["href"] = json!("https://example.com/thought?a=1&b=2");
    quote
        .spans
        .push(Span(json!({"image":"a".repeat(64),"alt":"A thought"})));
    let mut table = Block::paragraph("");
    table.kind = "table".into();
    table.attrs.insert("rows".into(), json!(1));
    table.attrs.insert("columns".into(), json!(1));
    table.cells = vec![Cell {
        row: 0,
        col: 0,
        blocks: vec![quote],
        attrs: Default::default(),
    }];
    let doc = Document::from_blocks(vec![table]);
    let fragment = Fragment {
        document: doc.clone(),
        bundle: Default::default(),
    };
    assert_eq!(
        Fragment::decode(&fragment.encode().unwrap())
            .unwrap()
            .document,
        doc
    );
    let offers = fragment.offers().unwrap();
    assert!(String::from_utf8_lossy(&offers["text/html"]).contains("<blockquote>"));
    let imported = html::import(std::str::from_utf8(&offers["text/html"]).unwrap()).unwrap();
    assert_eq!(imported.document.schema, 2);
    assert_eq!(imported.document.blocks[0].cells[0].blocks[0].kind, "quote");
}

#[test]
fn html_and_markdown_quote_round_trips_preserve_literal_words_and_inline_format() {
    let mut quote = Block::paragraph("A **literal** _thought_ <with> & signs.\nNext line.");
    quote.set_kind("quote");
    quote.spans[0].set_mark("bold", true);
    quote.spans[0].set_mark("italic", true);
    quote.spans[0].0["href"] = json!("note://another-note");
    let empty = {
        let mut b = Block::paragraph("");
        b.set_kind("quote");
        b
    };
    let doc = Document::from_blocks(vec![quote, empty, Block::paragraph("An ordinary ending.")]);
    for imported in [
        html::import(&html::export(&doc)).unwrap(),
        markdown::import(&markdown::export(&doc).unwrap().text).unwrap(),
    ] {
        assert_eq!(imported.document.schema, 2);
        assert_eq!(imported.document.blocks[0].kind, "quote");
        assert_eq!(imported.document.blocks[0].text(), doc.blocks[0].text());
        assert!(
            imported.document.blocks[0]
                .spans
                .iter()
                .all(|s| s.mark("bold")
                    && s.mark("italic")
                    && s.0["href"] == "note://another-note")
        );
        assert_eq!(imported.document.blocks[1].kind, "quote");
        assert_eq!(imported.document.blocks[1].text(), "");
        assert_eq!(imported.document.blocks.last().unwrap().kind, "paragraph");
    }
    let md = markdown::export(&doc).unwrap();
    assert!(md.text.starts_with("> "));
    assert!(md.warnings.is_empty());
    let nested = markdown::import("> first\n>\n> > nested\n\nordinary").unwrap();
    assert_eq!(
        nested
            .document
            .blocks
            .iter()
            .filter(|b| b.kind == "quote")
            .count(),
        2
    );
    assert!(!nested.warnings.is_empty());
    let active = markdown::import("> safe\n\n<script>alert(1)</script>").unwrap();
    assert!(!active.document.plain_text().contains("alert"));
}
