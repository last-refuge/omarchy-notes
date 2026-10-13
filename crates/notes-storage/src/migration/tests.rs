use super::*;
use crate::{SaveError, backup};
use notes_core::document::Block;
use std::{
    fs,
    time::{Duration, Instant},
};

fn fixture(root: &Path) -> PathBuf {
    fn copy(source: &Path, target: &Path) {
        fs::create_dir(target).unwrap();
        for file in fs::read_dir(source).unwrap() {
            let file = file.unwrap();
            if file.file_type().unwrap().is_dir() {
                copy(&file.path(), &target.join(file.file_name()));
            } else {
                fs::copy(file.path(), target.join(file.file_name())).unwrap();
            }
        }
    }
    let target = root.join("library");
    copy(Path::new("../../tests/fixtures/qt-v1"), &target);
    target
}
fn bodies(db: &Connection) -> Vec<(String, String, u32)> {
    db.prepare("SELECT id,body,body_schema FROM notes UNION ALL SELECT note_id,body,body_schema FROM note_revisions ORDER BY 1,2").unwrap()
        .query_map([], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}
fn quote() -> Document {
    let mut block = Block::paragraph("Keep room for a good thought.");
    block.set_kind("quote");
    Document::from_blocks(vec![block])
}

#[test]
fn quotes_require_explicit_backup_upgrade_and_survive_save_reopen_and_restore() {
    let temp = tempfile::tempdir().unwrap();
    let root = fixture(temp.path());
    let mut store = Store::open(&root).unwrap();
    let original = bodies(&store.db);
    let mut note = store.load().unwrap().notes.remove(0);
    note.body = quote();
    assert_eq!(store.save(&note).unwrap_err(), SaveError::UpgradeRequired);
    assert!(store.create(quote(), None).is_err());
    assert_eq!(bodies(&store.db), original);
    let target = temp.path().join("before-quotes");
    let receipt = store
        .upgrade_for_quotes(&target, &Cancellation::default(), |_| {})
        .unwrap();
    assert_eq!(store.schema().unwrap(), 3);
    assert_eq!(bodies(&store.db), original);
    assert_eq!(
        backup::inspect(&receipt.backup)
            .unwrap()
            .manifest
            .schema_version,
        2
    );
    let second = store
        .upgrade_for_quotes(
            &temp.path().join("unused"),
            &Cancellation::default(),
            |_| panic!("already upgraded"),
        )
        .unwrap();
    assert_eq!(second.database_sha256, receipt.database_sha256);
    assert!(!temp.path().join("unused").exists());
    note.revision = store.save(&note).unwrap().revision;
    let plain = store.create(Document::default(), None).unwrap();
    assert_eq!(plain.body.schema, 2);
    let new_backup = temp.path().join("new");
    store.backup_to(&new_backup).unwrap();
    drop(store);
    let store = Store::open(&root).unwrap();
    assert_eq!(
        store
            .load()
            .unwrap()
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap()
            .body,
        note.body
    );
    let old_restore = temp.path().join("rollback");
    backup::restore_to_new(&target, &old_restore).unwrap();
    let old = Store::open(&old_restore).unwrap();
    assert_eq!(old.schema().unwrap(), 2);
    assert_eq!(bodies(&old.db), original);
    let new_restore = temp.path().join("new-restore");
    backup::restore_to_new(&new_backup, &new_restore).unwrap();
    let new = Store::open(&new_restore).unwrap();
    assert_eq!(new.schema().unwrap(), 3);
    assert_eq!(bodies(&new.db), bodies(&store.db));
}

#[test]
fn cancellation_failures_and_retry_leave_bodies_and_schema_consistent() {
    for phase in [
        Progress::BackingUp,
        Progress::Verifying,
        Progress::Committing,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = fixture(temp.path());
        let mut store = Store::open(&root).unwrap();
        let original = bodies(&store.db);
        let cancel = Cancellation::default();
        let result = store.upgrade_for_quotes(&temp.path().join("cancelled"), &cancel, |at| {
            if at == phase {
                cancel.cancel()
            }
        });
        assert!(result.is_err());
        assert_eq!(store.schema().unwrap(), 2);
        assert_eq!(bodies(&store.db), original);
        assert!(receipt(&store.db).is_err());
        store
            .upgrade_for_quotes(&temp.path().join("retry"), &Cancellation::default(), |_| {})
            .unwrap();
        assert_eq!(store.schema().unwrap(), 3);
    }
    for phase in [Boundary::BackedUp, Boundary::BeforeCommit] {
        let temp = tempfile::tempdir().unwrap();
        let root = fixture(temp.path());
        let mut store = Store::open(&root).unwrap();
        let original = bodies(&store.db);
        assert!(
            store
                .upgrade_with(
                    &temp.path().join("failed"),
                    &Cancellation::default(),
                    |_| {},
                    |at| {
                        if at == phase {
                            return Err(std::io::Error::from_raw_os_error(libc::ENOSPC).into());
                        }
                        Ok(())
                    }
                )
                .is_err()
        );
        assert_eq!(store.schema().unwrap(), 2);
        assert_eq!(bodies(&store.db), original);
        assert!(receipt(&store.db).is_err());
        backup::inspect(&temp.path().join("failed")).unwrap();
    }
}

#[test]
fn unsupported_raw_history_and_inconsistent_database_versions_are_refused_before_writes() {
    for json in [
        r#"{"schema":99,"blocks":[]}"#.to_string(),
        serde_json::to_string(&quote()).unwrap(),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = fixture(temp.path());
        let db = Connection::open(root.join("library.sqlite3")).unwrap();
        db.execute("UPDATE note_revisions SET body=?1", [json])
            .unwrap();
        drop(db);
        let before = fs::read(root.join("library.sqlite3")).unwrap();
        assert!(Store::open(&root).is_err());
        assert_eq!(fs::read(root.join("library.sqlite3")).unwrap(), before);
    }
}

#[test]
fn killed_upgrades_are_atomic_and_retryable() {
    use std::process::{Command, Stdio};
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for phase in [
        Boundary::BackedUp,
        Boundary::BeforeCommit,
        Boundary::Committed,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = fixture(temp.path());
        let original = bodies(&Connection::open(root.join("library.sqlite3")).unwrap());
        let marker = temp.path().join("ready");
        let target = temp.path().join("backup");
        let log = temp.path().join("child.log");
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "migration::tests::crash_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("NOTES_MIGRATION_SOURCE", &root)
                .env("NOTES_MIGRATION_TARGET", &target)
                .env("NOTES_MIGRATION_MARKER", &marker)
                .env("NOTES_MIGRATION_PHASE", format!("{phase:?}"))
                .stdout(fs::File::create(&log).unwrap())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while !marker.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
                "{}",
                fs::read_to_string(&log).unwrap()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut store = Store::open(&root).unwrap();
        assert_eq!(bodies(&store.db), original);
        assert_eq!(
            store.schema().unwrap(),
            if phase == Boundary::Committed { 3 } else { 2 }
        );
        backup::inspect(&target).unwrap();
        let retry = temp.path().join("retry");
        store
            .upgrade_for_quotes(&retry, &Cancellation::default(), |_| {})
            .unwrap();
        assert_eq!(store.schema().unwrap(), 3);
        assert_eq!(bodies(&store.db), original);
        assert_eq!(retry.exists(), phase != Boundary::Committed);
    }
}
#[test]
#[ignore = "subprocess helper invoked by migration kill test"]
fn crash_child() {
    let source = PathBuf::from(std::env::var_os("NOTES_MIGRATION_SOURCE").unwrap());
    let target = PathBuf::from(std::env::var_os("NOTES_MIGRATION_TARGET").unwrap());
    let marker = PathBuf::from(std::env::var_os("NOTES_MIGRATION_MARKER").unwrap());
    let phase = std::env::var("NOTES_MIGRATION_PHASE").unwrap();
    let mut store = Store::open(&source).unwrap();
    store
        .upgrade_with(
            &target,
            &Cancellation::default(),
            |_| {},
            |at| {
                if format!("{at:?}") == phase {
                    fs::write(&marker, b"ready")?;
                    loop {
                        std::thread::park();
                    }
                }
                Ok(())
            },
        )
        .unwrap();
}
