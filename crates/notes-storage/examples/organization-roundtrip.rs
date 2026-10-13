//! Scratch-library driver for the actual Qt NoteStore interoperability oracle.
use anyhow::{Context, Result, ensure};
use notes_core::{
    document::{Block, Document},
    organization::{Change, Mutation},
};
use notes_storage::{Folder, Store};
use serde_json::{Value, json};
use std::path::Path;

fn folder(store: &mut Store, name: &str, parent: Option<String>) -> Result<Folder> {
    let Change::Folder(folder) = store.organize(Mutation::CreateFolder {
        name: name.into(),
        parent,
    })?
    else {
        unreachable!()
    };
    Ok(folder)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "Usage: organization-roundtrip seed|verify DIRECTORY"
    );
    let root = Path::new(&args[2]);
    let mut store = Store::open(root)?;
    match args[1].as_str() {
        "seed" => {
            ensure!(
                store.load()?.notes.is_empty(),
                "Use an empty scratch library"
            );
            let work = folder(&mut store, "Work", None)?;
            let personal = folder(&mut store, "Personal", None)?;
            let clients = folder(&mut store, "Clients", Some(work.id.clone()))?;
            let other = folder(&mut store, "Clients", Some(personal.id.clone()))?;
            let mut body = Document::default();
            body.blocks[0] = Block::paragraph("An idea worth keeping. #organization");
            let mut note = store.create(body, None)?;
            store.organize(Mutation::MoveNote {
                id: note.id.clone(),
                folder: Some(clients.id.clone()),
            })?;
            // Content queued with old folder metadata cannot undo the move.
            note.body
                .blocks
                .push(Block::paragraph("A draft saved after filing."));
            store.save(&note).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            store.organize(Mutation::RenameFolder {
                id: clients.id.clone(),
                name: "Projects".into(),
            })?;
            store.organize(Mutation::MoveFolder {
                id: clients.id.clone(),
                parent: Some(personal.id.clone()),
            })?;
            folder(&mut store, "Clients", Some(work.id.clone()))?;
            let mut second = Document::default();
            second.blocks[0] = Block::paragraph("The other Clients folder");
            store.create(second, Some(other.id))?;
            let library = store.load()?;
            let note = library.notes.iter().find(|n| n.id == note.id).unwrap();
            ensure!(note.folder.as_ref() == Some(&clients.id));
            let expected = json!({
                "work": work.id, "subject": clients.id, "note": note,
                "folders": library.folders.iter().map(|f| json!({
                    "id": f.id, "name": f.name, "parent": f.parent,
                })).collect::<Vec<_>>(),
            });
            std::fs::write(root.join("expected.json"), serde_json::to_vec(&expected)?)?;
        }
        "verify" => {
            let expected: Value =
                serde_json::from_slice(&std::fs::read(root.join("expected.json"))?)?;
            let qt: Value = serde_json::from_slice(&std::fs::read(root.join("qt-result.json"))?)?;
            let original: notes_storage::Note = serde_json::from_value(expected["note"].clone())?;
            let library = store.load()?;
            ensure!(library.schema == 2);
            let note = library
                .notes
                .iter()
                .find(|n| n.id == original.id)
                .context("Note missing")?;
            ensure!(note.body == original.body);
            ensure!(note.revision == original.revision && note.updated_at == original.updated_at);
            ensure!(note.folder.as_deref() == qt["destination"].as_str());
            let subject = library
                .folders
                .iter()
                .find(|f| Some(f.id.as_str()) == expected["subject"].as_str())
                .unwrap();
            ensure!(subject.name == "Renamed by Qt");
            ensure!(
                library
                    .folders
                    .iter()
                    .any(|f| Some(f.id.as_str()) == qt["child"].as_str()
                        && f.parent.as_deref() == qt["destination"].as_str()
                        && f.name == "Clients")
            );
            println!(
                "PASS: Rust → actual Qt NoteStore → Rust retains folder identities, nesting, note body/revision, and Qt folder edits."
            );
        }
        _ => anyhow::bail!("Expected seed or verify"),
    }
    Ok(())
}
