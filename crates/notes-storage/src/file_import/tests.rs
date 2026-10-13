use super::*;

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}
fn counts(store: &Store) -> (i64, i64, i64, i64) {
    (
        store
            .db
            .query_row("SELECT count(*) FROM notes", [], |r| r.get(0))
            .unwrap(),
        store
            .db
            .query_row("SELECT count(*) FROM folders", [], |r| r.get(0))
            .unwrap(),
        store
            .db
            .query_row("SELECT count(*) FROM attachments", [], |r| r.get(0))
            .unwrap(),
        store
            .db
            .query_row("SELECT count(*) FROM blobs", [], |r| r.get(0))
            .unwrap(),
    )
}
fn fixture(root: &Path) -> PathBuf {
    let source = root.join("Selected");
    std::fs::create_dir_all(source.join("Clients")).unwrap();
    std::fs::create_dir_all(source.join("assets")).unwrap();
    write(
        &source.join("Clients/a.md"),
        "# First\n\n**Bold** and *italic*.\n\n![Pixel](../assets/pixel.png)\n\n[Report](../assets/report.pdf)\n\n[Second](b.md)",
    );
    write(&source.join("Clients/b.md"), "# Second\n\nThe linked note.");
    std::fs::write(
        source.join("assets/report.pdf"),
        b"%PDF-private-import-fixture",
    )
    .unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([20, 40, 60, 255]))
        .save(source.join("assets/pixel.png"))
        .unwrap();
    source
}
#[test]
fn reviewed_folder_snapshot_imports_media_note_links_and_fresh_topology_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let source = fixture(temp.path());
    let mut store = Store::open(&temp.path().join("library")).unwrap();
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(
        counts(&store),
        (0, 0, 0, 0),
        "Preparation cannot mutate the library"
    );
    assert_eq!(prepared.summary().files, 2);
    assert_eq!(
        prepared.summary().folders,
        2,
        "Asset-only directories are not note folders"
    );
    assert_eq!(prepared.summary().assets, 2);
    assert!(prepared.summary().warnings.is_empty());
    // Commit consumes the reviewed bytes, not a second reading of mutable sources.
    write(
        &source.join("Clients/b.md"),
        "A changed source after review",
    );
    let report = store
        .import_prepared(prepared, None, false, &Cancellation::default(), |_| {})
        .unwrap();
    assert_eq!(counts(&store), (2, 2, 1, 2));
    let first = report
        .notes
        .iter()
        .find(|n| n.body.title() == "First")
        .unwrap();
    let second = report
        .notes
        .iter()
        .find(|n| n.body.title() == "Second")
        .unwrap();
    assert!(
        first
            .body
            .leaves()
            .iter()
            .flat_map(|b| &b.spans)
            .any(|span| span.mark("bold"))
    );
    assert!(
        first
            .body
            .leaves()
            .iter()
            .flat_map(|b| &b.spans)
            .any(|span| span.mark("italic"))
    );
    assert!(
        first
            .body
            .leaves()
            .iter()
            .flat_map(|b| &b.spans)
            .any(|span| span.0.get("href").and_then(|v| v.as_str())
                == Some(format!("note://{}", second.id).as_str()))
    );
    let hash = first.body.references("image").into_iter().next().unwrap();
    assert_eq!(
        media::read_blob(&store.root, &hash).unwrap(),
        std::fs::read(source.join("assets/pixel.png")).unwrap()
    );
    let id = first
        .body
        .references("attachment")
        .into_iter()
        .next()
        .unwrap();
    let attachment = media::attachment(&store.db, &id).unwrap();
    assert_eq!(
        media::read_blob(&store.root, &attachment.hash).unwrap(),
        b"%PDF-private-import-fixture"
    );
    assert_eq!(
        store
            .query(&notes_core::library::Query {
                text: "linked".into(),
                ..Default::default()
            })
            .unwrap()
            .rows[0]
            .id,
        second.id
    );
    let before_ids: HashSet<_> = report.notes.iter().map(|note| note.id.clone()).collect();
    let again = prepare(vec![source], &Cancellation::default(), |_| {}).unwrap();
    let copied = store
        .import_prepared(again, None, false, &Cancellation::default(), |_| {})
        .unwrap();
    assert!(
        copied
            .notes
            .iter()
            .all(|note| !before_ids.contains(&note.id))
    );
    assert!(
        copied
            .folders
            .iter()
            .any(|folder| folder.name == "Selected (2)")
    );
    assert_eq!(
        counts(&store),
        (4, 4, 2, 2),
        "Immutable content is shared without overwriting sources"
    );
    let root = store.root.clone();
    drop(store);
    assert_eq!(Store::open(&root).unwrap().load().unwrap().notes.len(), 4);
}
#[test]
fn missing_remote_and_outside_resources_require_review_and_keep_visible_labels() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("notes");
    std::fs::create_dir(&source).unwrap();
    write(&temp.path().join("private.txt"), "Must not be copied");
    write(
        &source.join("a.md"),
        "# Warnings\n\n![Missing](missing.png) ![Remote](https://example.invalid/private.png)\n\n[Outside](../private.txt)",
    );
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(prepared.summary().warnings.len(), 3);
    assert_eq!(prepared.summary().assets, 0);
    let mut store = Store::open(&temp.path().join("library")).unwrap();
    assert!(
        store
            .import_prepared(prepared, None, false, &Cancellation::default(), |_| {})
            .is_err()
    );
    assert_eq!(counts(&store), (0, 0, 0, 0));
    let prepared = prepare(vec![source], &Cancellation::default(), |_| {}).unwrap();
    let report = store
        .import_prepared(prepared, None, true, &Cancellation::default(), |_| {})
        .unwrap();
    let doc = &report.notes[0].body;
    assert!(doc.plain_text().contains("[Missing]"));
    assert!(doc.plain_text().contains("[Remote]"));
    assert!(doc.plain_text().contains("Outside"));
    assert!(doc.references("image").is_empty() && doc.references("attachment").is_empty());
    assert!(
        doc.leaves()
            .iter()
            .flat_map(|b| &b.spans)
            .all(|span| span.0.get("href").is_none())
    );
}
#[test]
fn sql_failure_and_cancellation_roll_back_all_imported_rows() {
    let temp = tempfile::tempdir().unwrap();
    let source = fixture(temp.path());
    let mut store = Store::open(&temp.path().join("library")).unwrap();
    store.db.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON notes WHEN NEW.title='Second' BEGIN SELECT RAISE(ABORT,'injected import fault'); END;").unwrap();
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    assert!(
        store
            .import_prepared(prepared, None, false, &Cancellation::default(), |_| {})
            .is_err()
    );
    assert_eq!(counts(&store), (0, 0, 0, 0));
    store
        .db
        .execute_batch("DROP TRIGGER reject_second;")
        .unwrap();
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    let cancel = Cancellation::default();
    assert!(
        store
            .import_prepared(prepared, None, false, &cancel, |progress| {
                if progress.phase == Phase::Commit && progress.completed == 1 {
                    cancel.cancel();
                }
            })
            .is_err()
    );
    assert_eq!(counts(&store), (0, 0, 0, 0));
    let prepared = prepare(vec![source], &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(
        store
            .import_prepared(prepared, None, false, &Cancellation::default(), |_| {})
            .unwrap()
            .notes
            .len(),
        2,
        "Retry uses the same durable immutable assets safely"
    );
}
#[test]
fn quote_import_requires_the_existing_verified_upgrade_and_source_is_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("quote.md");
    let original = "# Quote\n\n> Kept as an editable quotation.";
    write(&source, original);
    let mut store = Store::open(&temp.path().join("library")).unwrap();
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    assert!(prepared.summary().needs_quotes);
    assert!(
        store
            .import_prepared(prepared, None, false, &Cancellation::default(), |_| {})
            .is_err()
    );
    assert_eq!(counts(&store), (0, 0, 0, 0));
    store
        .upgrade_for_quotes(
            &temp.path().join("before-quotes"),
            &migration::Cancellation::default(),
            |_| {},
        )
        .unwrap();
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    let report = store
        .import_prepared(prepared, None, false, &Cancellation::default(), |_| {})
        .unwrap();
    assert_eq!(report.notes[0].body.schema, 2);
    assert!(
        report.notes[0]
            .body
            .blocks
            .iter()
            .any(|block| block.kind == "quote")
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), original);
}
#[test]
fn symlinks_invalid_text_and_cancelled_scan_do_not_change_the_library() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    write(&source.join("a.md"), "# Source");
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&source, &alias).unwrap();
    assert!(prepare(vec![alias], &Cancellation::default(), |_| {}).is_err());
    std::os::unix::fs::symlink(temp.path().join("private"), source.join("resource.png")).unwrap();
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(prepared.summary().warnings.len(), 1);
    std::fs::write(source.join("bad.txt"), b"invalid UTF8\xff").unwrap();
    assert!(prepare(vec![source.clone()], &Cancellation::default(), |_| {}).is_err());
    let cancel = Cancellation::default();
    cancel.cancel();
    assert!(prepare(vec![source], &cancel, |_| {}).is_err());
    let store = Store::open(&temp.path().join("library")).unwrap();
    assert_eq!(counts(&store), (0, 0, 0, 0));
}

#[test]
fn unsupported_inline_css_tags_attributes_and_deep_headings_require_review() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("formatting.md");
    write(
        &source,
        "# Imported\n\nText <sup>raised</sup> <sub>lowered</sub> <font color=\"red\">colored</font> <span style=\"font-family: Fancy; color: red\">styled</span>.\n\n#### Deep heading",
    );
    let prepared = prepare(vec![source.clone()], &Cancellation::default(), |_| {}).unwrap();
    assert!(
        prepared
            .summary()
            .warnings
            .iter()
            .any(|warning| warning.contains("Unsupported HTML formatting"))
    );
    assert!(
        prepared
            .summary()
            .warnings
            .iter()
            .any(|warning| warning.contains("attributes or formatting"))
    );
    assert!(
        prepared
            .summary()
            .warnings
            .iter()
            .any(|warning| warning.contains("CSS formatting"))
    );
    assert!(
        prepared
            .summary()
            .warnings
            .iter()
            .any(|warning| warning.contains("level-three headings"))
    );
    let mut store = Store::open(&temp.path().join("library")).unwrap();
    assert!(
        store
            .import_prepared(prepared, None, false, &Cancellation::default(), |_| {})
            .is_err()
    );
    assert_eq!(counts(&store), (0, 0, 0, 0));
    let prepared = prepare(vec![source], &Cancellation::default(), |_| {}).unwrap();
    let report = store
        .import_prepared(prepared, None, true, &Cancellation::default(), |_| {})
        .unwrap();
    let text = report.notes[0].body.plain_text();
    assert!(
        text.contains("raised")
            && text.contains("lowered")
            && text.contains("colored")
            && text.contains("styled")
    );
}

#[test]
fn local_file_link_keeps_its_complete_styled_label_before_one_attachment() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("report.pdf"), b"%PDF-label-fixture").unwrap();
    std::fs::write(
        source.join("note.md"),
        "# Title\n\n[**Bold** Report](report.pdf) after",
    )
    .unwrap();
    let cancel = Cancellation::default();
    let prepared = prepare(vec![source.join("note.md")], &cancel, |_| {}).unwrap();
    let mut store = Store::open(&temporary.path().join("library")).unwrap();
    let report = store
        .import_prepared(prepared, None, true, &cancel, |_| {})
        .unwrap();
    let document = &report.notes[0].body;
    assert!(document.plain_text().contains("Bold Report"));
    assert_eq!(document.references("attachment").len(), 1);
    let paragraph = document
        .blocks
        .iter()
        .find(|b| b.plain_text().contains("Bold Report"))
        .unwrap();
    assert!(paragraph.spans[0].mark("bold"));
    let file_index = paragraph
        .spans
        .iter()
        .position(|s| s.0.get("attachment").is_some())
        .unwrap();
    let label: String = paragraph.spans[..file_index]
        .iter()
        .map(Span::content)
        .collect();
    assert_eq!(label, "Bold Report");
    assert!(
        paragraph.spans[file_index + 1..]
            .iter()
            .map(Span::content)
            .collect::<String>()
            .contains("after")
    );
}
