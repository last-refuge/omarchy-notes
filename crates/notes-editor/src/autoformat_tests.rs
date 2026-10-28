use super::*;

fn type_text(e: &mut Editor, text: &str) {
    for ch in text.chars() {
        e.insert(&ch.to_string(), 0).unwrap();
    }
}
fn shape(e: &Editor) -> Vec<(String, String)> {
    e.document
        .blocks
        .iter()
        .map(|b| {
            let kind = match b.kind.as_str() {
                "list" => format!("list:{}", b.string("list")),
                "heading" => format!("h{}", b.number("level", 0)),
                other => other.to_owned(),
            };
            (kind, b.text())
        })
        .collect()
}
fn expect(e: &Editor, blocks: &[(&str, &str)]) {
    let blocks: Vec<_> = blocks
        .iter()
        .map(|(kind, text)| (kind.to_string(), text.to_string()))
        .collect();
    assert_eq!(shape(e), blocks);
}
fn paragraph(text: &str) -> Editor {
    let mut e = Editor::new(Document::from_blocks(vec![Block::paragraph(text)]));
    e.select(e.text().len(), false);
    e
}

#[test]
fn star_space_starts_a_bullet_list_that_enter_continues() {
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* ");
    expect(&e, &[("list:bullet", "")]);
    let block = &e.document.blocks[0];
    assert_eq!(block.number("indent", 9), 0);
    assert_eq!(e.selection.head.block, block.id);
    assert_eq!(e.selection.head.offset, 0);
    type_text(&mut e, "first");
    e.enter(false).unwrap();
    type_text(&mut e, "second");
    e.enter(false).unwrap();
    expect(
        &e,
        &[
            ("list:bullet", "first"),
            ("list:bullet", "second"),
            ("list:bullet", ""),
        ],
    );
    // Enter on the empty item ends the list, as in other rich text editors.
    e.enter(false).unwrap();
    expect(
        &e,
        &[
            ("list:bullet", "first"),
            ("list:bullet", "second"),
            ("paragraph", ""),
        ],
    );
}

#[test]
fn dash_space_also_starts_a_bullet_list() {
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "- item");
    expect(&e, &[("list:bullet", "item")]);
}

#[test]
fn undo_restores_the_typed_marker_before_removing_it() {
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* ");
    e.undo();
    expect(&e, &[("paragraph", "* ")]);
    assert_eq!(e.global(&e.selection.head), 2);
    e.undo();
    expect(&e, &[("paragraph", "")]);
    e.redo();
    expect(&e, &[("paragraph", "* ")]);
    e.redo();
    expect(&e, &[("list:bullet", "")]);
}

#[test]
fn only_a_lone_leading_marker_in_a_paragraph_converts() {
    for text in ["a*", "**", " *", "-*", "*-"] {
        let mut e = paragraph(text);
        type_text(&mut e, " ");
        expect(&e, &[("paragraph", &format!("{text} "))]);
    }
    for kind in ["h1", "quote", "bullet", "ordered", "check"] {
        let mut e = paragraph("*");
        e.document.blocks[0].set_kind(kind);
        let before = shape(&e);
        type_text(&mut e, " ");
        assert_eq!(shape(&e).len(), 1);
        assert_eq!(shape(&e)[0].0, before[0].0, "{kind} keeps its style");
        assert_eq!(shape(&e)[0].1, "* ", "{kind} keeps the typed marker");
    }
}

#[test]
fn pasted_or_replacing_text_does_not_convert() {
    let mut e = Editor::new(Document::default());
    e.insert("* ", 0).unwrap();
    expect(&e, &[("paragraph", "* ")]);
    let mut e = paragraph("*x");
    e.select_all();
    e.insert(" ", 0).unwrap();
    expect(&e, &[("paragraph", " ")]);
}

#[test]
fn text_after_the_marker_becomes_the_item_and_keeps_its_formatting() {
    let mut bold = Span::text("tail");
    bold.set_mark("bold", true);
    let mut e = Editor::new(Document::from_blocks(vec![Block {
        spans: vec![Span::text("*"), bold],
        ..Block::paragraph("")
    }]));
    e.select(1, false);
    type_text(&mut e, " ");
    expect(&e, &[("list:bullet", "tail")]);
    assert!(e.document.blocks[0].spans[0].mark("bold"));
    assert_eq!(e.selection.head.offset, 0);
    type_text(&mut e, "x");
    expect(&e, &[("list:bullet", "xtail")]);
}

#[test]
fn one_dot_starts_a_numbered_list_that_enter_continues() {
    for marker in ["1.", "1)"] {
        let mut e = Editor::new(Document::default());
        type_text(&mut e, &format!("{marker} one"));
        e.enter(false).unwrap();
        type_text(&mut e, "two");
        expect(&e, &[("list:ordered", "one"), ("list:ordered", "two")]);
    }
    // The list has no start number, so other numbers stay literal text.
    for text in ["2.", "11.", "1.5", "a1."] {
        let mut e = paragraph(text);
        type_text(&mut e, " ");
        expect(&e, &[("paragraph", &format!("{text} "))]);
    }
}

#[test]
fn brackets_start_a_checklist_that_enter_continues_unchecked() {
    for marker in ["[]", "[ ]"] {
        let mut e = Editor::new(Document::default());
        type_text(&mut e, &format!("{marker} task"));
        expect(&e, &[("list:check", "task")]);
        e.toggle_check(&e.document.blocks[0].id.clone());
        e.enter(false).unwrap();
        type_text(&mut e, "next");
        expect(&e, &[("list:check", "task"), ("list:check", "next")]);
        assert!(e.document.blocks[0].checked());
        assert!(!e.document.blocks[1].checked());
    }
}

#[test]
fn brackets_turn_a_list_item_into_a_checklist_item_at_the_same_level() {
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "- ");
    e.indent(false).unwrap();
    type_text(&mut e, "[ ] task");
    expect(&e, &[("list:check", "task")]);
    assert_eq!(e.document.blocks[0].number("indent", 0), 1);
    // A checklist item is already a checklist, and other lists keep their marker.
    for (kind, text) in [("check", "[] "), ("bullet", "1. "), ("ordered", "- ")] {
        let mut e = paragraph("");
        e.document.blocks[0].set_kind(kind);
        let before = shape(&e);
        type_text(&mut e, text);
        assert_eq!(shape(&e)[0].0, before[0].0, "{kind} keeps its style");
        assert_eq!(shape(&e)[0].1, text, "{kind} keeps the typed marker");
    }
}

#[test]
fn hashes_start_headings_up_to_level_three() {
    for (marker, kind) in [("#", "h1"), ("##", "h2"), ("###", "h3")] {
        let mut e = Editor::new(Document::default());
        type_text(&mut e, &format!("{marker} Head"));
        expect(&e, &[(kind, "Head")]);
        // Enter after a heading continues in body text.
        e.enter(false).unwrap();
        type_text(&mut e, "body");
        expect(&e, &[(kind, "Head"), ("paragraph", "body")]);
    }
    for text in ["####", "#a", "a#"] {
        let mut e = paragraph(text);
        type_text(&mut e, " ");
        expect(&e, &[("paragraph", &format!("{text} "))]);
    }
    // Headings and other lists are left as they are.
    let mut e = paragraph("#");
    e.document.blocks[0].set_kind("bullet");
    type_text(&mut e, " ");
    expect(&e, &[("list:bullet", "# ")]);
}

#[test]
fn undo_restores_every_typed_marker() {
    for marker in ["*", "-", "1.", "1)", "[]", "[ ]", "#", "##", "###"] {
        let mut e = Editor::new(Document::default());
        type_text(&mut e, &format!("{marker} "));
        assert_ne!(shape(&e)[0].0, "paragraph", "{marker} converts");
        e.undo();
        expect(&e, &[("paragraph", &format!("{marker} "))]);
        assert_eq!(e.global(&e.selection.head), marker.len() + 1);
    }
}

#[test]
fn backspace_at_the_start_of_a_list_item_leaves_the_list() {
    for kind in ["bullet", "ordered", "check"] {
        let mut e = Editor::new(Document::from_blocks(vec![
            Block::paragraph("intro"),
            Block::paragraph("one"),
            Block::paragraph("two"),
        ]));
        for block in &mut e.document.blocks[1..] {
            block.set_kind(kind);
        }
        e.select(
            e.global(&Position {
                block: e.document.blocks[2].id.clone(),
                offset: 0,
            }),
            false,
        );
        // The first Backspace removes the list formatting but keeps the text.
        e.delete(false).unwrap();
        assert_eq!(e.document.blocks.len(), 3, "{kind}");
        assert_eq!(e.document.blocks[2].kind, "paragraph", "{kind}");
        assert_eq!(e.document.blocks[2].text(), "two");
        assert_eq!(e.selection.head.block, e.document.blocks[2].id);
        assert_eq!(e.selection.head.offset, 0);
        // The next one joins the previous line, as for any paragraph.
        e.delete(false).unwrap();
        assert_eq!(e.document.blocks.len(), 2, "{kind}");
        assert_eq!(e.document.blocks[1].text(), "onetwo");
        assert_eq!(e.document.blocks[1].kind, "list");
        e.undo();
        assert_eq!(e.document.blocks.len(), 3);
        e.undo();
        assert_eq!(e.document.blocks[2].kind, "list", "{kind}");
    }
}

#[test]
fn backspace_steps_a_nested_item_out_one_level_at_a_time() {
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* item");
    e.select(0, false);
    e.indent(false).unwrap();
    e.indent(false).unwrap();
    e.delete(false).unwrap();
    assert_eq!(e.document.blocks[0].number("indent", 9), 1);
    e.delete(false).unwrap();
    assert_eq!(e.document.blocks[0].number("indent", 9), 0);
    assert_eq!(e.document.blocks[0].kind, "list");
    e.delete(false).unwrap();
    expect(&e, &[("paragraph", "item")]);
}

#[test]
fn backspace_right_after_a_typed_marker_takes_the_formatting_back() {
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* ");
    expect(&e, &[("list:bullet", "")]);
    // As in Google Docs and ProseMirror editors, the typed marker comes back.
    e.delete(false).unwrap();
    expect(&e, &[("paragraph", "* ")]);
    assert_eq!(e.global(&e.selection.head), 2);
    // From there Backspace deletes characters as usual.
    e.delete(false).unwrap();
    expect(&e, &[("paragraph", "*")]);
    e.undo();
    e.redo();
    // The conversion can be redone after being taken back.
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* ");
    e.delete(false).unwrap();
    e.redo();
    expect(&e, &[("list:bullet", "")]);
}

#[test]
fn backspace_restores_every_typed_marker_and_the_original_format() {
    for marker in ["*", "-", "1.", "1)", "[]", "[ ]", "#", "##", "###", ">"] {
        let mut e = Editor::new(Document::default());
        e.quotes_enabled = true;
        type_text(&mut e, &format!("{marker} "));
        e.delete(false).unwrap();
        expect(&e, &[("paragraph", &format!("{marker} "))]);
        assert_eq!(e.global(&e.selection.head), marker.len() + 1);
        assert_eq!(e.document.schema, 1, "{marker}");
    }
}

#[test]
fn backspace_takes_a_marker_back_only_while_nothing_else_has_happened() {
    // Moving the caret, even to the same place, ends the opportunity.
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* ");
    e.select(0, false);
    e.delete(false).unwrap();
    expect(&e, &[("paragraph", "")]);
    // Typing does too: Backspace then only deletes the new text.
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* a");
    e.delete(false).unwrap();
    expect(&e, &[("list:bullet", "")]);
    e.delete(false).unwrap();
    expect(&e, &[("paragraph", "")]);
    // Text after the caret returns with the marker.
    let mut e = paragraph("*tail");
    e.select(1, false);
    type_text(&mut e, " ");
    expect(&e, &[("list:bullet", "tail")]);
    e.delete(false).unwrap();
    expect(&e, &[("paragraph", "* tail")]);
}

#[test]
fn backspace_at_the_start_of_a_quote_leaves_the_quote() {
    for text in ["said", ""] {
        let mut e = Editor::new(Document::from_blocks(vec![
            Block::paragraph("intro"),
            Block::paragraph(text),
        ]));
        e.document.blocks[1].set_kind("quote");
        e.document.schema = 2;
        e.select(6, false);
        e.delete(false).unwrap();
        expect(&e, &[("paragraph", "intro"), ("paragraph", text)]);
        assert_eq!(e.selection.head.block, e.document.blocks[1].id);
        assert_eq!(e.selection.head.offset, 0);
        e.document.validate().unwrap();
        // The next Backspace joins the previous line, as for any paragraph.
        e.delete(false).unwrap();
        expect(&e, &[("paragraph", &format!("intro{text}"))]);
        e.undo();
        e.undo();
        assert_eq!(e.document.blocks[1].kind, "quote");
    }
}

#[test]
fn other_deletes_are_unchanged() {
    // Inside the item, with a selection, forward, and at the start of a heading.
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "* ab");
    e.delete(false).unwrap();
    expect(&e, &[("list:bullet", "a")]);
    e.select(0, false);
    e.delete(true).unwrap();
    expect(&e, &[("list:bullet", "")]);
    let mut e = Editor::new(Document::from_blocks(vec![
        Block::paragraph("a"),
        Block::paragraph("b"),
    ]));
    e.document.blocks[1].set_kind("h1");
    e.select(2, false);
    e.delete(false).unwrap();
    expect(&e, &[("paragraph", "ab")]);
}

#[test]
fn greater_than_starts_a_quote_only_when_the_library_supports_quotes() {
    // Quotes need the upgraded library format, so by default the marker stays
    // text and the note remains saveable in the original format.
    let mut e = Editor::new(Document::default());
    type_text(&mut e, "> said");
    expect(&e, &[("paragraph", "> said")]);
    assert_eq!(e.document.schema, 1);

    let mut e = Editor::new(Document::default());
    e.quotes_enabled = true;
    type_text(&mut e, "> said");
    expect(&e, &[("quote", "said")]);
    assert_eq!(e.document.schema, 2);
    e.document.validate().unwrap();
    // Enter continues the quote; Enter on an empty quote line ends it.
    e.enter(false).unwrap();
    type_text(&mut e, "more");
    expect(&e, &[("quote", "said"), ("quote", "more")]);
    e.enter(false).unwrap();
    e.enter(false).unwrap();
    expect(
        &e,
        &[("quote", "said"), ("quote", "more"), ("paragraph", "")],
    );
}

#[test]
fn undoing_a_typed_quote_restores_the_marker_and_the_original_format() {
    let mut e = Editor::new(Document::default());
    e.quotes_enabled = true;
    type_text(&mut e, "> ");
    assert_eq!(e.document.schema, 2);
    e.undo();
    expect(&e, &[("paragraph", "> ")]);
    assert_eq!(e.document.schema, 1);
    e.undo();
    e.redo();
    e.redo();
    expect(&e, &[("quote", "")]);
    assert_eq!(e.document.schema, 2);
}

#[test]
fn greater_than_converts_only_a_lone_leading_marker_in_a_paragraph() {
    for text in ["a>", ">>", " >"] {
        let mut e = paragraph(text);
        e.quotes_enabled = true;
        type_text(&mut e, " ");
        expect(&e, &[("paragraph", &format!("{text} "))]);
        assert_eq!(e.document.schema, 1);
    }
    for kind in ["h1", "bullet", "check"] {
        let mut e = paragraph(">");
        e.quotes_enabled = true;
        e.document.blocks[0].set_kind(kind);
        let before = shape(&e);
        type_text(&mut e, " ");
        assert_eq!(shape(&e)[0].0, before[0].0, "{kind} keeps its style");
        assert_eq!(shape(&e)[0].1, "> ", "{kind} keeps the typed marker");
        assert_eq!(e.document.schema, 1);
    }
}
