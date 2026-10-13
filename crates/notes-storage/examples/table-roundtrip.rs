//! Mutate, save and reopen real Qt table fixtures; emit documents for the Qt editor oracle.
use notes_core::document::Document;
use notes_editor::{Editor, Position, TableAction};
fn main() -> anyhow::Result<()> {
    let root = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("scratch output directory"),
    );
    std::fs::create_dir_all(&root)?;
    let documents = root.join("documents");
    std::fs::create_dir(&documents)?;
    let mut store = notes_storage::Store::open(&root.join("library"))?;
    let original =
        Document::from_json(include_str!("../../../tests/fixtures/qt-merged-table.json"))?;
    let table = original.blocks.iter().find(|b| b.kind == "table").unwrap();
    let mut expected = vec![];
    for (index, cell) in table.cells.iter().enumerate() {
        for action in [
            TableAction::RowBefore,
            TableAction::RowAfter,
            TableAction::ColumnBefore,
            TableAction::ColumnAfter,
            TableAction::DeleteRow,
            TableAction::DeleteColumn,
            TableAction::SplitCell,
        ] {
            let mut editor = Editor::new(original.clone());
            editor.select(
                editor.global(&Position {
                    block: cell.blocks[0].id.clone(),
                    offset: 0,
                }),
                false,
            );
            if !editor.table_context().unwrap().allows(action) {
                continue;
            }
            editor.table_action(action)?;
            let after = editor.document.clone();
            editor.undo();
            assert_eq!(editor.document, original);
            editor.redo();
            assert_eq!(editor.document, after);
            let mut note = store.create(original.clone(), None)?;
            note.body = after.clone();
            store.save(&note).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            std::fs::write(
                documents.join(format!("{index}-{action:?}.json")),
                serde_json::to_vec_pretty(&after)?,
            )?;
            expected.push((note.id, after));
        }
    }
    let mut editor = Editor::new(Document::default());
    editor.insert("Created in Rust", 0)?;
    editor.insert_table(2, 2)?;
    for text in ["One", "Two", "Three", "Four"] {
        editor.insert(text, 0)?;
        editor.tab(false)?;
    }
    assert_eq!(editor.table_context().unwrap().rows, 3);
    editor.leave_table()?;
    editor.insert("After the table", 0)?;
    let note = store.create(editor.document.clone(), None)?;
    std::fs::write(
        documents.join("created-in-rust.json"),
        serde_json::to_vec_pretty(&editor.document)?,
    )?;
    expected.push((note.id, editor.document));
    drop(store);
    let saved = notes_storage::Store::open(&root.join("library"))?.load()?;
    for (id, body) in &expected {
        assert_eq!(
            &saved.notes.iter().find(|n| &n.id == id).unwrap().body,
            body
        );
    }
    println!(
        "Saved and reopened {} table documents for Qt verification.",
        expected.len()
    );
    Ok(())
}
