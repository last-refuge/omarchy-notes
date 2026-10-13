//! Format-1 library backups. Publication never replaces an existing path.
//! All work is synchronous: call from a CLI or the single storage worker.
use crate::{Store, lock::LibraryLock, now};
use anyhow::{Context, Result, bail, ensure};
use notes_core::document::{Document, new_id};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const DATABASE: &str = "library.sqlite3";
const FORMAT: &str = "omarchy-notes-backup";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub schema_version: u32,
    pub created_at: i64,
    pub notes: u64,
    pub blobs: u64,
    /// Database and blob bytes, excluding the manifest (same meaning as Qt).
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_sha256: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VerifiedBackup {
    pub path: PathBuf,
    pub manifest: Manifest,
}

struct Inventory {
    schema: u32,
    notes: u64,
    blobs: BTreeMap<String, u64>,
}

/// Snapshot a closed library without migrating it. The Qt-compatible lock also
/// excludes a running Rust or Qt edition for the complete database/blob copy.
pub fn create(root: &Path, destination: &Path) -> Result<VerifiedBackup> {
    let root = root
        .canonicalize()
        .context("The library folder is missing.")?;
    let _lock = LibraryLock::acquire(&root)?;
    regular(&root.join(DATABASE))?;
    let db = Connection::open_with_flags(root.join(DATABASE), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    create_from(&db, &root, destination, &mut |_| Ok(()))
}

impl Store {
    /// Uses this store's existing ownership; cannot overlap a writer operation.
    pub fn backup_to(&mut self, destination: &Path) -> Result<VerifiedBackup> {
        create_from(&self.db, &self.root, destination, &mut |_| Ok(()))
    }
}

/// Inspect a completed, standalone backup without changing its contents.
pub fn inspect(root: &Path) -> Result<VerifiedBackup> {
    let (backup, _) = inspect_inner(root)?;
    Ok(backup)
}

/// Restore only to a new folder. The backup and any current library stay intact.
/// This deliberately does not open/migrate the restored library.
pub fn restore_to_new(source: &Path, destination: &Path) -> Result<VerifiedBackup> {
    restore_with(source, destination, &mut |_| Ok(()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Snapshot,
    Blob,
    Manifest,
    BeforePublish,
    Published,
}

pub(crate) fn create_from(
    db: &Connection,
    root: &Path,
    destination: &Path,
    checkpoint: &mut impl FnMut(Phase) -> Result<()>,
) -> Result<VerifiedBackup> {
    // Refuse future formats before creating a staging directory.
    supported_schema(db)?;
    let stage = Stage::new(destination)?;
    let path = stage.path.join(DATABASE);
    new_file(&path)?;
    let mut snapshot = Connection::open(&path)?;
    {
        let backup = Backup::new(db, &mut snapshot)?;
        let started = Instant::now();
        loop {
            use rusqlite::backup::StepResult::*;
            match backup.step(256)? {
                Done => break,
                More => {}
                Busy | Locked => {
                    ensure!(
                        started.elapsed() < Duration::from_secs(10),
                        "The database stayed busy. Close the other writer and retry the backup."
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => bail!("SQLite returned an unknown backup status."),
            }
        }
    }
    standalone(&snapshot)?;
    let inventory = validate_database(&snapshot)?;
    verify_search(&snapshot)?;
    snapshot.close().map_err(|(_, e)| e)?;
    regular(&path)?.sync_all()?;
    checkpoint(Phase::Snapshot)?;
    copy_blobs(root, &stage.path, &inventory, checkpoint)?;
    let (database_size, digest) = hash_file(&path)?;
    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: 1,
        app_version: format!("{}-rust-preview", env!("CARGO_PKG_VERSION")),
        schema_version: inventory.schema,
        created_at: now(),
        notes: inventory.notes,
        blobs: inventory.blobs.len() as u64,
        bytes: total_bytes(database_size, &inventory)?,
        database_sha256: Some(digest),
    };
    let mut file = new_file(&stage.path.join("manifest.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    // Read the exact completed files, not just the source/model we copied from.
    inspect_inner(&stage.path)?;
    sync_directory(&stage.path)?;
    checkpoint(Phase::Manifest)?;
    let path = stage.publish(checkpoint)?;
    Ok(VerifiedBackup { path, manifest })
}

fn restore_with(
    source: &Path,
    destination: &Path,
    checkpoint: &mut impl FnMut(Phase) -> Result<()>,
) -> Result<VerifiedBackup> {
    let (backup, inventory) = inspect_inner(source)?;
    let stage = Stage::new(destination)?;
    let database = backup.path.join(DATABASE);
    // Old manifests have no database digest. Capture one before copying anyway,
    // then verify the copied snapshot so concurrent source changes cannot slip in.
    let (size, digest) = hash_file(&database)?;
    if let Some(expected) = &backup.manifest.database_sha256 {
        ensure!(
            expected == &digest,
            "The backup database changed during restoration."
        );
    }
    copy_verified(&database, &stage.path.join(DATABASE), size, &digest)?;
    checkpoint(Phase::Snapshot)?;
    copy_blobs(&backup.path, &stage.path, &inventory, checkpoint)?;
    let db = Connection::open(stage.path.join(DATABASE))?;
    let actual = validate_database(&db)?;
    ensure!(
        actual.schema == inventory.schema
            && actual.notes == inventory.notes
            && actual.blobs == inventory.blobs,
        "The backup database changed during restoration."
    );
    verify_search(&db)?;
    db.close().map_err(|(_, e)| e)?;
    verify_blobs(&stage.path, &actual)?;
    // Checking the FTS index must not alter the restored database bytes.
    ensure!(
        hash_file(&stage.path.join(DATABASE))? == (size, digest),
        "The database changed while it was being verified."
    );
    sync_directory(&stage.path)?;
    let path = stage.publish(checkpoint)?;
    Ok(VerifiedBackup {
        path,
        manifest: backup.manifest,
    })
}

fn inspect_inner(root: &Path) -> Result<(VerifiedBackup, Inventory)> {
    let root = root
        .canonicalize()
        .context("The backup folder is missing.")?;
    let mut bytes = Vec::new();
    regular(&root.join("manifest.json"))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 1024 * 1024,
        "The backup manifest is too large."
    );
    let manifest: Manifest =
        serde_json::from_slice(&bytes).context("The backup manifest is incomplete or damaged.")?;
    ensure!(
        manifest.format == FORMAT && manifest.format_version == 1,
        "Unsupported backup format; update Notes before restoring it."
    );
    ensure!(
        (1..=3).contains(&manifest.schema_version),
        "Unsupported backup database schema {}; update Notes before restoring it.",
        manifest.schema_version
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        ensure!(
            !exists(&root.join(format!("{DATABASE}{suffix}")))?,
            "A completed backup must contain a standalone database without SQLite sidecars."
        );
    }
    let (size, digest) = hash_file(&root.join(DATABASE))?;
    if let Some(expected) = &manifest.database_sha256 {
        ensure!(
            valid_hash(expected) && expected == &digest,
            "The backup database failed its SHA-256 check."
        );
    }
    let db = immutable_database(&root.join(DATABASE))?;
    let journal: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    ensure!(
        journal == "delete",
        "The backup database is not a standalone rollback-journal snapshot."
    );
    let inventory = validate_database(&db)?;
    ensure!(
        inventory.schema == manifest.schema_version,
        "The manifest and database schema versions disagree."
    );
    ensure!(
        inventory.notes == manifest.notes
            && inventory.blobs.len() as u64 == manifest.blobs
            && total_bytes(size, &inventory)? == manifest.bytes,
        "The backup counts or recorded size do not match its contents."
    );
    verify_blobs(&root, &inventory)?;
    Ok((
        VerifiedBackup {
            path: root,
            manifest,
        },
        inventory,
    ))
}

fn verify_blobs(root: &Path, inventory: &Inventory) -> Result<()> {
    for (hash, size) in &inventory.blobs {
        let (actual_size, actual_hash) = hash_file(&blob_path(root, hash)?)?;
        ensure!(
            actual_size == *size && actual_hash == *hash,
            "Attachment {hash} is missing or damaged; the backup is incomplete."
        );
    }
    Ok(())
}

fn supported_schema(db: &Connection) -> Result<u32> {
    let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        (1..=3).contains(&version),
        "Unsupported database schema {version}; no migration or backup was performed."
    );
    Ok(version)
}

fn validate_database(db: &Connection) -> Result<Inventory> {
    let schema = supported_schema(db)?;
    crate::migration::validate_documents(db, schema)?;
    if schema == 3 {
        crate::migration::receipt(db)?;
    }
    let mut checks = db.prepare("PRAGMA quick_check")?;
    let results = checks
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        results == ["ok"],
        "The database failed its integrity check. Preserve it for recovery."
    );
    ensure!(
        !db.prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some(),
        "The database contains broken foreign-key references."
    );
    let mut blobs = BTreeMap::new();
    let mut query = db.prepare("SELECT hash,size FROM blobs")?;
    for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))? {
        let (hash, size) = row?;
        ensure!(
            valid_hash(&hash),
            "The database contains an invalid blob address."
        );
        blobs.insert(hash, size);
    }
    let mut attachments = BTreeMap::new();
    let mut query = db.prepare("SELECT id,note_id,blob_hash,size FROM attachments")?;
    for row in query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, u64>(3)?,
        ))
    })? {
        let (id, note, hash, size) = row?;
        ensure!(
            blobs.get(&hash) == Some(&size),
            "Attachment {id} has inconsistent blob metadata."
        );
        attachments.insert(id, note);
    }
    let mut query = db.prepare("SELECT id,body,body_schema FROM notes UNION ALL SELECT note_id,body,body_schema FROM note_revisions")?;
    for row in query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, u32>(2)?,
        ))
    })? {
        let (id, json, schema) = row?;
        let body = Document::from_json(&json)
            .with_context(|| format!("Note {id} or its revision history cannot be read."))?;
        ensure!(
            schema == body.schema,
            "Note {id} has inconsistent document version metadata."
        );
        for hash in body.references("image") {
            ensure!(
                blobs.contains_key(&hash),
                "Note {id} references an unregistered image: {hash}."
            );
        }
        for attachment in body.references("attachment") {
            ensure!(
                attachments.get(&attachment) == Some(&id),
                "Note {id} references a missing or foreign attachment: {attachment}."
            );
        }
    }
    if schema >= 2 {
        let dangling: u64 = db.query_row("SELECT count(*) FROM note_attachment_refs r LEFT JOIN attachments a ON a.id=r.attachment_id AND a.note_id=r.note_id WHERE a.id IS NULL", [], |r| r.get(0))?;
        ensure!(
            dangling == 0,
            "The attachment reference index contains missing attachments."
        );
    }
    let notes = db.query_row("SELECT count(*) FROM notes", [], |r| r.get(0))?;
    Ok(Inventory {
        schema,
        notes,
        blobs,
    })
}

fn verify_search(db: &Connection) -> Result<()> {
    // FTS5 compares the index against its external-content table when rank=1.
    // This command verifies; it does not rebuild or change the index.
    db.execute(
        "INSERT INTO notes_fts(notes_fts,rank) VALUES('integrity-check',1)",
        [],
    )
    .context("The search index failed verification. Preserve the library for repair.")?;
    Ok(())
}

fn standalone(db: &Connection) -> Result<()> {
    let journal: String = db.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
    ensure!(
        journal == "delete",
        "Could not create a standalone database snapshot."
    );
    db.pragma_update(None, "synchronous", "FULL")?;
    Ok(())
}

fn total_bytes(database: u64, inventory: &Inventory) -> Result<u64> {
    inventory.blobs.values().try_fold(database, |total, size| {
        total
            .checked_add(*size)
            .context("Backup size is out of range.")
    })
}

fn immutable_database(path: &Path) -> Result<Connection> {
    // Percent-encode the native absolute path, including '?' and '#'. immutable
    // prevents SQLite from creating sidecars while inspecting a completed backup.
    let encoded: String = path
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|b| format!("%{b:02X}"))
        .collect();
    Ok(Connection::open_with_flags(
        format!("file:{encoded}?immutable=1"),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?)
}

fn copy_blobs(
    source: &Path,
    destination: &Path,
    inventory: &Inventory,
    checkpoint: &mut impl FnMut(Phase) -> Result<()>,
) -> Result<()> {
    let root = destination.join("blobs");
    fs::create_dir(&root)?;
    let mut shards = BTreeSet::new();
    for (hash, size) in &inventory.blobs {
        let shard = root.join(&hash[..2]);
        if shards.insert(shard.clone()) {
            fs::create_dir(&shard)?;
        }
        copy_verified(&blob_path(source, hash)?, &shard.join(hash), *size, hash)?;
        checkpoint(Phase::Blob)?;
    }
    for shard in shards {
        sync_directory(&shard)?;
    }
    sync_directory(&root)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn blob_path(root: &Path, hash: &str) -> Result<PathBuf> {
    ensure!(valid_hash(hash), "Invalid blob address.");
    for path in [root.join("blobs"), root.join("blobs").join(&hash[..2])] {
        ensure!(
            fs::symlink_metadata(&path)?.file_type().is_dir(),
            "An attachment directory is missing or is a symbolic link."
        );
    }
    Ok(root.join("blobs").join(&hash[..2]).join(hash))
}

fn regular(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .with_context(|| format!("Could not read {}.", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "{} is not a regular file.",
        path.display()
    );
    Ok(file)
}

fn new_file(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}

fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = regular(path)?;
    let size = file.metadata()?.len();
    let digest = stream(&mut file, None, size)?;
    Ok((size, digest))
}

fn copy_verified(source: &Path, destination: &Path, size: u64, digest: &str) -> Result<()> {
    let mut input = regular(source)?;
    ensure!(
        input.metadata()?.len() == size,
        "{} has an unexpected size.",
        source.display()
    );
    let mut output = new_file(destination)?;
    ensure!(
        stream(&mut input, Some(&mut output), size)? == digest,
        "{} failed its SHA-256 check.",
        source.display()
    );
    output.sync_all()?;
    Ok(())
}

fn stream(input: &mut File, mut output: Option<&mut File>, size: u64) -> Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut remaining = size;
    while remaining > 0 {
        let length = remaining.min(buffer.len() as u64) as usize;
        let read = input.read(&mut buffer[..length])?;
        ensure!(read > 0, "A backup file shrank while it was being read.");
        digest.update(&buffer[..read]);
        if let Some(output) = output.as_deref_mut() {
            output.write_all(&buffer[..read])?;
        }
        remaining -= read as u64;
    }
    ensure!(
        input.read(&mut buffer[..1])? == 0,
        "A backup file grew while it was being read."
    );
    Ok(format!("{:x}", digest.finalize()))
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?
        .sync_all()
        .with_context(|| format!("Could not durably sync {}.", path.display()))
}

struct Stage {
    path: PathBuf,
    destination: PathBuf,
    published: bool,
}
impl Stage {
    fn new(destination: &Path) -> Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        let name = destination
            .file_name()
            .context("Choose a new folder name.")?;
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .context("The destination's parent folder must already exist.")?;
        let destination = parent.join(name);
        ensure!(
            !exists(&destination)?,
            "{} already exists. Choose a new folder; existing files will not be replaced.",
            destination.display()
        );
        let path = parent.join(format!(".omarchy-notes-{}.partial", new_id()));
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self {
            path,
            destination,
            published: false,
        })
    }
    fn publish(mut self, checkpoint: &mut impl FnMut(Phase) -> Result<()>) -> Result<PathBuf> {
        sync_directory(&self.path)?;
        checkpoint(Phase::BeforePublish)?;
        rename_new(&self.path, &self.destination)?;
        self.published = true;
        sync_directory(self.destination.parent().unwrap()).with_context(|| format!("The completed copy exists at {}, but its directory sync failed. Inspect it before relying on it.", self.destination.display()))?;
        checkpoint(Phase::Published)?;
        Ok(self.destination.clone())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn rename_new(source: &Path, destination: &Path) -> Result<()> {
    let from = CString::new(source.as_os_str().as_bytes())?;
    let to = CString::new(destination.as_os_str().as_bytes())?;
    // Linux's no-replace rename is atomic even if another process creates the
    // destination after our initial check. No remove/replace fallback is safe.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context(
            "Could not publish the completed copy; an existing destination is never overwritten.",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
