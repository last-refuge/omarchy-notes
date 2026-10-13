use anyhow::Result;
use notes_core::document::{Block, Cell, Document, Span};
use notes_export::{Cancellation, Paper, export};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};
fn main() -> Result<()> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/pdf-check".into()),
    );
    std::fs::create_dir_all(&output)?;
    let root = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/qt-v1"
    ));
    let fixture: Value = serde_json::from_slice(&std::fs::read(root.join("documents.json"))?)?;
    let document = Document::from_json(&fixture[1]["body"].to_string())?;
    for (name, paper) in [
        ("planning-letter.pdf", Paper::Letter),
        ("planning-a4.pdf", Paper::A4),
    ] {
        let report = export(
            &root,
            &document,
            &output.join(name),
            paper,
            &Cancellation::default(),
        )?;
        println!(
            "{name}: {} pages, {} attachment labels",
            report.pages, report.attachment_labels
        );
    }
    let mut title = Block::paragraph("The long view");
    title.set_kind("h1");
    let mut document = Document {
        blocks: vec![
            title,
            Block::paragraph(
                "Pagination review · Long paragraphs, repeated table headers, and merged cells.",
            ),
        ],
        ..Default::default()
    };
    document.blocks.push(Block::paragraph(
        &(0..45)
            .map(|i| {
                format!("Sentence {i:02}: A good page gives every thought enough room to breathe. ")
            })
            .collect::<String>(),
    ));
    let mut table = Block::paragraph("");
    table.kind = "table".into();
    table.attrs.insert("rows".into(), json!(14));
    table.attrs.insert("columns".into(), json!(2));
    for row in 0..14 {
        for col in 0..2 {
            let text = if row == 0 {
                if col == 0 {
                    "Milestone".into()
                } else {
                    "Journal".into()
                }
            } else if row == 5 && col == 1 {
                (0..60)
                    .map(|i| format!("Long-row-{i:02}: This line must survive a page break. "))
                    .collect()
            } else {
                format!("Cell-{row:02}-{col}: A little progress, made with care.")
            };
            table.cells.push(Cell {
                row,
                col,
                blocks: vec![Block::paragraph(&text)],
                attrs: BTreeMap::new(),
            });
        }
    }
    document.blocks.push(table);
    let mut table = Block::paragraph("");
    table.kind = "table".into();
    table.attrs.insert("rows".into(), json!(3));
    table.attrs.insert("columns".into(), json!(3));
    for (row, col, rs, cs, text) in [
        (0, 0, 1, 3, "A merged header"),
        (1, 0, 2, 1, "Two rows, one thought"),
        (1, 1, 1, 2, "Across two columns"),
        (2, 1, 1, 1, "Left"),
        (2, 2, 1, 1, "Right"),
    ] {
        table.cells.push(Cell {
            row,
            col,
            blocks: vec![Block::paragraph(text)],
            attrs: BTreeMap::from([("rowSpan".into(), json!(rs)), ("colSpan".into(), json!(cs))]),
        });
    }
    document.blocks.push(table);
    document.blocks.push(Block::paragraph(
        "Final sentence — café, coöperate, Ελληνικά, 日本語, مرحبا بالعالم.",
    ));
    let mut link = Block::paragraph("");
    link.spans = vec![Span(
        json!({"text":"A link with punctuation", "href":"https://example.com/a?x='quoted'&y=two"}),
    )];
    document.blocks.push(link);
    let mut rtl = Block::paragraph("");
    rtl.spans = vec![Span(
        json!({"text":"مرحبا بالعالم", "href":"https://example.com/arabic"}),
    )];
    document.blocks.push(rtl);
    let report = export(
        &root,
        &document,
        &output.join("pagination.pdf"),
        Paper::Letter,
        &Cancellation::default(),
    )?;
    println!("pagination.pdf: {} pages", report.pages);
    let mut title = Block::paragraph("A little room for a quotation");
    title.set_kind("h1");
    let mut quote=Block::paragraph(&(0..140).map(|i|format!("Thought {i:03}: A notebook makes room for attention, curiosity, and a little clarity. ")).collect::<String>());
    quote.set_kind("quote");
    quote.spans[0].set_mark("italic", true);
    let mut cell_quote = Block::paragraph("A quotation in a table cell.");
    cell_quote.set_kind("quote");
    let mut table = Block::paragraph("");
    table.kind = "table".into();
    table.attrs.insert("rows".into(), json!(1));
    table.attrs.insert("columns".into(), json!(1));
    table.cells = vec![Cell {
        row: 0,
        col: 0,
        blocks: vec![cell_quote],
        attrs: Default::default(),
    }];
    let document = Document::from_blocks(vec![title, quote, table]);
    for (name, paper) in [("quotes-letter", Paper::Letter), ("quotes-a4", Paper::A4)] {
        let report = export(
            &root,
            &document,
            &output.join(format!("{name}.pdf")),
            paper,
            &Cancellation::default(),
        )?;
        println!("{name}.pdf: {} pages", report.pages);
    }
    Ok(())
}
