//! Explicit, backed-up opt-in to database 3 and document 2. Opening never opts in.
use crate::{Store, backup, now};
use anyhow::{Context, Result, ensure};
use notes_core::document::Document;
use rusqlite::{Connection, TransactionBehavior, params};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    fn check(&self) -> Result<()> {
        ensure!(
            !self.0.load(Ordering::Relaxed),
            "Upgrade cancelled. The library format is unchanged; any completed backup is retained."
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    BackingUp,
    Verifying,
    Committing,
}

#[derive(Clone, Debug)]
pub struct Upgrade {
    pub backup: PathBuf,
    pub database_sha256: String,
    pub applied_at: i64,
}

pub(crate) fn receipt(db: &Connection) -> Result<Upgrade> {
    let receipt = db.query_row("SELECT backup_path,database_sha256,applied_at FROM schema_migrations WHERE version=3", [], |r| Ok(Upgrade { backup: PathBuf::from(r.get::<_, String>(0)?), database_sha256:r.get(1)?, applied_at:r.get(2)? }))
        .context("The schema-3 migration record is missing or invalid. Preserve this library for recovery.")?;
    ensure!(
        receipt.backup.is_absolute()
            && receipt.database_sha256.len() == 64
            && receipt
                .database_sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "The schema-3 migration record is invalid."
    );
    Ok(receipt)
}

pub(crate) fn validate_documents(db: &Connection, database_schema: u32) -> Result<()> {
    let mut query = db.prepare("SELECT id,body,body_schema FROM notes UNION ALL SELECT note_id,body,body_schema FROM note_revisions")?;
    for row in query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, u32>(2)?,
        ))
    })? {
        let (id, json, schema) = row?;
        let body = Document::from_json(&json).with_context(|| format!("Note {id} or its history has an unsupported document. Its data has not been changed."))?;
        ensure!(
            body.schema == schema && (database_schema >= 3 || schema == 1),
            "Note {id} has inconsistent document/database versions. Its data has not been changed."
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    BackedUp,
    BeforeCommit,
    Committed,
}

impl Store {
    pub fn upgrade_for_quotes(
        &mut self,
        destination: &Path,
        cancel: &Cancellation,
        progress: impl FnMut(Progress),
    ) -> Result<Upgrade> {
        self.upgrade_with(destination, cancel, progress, |_| Ok(()))
    }
    fn upgrade_with(
        &mut self,
        destination: &Path,
        cancel: &Cancellation,
        mut progress: impl FnMut(Progress),
        mut boundary: impl FnMut(Boundary) -> Result<()>,
    ) -> Result<Upgrade> {
        if self.schema()? == 3 {
            return receipt(&self.db);
        }
        ensure!(
            self.schema()? == 2,
            "Only a schema-2 library can enable quotes."
        );
        ensure!(
            !self.fail_saves,
            "Simulated storage failure. The library format is unchanged."
        );
        cancel.check()?;
        progress(Progress::BackingUp);
        let backup = backup::create_from(&self.db, &self.root, destination, &mut |phase| {
            cancel.check()?;
            if phase == backup::Phase::Manifest {
                progress(Progress::Verifying);
            }
            Ok(())
        })?;
        // The ownership lock and single writer cover the snapshot through commit.
        // No original body/history, identity, index or timestamp is rewritten.
        boundary(Boundary::BackedUp)?;
        cancel.check()?;
        progress(Progress::Committing);
        let receipt = Upgrade {
            backup: backup.path,
            database_sha256: backup
                .manifest
                .database_sha256
                .context("The pre-upgrade backup has no database digest.")?,
            applied_at: now(),
        };
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL, backup_path TEXT NOT NULL, database_sha256 TEXT NOT NULL);")?;
        tx.execute(
            "INSERT INTO schema_migrations VALUES(3,?1,?2,?3)",
            params![
                receipt.applied_at,
                receipt
                    .backup
                    .to_str()
                    .context("Choose a UTF-8 backup path for the migration record.")?,
                receipt.database_sha256
            ],
        )?;
        tx.pragma_update(None, "user_version", 3)?;
        boundary(Boundary::BeforeCommit)?;
        cancel.check()?;
        tx.commit()?;
        boundary(Boundary::Committed)?;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
