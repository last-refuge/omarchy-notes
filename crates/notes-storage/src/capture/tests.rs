use super::*;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
fn request() -> Capture {
    Capture {
        request_id: notes_core::document::new_id(),
        text: "A thought\r\nWith #capture and 👩‍💻".into(),
    }
}
#[test]
fn capture_is_atomic_idempotent_and_survives_edits_deletion_and_backup() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let mut s = Store::open(&root).unwrap();
    let req = request();
    let first = s.capture(&req).unwrap();
    let receipt = first.receipt;
    assert_eq!(s.schema().unwrap(), 2);
    assert_eq!(
        s.capture_status(&req.request_id).unwrap(),
        Some(receipt.clone())
    );
    assert_eq!(
        first.created.as_ref().unwrap().body.plain_text(),
        "A thought\nWith #capture and 👩‍💻"
    );
    let mut note = first.created.unwrap();
    note.body = Document::from_blocks(vec![Block::paragraph("Edited elsewhere")]);
    s.save(&note).unwrap();
    assert!(s.capture(&req).unwrap().created.is_none());
    assert_eq!(s.load().unwrap().notes.len(), 1);
    let mut changed = req.clone();
    changed.text.push('!');
    assert!(s.capture(&changed).is_err());
    s.db.execute("DELETE FROM notes WHERE id=?1", [&receipt.note_id])
        .unwrap();
    let repeated = s.capture(&req).unwrap();
    assert_eq!(repeated.receipt, receipt);
    assert!(repeated.created.is_none());
    assert!(s.load().unwrap().notes.is_empty());
    let backup = temp.path().join("backup");
    s.backup_to(&backup).unwrap();
    drop(s);
    let restored = temp.path().join("restored");
    crate::backup::restore_to_new(&backup, &restored).unwrap();
    let mut s = Store::open(&restored).unwrap();
    assert_eq!(s.capture(&req).unwrap().receipt, receipt);
    assert!(s.load().unwrap().notes.is_empty());
}
#[test]
fn rejected_or_failed_capture_never_leaves_a_note_or_receipt() {
    let t = tempfile::tempdir().unwrap();
    let mut s = Store::open(t.path()).unwrap();
    let req = request();
    assert!(
        s.capture_with(
            &req,
            || anyhow::bail!("injected disk write failure"),
            || Ok(())
        )
        .is_err()
    );
    assert!(s.load().unwrap().notes.is_empty());
    assert!(s.capture_status(&req.request_id).unwrap().is_none());
    s.fail_saves = true;
    assert!(s.capture(&req).is_err());
    s.fail_saves = false;
    for text in ["".to_string(), " \n\t".into(), "x".repeat(MAX_TEXT + 1)] {
        assert!(
            s.capture(&Capture {
                request_id: req.request_id.clone(),
                text
            })
            .is_err()
        );
    }
    assert!(
        s.capture(&Capture {
            request_id: "not-an-id".into(),
            text: "keep".into()
        })
        .is_err()
    );
    assert!(s.load().unwrap().notes.is_empty());
    let first = s.capture(&req).unwrap().receipt;
    s.fail_saves = true;
    assert_eq!(
        s.capture(&req).unwrap().receipt,
        first,
        "a committed lookup needs no new write"
    );
}
#[test]
fn lookup_does_not_create_or_migrate_a_library() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("missing");
    let req = request();
    assert!(inspect(&root, &req.request_id).unwrap().is_none());
    assert!(!root.exists());
    let s = Store::open(&root).unwrap();
    s.db.pragma_update(None, "user_version", 1).unwrap();
    drop(s);
    let path = root.join("library.sqlite3");
    let before = std::fs::read(&path).unwrap();
    assert!(inspect(&root, &req.request_id).unwrap().is_none());
    assert_eq!(std::fs::read(path).unwrap(), before);
}
struct Kill(Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn lost_reply_and_killed_capture_retry_without_duplicate_notes() {
    for boundary in ["before", "after"] {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("library");
        let ready = t.path().join("ready");
        let req = request();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "capture::tests::capture_child", "--ignored"])
            .env("NOTES_CAPTURE_TEST_ROOT", &root)
            .env("NOTES_CAPTURE_TEST_READY", &ready)
            .env("NOTES_CAPTURE_TEST_BOUNDARY", boundary)
            .env("NOTES_CAPTURE_TEST_ID", &req.request_id)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut child = Kill(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready.exists() {
            assert!(child.0.try_wait().unwrap().is_none());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut s = Store::open(&root).unwrap();
        assert_eq!(
            s.load().unwrap().notes.len(),
            usize::from(boundary == "after")
        );
        assert_eq!(
            s.capture_status(&req.request_id).unwrap().is_some(),
            boundary == "after"
        );
        let saved = s.capture(&req).unwrap();
        assert_eq!(saved.created.is_some(), boundary == "before");
        assert_eq!(s.capture(&req).unwrap().receipt, saved.receipt);
        assert_eq!(s.load().unwrap().notes.len(), 1);
    }
}
#[test]
#[ignore = "subprocess helper invoked by capture kill test"]
fn capture_child() {
    let root = std::env::var("NOTES_CAPTURE_TEST_ROOT").unwrap();
    let phase = std::env::var("NOTES_CAPTURE_TEST_BOUNDARY").unwrap();
    let mut req = request();
    req.request_id = std::env::var("NOTES_CAPTURE_TEST_ID").unwrap();
    let stop = |at: &str| -> Result<()> {
        if phase == at {
            std::fs::write(std::env::var("NOTES_CAPTURE_TEST_READY").unwrap(), b"ready")?;
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        Ok(())
    };
    Store::open(Path::new(&root))
        .unwrap()
        .capture_with(&req, || stop("before"), || stop("after"))
        .unwrap();
}

#[test]
fn corrupt_receipts_are_not_reported_as_saved_or_replaced() {
    let t = tempfile::tempdir().unwrap();
    let mut s = Store::open(t.path()).unwrap();
    let req = request();
    s.capture(&req).unwrap();
    s.db.execute(
        "UPDATE rust_capture_requests_v1 SET payload_sha256='bad'",
        [],
    )
    .unwrap();
    assert!(s.capture_status(&req.request_id).is_err());
    assert!(s.capture(&req).is_err());
    assert_eq!(s.load().unwrap().notes.len(), 1);
    let digest: String =
        s.db.query_row(
            "SELECT payload_sha256 FROM rust_capture_requests_v1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(digest, "bad");
}
