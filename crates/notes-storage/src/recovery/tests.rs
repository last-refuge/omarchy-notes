use super::*;
use notes_core::{
    document::{Block, Span},
    library::{Query, Scope},
    organization::{Change, Mutation},
};
use serde_json::json;

fn body(text: &str) -> Document {
    Document::from_blocks(vec![Block::paragraph(text)])
}
fn note(store: &mut Store, text: &str, folder: Option<&str>) -> Note {
    store.create(body(text), folder.map(str::to_owned)).unwrap()
}
fn folder(store: &mut Store, name: &str, parent: Option<&str>) -> String {
    let Change::Folder(folder) = store
        .organize(Mutation::CreateFolder {
            name: name.into(),
            parent: parent.map(str::to_owned),
        })
        .unwrap()
    else {
        panic!()
    };
    folder.id
}
fn copy_fixture(destination: &Path) {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/qt-v1");
    let mut pending = vec![(source, destination.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        std::fs::create_dir_all(&to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                pending.push((entry.path(), target));
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}
fn all_ids(blocks: &[Block]) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut pending: Vec<_> = blocks.iter().collect();
    while let Some(block) = pending.pop() {
        ids.insert(block.id.clone());
        pending.extend(block.cells.iter().flat_map(|c| c.blocks.iter()));
    }
    ids
}
#[test]
fn trash_restore_and_receipt_undo_preserve_latest_content_and_reject_stale_saves() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let home = folder(&mut store, "Home", None);
    let mut latest = note(&mut store, "First", Some(&home));
    latest
        .body
        .extra
        .insert("future".into(), json!({"keep": [1, "😀"]}));
    latest.body.blocks[0].spans = vec![Span(
        json!({"text": "Latest draft", "future": {"style": "keep"}}),
    )];
    let saved = store.save(&latest).unwrap();
    latest.revision = saved.revision;
    let original_date = saved.updated_at;
    let receipt = store
        .trash(&[(latest.id.clone(), latest.revision)])
        .unwrap();
    assert!(store.load().unwrap().notes.is_empty());
    assert_eq!(store.load_trash(&latest.id).unwrap().note.body, latest.body);
    assert!(
        store.save(&latest).is_err(),
        "a save queued after deletion must not resurrect the note"
    );
    let restored = store.undo_trash(&receipt).unwrap().remove(0);
    assert_eq!(restored.body, latest.body);
    assert_eq!(restored.folder.as_deref(), Some(home.as_str()));
    assert_eq!(
        restored.updated_at, original_date,
        "deletion is not a content edit"
    );
    assert_eq!(
        store.save(&latest).unwrap_err(),
        SaveError::Conflict(restored.revision)
    );
    let later = store
        .trash(&[(restored.id.clone(), restored.revision)])
        .unwrap();
    assert!(
        store.undo_trash(&receipt).is_err(),
        "an old Undo must not restore a newer deletion"
    );
    assert!(store.load_trash(&latest.id).is_ok());
    let restored = store.undo_trash(&later).unwrap().remove(0);
    drop(store);
    let store = Store::open(tmp.path()).unwrap();
    let reopened = store.load().unwrap().notes.remove(0);
    assert_eq!(reopened.body, latest.body);
    assert_eq!(reopened.revision, restored.revision);
    assert_eq!(reopened.folder, restored.folder);
}
#[test]
fn stale_and_failed_multi_note_operations_roll_back_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let a = note(&mut store, "A", None);
    let b = note(&mut store, "B", None);
    assert!(
        store
            .trash(&[(a.id.clone(), 1), (b.id.clone(), 99)])
            .is_err()
    );
    assert_eq!(store.load().unwrap().notes.len(), 2);
    store.fail_saves = true;
    assert!(store.trash(&[(a.id.clone(), 1)]).is_err());
    store.fail_saves = false;
    let receipt = store
        .trash(&[(a.id.clone(), 1), (b.id.clone(), 1)])
        .unwrap();
    assert!(
        store
            .restore_notes(&[(a.id.clone(), 2), (b.id.clone(), 99)])
            .is_err()
    );
    assert!(store.load().unwrap().notes.is_empty());
    let mut changed_receipt = receipt.clone();
    changed_receipt.entries[0].deleted_at += 1;
    assert!(store.undo_trash(&changed_receipt).is_err());
    assert!(
        store
            .delete_permanently(&[(a.id.clone(), 2), (b.id.clone(), 99)])
            .is_err()
    );
    assert!(store.load_trash(&a.id).is_ok() && store.load_trash(&b.id).is_ok());
    assert_eq!(store.undo_trash(&receipt).unwrap().len(), 2);
    assert!(
        store.delete_permanently(&[(a.id.clone(), 3)]).is_err(),
        "live notes cannot be permanently deleted"
    );
}
#[test]
fn nested_folder_delete_preserves_old_trash_and_undo_restores_only_notes_unfiled() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let parent = folder(&mut store, "Work", None);
    let child = folder(&mut store, "Clients", Some(&parent));
    let grandchild = folder(&mut store, "Archive", Some(&child));
    let other = folder(&mut store, "Personal", None);
    let same_name = folder(&mut store, "Clients", Some(&other));
    let a = note(&mut store, "A", Some(&parent));
    let b = note(&mut store, "B", Some(&child));
    let c = note(&mut store, "C", Some(&grandchild));
    let old = note(&mut store, "Old trash", Some(&child));
    let old_receipt = store.trash(&[(old.id.clone(), 1)]).unwrap();
    store
        .db
        .execute("UPDATE notes SET deleted_at=10 WHERE id=?1", [&old.id])
        .unwrap();
    let outside = note(&mut store, "Outside", Some(&same_name));
    assert!(
        store
            .delete_folder(&parent, &[(a.id.clone(), 1), (b.id.clone(), 1)])
            .is_err()
    );
    assert_eq!(store.folders().unwrap().len(), 5);
    assert_eq!(store.load().unwrap().notes.len(), 4);
    let expected = [(a.id.clone(), 1), (b.id.clone(), 1), (c.id.clone(), 1)];
    let deleted = store.delete_folder(&parent, &expected).unwrap();
    assert_eq!(
        deleted.folder_ids,
        [grandchild.clone(), child.clone(), parent.clone()]
    );
    assert_eq!(store.folders().unwrap().len(), 2);
    assert_eq!(store.load().unwrap().notes[0].id, outside.id);
    assert_eq!(store.load_trash(&old.id).unwrap().deleted_at, 10);
    assert!(store.load_trash(&old.id).unwrap().note.folder.is_none());
    assert!(store.undo_trash(&old_receipt).is_err());
    let restored = store.undo_trash(&deleted.trash).unwrap();
    assert_eq!(restored.len(), 3);
    assert!(restored.iter().all(|n| n.folder.is_none()));
    assert_eq!(
        store.folders().unwrap().len(),
        2,
        "folder Undo does not reconstruct the tree"
    );
    drop(store);
    let store = Store::open(tmp.path()).unwrap();
    assert_eq!(store.load().unwrap().notes.len(), 4);
    assert_eq!(
        store
            .query(&Query {
                scope: Scope::Folder(None),
                ..Default::default()
            })
            .unwrap()
            .rows
            .len(),
        3
    );
    assert_eq!(store.load_trash(&old.id).unwrap().deleted_at, 10);
}
#[test]
fn failed_folder_delete_restores_every_folder_and_note() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let parent = folder(&mut store, "Parent", None);
    let child = folder(&mut store, "Child", Some(&parent));
    let a = note(&mut store, "A", Some(&child));
    store.db.execute_batch(&format!("CREATE TRIGGER reject_parent BEFORE DELETE ON folders WHEN old.id='{parent}' BEGIN SELECT RAISE(ABORT,'injected parent deletion failure'); END;")).unwrap();
    assert!(store.delete_folder(&parent, &[(a.id.clone(), 1)]).is_err());
    assert_eq!(store.folders().unwrap().len(), 2);
    assert_eq!(
        store.load().unwrap().notes[0].folder.as_deref(),
        Some(child.as_str())
    );
    assert!(store.load_trash(&a.id).is_err());
}
#[test]
fn duplicate_renews_all_identities_preserves_unknown_rich_data_and_reuses_asset_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    copy_fixture(tmp.path());
    let mut store = Store::open(tmp.path()).unwrap();
    let mut source = store
        .load()
        .unwrap()
        .notes
        .into_iter()
        .find(|n| !n.body.references("attachment").is_empty())
        .unwrap();
    source
        .body
        .extra
        .insert("future".into(), json!({"nested": [1, "keep"]}));
    source.body.blocks[0]
        .attrs
        .insert("future".into(), json!({"block": true}));
    source.body.blocks[0].spans[0].0["future"] = json!({"style": "keep"});
    let table = source
        .body
        .blocks
        .iter_mut()
        .find(|b| b.kind == "table")
        .unwrap();
    table.cells[0]
        .attrs
        .insert("future".into(), json!(["cell", 7]));
    source.pinned = true;
    source.revision = store.save(&source).unwrap().revision;
    let before = source.body.clone();
    let copy = store.duplicate(&source.id, source.revision).unwrap();
    assert_ne!(copy.id, source.id);
    assert!(all_ids(&copy.body.blocks).is_disjoint(&all_ids(&source.body.blocks)));
    assert_eq!(copy.body.plain_text(), source.body.plain_text());
    assert_eq!(copy.body.extra, source.body.extra);
    assert_eq!(copy.body.blocks[0].attrs, source.body.blocks[0].attrs);
    assert_eq!(
        copy.body.blocks[0].spans[0].0["future"],
        source.body.blocks[0].spans[0].0["future"]
    );
    assert_eq!(
        copy.body
            .blocks
            .iter()
            .find(|b| b.kind == "table")
            .unwrap()
            .cells[0]
            .attrs,
        table_attrs(&source.body)
    );
    assert_eq!(copy.folder, source.folder);
    assert!(!copy.pinned && copy.revision == 1);
    assert_eq!(
        copy.body.references("image"),
        source.body.references("image")
    );
    let old = source.body.references("attachment")[0].clone();
    let new = copy.body.references("attachment")[0].clone();
    assert_ne!(old, new);
    let old_info = media::attachment_info(tmp.path(), &old).unwrap();
    let new_info = media::attachment_info(tmp.path(), &new).unwrap();
    assert_eq!(old_info.hash, new_info.hash);
    assert_eq!(
        media::read_blob(tmp.path(), &old_info.hash).unwrap(),
        media::read_blob(tmp.path(), &new_info.hash).unwrap()
    );
    assert_eq!(load_note(&store.db, &source.id).unwrap().0.body, before);
    assert!(store.duplicate(&source.id, source.revision - 1).is_err());
    std::fs::remove_file(safe_blob_path(tmp.path(), &old_info.hash).unwrap()).unwrap();
    let count = store.load().unwrap().notes.len();
    assert!(store.duplicate(&source.id, source.revision).is_err());
    assert_eq!(
        store.load().unwrap().notes.len(),
        count,
        "missing assets refuse a partial duplicate"
    );
}
fn table_attrs(document: &Document) -> std::collections::BTreeMap<String, serde_json::Value> {
    document
        .blocks
        .iter()
        .find(|b| b.kind == "table")
        .unwrap()
        .cells[0]
        .attrs
        .clone()
}
#[test]
fn history_trash_live_copies_and_pending_import_grace_keep_assets_recoverable() {
    let tmp = tempfile::tempdir().unwrap();
    copy_fixture(tmp.path());
    let mut store = Store::open(tmp.path()).unwrap();
    let mut source = store
        .load()
        .unwrap()
        .notes
        .into_iter()
        .find(|n| !n.body.references("attachment").is_empty())
        .unwrap();
    source.revision = store.save(&source).unwrap().revision;
    let copy = store.duplicate(&source.id, source.revision).unwrap();
    let old_attachment = source.body.references("attachment")[0].clone();
    let hash = media::attachment_info(tmp.path(), &old_attachment)
        .unwrap()
        .hash;
    source.body = body("Media removed from current body");
    source.revision = store.save(&source).unwrap().revision;
    store
        .db
        .execute_batch("UPDATE blobs SET created_at=0; UPDATE attachments SET created_at=0;")
        .unwrap();
    let at = now() + GARBAGE_GRACE_MS + 1;
    let report = store.collect_garbage(at).unwrap();
    assert_eq!(report.blobs_removed, 0);
    assert!(
        media::attachment_info(tmp.path(), &old_attachment).is_ok(),
        "saved history retains old attachment metadata"
    );
    let trash = store
        .trash(&[(source.id.clone(), source.revision)])
        .unwrap();
    store.collect_garbage(at).unwrap();
    assert!(media::read_blob(tmp.path(), &hash).is_ok());
    store
        .delete_permanently(&[(source.id.clone(), trash.entries[0].revision)])
        .unwrap();
    store.collect_garbage(at).unwrap();
    assert!(
        media::read_blob(tmp.path(), &hash).is_ok(),
        "the live duplicate keeps shared immutable bytes"
    );
    assert!(load_note(&store.db, &copy.id).is_ok());
    let pending = note(&mut store, "Pending import", None);
    let file = tmp.path().join("pending.txt");
    std::fs::write(&file, b"unique pending asset bytes").unwrap();
    let fragment = store
        .import_media(
            &pending.id,
            media::Import::Files {
                paths: vec![file],
                images: Some(false),
            },
        )
        .unwrap();
    let attachment = fragment.references("attachment")[0].clone();
    let pending_hash = media::attachment_info(tmp.path(), &attachment)
        .unwrap()
        .hash;
    store.collect_garbage(now()).unwrap();
    assert!(
        media::read_blob(tmp.path(), &pending_hash).is_ok(),
        "a not-yet-saved insertion has a 24-hour grace period"
    );
    store
        .db
        .execute(
            "UPDATE attachments SET created_at=0 WHERE id=?1",
            [&attachment],
        )
        .unwrap();
    store
        .db
        .execute(
            "UPDATE blobs SET created_at=0 WHERE hash=?1",
            [&pending_hash],
        )
        .unwrap();
    let report = store.collect_garbage(at).unwrap();
    assert_eq!(report.attachments_removed, 1);
    assert_eq!(report.blobs_removed, 1);
    assert!(media::read_blob(tmp.path(), &pending_hash).is_err());
}
#[test]
fn purge_boundary_and_capture_receipts_survive_restart_without_resurrection() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let request = capture::Capture {
        request_id: new_id(),
        text: "Capture remains acknowledged".into(),
    };
    let outcome = store.capture(&request).unwrap();
    let captured = outcome.created.unwrap();
    let boundary = note(&mut store, "Exactly thirty days", None);
    store
        .trash(&[(captured.id.clone(), 1), (boundary.id.clone(), 1)])
        .unwrap();
    store
        .db
        .execute(
            "UPDATE notes SET deleted_at=1000 WHERE id=?1",
            [&captured.id],
        )
        .unwrap();
    store
        .db
        .execute(
            "UPDATE notes SET deleted_at=1001 WHERE id=?1",
            [&boundary.id],
        )
        .unwrap();
    let purge = store.purge_expired(TRASH_RETENTION_MS + 1001).unwrap();
    assert_eq!(
        purge.purged_ids.as_slice(),
        std::slice::from_ref(&captured.id)
    );
    assert!(!purge.has_more);
    assert!(
        store.load_trash(&boundary.id).is_ok(),
        "exactly thirty days is retained until it is older than the boundary"
    );
    assert_eq!(
        store
            .capture_status(&request.request_id)
            .unwrap()
            .unwrap()
            .note_id,
        captured.id
    );
    drop(store);
    let mut store = Store::open(tmp.path()).unwrap();
    let retried = store.capture(&request).unwrap();
    assert!(retried.created.is_none());
    assert_eq!(retried.receipt, outcome.receipt);
    assert!(store.load().unwrap().notes.is_empty());
    assert_eq!(
        store
            .query(&Query {
                scope: Scope::Trash,
                ..Default::default()
            })
            .unwrap()
            .rows
            .len(),
        1
    );
}

#[test]
fn worker_fifo_serializes_saves_deletion_restore_and_echoes_query_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let mut a = note(&mut store, "Before", None);
    a.body = body("Latest benchmark draft");
    let (worker, _) = Worker::from_store(store).unwrap();
    worker
        .requests
        .send(Request::Save {
            note: a.clone(),
            local_revision: 17,
            operation: 41,
        })
        .unwrap();
    worker
        .requests
        .send(Request::Trash {
            notes: vec![(a.id.clone(), 2)],
            operation: 42,
        })
        .unwrap();
    a.revision = 2;
    worker
        .requests
        .send(Request::Save {
            note: a.clone(),
            local_revision: 18,
            operation: 43,
        })
        .unwrap();
    let query = Query {
        generation: 7,
        scope: Scope::All,
        text: "bench".into(),
        limit: 2,
        ..Default::default()
    };
    worker
        .requests
        .send(Request::Query {
            operation: 44,
            query: query.clone(),
        })
        .unwrap();
    let next = || worker.events.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(
        next(),
        Event::Saved {
            operation: 41,
            local_revision: 17,
            result: Ok(Saved { revision: 2, .. }),
            ..
        }
    ));
    let Event::Trashed {
        operation: 42,
        result: Ok(receipt),
    } = next()
    else {
        panic!()
    };
    assert!(matches!(
        next(),
        Event::Saved {
            operation: 43,
            result: Err(SaveError::Failed(_)),
            ..
        }
    ));
    let Event::Queried {
        operation: 44,
        query: echoed,
        result: Ok(page),
    } = next()
    else {
        panic!()
    };
    assert_eq!(echoed, query);
    assert!(page.rows.is_empty());
    worker
        .requests
        .send(Request::UndoTrash {
            operation: 45,
            receipt,
        })
        .unwrap();
    worker
        .requests
        .send(Request::Save {
            note: a,
            local_revision: 19,
            operation: 46,
        })
        .unwrap();
    let Event::Restored {
        operation: 45,
        result: Ok(notes),
    } = next()
    else {
        panic!()
    };
    assert_eq!(notes[0].body.title(), "Latest benchmark draft");
    assert_eq!(notes[0].revision, 4);
    assert!(matches!(
        next(),
        Event::Saved {
            operation: 46,
            result: Err(SaveError::Conflict(4)),
            ..
        }
    ));
}

#[test]
fn opaque_extension_references_in_history_keep_registered_assets() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let mut a = note(&mut store, "Future extension data", None);
    let files: Vec<_> = (0..5)
        .map(|index| {
            let path = tmp.path().join(format!("future-{index}.bin"));
            std::fs::write(&path, format!("distinct future asset bytes {index}")).unwrap();
            path
        })
        .collect();
    let fragment = store
        .import_media(
            &a.id,
            media::Import::Files {
                paths: files,
                images: Some(false),
            },
        )
        .unwrap();
    let ids = fragment.references("attachment");
    let hashes: Vec<_> = ids
        .iter()
        .map(|id| media::attachment_info(tmp.path(), id).unwrap().hash)
        .collect();
    a.body.extra.insert(
        "future".into(),
        json!({"address": format!("attachment:{}", ids[0])}),
    );
    a.body.blocks[0]
        .attrs
        .insert("future".into(), json!(hashes[1]));
    a.body.blocks[0].spans[0].0["future"] = json!([ids[2]]);
    let mut nested = Block::paragraph("Nested opaque reference");
    nested.attrs.insert("future".into(), json!({"id": ids[4]}));
    a.body.blocks.push(Block {
        id: new_id(),
        kind: "table".into(),
        spans: vec![],
        cells: vec![notes_core::document::Cell {
            row: 0,
            col: 0,
            blocks: vec![nested],
            attrs: BTreeMap::from([("future".into(), json!(format!("blob://{}", hashes[3])))]),
        }],
        attrs: BTreeMap::from([("rows".into(), json!(1)), ("columns".into(), json!(1))]),
    });
    a.revision = store.save(&a).unwrap().revision;
    a.body = body("Current body without future references");
    a.revision = store.save(&a).unwrap().revision;
    store
        .db
        .execute_batch("UPDATE blobs SET created_at=0;UPDATE attachments SET created_at=0;")
        .unwrap();
    let report = store.collect_garbage(now() + GARBAGE_GRACE_MS + 1).unwrap();
    assert_eq!(
        report.blobs_removed, 0,
        "unknown JSON asset addresses in history are retained conservatively"
    );
    for hash in &hashes {
        assert!(media::read_blob(tmp.path(), hash).is_ok());
    }
    for index in [0, 2, 4] {
        assert!(media::attachment_info(tmp.path(), &ids[index]).is_ok());
    }
}

#[test]
fn surviving_opaque_attachment_links_and_unreadable_history_refuse_destructive_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let owner = note(&mut store, "Asset owner", None);
    let file = tmp.path().join("protected.bin");
    std::fs::write(&file, b"protected opaque extension asset").unwrap();
    let fragment = store
        .import_media(
            &owner.id,
            media::Import::Files {
                paths: vec![file],
                images: Some(false),
            },
        )
        .unwrap();
    let id = fragment.references("attachment")[0].clone();
    let hash = media::attachment_info(tmp.path(), &id).unwrap().hash;
    let mut survivor = note(&mut store, "Future client", None);
    survivor
        .body
        .extra
        .insert("future_attachment".into(), json!(id));
    survivor.revision = store.save(&survivor).unwrap().revision;
    let receipt = store.trash(&[(owner.id.clone(), owner.revision)]).unwrap();
    assert!(
        store
            .delete_permanently(&[(owner.id.clone(), receipt.entries[0].revision)])
            .is_err()
    );
    assert!(store.load_trash(&owner.id).is_ok());
    assert!(media::read_blob(tmp.path(), &hash).is_ok());
    store.db.execute_batch("UPDATE blobs SET created_at=0;UPDATE attachments SET created_at=0;UPDATE note_revisions SET body='unsupported future payload';").unwrap();
    assert!(store.collect_garbage(now() + GARBAGE_GRACE_MS + 1).is_err());
    assert!(media::attachment_info(tmp.path(), &id).is_ok());
    assert!(media::read_blob(tmp.path(), &hash).is_ok());
}

#[test]
fn housekeeping_rejects_symlinked_blob_directories_without_removing_metadata() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let owner = note(&mut store, "Unused old asset", None);
    let file = tmp.path().join("unused.bin");
    std::fs::write(&file, b"unused old asset bytes").unwrap();
    let fragment = store
        .import_media(
            &owner.id,
            media::Import::Files {
                paths: vec![file],
                images: Some(false),
            },
        )
        .unwrap();
    let id = fragment.references("attachment")[0].clone();
    let hash = media::attachment_info(tmp.path(), &id).unwrap().hash;
    store
        .db
        .execute_batch("UPDATE blobs SET created_at=0;UPDATE attachments SET created_at=0;")
        .unwrap();
    let directory = tmp.path().join("blobs").join(&hash[..2]);
    let moved = tmp.path().join("untouched-by-cleanup");
    std::fs::rename(&directory, &moved).unwrap();
    symlink(&moved, &directory).unwrap();
    assert!(store.collect_garbage(now() + GARBAGE_GRACE_MS + 1).is_err());
    assert!(moved.join(&hash).is_file());
    assert!(
        media::attachment_info(tmp.path(), &id).is_ok(),
        "failed cleanup rolls metadata deletion back too"
    );
}
