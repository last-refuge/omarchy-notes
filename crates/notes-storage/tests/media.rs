use notes_core::document::{Block, Document, Span};
use notes_exchange::Fragment;
use notes_storage::{
    Store,
    media::{self, Import},
};
use std::path::PathBuf;
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/qt-v1/blobs")
        .join(name)
}
fn image_path() -> PathBuf {
    fixture("bd/bdec1de53e3c856d6973c7324a5d719d6f6169e01a6f75ae3b4453f419d625e2")
}
fn pdf_path() -> PathBuf {
    fixture("d7/d7449961543b028f75a5778a6cbcfb8d3b7ea2dfe66da1efc60d9747f4fc4803")
}
fn files() -> Import {
    Import::Files {
        paths: vec![image_path(), pdf_path()],
        images: None,
    }
}
#[test]
fn files_survive_reopen_and_portable_copy_remaps_attachment_ownership() {
    let source = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let mut store = Store::open(source.path()).unwrap();
    let mut note = store.create(Document::default(), None).unwrap();
    note.body = store.import_media(&note.id, files()).unwrap();
    store.save(&note).unwrap();
    drop(store);
    let store = Store::open(source.path()).unwrap();
    let note = store.load().unwrap().notes.remove(0);
    let items = media::load(source.path(), &note.body).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|m| m.error.is_none()));
    assert!(items[0].preview.is_some());
    assert_eq!(items[1].mime, "application/pdf");
    let portable = media::portable(source.path(), note.body.clone()).unwrap();
    let fragment = Fragment::decode(&portable.encode().unwrap()).unwrap();
    let mut target = Store::open(dest.path()).unwrap();
    let mut copy = target.create(Document::default(), None).unwrap();
    copy.body = target
        .import_media(&copy.id, Import::Fragment(fragment))
        .unwrap();
    target.save(&copy).unwrap();
    assert_eq!(copy.body.references("image"), note.body.references("image"));
    assert_ne!(
        copy.body.references("attachment"),
        note.body.references("attachment")
    );
    let db = rusqlite::Connection::open(dest.path().join("library.sqlite3")).unwrap();
    let owner: String = db
        .query_row("SELECT note_id FROM attachments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(owner, copy.id);
    for item in &items {
        assert_eq!(
            media::read_blob(dest.path(), &item.hash).unwrap(),
            media::read_blob(source.path(), &item.hash).unwrap()
        );
    }
    // Saving never overwrites an existing path; image alt text without an extension can open.
    let output = dest.path().join("export.pdf");
    media::export_file(source.path(), &items[1], &output).unwrap();
    assert_eq!(
        std::fs::read(&output).unwrap(),
        std::fs::read(pdf_path()).unwrap()
    );
    assert!(media::export_file(source.path(), &items[1], &output).is_err());
    assert_eq!(
        media::open_copy(source.path(), &items[0])
            .unwrap()
            .extension()
            .unwrap(),
        "png"
    );
    // A lone image offers native image bytes in addition to rich/plain text.
    let mut image_doc = note.body.clone();
    image_doc.blocks.truncate(1);
    assert_eq!(
        media::portable(source.path(), image_doc)
            .unwrap()
            .offers()
            .unwrap()["image/png"],
        std::fs::read(image_path()).unwrap()
    );
}
#[test]
fn invalid_or_failed_import_never_changes_note_or_attachment_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let note = store.create(Document::default(), None).unwrap();
    store.fail_saves = true;
    assert!(store.import_media(&note.id, files()).is_err());
    store.fail_saves = false;
    assert!(
        store
            .import_media(
                &note.id,
                Import::Image {
                    bytes: b"invalid".to_vec(),
                    name: "Bad.png".into()
                }
            )
            .is_err()
    );
    let doc = store.import_media(&note.id, files()).unwrap();
    let mut fragment = media::portable(dir.path(), doc.clone()).unwrap();
    fragment.bundle.assets[0].hash = "0".repeat(64);
    assert!(
        store
            .import_media(&note.id, Import::Fragment(fragment))
            .is_err()
    );
    let db = rusqlite::Connection::open(dir.path().join("library.sqlite3")).unwrap();
    let count: u32 = db
        .query_row("SELECT count(*) FROM attachments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(store.load().unwrap().notes[0].body, note.body);
    // Missing/corrupt media is a visible error and cannot be copied as if intact.
    let hash = doc.references("image")[0].clone();
    let path = dir.path().join("blobs").join(&hash[..2]).join(&hash);
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(media::portable(dir.path(), doc.clone()).is_err());
    let loaded = media::load(dir.path(), &doc).unwrap();
    assert!(loaded[0].error.as_ref().unwrap().contains("integrity"));
    assert!(loaded[0].preview.is_none());
    std::fs::remove_file(path).unwrap();
    assert!(media::load(dir.path(), &doc).unwrap()[0].error.is_some());
}
#[test]
fn duplicate_files_deduplicate_bytes_and_executables_require_export() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let note = store.create(Document::default(), None).unwrap();
    let doc = store
        .import_media(
            &note.id,
            Import::Files {
                paths: vec![image_path(), image_path()],
                images: Some(true),
            },
        )
        .unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.references("image").len(), 1);
    let path = dir.path().join("looks-like-a-document.pdf");
    std::fs::write(&path, b"#!/bin/sh\necho NO").unwrap();
    let doc = store
        .import_media(
            &note.id,
            Import::Files {
                paths: vec![path],
                images: Some(false),
            },
        )
        .unwrap();
    let item = media::load(dir.path(), &doc).unwrap().remove(0);
    assert!(media::open_copy(dir.path(), &item).is_err());
    media::export_file(dir.path(), &item, &dir.path().join("exported.pdf")).unwrap();
}
#[test]
fn references_from_another_library_require_portable_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let note = store.create(Document::default(), None).unwrap();
    let mut block = Block::paragraph("");
    block.spans.push(Span(
        serde_json::json!({"image":"a".repeat(64),"alt":"Missing"}),
    ));
    let fragment = Fragment {
        document: Document {
            blocks: vec![block],
            ..Default::default()
        },
        bundle: Default::default(),
    };
    assert!(
        store
            .import_media(&note.id, Import::Fragment(fragment))
            .is_err()
    );
    assert_eq!(store.load().unwrap().notes[0].body, note.body);
}

#[test]
fn recovery_note_copy_owns_its_attachment_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let source = store.create(Document::default(), None).unwrap();
    let doc = store.import_media(&source.id, files()).unwrap();
    let copy = store.create(doc.clone(), None).unwrap();
    assert_ne!(
        doc.references("attachment"),
        copy.body.references("attachment")
    );
    let db = rusqlite::Connection::open(dir.path().join("library.sqlite3")).unwrap();
    let owner: String = db
        .query_row(
            "SELECT note_id FROM attachments WHERE id=?1",
            [&copy.body.references("attachment")[0]],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(owner, copy.id);
}
#[test]
fn special_files_are_rejected_without_blocking() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let note = store.create(Document::default(), None).unwrap();
    let path = dir.path().join("pipe");
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: valid NUL-terminated pathname in an owned temporary directory.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(
        store
            .import_media(
                &note.id,
                Import::Files {
                    paths: vec![path],
                    images: None
                }
            )
            .is_err()
    );
}

#[test]
fn oversized_files_and_image_dimensions_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let note = store.create(Document::default(), None).unwrap();
    let path = dir.path().join("large.bin");
    std::fs::File::create(&path)
        .unwrap()
        .set_len((notes_exchange::MAX_ASSET + 1) as u64)
        .unwrap();
    assert!(
        store
            .import_media(
                &note.id,
                Import::Files {
                    paths: vec![path],
                    images: Some(false)
                }
            )
            .is_err()
    );
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbaImage::new(12001, 1)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    assert!(
        store
            .import_media(
                &note.id,
                Import::Image {
                    bytes: bytes.into_inner(),
                    name: "wide.png".into()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("12,000")
    );
    assert_eq!(store.load().unwrap().notes[0].body, note.body);
}
