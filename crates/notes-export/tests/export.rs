use notes_core::document::{Block, Document, Span};
use notes_export::{Cancellation, Paper, export};
use serde_json::json;
use std::path::Path;
#[test]
fn long_quotes_split_at_text_lines_and_preserve_the_document() {
    let root = tempfile::tempdir().unwrap();
    let mut quote=Block::paragraph(&(0..140).map(|i|format!("Thought {i}: A notebook makes room for attention, curiosity, and a little clarity. ")).collect::<String>());
    quote.set_kind("quote");
    quote.spans[0].set_mark("italic", true);
    let document = Document::from_blocks(vec![quote]);
    let original = document.clone();
    for paper in [Paper::Letter, Paper::A4] {
        let report = export(
            root.path(),
            &document,
            &root.path().join(format!("{}.pdf", paper.label())),
            paper,
            &Cancellation::default(),
        )
        .unwrap();
        assert!(report.pages > 2 && report.pages < 15);
        assert_eq!(document, original);
    }
}
#[test]
fn success_is_complete_private_and_never_overwrites() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.pdf");
    let document = Document {
        blocks: vec![Block::paragraph("A café for ideas — αβγ")],
        ..Default::default()
    };
    let report = export(
        root.path(),
        &document,
        &path,
        Paper::A4,
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(report.pages, 1);
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.ends_with(b"%%EOF\n"));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        export(
            root.path(),
            &document,
            &path,
            Paper::Letter,
            &Cancellation::default()
        )
        .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
#[test]
fn cancellation_and_failed_assets_leave_no_partial_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.pdf");
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(
        export(
            root.path(),
            &Document::default(),
            &path,
            Paper::Letter,
            &cancel
        )
        .is_err()
    );
    let mut document = Document::default();
    document.blocks[0].spans = vec![Span(json!({"image":"a".repeat(64)}))];
    assert!(
        export(
            root.path(),
            &document,
            &path,
            Paper::Letter,
            &Cancellation::default()
        )
        .is_err()
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
#[test]
fn unsupported_glyphs_and_nul_fail_explicitly_instead_of_losing_text() {
    let root = tempfile::tempdir().unwrap();
    for text in ["before\0after", "unassigned: \u{10ffff}"] {
        let doc = Document {
            blocks: vec![Block::paragraph(text)],
            ..Default::default()
        };
        assert!(
            export(
                root.path(),
                &doc,
                &root.path().join("note.pdf"),
                Paper::Letter,
                &Cancellation::default()
            )
            .is_err()
        );
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
#[test]
fn source_document_and_qt_library_are_unchanged() {
    let root = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/qt-v1"
    ));
    let before = std::fs::read(root.join("library.sqlite3")).unwrap();
    let fixture: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("documents.json")).unwrap()).unwrap();
    let document = Document::from_json(&fixture[0]["body"].to_string()).unwrap();
    let clone = document.clone();
    let output = tempfile::tempdir().unwrap();
    export(
        root,
        &document,
        &output.path().join("note.pdf"),
        Paper::Letter,
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(document, clone);
    assert_eq!(std::fs::read(root.join("library.sqlite3")).unwrap(), before);
}
