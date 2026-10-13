use super::*;
use notes_core::document::Span;
use std::os::unix::fs::{PermissionsExt, symlink};

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for item in fs::read_dir(source).unwrap() {
        let item = item.unwrap();
        let path = target.join(item.file_name());
        if item.file_type().unwrap().is_dir() {
            copy_tree(&item.path(), &path);
        } else {
            fs::copy(item.path(), path).unwrap();
        }
    }
}
fn fixture(root: &Path) -> PathBuf {
    let path = root.join("library");
    copy_tree(Path::new("../../tests/fixtures/qt-v1"), &path);
    path
}
fn database(root: &Path) -> Connection {
    Connection::open(root.join(DATABASE)).unwrap()
}
fn tree_hashes(root: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(root: &Path, directory: &Path, out: &mut BTreeMap<PathBuf, String>) {
        for item in fs::read_dir(directory).unwrap() {
            let item = item.unwrap();
            if item.file_type().unwrap().is_dir() {
                visit(root, &item.path(), out);
            } else {
                out.insert(
                    item.path().strip_prefix(root).unwrap().to_owned(),
                    hash_file(&item.path()).unwrap().1,
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
fn logical_dump(root: &Path) -> BTreeMap<String, Vec<String>> {
    let db = immutable_database(&root.join(DATABASE)).unwrap();
    let mut names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names = names
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    names
        .into_iter()
        .map(|name| {
            let mut query = db
                .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                .unwrap();
            let count = query.column_count();
            let mut rows = query
                .query_map([], |r| {
                    (0..count)
                        .map(|i| r.get::<_, rusqlite::types::Value>(i))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .map(|r| format!("{:?}", r.unwrap()))
                .collect::<Vec<_>>();
            rows.sort();
            (name, rows)
        })
        .collect()
}
fn refresh_legacy_manifest(root: &Path) {
    let path = root.join("manifest.json");
    let mut manifest: Manifest = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest.database_sha256 = None;
    let db = database(root);
    manifest.schema_version = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    manifest.bytes = db
        .query_row::<u64, _, _>("SELECT coalesce(sum(size),0) FROM blobs", [], |r| r.get(0))
        .unwrap()
        + fs::metadata(root.join(DATABASE)).unwrap().len();
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
}

#[test]
fn wal_snapshot_preserves_all_tables_history_trash_and_assets() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fixture(temporary.path());
    let backup = temporary.path().join("backup ? # ü");
    let restored = temporary.path().join("restored");
    let mut store = Store::open(&root).unwrap();
    store
        .db
        .execute_batch(
            "INSERT INTO folders VALUES('parent',NULL,'Parent',7,11,12,NULL);
        INSERT INTO folders VALUES('child','parent','Child',8,13,14,NULL);
        INSERT INTO smart_folders VALUES('smart','History','{\"tags\":[\"planning\"]}',5,15,16);
        UPDATE notes SET pinned=1,deleted_at=17,folder_id='child' WHERE title='Groceries';",
        )
        .unwrap();
    let mut body = Document::default();
    body.blocks[0].spans = vec![Span::text("Acknowledged in WAL")];
    let note = store.create(body, None).unwrap();
    assert!(root.join("library.sqlite3-wal").exists());
    let result = store.backup_to(&backup).unwrap();
    assert_eq!((result.manifest.notes, result.manifest.blobs), (4, 2));
    assert_eq!(
        fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(backup.join(DATABASE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let before = tree_hashes(&backup);
    assert_eq!(inspect(&backup).unwrap().manifest, result.manifest);
    restore_to_new(&backup, &restored).unwrap();
    assert_eq!(before, tree_hashes(&backup));
    assert_eq!(logical_dump(&backup), logical_dump(&restored));
    assert!(!restored.join("manifest.json").exists());
    let mut reopened = Store::open(&restored).unwrap();
    let found = reopened
        .load()
        .unwrap()
        .notes
        .into_iter()
        .find(|n| n.id == note.id)
        .unwrap();
    assert_eq!(found.body.plain_text(), "Acknowledged in WAL");
    reopened.create(Document::default(), None).unwrap();
    assert_eq!(store.load().unwrap().notes.len(), 3); // the trashed note remains hidden
    assert_eq!(before, tree_hashes(&backup));
}

#[test]
fn old_manifest_and_schema_one_restore_without_migrating_source() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fixture(temporary.path());
    database(&root).execute_batch("DROP TABLE note_tags; DROP TABLE note_attachment_refs; DROP TABLE smart_folders; ALTER TABLE notes DROP COLUMN thumbnail; PRAGMA user_version=1;").unwrap();
    let original = tree_hashes(&root);
    let backup = temporary.path().join("old");
    assert_eq!(create(&root, &backup).unwrap().manifest.schema_version, 1);
    assert_eq!(original, tree_hashes(&root));
    refresh_legacy_manifest(&backup);
    let old = tree_hashes(&backup);
    assert!(inspect(&backup).unwrap().manifest.database_sha256.is_none());
    let restored = temporary.path().join("restored");
    restore_to_new(&backup, &restored).unwrap();
    assert_eq!(logical_dump(&backup), logical_dump(&restored));
    assert_eq!(old, tree_hashes(&backup));
    let store = Store::open(&restored).unwrap();
    assert_eq!(store.load().unwrap().notes.len(), 3);
    assert_eq!(supported_schema(&store.db).unwrap(), 2);
    assert_eq!(original, tree_hashes(&root));
}

#[test]
fn backups_stream_legacy_assets_above_the_editor_import_limit() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fixture(temporary.path());
    let large = root.join("large");
    let file = new_file(&large).unwrap();
    file.set_len(33 * 1024 * 1024).unwrap();
    let (size, hash) = hash_file(&large).unwrap();
    let shard = root.join("blobs").join(&hash[..2]);
    fs::create_dir_all(&shard).unwrap();
    fs::rename(large, shard.join(&hash)).unwrap();
    database(&root)
        .execute(
            "INSERT INTO blobs VALUES(?1,?2,0)",
            rusqlite::params![hash, size],
        )
        .unwrap();
    let backup = temporary.path().join("backup");
    assert_eq!(create(&root, &backup).unwrap().manifest.blobs, 3);
    let restored = temporary.path().join("restored");
    restore_to_new(&backup, &restored).unwrap();
    assert_eq!(
        hash_file(&blob_path(&restored, &hash).unwrap()).unwrap(),
        (size, hash)
    );
}

#[test]
fn damaged_assets_database_and_manifest_never_publish_a_restore() {
    for corruption in [
        "missing",
        "same-size",
        "database",
        "future-format",
        "future-schema",
        "counts",
        "sidecar",
        "symlink",
        "fifo",
        "shard-symlink",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = fixture(temporary.path());
        let backup = temporary.path().join("backup");
        create(&root, &backup).unwrap();
        let hash: String = database(&backup)
            .query_row("SELECT hash FROM blobs LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let blob = blob_path(&backup, &hash).unwrap();
        match corruption {
            "missing" => fs::remove_file(&blob).unwrap(),
            "same-size" => {
                let mut bytes = fs::read(&blob).unwrap();
                bytes[0] ^= 1;
                fs::write(&blob, bytes).unwrap();
            }
            "database" => {
                database(&backup)
                    .execute("UPDATE notes SET title='Damaged'", [])
                    .unwrap();
            }
            "sidecar" => {
                fs::write(backup.join("library.sqlite3-wal"), b"unfinished").unwrap();
            }
            "symlink" => {
                fs::remove_file(&blob).unwrap();
                symlink(blob_path(&root, &hash).unwrap(), &blob).unwrap();
            }
            "fifo" => {
                fs::remove_file(&blob).unwrap();
                let path = CString::new(blob.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "shard-symlink" => {
                let shard = blob.parent().unwrap();
                fs::remove_dir_all(shard).unwrap();
                symlink(root.join("blobs").join(&hash[..2]), shard).unwrap();
            }
            other => {
                let path = backup.join("manifest.json");
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                let (key, value_number) = match other {
                    "future-format" => ("formatVersion", 2),
                    "future-schema" => ("schemaVersion", 99),
                    _ => ("notes", 999),
                };
                value[key] = value_number.into();
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
        }
        let target = temporary.path().join("restore");
        assert!(inspect(&backup).is_err(), "{corruption}");
        assert!(restore_to_new(&backup, &target).is_err(), "{corruption}");
        assert!(!target.exists());
    }
}

#[test]
fn unsupported_documents_history_and_broken_references_are_refused() {
    for corruption in [
        "future-database",
        "future-body",
        "version-mismatch",
        "history-image",
        "attachment-ref",
        "blob-hash",
        "foreign-key",
        "fts",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = fixture(temporary.path());
        let db = database(&root);
        match corruption {
            "future-database" => db.pragma_update(None, "user_version", 99).unwrap(),
            "future-body" => {
                db.execute(
                    "UPDATE note_revisions SET body=json_set(body,'$.schema',99)",
                    [],
                )
                .unwrap();
            }
            "version-mismatch" => {
                db.execute("UPDATE note_revisions SET body_schema=2", [])
                    .unwrap();
            }
            "history-image" => {
                let mut body = Document::default();
                body.blocks[0].spans = vec![Span(serde_json::json!({"image":"0".repeat(64)}))];
                db.execute(
                    "UPDATE note_revisions SET body=?1",
                    [serde_json::to_string(&body).unwrap()],
                )
                .unwrap();
            }
            "attachment-ref" => {
                db.execute(
                    "UPDATE note_attachment_refs SET attachment_id='missing'",
                    [],
                )
                .unwrap();
            }
            "blob-hash" => {
                db.execute("INSERT INTO blobs VALUES('../../bad',0,0)", [])
                    .unwrap();
            }
            "foreign-key" => {
                db.pragma_update(None, "foreign_keys", false).unwrap();
                db.execute("UPDATE notes SET folder_id='missing'", [])
                    .unwrap();
            }
            "fts" => {
                db.execute("INSERT INTO notes_fts(notes_fts) VALUES('delete-all')", [])
                    .unwrap();
            }
            _ => unreachable!(),
        }
        drop(db);
        let before = tree_hashes(&root);
        let backup = temporary.path().join("backup");
        assert!(create(&root, &backup).is_err(), "{corruption}");
        assert!(!backup.exists());
        assert_eq!(before, tree_hashes(&root));
        assert!(!fs::read_dir(temporary.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".partial")
        }));
    }
}

#[test]
fn existing_targets_races_and_unrelated_partial_directories_are_preserved() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fixture(temporary.path());
    let unrelated = temporary.path().join("backup.partial");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("important"), b"keep").unwrap();
    let target = temporary.path().join("backup");
    let db = database(&root);
    let result = create_from(&db, &root, &target, &mut |phase| {
        if phase == Phase::BeforePublish {
            fs::create_dir(&target)?;
            fs::write(target.join("winner"), b"mine")?;
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(fs::read(target.join("winner")).unwrap(), b"mine");
    assert_eq!(fs::read(unrelated.join("important")).unwrap(), b"keep");
    assert!(create(&root, &target).is_err());
    fs::remove_file(target.join("winner")).unwrap();
    assert!(create(&root, &target).is_err()); // even an empty directory
    fs::remove_dir(&target).unwrap();
    symlink(temporary.path().join("missing"), &target).unwrap();
    assert!(create(&root, &target).is_err());
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fs::remove_file(&target).unwrap();
    create(&root, &target).unwrap();
    let before = tree_hashes(&root);
    assert!(restore_to_new(&target, &root).is_err());
    assert_eq!(before, tree_hashes(&root));
}

#[test]
fn injected_write_failures_preserve_sources_and_never_acknowledge_partial_copies() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fixture(temporary.path());
    let original = tree_hashes(&root);
    let db = database(&root);
    for phase in [
        Phase::Snapshot,
        Phase::Blob,
        Phase::Manifest,
        Phase::BeforePublish,
        Phase::Published,
    ] {
        let target = temporary.path().join(format!("{phase:?}"));
        let result = create_from(&db, &root, &target, &mut |at| {
            if at == phase {
                return Err(std::io::Error::from_raw_os_error(libc::ENOSPC).into());
            }
            Ok(())
        });
        assert!(result.is_err());
        if phase == Phase::Published {
            inspect(&target).unwrap();
        } else {
            assert!(!target.exists());
        }
        assert_eq!(original, tree_hashes(&root));
    }
    let source = temporary.path().join("Published");
    let original_backup = tree_hashes(&source);
    for phase in [
        Phase::Snapshot,
        Phase::Blob,
        Phase::BeforePublish,
        Phase::Published,
    ] {
        let target = temporary.path().join(format!("restore-{phase:?}"));
        assert!(
            restore_with(&source, &target, &mut |at| {
                if at == phase {
                    anyhow::bail!("Injected write failure");
                }
                Ok(())
            })
            .is_err()
        );
        if phase == Phase::Published {
            assert_eq!(logical_dump(&source), logical_dump(&target));
        } else {
            assert!(!target.exists());
        }
        assert_eq!(original_backup, tree_hashes(&source));
    }
}

#[test]
fn library_ownership_blocks_backup_without_modifying_the_source() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fixture(temporary.path());
    let mut store = Store::open(&root).unwrap();
    let target = temporary.path().join("backup");
    assert!(create(&root, &target).is_err());
    assert!(!target.exists());
    store.backup_to(&target).unwrap();
}

#[test]
fn restoring_revalidates_copied_bytes_and_search_index() {
    let temporary = tempfile::tempdir().unwrap();
    let root = fixture(temporary.path());
    let backup = temporary.path().join("backup");
    create(&root, &backup).unwrap();
    let hash: String = database(&backup)
        .query_row("SELECT hash FROM blobs LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let target = temporary.path().join("restore");
    assert!(
        restore_with(&backup, &target, &mut |at| {
            if at == Phase::Snapshot {
                fs::write(blob_path(&backup, &hash)?, b"changed")?;
            }
            Ok(())
        })
        .is_err()
    );
    assert!(!target.exists());
    fs::copy(
        blob_path(&root, &hash).unwrap(),
        blob_path(&backup, &hash).unwrap(),
    )
    .unwrap();
    // A successful write syscall isn't our only check: reread staged assets
    // before publication and detect corruption of the destination itself.
    let mut corrupted = false;
    assert!(
        restore_with(&backup, &target, &mut |at| {
            if at == Phase::Blob && !corrupted {
                let stage = fs::read_dir(temporary.path())?
                    .find_map(|entry| {
                        let entry = entry.ok()?;
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with(".omarchy-notes-")
                            .then(|| entry.path())
                    })
                    .unwrap();
                let shard = fs::read_dir(stage.join("blobs"))?.next().unwrap()?.path();
                let asset = fs::read_dir(shard)?.next().unwrap()?.path();
                let mut bytes = fs::read(&asset)?;
                bytes[0] ^= 1;
                fs::write(asset, bytes)?;
                corrupted = true;
            }
            Ok(())
        })
        .is_err()
    );
    assert!(corrupted && !target.exists());
    database(&backup)
        .execute("INSERT INTO notes_fts(notes_fts) VALUES('delete-all')", [])
        .unwrap();
    refresh_legacy_manifest(&backup);
    assert!(restore_to_new(&backup, &target).is_err());
    assert!(!target.exists());
}

#[test]
fn sigkill_at_publication_boundaries_leaves_originals_and_only_complete_destinations() {
    use std::process::{Command, Stdio};
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for mode in ["backup", "restore"] {
        for phase in [
            Phase::Snapshot,
            Phase::Blob,
            Phase::Manifest,
            Phase::BeforePublish,
            Phase::Published,
        ] {
            if mode == "restore" && phase == Phase::Manifest {
                continue;
            }
            let temporary = tempfile::tempdir().unwrap();
            let root = fixture(temporary.path());
            let source = if mode == "restore" {
                let backup = temporary.path().join("source");
                create(&root, &backup).unwrap();
                backup
            } else {
                root.clone()
            };
            let original = tree_hashes(&source);
            let target = temporary.path().join("destination");
            let marker = temporary.path().join("ready");
            let log = temporary.path().join("child.log");
            let mut child = Child(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "backup::tests::crash_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("NOTES_BACKUP_TEST_SOURCE", &source)
                    .env("NOTES_BACKUP_TEST_TARGET", &target)
                    .env("NOTES_BACKUP_TEST_PHASE", format!("{phase:?}"))
                    .env("NOTES_BACKUP_TEST_MODE", mode)
                    .env("NOTES_BACKUP_TEST_MARKER", &marker)
                    .stdout(File::create(&log).unwrap())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(15);
            while !marker.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
                    "Child did not reach {mode}/{phase:?}: {}",
                    fs::read_to_string(&log).unwrap()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            if mode == "backup" {
                // Stale Rust ownership is safely reclaimed after the killed PID
                // has been reaped; the test never edits/removes a live lock.
                let lock = LibraryLock::acquire(&source).unwrap();
                drop(lock);
            }
            assert_eq!(original, tree_hashes(&source));
            if phase == Phase::Published {
                if mode == "backup" {
                    inspect(&target).unwrap();
                } else {
                    assert_eq!(logical_dump(&source), logical_dump(&target));
                }
            } else {
                assert!(!target.exists());
            }
            let retry = temporary.path().join("retry");
            if mode == "backup" {
                create(&source, &retry).unwrap();
            } else {
                restore_to_new(&source, &retry).unwrap();
            }
        }
    }
}

#[test]
#[ignore = "subprocess helper; invoked by the SIGKILL test"]
fn crash_child() {
    let source = PathBuf::from(std::env::var_os("NOTES_BACKUP_TEST_SOURCE").unwrap());
    let target = PathBuf::from(std::env::var_os("NOTES_BACKUP_TEST_TARGET").unwrap());
    let phase = std::env::var("NOTES_BACKUP_TEST_PHASE").unwrap();
    let marker = PathBuf::from(std::env::var_os("NOTES_BACKUP_TEST_MARKER").unwrap());
    let mut checkpoint = |at: Phase| -> Result<()> {
        if format!("{at:?}") == phase {
            fs::write(&marker, b"ready")?;
            loop {
                std::thread::park();
            }
        }
        Ok(())
    };
    if std::env::var("NOTES_BACKUP_TEST_MODE").unwrap() == "backup" {
        let _lock = LibraryLock::acquire(&source).unwrap();
        create_from(&database(&source), &source, &target, &mut checkpoint).unwrap();
    } else {
        restore_with(&source, &target, &mut checkpoint).unwrap();
    }
}
