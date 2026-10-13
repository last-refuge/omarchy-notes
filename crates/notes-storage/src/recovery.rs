//! Revision-checked recoverable deletion on the same writer as content saves.
use super::*;
use std::collections::{BTreeMap, HashSet};

pub const TRASH_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
pub const GARBAGE_GRACE_MS: i64 = 24 * 60 * 60 * 1_000;
pub const MAX_MUTATION_NOTES: usize = 10_000;

#[derive(Clone, Debug)]
pub struct TrashEntry {
    pub note: Note,
    pub deleted_at: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrashToken {
    pub id: String,
    pub revision: i64,
    pub deleted_at: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrashReceipt {
    pub id: String,
    pub entries: Vec<TrashToken>,
}
#[derive(Clone, Debug)]
pub struct FolderDeletion {
    pub folder_ids: Vec<String>,
    pub trash: TrashReceipt,
}
#[derive(Clone, Debug, Default)]
pub struct GarbageReport {
    pub attachments_removed: usize,
    pub blobs_removed: usize,
    pub bytes_freed: u64,
    pub unremoved_files: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct PurgeReport {
    pub purged_ids: Vec<String>,
    pub has_more: bool,
    pub garbage: GarbageReport,
}

impl Store {
    /// Duplicate the acknowledged version. The source and its history are never
    /// modified; new attachment records reuse validated immutable blob bytes.
    pub fn duplicate(&mut self, id: &str, expected_revision: i64) -> Result<Note> {
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. The original note is unchanged."
        );
        let source = load_note(&self.db, id)?;
        ensure!(
            source.1.is_none(),
            "Restore the note before duplicating it."
        );
        ensure!(
            source.0.revision == expected_revision,
            "The note changed before duplication. Save the latest draft and try again."
        );
        for hash in source.0.body.references("image") {
            let bytes = media::read_blob(&self.root, &hash)?;
            let size: Option<u64> = self
                .db
                .query_row("SELECT size FROM blobs WHERE hash=?1", [&hash], |r| {
                    r.get(0)
                })
                .optional()?;
            ensure!(
                size == Some(bytes.len() as u64),
                "An image has missing or inconsistent blob metadata. The original note is unchanged."
            );
        }
        for id in source.0.body.references("attachment") {
            let info = media::attachment(&self.db, &id)?;
            let bytes = media::read_blob(&self.root, &info.hash)?;
            let registered_size: Option<u64> = self
                .db
                .query_row("SELECT size FROM blobs WHERE hash=?1", [&info.hash], |r| {
                    r.get(0)
                })
                .optional()?;
            ensure!(
                bytes.len() as u64 == info.size && registered_size == Some(info.size),
                "An attachment has inconsistent stored bytes. The original note is unchanged."
            );
        }
        self.create(
            notes_core::library::fresh_document(&source.0.body),
            source.0.folder,
        )
    }
    pub fn load_trash(&self, id: &str) -> Result<TrashEntry> {
        let (note, deleted) = load_note(&self.db, id)?;
        Ok(TrashEntry {
            note,
            deleted_at: deleted.context("This note is not in Recently Deleted.")?,
        })
    }
    pub fn trash(&mut self, notes: &[(String, i64)]) -> Result<TrashReceipt> {
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. Your notes are unchanged."
        );
        checked_notes(notes, false)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted_at = now();
        let mut entries = vec![];
        for (id, expected) in notes {
            let (revision, deleted) = metadata(&tx, id)?;
            ensure!(
                deleted.is_none(),
                "The note is already in Recently Deleted."
            );
            ensure!(
                revision == *expected,
                "The note changed before deletion. Save the latest draft and try again."
            );
            let next = next_revision(revision)?;
            tx.execute(
                "UPDATE notes SET deleted_at=?2,revision=?3 WHERE id=?1",
                params![id, deleted_at, next],
            )?;
            entries.push(TrashToken {
                id: id.clone(),
                revision: next,
                deleted_at,
            });
        }
        tx.commit()?;
        Ok(TrashReceipt {
            id: new_id(),
            entries,
        })
    }
    pub fn restore_notes(&mut self, notes: &[(String, i64)]) -> Result<Vec<Note>> {
        checked_notes(notes, false)?;
        self.restore_checked(notes, None)
    }
    pub fn undo_trash(&mut self, receipt: &TrashReceipt) -> Result<Vec<Note>> {
        capture::validate_id(&receipt.id)?;
        let notes: Vec<_> = receipt
            .entries
            .iter()
            .map(|entry| (entry.id.clone(), entry.revision))
            .collect();
        checked_notes(&notes, true)?;
        ensure!(
            receipt.entries.iter().all(|entry| entry.deleted_at >= 0),
            "Invalid deletion receipt."
        );
        self.restore_checked(&notes, Some(receipt))
    }
    fn restore_checked(
        &mut self,
        notes: &[(String, i64)],
        receipt: Option<&TrashReceipt>,
    ) -> Result<Vec<Note>> {
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. Your notes remain in Recently Deleted."
        );
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut restored = vec![];
        for (index, (id, expected)) in notes.iter().enumerate() {
            let (revision, deleted) = metadata(&tx, id)?;
            let deleted = deleted.context("The note is no longer in Recently Deleted.")?;
            ensure!(
                revision == *expected
                    && receipt.is_none_or(|r| r.entries[index].deleted_at == deleted),
                "This deletion has changed. Refresh Recently Deleted before restoring it."
            );
            let next = next_revision(revision)?;
            // A removed folder is not recreated by note recovery or folder Undo.
            tx.execute("UPDATE notes SET deleted_at=NULL,revision=?2,folder_id=CASE WHEN EXISTS(SELECT 1 FROM folders f WHERE f.id=notes.folder_id AND f.deleted_at IS NULL) THEN folder_id ELSE NULL END WHERE id=?1", params![id, next])?;
            restored.push(load_note(&tx, id)?.0);
        }
        tx.commit()?;
        Ok(restored)
    }
    pub fn delete_permanently(&mut self, notes: &[(String, i64)]) -> Result<Vec<String>> {
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. Your notes remain in Recently Deleted."
        );
        checked_notes(notes, false)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids: HashSet<_> = notes.iter().map(|(id, _)| id.clone()).collect();
        for (id, expected) in notes {
            let (revision, deleted) = metadata(&tx, id)?;
            ensure!(
                deleted.is_some(),
                "Only notes in Recently Deleted can be deleted permanently."
            );
            ensure!(
                revision == *expected,
                "The deleted note changed. Refresh Recently Deleted first."
            );
        }
        protect_foreign_attachments(&tx, &ids)?;
        for (id, _) in notes {
            tx.execute("DELETE FROM notes WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(notes.iter().map(|(id, _)| id.clone()).collect())
    }
    /// Atomically remove a subtree and trash its live notes. The receipt undoes
    /// newly trashed notes only; folder identities and names stay removed.
    pub fn delete_folder(
        &mut self,
        id: &str,
        expected_notes: &[(String, i64)],
    ) -> Result<FolderDeletion> {
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. The folder and its notes are unchanged."
        );
        let expected = checked_notes(expected_notes, true)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let folders = organization::folders(&tx)?;
        let folder_ids = notes_core::organization::descendants(&folders, id)?;
        let mut actual = BTreeMap::new();
        for folder in &folder_ids {
            let mut query = tx.prepare(
                "SELECT id,revision FROM notes WHERE folder_id=?1 AND deleted_at IS NULL",
            )?;
            for row in query.query_map([folder], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })? {
                let (id, revision) = row?;
                actual.insert(id, revision);
            }
            let overflowing: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM notes WHERE folder_id=?1 AND (revision<1 OR revision>=?2))",
                params![folder, i64::MAX],
                |r| r.get(0),
            )?;
            ensure!(
                !overflowing,
                "A note revision is out of range. Preserve this folder for recovery."
            );
        }
        ensure!(
            actual.len() == expected.len()
                && actual
                    .iter()
                    .all(|(id, revision)| expected.get(id.as_str()) == Some(revision)),
            "The folder contents changed before deletion. Save all current drafts and review the deletion again."
        );
        let deleted_at = now();
        let entries = actual
            .into_iter()
            .map(|(id, revision)| {
                Ok(TrashToken {
                    id,
                    revision: next_revision(revision)?,
                    deleted_at,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for folder in &folder_ids {
            // Existing trash keeps its original retention date, but loses the
            // removed parent. Bump its revision so an old restore request fails.
            tx.execute("UPDATE notes SET deleted_at=COALESCE(deleted_at,?2),folder_id=NULL,revision=revision+1 WHERE folder_id=?1", params![folder, deleted_at])?;
        }
        for folder in &folder_ids {
            tx.execute("DELETE FROM folders WHERE id=?1", [folder])?;
        }
        tx.commit()?;
        Ok(FolderDeletion {
            folder_ids,
            trash: TrashReceipt {
                id: new_id(),
                entries,
            },
        })
    }
    /// A bounded 1,000-note purge batch plus conservative asset collection.
    /// Captures retain their independent receipts after their note disappears.
    pub fn purge_expired(&mut self, at: i64) -> Result<PurgeReport> {
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. Recently Deleted is unchanged."
        );
        ensure!(at >= 0, "Invalid housekeeping timestamp.");
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut ids: Vec<String> = {
            let mut query = tx.prepare("SELECT id FROM notes WHERE deleted_at IS NOT NULL AND deleted_at<?1 ORDER BY deleted_at,id LIMIT 1001")?;
            query
                .query_map([at.saturating_sub(TRASH_RETENTION_MS)], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        let has_more = ids.len() > 1000;
        ids.truncate(1000);
        protect_foreign_attachments(&tx, &ids.iter().cloned().collect())?;
        for id in &ids {
            tx.execute("DELETE FROM notes WHERE id=?1", [id])?;
        }
        let references = references(&tx, &HashSet::new())?;
        let (mut garbage, files) = prune_assets(&tx, &self.root, at, &references)?;
        tx.commit()?;
        unlink_files(&files, &mut garbage);
        Ok(PurgeReport {
            purged_ids: ids,
            has_more,
            garbage,
        })
    }
    pub fn collect_garbage(&mut self, at: i64) -> Result<GarbageReport> {
        ensure!(
            !self.fail_saves && at >= 0,
            "Could not start asset housekeeping."
        );
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let references = references(&tx, &HashSet::new())?;
        let (mut report, files) = prune_assets(&tx, &self.root, at, &references)?;
        tx.commit()?;
        unlink_files(&files, &mut report);
        Ok(report)
    }
}

fn checked_notes(notes: &[(String, i64)], empty: bool) -> Result<BTreeMap<&str, i64>> {
    ensure!(
        (empty || !notes.is_empty()) && notes.len() <= MAX_MUTATION_NOTES,
        "Choose 1–10,000 notes for this operation."
    );
    let mut result = BTreeMap::new();
    for (id, revision) in notes {
        ensure!(
            !id.is_empty() && id.len() <= 4096 && *revision > 0,
            "Save the note before changing its deletion state."
        );
        ensure!(
            result.insert(id.as_str(), *revision).is_none(),
            "A note appears twice in this operation."
        );
    }
    Ok(result)
}
fn next_revision(revision: i64) -> Result<i64> {
    revision
        .checked_add(1)
        .context("The note revision is out of range. Preserve the note for recovery.")
}
fn metadata(db: &Connection, id: &str) -> Result<(i64, Option<i64>)> {
    db.query_row(
        "SELECT revision,deleted_at FROM notes WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .context("The note no longer exists.")
}
fn load_note(db: &Connection, id: &str) -> Result<(Note, Option<i64>)> {
    let row = db.query_row("SELECT folder_id,body,revision,created_at,updated_at,pinned,deleted_at FROM notes WHERE id=?1", [id], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, bool>(5)?, r.get::<_, Option<i64>>(6)?))).optional()?.context("The note no longer exists.")?;
    Ok((
        Note {
            id: id.into(),
            folder: row.0,
            body: Document::from_json(&row.1)?,
            revision: row.2,
            created_at: row.3,
            updated_at: row.4,
            pinned: row.5,
        },
        row.6,
    ))
}
#[derive(Default)]
struct References {
    images: HashSet<String>,
    attachments: HashSet<String>,
}
fn references(db: &Connection, excluded: &HashSet<String>) -> Result<References> {
    let mut result = References::default();
    let known = References {
        images: {
            let mut query = db.prepare("SELECT hash FROM blobs")?;
            query
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        },
        attachments: {
            let mut query = db.prepare("SELECT id FROM attachments")?;
            query
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        },
    };
    let mut query =
        db.prepare("SELECT id,body FROM notes UNION ALL SELECT note_id,body FROM note_revisions")?;
    for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, json) = row?;
        if excluded.contains(&id) {
            continue;
        }
        // Never skip an unreadable body and then collect its unknown references.
        let document = Document::from_json(&json).with_context(|| {
            format!("Note {id} or its history could not be checked. No assets were removed.")
        })?;
        result.images.extend(document.references("image"));
        result.attachments.extend(document.references("attachment"));
        // Extension properties are preserved without interpreting their schema.
        // Conservatively keep registered asset addresses found in their JSON
        // strings rather than deciding that an unknown property is unreferenced.
        for value in document.extra.values() {
            opaque_references(value, &known, &mut result);
        }
        block_extension_references(&document.blocks, &known, &mut result);
    }
    Ok(result)
}
fn block_extension_references(
    blocks: &[notes_core::document::Block],
    known: &References,
    out: &mut References,
) {
    for block in blocks {
        for value in block.attrs.values() {
            opaque_references(value, known, out);
        }
        for span in &block.spans {
            if let Some(object) = span.0.as_object() {
                for (key, value) in object {
                    if key != "text" {
                        opaque_references(value, known, out);
                    }
                }
            }
        }
        for cell in &block.cells {
            for value in cell.attrs.values() {
                opaque_references(value, known, out);
            }
            block_extension_references(&cell.blocks, known, out);
        }
    }
}
fn opaque_references(value: &serde_json::Value, known: &References, out: &mut References) {
    match value {
        serde_json::Value::String(text) => {
            let mut keep = |token: &str| {
                if known.images.contains(token) {
                    out.images.insert(token.to_owned());
                }
                if known.attachments.contains(token) {
                    out.attachments.insert(token.to_owned());
                }
            };
            if text.len() <= 4096 {
                keep(text);
            }
            for token in text.split(|ch: char| !ch.is_ascii_hexdigit() && ch != '-') {
                if !token.is_empty() {
                    keep(token);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                opaque_references(value, known, out);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                opaque_references(value, known, out);
            }
        }
        _ => {}
    }
}
fn protect_foreign_attachments(db: &Connection, removed: &HashSet<String>) -> Result<()> {
    if removed.is_empty() {
        return Ok(());
    }
    let remaining = references(db, removed)?;
    let mut query = db.prepare("SELECT id,note_id FROM attachments")?;
    for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, owner) = row?;
        ensure!(
            !removed.contains(&owner) || !remaining.attachments.contains(&id),
            "Another note or retained revision references an attachment owned by this deleted note. Preserve it for recovery; permanent deletion was refused."
        );
    }
    Ok(())
}
fn safe_blob_path(root: &Path, hash: &str) -> Result<PathBuf> {
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid blob address. No files were removed."
    );
    let blobs = root.join("blobs");
    let directory = blobs.join(&hash[..2]);
    for path in [&blobs, &directory] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Unexpected media directory. No files were removed."
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let path = directory.join(hash);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) => ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Unexpected blob file. No files were removed."
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}
struct PendingUnlink {
    hash: String,
    path: PathBuf,
    bytes: u64,
}
fn prune_assets(
    db: &Connection,
    root: &Path,
    at: i64,
    references: &References,
) -> Result<(GarbageReport, Vec<PendingUnlink>)> {
    let cutoff = at.saturating_sub(GARBAGE_GRACE_MS);
    let mut report = GarbageReport::default();
    let attachments: Vec<String> = {
        let mut query = db.prepare("SELECT a.id FROM attachments a WHERE a.created_at<?1 AND NOT EXISTS(SELECT 1 FROM note_attachment_refs r WHERE r.attachment_id=a.id)")?;
        query
            .query_map([cutoff], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    for id in attachments {
        if !references.attachments.contains(&id) {
            report.attachments_removed +=
                db.execute("DELETE FROM attachments WHERE id=?1", [&id])?;
        }
    }
    let candidates: Vec<(String, u64)> = {
        let mut query = db.prepare("SELECT hash,size FROM blobs WHERE created_at<?1 AND hash NOT IN(SELECT blob_hash FROM attachments) AND hash NOT IN(SELECT blob_hash FROM note_blob_refs)")?;
        query
            .query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut files = vec![];
    for (hash, _size) in candidates {
        if !references.images.contains(&hash) {
            let path = safe_blob_path(root, &hash)?;
            let size = std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
            report.blobs_removed += db.execute("DELETE FROM blobs WHERE hash=?1", [&hash])?;
            files.push(PendingUnlink {
                hash,
                path,
                bytes: size,
            });
        }
    }
    Ok((report, files))
}
fn unlink_files(files: &[PendingUnlink], report: &mut GarbageReport) {
    for file in files {
        match std::fs::remove_file(&file.path) {
            Ok(()) => report.bytes_freed = report.bytes_freed.saturating_add(file.bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => report.unremoved_files.push(file.hash.clone()),
        }
    }
}

#[cfg(test)]
#[path = "recovery/tests.rs"]
mod tests;
