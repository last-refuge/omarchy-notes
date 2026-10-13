//! Schema-compatible SQLite storage. Only committed writes produce acknowledgments.
pub mod backup;
pub mod capture;
pub mod file_import;
mod lock;
pub mod media;
pub mod migration;
pub mod organization;
pub mod portable;
pub mod recovery;
pub mod search;
use anyhow::{Context, Result, ensure};
use notes_core::document::{Document, new_id};
pub use recovery::{
    FolderDeletion, GarbageReport, PurgeReport, TrashEntry, TrashReceipt, TrashToken,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub folder: Option<String>,
    pub body: Document,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub pinned: bool,
}
pub use notes_core::organization::Folder;
pub struct Library {
    pub schema: u32,
    pub root: PathBuf,
    pub notes: Vec<Note>,
    pub folders: Vec<Folder>,
}
pub struct Store {
    db: Connection,
    _lock: lock::LibraryLock,
    pub root: PathBuf,
    pub fail_saves: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum SaveError {
    UpgradeRequired,
    Conflict(i64),
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct Saved {
    pub revision: i64,
    pub updated_at: i64,
}

/// Ownership contention is retryable; validation and filesystem errors are not.
#[derive(Debug)]
pub struct LibraryBusy;
impl std::fmt::Display for LibraryBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("This library is locked by another Notes process. A Qt or unrecognised owner cannot receive Rust capture requests; close that edition first.")
    }
}
impl std::error::Error for LibraryBusy {}

impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root).context("Could not create the notes directory.")?;
        let root = root.canonicalize()?;
        let lock = lock::LibraryLock::acquire(&root)?;
        let path = root.join("library.sqlite3");
        if path.exists() {
            let probe =
                Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let version: i64 = probe.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            ensure!(
                (1..=3).contains(&version),
                "Unsupported database schema {version}; the library has not been migrated."
            );
            let check: String = probe.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
            ensure!(
                check == "ok",
                "The database failed its integrity check. Preserve it for recovery."
            );
            migration::validate_documents(&probe, version as u32)?;
            if version == 3 {
                migration::receipt(&probe)?;
            }
        }
        let db = Connection::open(&path)?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.pragma_update(None, "foreign_keys", true)?;
        let journal: String = db.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
        ensure!(
            journal == "wal",
            "This filesystem does not support WAL storage."
        );
        db.pragma_update(None, "synchronous", "FULL")?;
        let mut store = Self {
            db,
            _lock: lock,
            root,
            fail_saves: false,
        };
        store.migrate()?;
        Ok(store)
    }
    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self.db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version >= 2 {
            return Ok(());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if version == 0 {
            tx.execute_batch(include_str!("schema-v1.sql"))?;
        }
        tx.execute_batch(include_str!("schema-v2.sql"))?;
        let rows: Vec<(String, String)> = {
            let mut q = tx.prepare("SELECT id,body FROM notes")?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        for (id, json) in rows {
            derived(&tx, &id, &Document::from_json(&json)?)?;
        }
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
        Ok(())
    }
    pub fn load(&self) -> Result<Library> {
        let mut q=self.db.prepare("SELECT id,folder_id,body,revision,created_at,updated_at,pinned FROM notes WHERE deleted_at IS NULL ORDER BY pinned DESC,updated_at DESC,id")?;
        let rows = q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?;
        let mut notes = vec![];
        for row in rows {
            let (id, folder, json, revision, created_at, updated_at, pinned) = row?;
            let body = Document::from_json(&json).with_context(|| {
                format!("Could not decode note {id}. Its original data is unchanged.")
            })?;
            notes.push(Note {
                id,
                folder,
                body,
                revision,
                created_at,
                updated_at,
                pinned,
            });
        }
        let folders = self.folders()?;
        Ok(Library {
            notes,
            folders,
            schema: self.schema()?,
            root: self.root.clone(),
        })
    }
    pub fn schema(&self) -> Result<u32> {
        Ok(self.db.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
    pub fn create(&mut self, body: Document, folder: Option<String>) -> Result<Note> {
        self.create_with(body, folder, |_, _| Ok(()))
    }
    fn create_with(
        &mut self,
        body: Document,
        folder: Option<String>,
        before_commit: impl FnOnce(&Connection, &str) -> Result<()>,
    ) -> Result<Note> {
        self.create_with_id(new_id(), body, folder, before_commit)
    }
    fn create_with_id(
        &mut self,
        id: String,
        mut body: Document,
        folder: Option<String>,
        before_commit: impl FnOnce(&Connection, &str) -> Result<()>,
    ) -> Result<Note> {
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. Your draft remains in memory."
        );
        body.validate()?;
        let schema = self.schema()?;
        ensure!(
            schema == 3 || body.schema == 1,
            "Enable quotes for this library before saving a schema-2 document. Your draft is unchanged."
        );
        if schema == 3 {
            body.schema = 2;
        }
        let attachments = media::copy_attachments(&self.db, &mut body)?;
        let time = now();
        let json = serde_json::to_string(&body)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(folder) = &folder {
            ensure!(
                tx.query_row(
                    "SELECT count(*) FROM folders WHERE id=?1 AND deleted_at IS NULL",
                    [folder],
                    |r| r.get::<_, i64>(0)
                )? == 1,
                "The selected folder is unavailable."
            );
        }
        tx.execute("INSERT INTO notes(id,folder_id,title,snippet,plain_text,body,body_schema,has_attachments,has_checklist,created_at,updated_at,revision) VALUES(?1,?2,?3,?4,?5,?6,?10,?7,?8,?9,?9,1)",params![id,folder,body.title(),body.snippet(),body.plain_text(),json,media(&body),checklist(&body),time,body.schema])?;
        for info in attachments {
            tx.execute("INSERT INTO attachments(id,note_id,blob_hash,file_name,mime_type,size,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![info.id,id,info.hash,info.name,info.mime,info.size,time])?;
        }
        derived(&tx, &id, &body)?;
        before_commit(&tx, &id)?;
        tx.commit()?;
        Ok(Note {
            id,
            folder,
            body,
            revision: 1,
            created_at: time,
            updated_at: time,
            pinned: false,
        })
    }
    pub fn save(&mut self, note: &Note) -> std::result::Result<Saved, SaveError> {
        self.save_inner(note)
            .map_err(|e| SaveError::Failed(format!("{e:#}")))?
    }
    fn save_inner(&mut self, note: &Note) -> Result<std::result::Result<Saved, SaveError>> {
        ensure!(!self.fail_saves, "Simulated storage failure.");
        note.body.validate()?;
        let schema = self.schema()?;
        if schema < 3 && note.body.schema > 1 {
            return Ok(Err(SaveError::UpgradeRequired));
        }
        // Revision zero is an in-memory draft with a stable identity. Its first
        // acknowledgement is a committed INSERT, never an upsert: an existing
        // record (including trash) must not be overwritten by a stale draft.
        if note.revision == 0 {
            let created = self.create_with_id(
                note.id.clone(),
                note.body.clone(),
                note.folder.clone(),
                |tx, id| {
                    tx.execute(
                        "UPDATE notes SET pinned=?2 WHERE id=?1",
                        params![id, note.pinned],
                    )?;
                    Ok(())
                },
            )?;
            return Ok(Ok(Saved {
                revision: created.revision,
                updated_at: created.updated_at,
            }));
        }
        let mut body = note.body.clone();
        if schema == 3 {
            body.schema = 2;
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<(i64, Option<i64>)> = tx
            .query_row(
                "SELECT revision,deleted_at FROM notes WHERE id=?1",
                [&note.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (revision, deleted) =
            current.context("The note no longer exists. Save the draft as a new note.")?;
        ensure!(
            deleted.is_none(),
            "This note was deleted elsewhere. Save the draft as a new note."
        );
        if revision != note.revision {
            return Ok(Err(SaveError::Conflict(revision)));
        }
        let json = serde_json::to_string(&body)?;
        let timestamp = now();
        let next = revision
            .checked_add(1)
            .context("The note revision is out of range. Save a copy instead.")?;
        tx.execute("UPDATE notes SET title=?2,snippet=?3,plain_text=?4,body=?5,body_schema=?11,has_attachments=?6,has_checklist=?7,updated_at=?8,revision=?9,pinned=?10 WHERE id=?1",params![note.id,body.title(),body.snippet(),body.plain_text(),json,media(&body),checklist(&body),timestamp,next,note.pinned,body.schema])?;
        derived(&tx, &note.id, &note.body)?;
        let last: Option<i64> = tx.query_row(
            "SELECT max(created_at) FROM note_revisions WHERE note_id=?1",
            [&note.id],
            |r| r.get(0),
        )?;
        if last.is_none_or(|last| timestamp - last >= 600_000) {
            tx.execute("INSERT INTO note_revisions(note_id,revision,body,body_schema,created_at) VALUES(?1,?2,?3,?5,?4)",params![note.id,next,json,timestamp,body.schema])?;
        }
        tx.commit()?;
        Ok(Ok(Saved {
            revision: next,
            updated_at: timestamp,
        }))
    }
}
fn media(doc: &Document) -> bool {
    !doc.references("image").is_empty() || !doc.references("attachment").is_empty()
}
fn checklist(doc: &Document) -> bool {
    doc.leaves()
        .iter()
        .any(|b| b.kind == "list" && b.string("list") == "check")
}
fn derived(db: &Connection, id: &str, body: &Document) -> Result<()> {
    db.execute("DELETE FROM note_tags WHERE note_id=?1", [id])?;
    for tag in body.tags() {
        db.execute(
            "INSERT INTO note_tags(note_id,tag) VALUES(?1,?2)",
            params![id, tag],
        )?;
    }
    db.execute("DELETE FROM note_blob_refs WHERE note_id=?1", [id])?;
    let images = body.references("image");
    for hash in &images {
        db.execute("INSERT OR IGNORE INTO note_blob_refs(note_id,blob_hash) SELECT ?1,hash FROM blobs WHERE hash=?2",params![id,hash])?;
    }
    db.execute("DELETE FROM note_attachment_refs WHERE note_id=?1", [id])?;
    for attachment in body.references("attachment") {
        db.execute("INSERT OR IGNORE INTO note_attachment_refs(note_id,attachment_id) SELECT ?1,id FROM attachments WHERE id=?2",params![id,attachment])?;
    }
    db.execute(
        "UPDATE notes SET thumbnail=(SELECT hash FROM blobs WHERE hash=?2) WHERE id=?1",
        params![id, images.first()],
    )?;
    Ok(())
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub enum Request {
    PrepareFileImport {
        paths: Vec<PathBuf>,
        cancel: file_import::Cancellation,
        operation: u64,
    },
    CommitFileImport {
        prepared: file_import::PreparedImport,
        folder: Option<String>,
        accept_warnings: bool,
        cancel: file_import::Cancellation,
        operation: u64,
    },
    PortableExport {
        selection: portable::Selection,
        destination: PathBuf,
        format: portable::Format,
        cancel: backup::Cancellation,
        operation: u64,
    },
    BackUp {
        destination: PathBuf,
        cancel: backup::Cancellation,
        operation: u64,
    },
    Query {
        operation: u64,
        query: notes_core::library::Query,
    },
    Duplicate {
        id: String,
        expected_revision: i64,
        operation: u64,
    },
    LoadTrash {
        id: String,
        operation: u64,
    },
    Trash {
        notes: Vec<(String, i64)>,
        operation: u64,
    },
    Restore {
        notes: Vec<(String, i64)>,
        operation: u64,
    },
    UndoTrash {
        receipt: TrashReceipt,
        operation: u64,
    },
    DeletePermanently {
        notes: Vec<(String, i64)>,
        operation: u64,
    },
    DeleteFolder {
        id: String,
        expected_notes: Vec<(String, i64)>,
        operation: u64,
    },
    Purge {
        at: i64,
        operation: u64,
    },
    Organize {
        operation: u64,
        mutation: notes_core::organization::Mutation,
    },
    Capture {
        request: capture::Capture,
        reply: SyncSender<std::result::Result<capture::Receipt, String>>,
    },
    CaptureStatus {
        request_id: String,
        reply: SyncSender<std::result::Result<Option<capture::Receipt>, String>>,
    },
    Upgrade {
        destination: PathBuf,
        cancel: migration::Cancellation,
        operation: u64,
    },
    Import {
        id: String,
        operation: u64,
        content: media::Import,
    },
    Save {
        note: Note,
        local_revision: u64,
        operation: u64,
    },
    Create {
        body: Document,
        folder: Option<String>,
        operation: u64,
    },
    Export {
        notes: Vec<Note>,
        destination: PathBuf,
        operation: u64,
    },
}
pub enum Event {
    FileImportProgress {
        operation: u64,
        progress: file_import::Progress,
    },
    FileImportPrepared {
        operation: u64,
        result: std::result::Result<file_import::PreparedImport, String>,
    },
    FileImportCommitted {
        operation: u64,
        result: std::result::Result<file_import::Report, String>,
    },
    PortabilityProgress {
        operation: u64,
        progress: backup::Progress,
    },
    PortableExported {
        operation: u64,
        result: std::result::Result<portable::Report, String>,
    },
    BackedUp {
        operation: u64,
        result: std::result::Result<backup::VerifiedBackup, String>,
    },
    Queried {
        operation: u64,
        query: notes_core::library::Query,
        result: std::result::Result<notes_core::library::Page, String>,
    },
    Duplicated {
        operation: u64,
        source: String,
        result: std::result::Result<Note, String>,
    },
    TrashLoaded {
        operation: u64,
        id: String,
        result: std::result::Result<TrashEntry, String>,
    },
    Trashed {
        operation: u64,
        result: std::result::Result<TrashReceipt, String>,
    },
    Restored {
        operation: u64,
        result: std::result::Result<Vec<Note>, String>,
    },
    Deleted {
        operation: u64,
        result: std::result::Result<Vec<String>, String>,
    },
    FolderDeleted {
        operation: u64,
        result: std::result::Result<FolderDeletion, String>,
    },
    Purged {
        operation: u64,
        result: std::result::Result<PurgeReport, String>,
    },
    Organized {
        operation: u64,
        result: std::result::Result<notes_core::organization::Change, String>,
    },
    Captured {
        note: Note,
    },
    UpgradeProgress {
        operation: u64,
        progress: migration::Progress,
    },
    Upgraded {
        operation: u64,
        result: std::result::Result<migration::Upgrade, String>,
    },
    Imported {
        id: String,
        operation: u64,
        result: std::result::Result<Document, String>,
    },
    Saved {
        id: String,
        local_revision: u64,
        operation: u64,
        result: std::result::Result<Saved, SaveError>,
    },
    Created {
        operation: u64,
        result: std::result::Result<Note, String>,
    },
    Exported {
        operation: u64,
        result: std::result::Result<PathBuf, String>,
    },
}
pub struct Worker {
    pub requests: SyncSender<Request>,
    pub events: Receiver<Event>,
    root: PathBuf,
}
fn import_progress(
    sender: &mpsc::Sender<Event>,
    operation: u64,
) -> impl FnMut(file_import::Progress) + '_ {
    let mut last_phase = None;
    let mut last_sent = std::time::Instant::now();
    move |progress| {
        if last_phase != Some(progress.phase)
            || (progress.total > 0 && progress.completed == progress.total)
            || last_sent.elapsed() >= Duration::from_millis(50)
        {
            last_phase = Some(progress.phase);
            last_sent = std::time::Instant::now();
            let _ = sender.send(Event::FileImportProgress {
                operation,
                progress,
            });
        }
    }
}
impl Worker {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn start(root: PathBuf, fail_saves: bool) -> Result<(Self, Library)> {
        let mut store = Store::open(&root)?;
        store.fail_saves = fail_saves;
        Self::from_store(store)
    }
    pub fn from_store(mut store: Store) -> Result<(Self, Library)> {
        let root = store.root.clone();
        let (requests, rx) = mpsc::sync_channel(32);
        let (tx, events) = mpsc::channel();
        let (ready_tx, ready) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("notes-storage".into())
            .spawn(move || {
                let snapshot = store.load().map_err(|e| format!("{e:#}"));
                let failed = snapshot.is_err();
                let _ = ready_tx.send(snapshot);
                if failed {
                    return;
                }
                while let Ok(request) = rx.recv() {
                    let event = match request {
                        Request::PrepareFileImport {
                            paths,
                            cancel,
                            operation,
                        } => Event::FileImportPrepared {
                            operation,
                            result: file_import::prepare(
                                paths,
                                &cancel,
                                import_progress(&tx, operation),
                            )
                            .map_err(|e| format!("{e:#}")),
                        },
                        Request::CommitFileImport {
                            prepared,
                            folder,
                            accept_warnings,
                            cancel,
                            operation,
                        } => Event::FileImportCommitted {
                            operation,
                            result: store
                                .import_prepared(
                                    prepared,
                                    folder,
                                    accept_warnings,
                                    &cancel,
                                    import_progress(&tx, operation),
                                )
                                .map_err(|e| format!("{e:#}")),
                        },
                        Request::PortableExport {
                            selection,
                            destination,
                            format,
                            cancel,
                            operation,
                        } => Event::PortableExported {
                            operation,
                            result: store
                                .export_portable(
                                    selection,
                                    &destination,
                                    format,
                                    &cancel,
                                    |progress| {
                                        let _ = tx.send(Event::PortabilityProgress {
                                            operation,
                                            progress,
                                        });
                                    },
                                )
                                .map_err(|e| format!("{e:#}")),
                        },
                        Request::BackUp {
                            destination,
                            cancel,
                            operation,
                        } => Event::BackedUp {
                            operation,
                            result: store
                                .backup_controlled(&destination, &cancel, |progress| {
                                    let _ = tx.send(Event::PortabilityProgress {
                                        operation,
                                        progress,
                                    });
                                })
                                .map_err(|e| format!("{e:#}")),
                        },
                        Request::Query { operation, query } => Event::Queried {
                            operation,
                            result: store.query(&query).map_err(|e| format!("{e:#}")),
                            query,
                        },
                        Request::Duplicate {
                            id,
                            expected_revision,
                            operation,
                        } => Event::Duplicated {
                            operation,
                            result: store
                                .duplicate(&id, expected_revision)
                                .map_err(|e| format!("{e:#}")),
                            source: id,
                        },
                        Request::LoadTrash { id, operation } => Event::TrashLoaded {
                            operation,
                            result: store.load_trash(&id).map_err(|e| format!("{e:#}")),
                            id,
                        },
                        Request::Trash { notes, operation } => Event::Trashed {
                            operation,
                            result: store.trash(&notes).map_err(|e| format!("{e:#}")),
                        },
                        Request::Restore { notes, operation } => Event::Restored {
                            operation,
                            result: store.restore_notes(&notes).map_err(|e| format!("{e:#}")),
                        },
                        Request::UndoTrash { receipt, operation } => Event::Restored {
                            operation,
                            result: store.undo_trash(&receipt).map_err(|e| format!("{e:#}")),
                        },
                        Request::DeletePermanently { notes, operation } => Event::Deleted {
                            operation,
                            result: store
                                .delete_permanently(&notes)
                                .map_err(|e| format!("{e:#}")),
                        },
                        Request::DeleteFolder {
                            id,
                            expected_notes,
                            operation,
                        } => Event::FolderDeleted {
                            operation,
                            result: store
                                .delete_folder(&id, &expected_notes)
                                .map_err(|e| format!("{e:#}")),
                        },
                        Request::Purge { at, operation } => Event::Purged {
                            operation,
                            result: store.purge_expired(at).map_err(|e| format!("{e:#}")),
                        },
                        Request::Organize {
                            operation,
                            mutation,
                        } => Event::Organized {
                            operation,
                            result: store.organize(mutation).map_err(|e| format!("{e:#}")),
                        },
                        Request::Capture { request, reply } => {
                            match store.capture(&request) {
                                Ok(outcome) => {
                                    let _ = reply.send(Ok(outcome.receipt));
                                    if let Some(note) = outcome.created {
                                        let _ = tx.send(Event::Captured { note });
                                    }
                                }
                                Err(error) => {
                                    let _ = reply.send(Err(format!("{error:#}")));
                                }
                            }
                            continue;
                        }
                        Request::CaptureStatus { request_id, reply } => {
                            let _ = reply.send(
                                store
                                    .capture_status(&request_id)
                                    .map_err(|e| format!("{e:#}")),
                            );
                            continue;
                        }
                        Request::Upgrade {
                            destination,
                            cancel,
                            operation,
                        } => Event::Upgraded {
                            operation,
                            result: store
                                .upgrade_for_quotes(&destination, &cancel, |progress| {
                                    let _ = tx.send(Event::UpgradeProgress {
                                        operation,
                                        progress,
                                    });
                                })
                                .map_err(|e| format!("{e:#}")),
                        },
                        Request::Import {
                            id,
                            operation,
                            content,
                        } => Event::Imported {
                            result: store.import_media(&id, content).map_err(|e| e.to_string()),
                            id,
                            operation,
                        },
                        Request::Save {
                            note,
                            local_revision,
                            operation,
                        } => Event::Saved {
                            id: note.id.clone(),
                            local_revision,
                            operation,
                            result: store.save(&note),
                        },
                        Request::Create {
                            body,
                            folder,
                            operation,
                        } => Event::Created {
                            operation,
                            result: store.create(body, folder).map_err(|e| format!("{e:#}")),
                        },
                        Request::Export {
                            notes,
                            destination,
                            operation,
                        } => Event::Exported {
                            operation,
                            result: store
                                .export_recovery(&notes, &destination)
                                .map(|()| destination)
                                .map_err(|e| format!("{e:#}")),
                        },
                    };
                    // An IPC client can still own a sender while a window closes.
                    // Finish queued work even if there is no longer a UI receiver.
                    let _ = tx.send(event);
                }
            })?;
        let library = ready
            .recv()
            .context("Storage worker exited during startup.")?
            .map_err(anyhow::Error::msg)?;
        Ok((
            Self {
                requests,
                events,
                root,
            },
            library,
        ))
    }
}

impl Store {
    /// Export exact live drafts with their immutable assets. A manifest is written
    /// last in a new directory; partial exports cannot be mistaken for complete ones.
    pub fn export_recovery(&self, notes: &[Note], destination: &Path) -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        std::fs::create_dir(destination)
            .context("Choose a new directory for the recovery bundle.")?;
        let mut attachments = vec![];
        let mut hashes = std::collections::BTreeSet::new();
        for note in notes {
            hashes.extend(note.body.references("image"));
            for id in note.body.references("attachment") {
                let value=self.db.query_row("SELECT id,note_id,blob_hash,file_name,mime_type,size,created_at FROM attachments WHERE id=?1",[&id],|r|Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"note_id":r.get::<_,String>(1)?,"blob_hash":r.get::<_,String>(2)?,"file_name":r.get::<_,String>(3)?,"mime_type":r.get::<_,String>(4)?,"size":r.get::<_,i64>(5)?,"created_at":r.get::<_,i64>(6)?}))).optional()?.with_context(||format!("Attachment {id} is missing; export is incomplete."))?;
                hashes.insert(value["blob_hash"].as_str().unwrap().to_owned());
                attachments.push(value);
            }
        }
        for hash in hashes {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
                "Invalid asset hash."
            );
            let relative = PathBuf::from("blobs").join(&hash[..2]).join(&hash);
            let data = std::fs::read(self.root.join(&relative))
                .context("A referenced asset is missing; export is incomplete.")?;
            ensure!(
                format!("{:x}", Sha256::digest(&data)) == hash,
                "A referenced asset is corrupt; export is incomplete."
            );
            let path = destination.join(relative);
            std::fs::create_dir_all(path.parent().unwrap())?;
            let mut file = std::fs::File::create(&path)?;
            file.write_all(&data)?;
            file.sync_all()?;
            std::fs::File::open(path.parent().unwrap())?.sync_all()?;
        }
        let manifest = serde_json::json!({"omarchy_notes_recovery":1,"notes":notes,"attachments":attachments,"source_library":self.root});
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join("recovery.json"))?;
        file.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes())?;
        file.sync_all()?;
        if destination.join("blobs").exists() {
            std::fs::File::open(destination.join("blobs"))?.sync_all()?;
        }
        std::fs::File::open(destination)?.sync_all()?;
        std::fs::File::open(destination.parent().unwrap_or(Path::new(".")))?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_identity_is_committed_once_without_overwriting_existing_notes() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let mut draft = Note {
            id: new_id(),
            folder: None,
            body: Document::from_blocks(vec![notes_core::document::Block::paragraph(
                "A small thought",
            )]),
            revision: 0,
            created_at: 0,
            updated_at: 0,
            pinned: true,
        };
        assert!(store.load().unwrap().notes.is_empty());
        store.fail_saves = true;
        assert!(store.save(&draft).is_err());
        assert!(store.load().unwrap().notes.is_empty());
        store.fail_saves = false;
        let saved = store.save(&draft).unwrap();
        assert_eq!(saved.revision, 1);
        assert!(saved.updated_at > 0);
        let stale = draft.clone();
        draft.revision = saved.revision;
        draft.body.blocks[0].spans = vec![notes_core::document::Span::text("A newer thought")];
        assert_eq!(store.save(&draft).unwrap().revision, 2);
        assert!(store.save(&stale).is_err());
        drop(store);
        let store = Store::open(dir.path()).unwrap();
        let notes = store.load().unwrap().notes;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, draft.id);
        assert_eq!(notes[0].body, draft.body);
        assert!(notes[0].pinned);
        assert!(notes[0].created_at > 0);
    }
    #[test]
    fn committed_notes_reopen_and_conflicts_never_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let mut note = store.create(Document::default(), None).unwrap();
        let stale = note.clone();
        note.body.blocks[0].spans = vec![notes_core::document::Span::text("Hello #world")];
        assert_eq!(store.save(&note).unwrap().revision, 2);
        assert_eq!(store.save(&stale).unwrap_err(), SaveError::Conflict(2));
        drop(store);
        let store = Store::open(dir.path()).unwrap();
        let notes = store.load().unwrap().notes;
        assert_eq!(notes[0].body.title(), "Hello #world");
        assert_eq!(notes[0].revision, 2);
        assert_eq!(
            store
                .db
                .query_row(
                    "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'Hello'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
    #[test]
    fn failed_derived_write_rolls_back_body_revision_and_search_index() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let mut body = Document::default();
        body.blocks[0].spans = vec![notes_core::document::Span::text("Before")];
        let mut note = store.create(body, None).unwrap();
        store.db.execute_batch("CREATE TRIGGER fail_tag BEFORE INSERT ON note_tags WHEN new.tag='fail' BEGIN SELECT RAISE(ABORT, 'injected derived write failure'); END;").unwrap();
        note.body.blocks[0].spans = vec![notes_core::document::Span::text("After #fail")];
        assert!(matches!(store.save(&note), Err(SaveError::Failed(_))));
        let unchanged = store.load().unwrap().notes.remove(0);
        assert_eq!(unchanged.body.title(), "Before");
        assert_eq!(unchanged.revision, 1);
        let before: i64 = store
            .db
            .query_row(
                "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'Before'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let after: i64 = store
            .db
            .query_row(
                "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'After'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((before, after), (1, 0));
    }
    #[test]
    fn qt_fixture_is_preserved_and_failure_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::copy(
            "../../tests/fixtures/qt-v1/library.sqlite3",
            dir.path().join("library.sqlite3"),
        )
        .unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let notes = store.load().unwrap().notes;
        assert_eq!(notes.len(), 3);
        for mut note in notes {
            let original = note.body.clone();
            let saved = store.save(&note).unwrap();
            note.revision = saved.revision;
            store.fail_saves = true;
            assert!(store.save(&note).is_err());
            store.fail_saves = false;
            let again = store
                .load()
                .unwrap()
                .notes
                .into_iter()
                .find(|n| n.id == note.id)
                .unwrap();
            assert_eq!(again.body, original);
            assert_eq!(again.revision, saved.revision);
        }
    }
    #[test]
    fn future_schema_and_live_lock_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(Store::open(dir.path()).is_err());
        drop(store);
        let db = Connection::open(dir.path().join("library.sqlite3")).unwrap();
        db.pragma_update(None, "user_version", 99).unwrap();
        drop(db);
        let before = std::fs::read(dir.path().join("library.sqlite3")).unwrap();
        assert!(Store::open(dir.path()).is_err());
        assert_eq!(
            before,
            std::fs::read(dir.path().join("library.sqlite3")).unwrap()
        );
    }
    #[test]
    fn schema_one_migrates_transactionally() {
        let dir = tempfile::tempdir().unwrap();
        let db = Connection::open(dir.path().join("library.sqlite3")).unwrap();
        db.execute_batch(include_str!("schema-v1.sql")).unwrap();
        db.execute(
            "ATTACH DATABASE ?1 AS fixture",
            ["../../tests/fixtures/qt-v1/library.sqlite3"],
        )
        .unwrap();
        db.execute_batch("INSERT INTO folders SELECT * FROM fixture.folders;
            INSERT INTO notes SELECT id,folder_id,title,snippet,plain_text,body,body_schema,has_attachments,has_checklist,pinned,created_at,updated_at,deleted_at,revision FROM fixture.notes;
            INSERT INTO blobs SELECT * FROM fixture.blobs;
            INSERT INTO attachments SELECT * FROM fixture.attachments;
            DETACH DATABASE fixture;").unwrap();
        db.pragma_update(None, "user_version", 1).unwrap();
        drop(db);
        let mut store = Store::open(dir.path()).unwrap();
        store.create(Document::default(), None).unwrap();
        assert_eq!(store.load().unwrap().notes.len(), 4);
        let references: i64 = store
            .db
            .query_row("SELECT count(*) FROM note_attachment_refs", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(references, 1);
        assert_eq!(
            store
                .db
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn recovery_exports_exact_documents_and_verifies_assets() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = Path::new("../../tests/fixtures/qt-v1");
        std::fs::copy(
            fixture.join("library.sqlite3"),
            dir.path().join("library.sqlite3"),
        )
        .unwrap();
        let store = Store::open(dir.path()).unwrap();
        let notes = store.load().unwrap().notes;
        let mut q = store.db.prepare("SELECT hash FROM blobs").unwrap();
        let hashes: Vec<String> = q
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for hash in &hashes {
            let relative = PathBuf::from("blobs").join(&hash[..2]).join(hash);
            std::fs::create_dir_all(dir.path().join(&relative).parent().unwrap()).unwrap();
            std::fs::copy(fixture.join(&relative), dir.path().join(&relative)).unwrap();
        }
        let out = dir.path().join("recovery");
        store.export_recovery(&notes, &out).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.join("recovery.json")).unwrap()).unwrap();
        let recovered: Vec<Note> = serde_json::from_value(manifest["notes"].clone()).unwrap();
        for (a, b) in notes.iter().zip(&recovered) {
            assert_eq!(a.body, b.body);
            assert_eq!(a.id, b.id);
        }
        assert_eq!(manifest["attachments"].as_array().unwrap().len(), 1);
        for hash in &hashes {
            let relative = PathBuf::from("blobs").join(&hash[..2]).join(hash);
            assert_eq!(
                std::fs::read(out.join(&relative)).unwrap(),
                std::fs::read(fixture.join(&relative)).unwrap()
            );
        }
        let hash = &hashes[0];
        std::fs::write(
            dir.path().join("blobs").join(&hash[..2]).join(hash),
            b"corrupt",
        )
        .unwrap();
        let failed = dir.path().join("failed-export");
        assert!(store.export_recovery(&notes, &failed).is_err());
        assert!(!failed.join("recovery.json").exists());
    }
    #[test]
    fn future_document_metadata_is_rejected_without_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.create(Document::default(), None).unwrap();
        store
            .db
            .execute("UPDATE notes SET body_schema=99", [])
            .unwrap();
        drop(store);
        let before = std::fs::read(dir.path().join("library.sqlite3")).unwrap();
        assert!(Store::open(dir.path()).is_err());
        assert_eq!(
            before,
            std::fs::read(dir.path().join("library.sqlite3")).unwrap()
        );
    }
}
