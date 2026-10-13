//! Idempotent capture: the note and its receipt commit in the same transaction.
//! Receipts intentionally outlive deletion of their note, preventing resurrection.
use crate::{Note, Store};
use anyhow::{Result, ensure};
use notes_core::document::{Block, Document};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const MAX_TEXT: usize = 32 * 1024 * 1024;
const TABLE: &str = "rust_capture_requests_v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub request_id: String,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub request_id: String,
    pub note_id: String,
    pub committed_at: i64,
}
pub struct Outcome {
    pub receipt: Receipt,
    pub created: Option<Note>,
}
pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        id.len() == 36
            && id.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
                }
            }),
        "A capture request ID must be a lowercase UUID (36 characters)."
    );
    Ok(())
}
impl Capture {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.request_id)?;
        ensure!(
            self.text.len() <= MAX_TEXT,
            "Capture exceeds the 32 MiB text limit."
        );
        ensure!(
            !self.text.trim().is_empty(),
            "Nothing to capture. Enter some text first."
        );
        Ok(())
    }
    fn normalized(&self) -> Result<(String, String)> {
        self.validate()?;
        let text = self.text.replace("\r\n", "\n").replace('\r', "\n");
        let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
        Ok((text, digest))
    }
}
fn lookup(db: &Connection, id: &str) -> Result<Option<(Receipt, String)>> {
    validate_id(id)?;
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [TABLE],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    let result: Option<(Receipt, String)> = db.query_row("SELECT note_id,committed_at,payload_sha256 FROM rust_capture_requests_v1 WHERE request_id=?1", [id], |r| Ok((Receipt { request_id:id.into(),note_id:r.get(0)?,committed_at:r.get(1)? },r.get(2)?))).optional()?;
    if let Some((receipt, digest)) = &result {
        validate_id(&receipt.note_id)?;
        ensure!(
            receipt.committed_at >= 0
                && digest.len() == 64
                && digest
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "The capture receipt is invalid. Preserve this library for recovery; no new note was created."
        );
    }
    Ok(result)
}
/// Read-only lookup; does not create a library, acquire a writer lock or migrate.
/// An absent receipt means no committed capture at this read's snapshot, not that
/// another currently running request cannot subsequently finish.
pub fn inspect(root: &Path, id: &str) -> Result<Option<Receipt>> {
    validate_id(id)?;
    let path = root.join("library.sqlite3");
    if !path.try_exists()? {
        return Ok(None);
    }
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(
        (1..=3).contains(&version),
        "Unsupported database schema {version}; no lookup was performed."
    );
    Ok(lookup(&db, id)?.map(|(r, _)| r))
}
impl Store {
    pub fn capture_status(&self, id: &str) -> Result<Option<Receipt>> {
        Ok(lookup(&self.db, id)?.map(|(r, _)| r))
    }
    pub fn capture(&mut self, request: &Capture) -> Result<Outcome> {
        self.capture_with(request, || Ok(()), || Ok(()))
    }
    fn capture_with(
        &mut self,
        request: &Capture,
        before_commit: impl FnOnce() -> Result<()>,
        after_commit: impl FnOnce() -> Result<()>,
    ) -> Result<Outcome> {
        let (text, digest) = request.normalized()?;
        if let Some((receipt, prior)) = lookup(&self.db, &request.request_id)? {
            ensure!(
                prior == digest,
                "This request ID already belongs to different text. Keep the original ID for its original capture; use a new ID for a new note."
            );
            return Ok(Outcome {
                receipt,
                created: None,
            });
        }
        let body = Document::from_blocks(text.split('\n').map(Block::paragraph).collect());
        let committed_at = crate::now();
        let created=self.create_with(body,None,|db,id| {
            // Optional additive metadata is readable/ignored by Qt. No document
            // format is enabled here, and backup snapshots preserve this table.
            db.execute_batch("CREATE TABLE IF NOT EXISTS rust_capture_requests_v1 (request_id TEXT PRIMARY KEY NOT NULL, payload_sha256 TEXT NOT NULL, note_id TEXT NOT NULL, committed_at INTEGER NOT NULL) WITHOUT ROWID;")?;
            db.execute("INSERT INTO rust_capture_requests_v1 VALUES (?1,?2,?3,?4)",params![request.request_id,digest,id,committed_at])?;
            before_commit()
        })?;
        after_commit()?;
        Ok(Outcome {
            receipt: Receipt {
                request_id: request.request_id.clone(),
                note_id: created.id.clone(),
                committed_at,
            },
            created: Some(created),
        })
    }
}

#[cfg(test)]
mod tests;
