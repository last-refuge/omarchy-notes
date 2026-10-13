//! Compatible folder operations on the same serialized writer as note saves.
use super::*;
use notes_core::organization::{self, Change, Mutation};

fn folders(db: &Connection) -> Result<Vec<Folder>> {
    let mut q = db.prepare("SELECT id,name,parent_id,sort_key FROM folders WHERE deleted_at IS NULL ORDER BY sort_key,name,id")?;
    Ok(q.query_map([], |r| {
        Ok(Folder {
            id: r.get(0)?,
            name: r.get(1)?,
            parent: r.get(2)?,
            sort_key: r.get(3)?,
        })
    })?
    .collect::<rusqlite::Result<_>>()?)
}
fn unique(db: &Connection, name: &str, parent: Option<&str>, id: &str) -> Result<()> {
    // SQLite NOCASE deliberately matches the existing Qt sibling-name rule.
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM folders WHERE name=?1 COLLATE NOCASE AND id!=?2 AND parent_id IS ?3)", params![name, id, parent], |r| r.get(0))?;
    ensure!(
        !exists,
        "There is already a folder called “{name}” in this location."
    );
    Ok(())
}
impl Store {
    pub fn folders(&self) -> Result<Vec<Folder>> {
        folders(&self.db)
    }
    pub fn organize(&mut self, mutation: Mutation) -> Result<Change> {
        ensure!(!self.fail_saves, "Simulated storage failure.");
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = folders(&tx)?;
        let change = match mutation {
            Mutation::CreateFolder { name, parent } => {
                let name = organization::name(&name)?;
                organization::parent(&current, None, parent.as_deref())?;
                unique(&tx, &name, parent.as_deref(), "")?;
                let folder = Folder {
                    id: new_id(),
                    name,
                    parent,
                    sort_key: 0,
                };
                tx.execute("INSERT INTO folders(id,parent_id,name,sort_key,created_at,updated_at) VALUES(?1,?2,?3,0,?4,?4)", params![folder.id, folder.parent, folder.name, now()])?;
                Change::Folder(folder)
            }
            Mutation::RenameFolder { id, name } => {
                let mut folder = current
                    .iter()
                    .find(|f| f.id == id)
                    .context("That folder no longer exists.")?
                    .clone();
                folder.name = organization::name(&name)?;
                unique(&tx, &folder.name, folder.parent.as_deref(), &id)?;
                tx.execute(
                    "UPDATE folders SET name=?2,updated_at=?3 WHERE id=?1",
                    params![id, folder.name, now()],
                )?;
                Change::Folder(folder)
            }
            Mutation::MoveFolder { id, parent } => {
                let mut folder = current
                    .iter()
                    .find(|f| f.id == id)
                    .context("That folder no longer exists.")?
                    .clone();
                organization::parent(&current, Some(&id), parent.as_deref())?;
                unique(&tx, &folder.name, parent.as_deref(), &id)?;
                folder.parent = parent;
                tx.execute(
                    "UPDATE folders SET parent_id=?2,updated_at=?3 WHERE id=?1",
                    params![id, folder.parent, now()],
                )?;
                Change::Folder(folder)
            }
            Mutation::MoveNote { id, folder } => {
                organization::parent(&current, None, folder.as_deref())?;
                // Like Qt, filing does not change the content revision, edited
                // date, history or FTS. Autosave never writes folder_id, so an
                // already queued content save cannot undo this move.
                ensure!(
                    tx.execute(
                        "UPDATE notes SET folder_id=?2 WHERE id=?1 AND deleted_at IS NULL",
                        params![id, folder]
                    )? == 1,
                    "The note no longer exists or is in Recently Deleted."
                );
                Change::NoteMoved { id, folder }
            }
        };
        tx.commit()?;
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create(s: &mut Store, name: &str, parent: Option<&str>) -> Folder {
        let Change::Folder(f) = s
            .organize(Mutation::CreateFolder {
                name: name.into(),
                parent: parent.map(str::to_owned),
            })
            .unwrap()
        else {
            panic!()
        };
        f
    }
    #[test]
    fn nested_folders_moves_and_renames_reopen_without_touching_note_content() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(tmp.path()).unwrap();
        let work = create(&mut s, " Work ", None);
        let personal = create(&mut s, "Personal", None);
        let client = create(&mut s, "Clients", Some(&work.id));
        let other = create(&mut s, "Clients", Some(&personal.id));
        assert_ne!(client.id, other.id);
        assert!(
            s.organize(Mutation::CreateFolder {
                name: "clients".into(),
                parent: Some(work.id.clone())
            })
            .is_err()
        );
        let mut body = Document::default();
        body.blocks[0] = notes_core::document::Block::paragraph("A living draft");
        let note = s.create(body, None).unwrap();
        let before: (String, i64, i64) =
            s.db.query_row(
                "SELECT body,revision,updated_at FROM notes WHERE id=?1",
                [&note.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        s.organize(Mutation::MoveNote {
            id: note.id.clone(),
            folder: Some(client.id.clone()),
        })
        .unwrap();
        let after: (String, i64, i64) =
            s.db.query_row(
                "SELECT body,revision,updated_at FROM notes WHERE id=?1",
                [&note.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(before, after);
        let mut stale_metadata = note.clone();
        stale_metadata
            .body
            .blocks
            .push(notes_core::document::Block::paragraph("Still writing"));
        s.save(&stale_metadata).unwrap();
        assert_eq!(s.load().unwrap().notes[0].folder.as_ref(), Some(&client.id));
        assert!(
            s.organize(Mutation::MoveFolder {
                id: work.id.clone(),
                parent: Some(client.id.clone())
            })
            .is_err()
        );
        assert!(
            s.organize(Mutation::MoveFolder {
                id: client.id.clone(),
                parent: Some(personal.id.clone())
            })
            .is_err()
        );
        s.organize(Mutation::RenameFolder {
            id: client.id.clone(),
            name: "Projects".into(),
        })
        .unwrap();
        s.organize(Mutation::MoveFolder {
            id: client.id.clone(),
            parent: Some(personal.id.clone()),
        })
        .unwrap();
        assert!(
            s.organize(Mutation::MoveNote {
                id: note.id.clone(),
                folder: Some("missing".into())
            })
            .is_err()
        );
        s.fail_saves = true;
        assert!(
            s.organize(Mutation::MoveNote {
                id: note.id.clone(),
                folder: None
            })
            .is_err()
        );
        drop(s);
        let s = Store::open(tmp.path()).unwrap();
        let loaded = s.load().unwrap();
        assert_eq!(loaded.notes[0].body, stale_metadata.body);
        assert_eq!(loaded.notes[0].folder.as_ref(), Some(&client.id));
        let folder = loaded.folders.iter().find(|f| f.id == client.id).unwrap();
        assert_eq!(folder.name, "Projects");
        assert_eq!(folder.parent.as_ref(), Some(&personal.id));
        assert_eq!(s.schema().unwrap(), 2);
    }
}
