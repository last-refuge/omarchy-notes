use super::*;

fn editor(rows: usize, cols: usize) -> Editor {
    let mut e = Editor::new(Document {
        blocks: vec![Block::paragraph("Before")],
        ..Default::default()
    });
    e.insert_table(rows, cols).unwrap();
    e
}
fn table(e: &Editor) -> &Block {
    e.document
        .blocks
        .iter()
        .find(|b| b.kind == "table")
        .unwrap()
}
fn select(e: &mut Editor, row: usize, col: usize) {
    let cell = table(e)
        .cells
        .iter()
        .find(|c| {
            c.row <= row
                && row < c.row + span(c, true)
                && c.col <= col
                && col < c.col + span(c, false)
        })
        .unwrap()
        .clone();
    e.select_cell(&cell, false);
}
fn valid(e: &Editor) {
    e.document.validate().unwrap();
    assert!(e.document.block(&e.selection.head.block).is_some());
    assert!(e.document.block(&e.selection.anchor.block).is_some());
    let t = table(e);
    let mut copy = t.clone();
    normalize(&mut copy).unwrap();
    assert_eq!(
        &copy, t,
        "all cells must cover the grid exactly once in logical order"
    );
}
fn merged() -> Editor {
    let mut e = editor(3, 3);
    let id = table(&e).id.clone();
    let mut t = table(&e).clone();
    t.attrs
        .insert("future-table-metadata".into(), json!({"keep": true}));
    t.cells
        .retain(|c| !(c.row < 2 && c.col < 2 && (c.row != 0 || c.col != 0)));
    let cell = &mut t.cells[0];
    cell.attrs.insert("rowSpan".into(), json!(2));
    cell.attrs.insert("colSpan".into(), json!(2));
    cell.attrs
        .insert("future-cell-metadata".into(), json!("keep"));
    cell.blocks = vec![
        Block::paragraph("Merged 👩‍👩‍👧‍👦"),
        Block::paragraph("second paragraph"),
    ];
    cell.blocks[0].spans[0].set_mark("bold", true);
    e.install_table(t);
    select(&mut e, 0, 0);
    assert_eq!(e.table_context().unwrap().id, id);
    e
}

#[test]
fn insertion_preserves_text_selection_and_one_undo_restores_everything() {
    let mut e = Editor::new(Document {
        blocks: vec![Block::paragraph("A paragraph")],
        ..Default::default()
    });
    e.select(2, false);
    e.select(7, true);
    let before = e.snapshot();
    e.insert_table(2, 3).unwrap();
    assert_eq!(e.document.blocks[0], before.document.blocks[0]);
    assert_eq!(e.table_context().unwrap().column, 0);
    assert_eq!(table(&e).cells.len(), 6);
    let after = e.document.clone();
    e.undo();
    assert_eq!(e.document, before.document);
    assert_eq!(e.selection, before.selection);
    e.redo();
    assert_eq!(e.document, after);
    valid(&e);
}
#[test]
fn tab_selects_whole_cells_skips_merged_coverage_and_appends_once() {
    let mut e = merged();
    let id = table(&e).id.clone();
    let cell_ids: Vec<_> = table(&e)
        .cells
        .iter()
        .map(|c| first_leaf(&c.blocks).unwrap().id.clone())
        .collect();
    let initial = e.selection.clone();
    e.tab(true).unwrap();
    assert_eq!(e.selection, initial);
    for expected in cell_ids.iter().skip(1) {
        assert!(e.tab(false).unwrap());
        assert_eq!(&e.selection.anchor.block, expected);
    }
    let before = e.snapshot();
    e.tab(false).unwrap();
    assert_eq!(e.table_context().unwrap().row, 3);
    assert_eq!(table(&e).number("rows", 0), 4);
    assert_eq!(table(&e).id, id);
    valid(&e);
    e.undo();
    assert_eq!(e.document, before.document);
    assert_eq!(e.selection, before.selection);
    select(&mut e, 0, 2);
    e.tab(true).unwrap();
    let range = e.range();
    assert_eq!(&e.text()[range], "Merged 👩‍👩‍👧‍👦\nsecond paragraph");
}
#[test]
fn inserting_through_a_merged_cell_expands_it_and_preserves_ids_and_metadata() {
    for (row, col, action, rs, cs) in [
        (0, 2, TableAction::RowAfter, 3, 2),
        (2, 0, TableAction::ColumnAfter, 2, 3),
    ] {
        let mut e = merged();
        select(&mut e, row, col);
        let before = e.snapshot();
        let old = table(&e).cells[0].clone();
        e.table_action(action).unwrap();
        valid(&e);
        let new = &table(&e).cells[0];
        assert_eq!(span(new, true), rs);
        assert_eq!(span(new, false), cs);
        assert_eq!(new.blocks, old.blocks);
        assert_eq!(new.attrs["future-cell-metadata"], json!("keep"));
        assert_eq!(
            table(&e).attrs["future-table-metadata"],
            json!({"keep":true})
        );
        assert_eq!(e.selection, before.selection);
        e.undo();
        assert_eq!(e.document, before.document);
    }
}
#[test]
fn deleting_through_merged_coverage_retains_contents_until_the_cell_disappears() {
    for action in [TableAction::DeleteRow, TableAction::DeleteColumn] {
        let mut e = merged();
        let before = e.snapshot();
        let contents = table(&e).cells[0].blocks.clone();
        e.table_action(action).unwrap();
        valid(&e);
        assert_eq!(table(&e).cells[0].blocks, contents);
        assert_eq!(e.selection, before.selection);
        e.table_action(action).unwrap();
        valid(&e);
        assert!(!e.text().contains("Merged"));
        e.undo();
        e.undo();
        assert_eq!(e.document, before.document);
        assert_eq!(e.selection, before.selection);
    }
}
#[test]
fn split_keeps_rich_content_and_materializes_empty_cells() {
    let mut e = merged();
    let before = e.snapshot();
    let contents = table(&e).cells[0].blocks.clone();
    e.table_action(TableAction::SplitCell).unwrap();
    valid(&e);
    assert_eq!(table(&e).cells.len(), 9);
    assert_eq!(table(&e).cells[0].blocks, contents);
    assert!(!e.table_context().unwrap().allows(TableAction::SplitCell));
    e.undo();
    assert_eq!(e.document, before.document);
}
#[test]
fn structural_sequences_preserve_a_valid_grid_and_are_individually_undoable() {
    // Each axis, edge and merged-cell position; validate the complete resulting grid.
    for row in 0..3 {
        for col in 0..3 {
            for action in [
                TableAction::RowBefore,
                TableAction::RowAfter,
                TableAction::ColumnBefore,
                TableAction::ColumnAfter,
                TableAction::DeleteRow,
                TableAction::DeleteColumn,
            ] {
                let mut e = merged();
                select(&mut e, row, col);
                let before = e.snapshot();
                e.table_action(action).unwrap();
                valid(&e);
                let after = e.document.clone();
                e.undo();
                assert_eq!(e.document, before.document);
                assert_eq!(e.selection, before.selection);
                e.redo();
                assert_eq!(e.document, after);
                valid(&e);
            }
        }
    }
}
#[test]
fn invalid_overlap_cross_cell_selection_limits_and_preedit_never_mutate() {
    let mut e = editor(1, 1);
    let before = e.snapshot();
    let revision = e.revision;
    assert!(e.table_action(TableAction::DeleteRow).is_err());
    assert!(e.table_action(TableAction::DeleteColumn).is_err());
    assert!(e.insert_table(0, 3).is_err());
    assert!(e.insert_table(1001, 1).is_err());
    assert_eq!(e.document, before.document);
    assert_eq!(e.revision, revision);
    let mut t = table(&e).clone();
    t.cells.push(empty_cell(0, 0));
    e.install_table(t);
    let before = e.snapshot();
    assert!(e.table_action(TableAction::RowAfter).is_err());
    assert_eq!(e.document, before.document);
    assert_eq!(e.selection, before.selection);
    let mut e = editor(1, 2);
    let other = table(&e).cells[1].blocks[0].id.clone();
    e.select(
        e.global(&Position {
            block: other,
            offset: 0,
        }),
        true,
    );
    let before = e.snapshot();
    assert!(e.table_context().is_none());
    assert!(e.table_action(TableAction::RowAfter).is_err());
    assert!(e.tab(false).is_err());
    assert_eq!(e.document, before.document);
    e.select(0, false);
    e.input(None, "候補", Some(None)).unwrap();
    let before = e.document.clone();
    let mark = e.marked.clone();
    assert!(e.insert_table(2, 2).is_err());
    assert_eq!(e.document, before);
    assert_eq!(e.marked, mark);
}
#[test]
fn missing_cells_are_materialized_and_nested_actions_only_change_the_inner_table() {
    let mut e = editor(2, 2);
    let mut t = table(&e).clone();
    t.cells.pop();
    e.install_table(t);
    let before = e.document.clone();
    e.table_action(TableAction::RowAfter).unwrap();
    valid(&e);
    e.undo();
    assert_eq!(e.document, before);
    e.insert_table(2, 2).unwrap();
    let inner = e.table_context().unwrap().id;
    let outer = table(&e).clone();
    e.table_action(TableAction::ColumnAfter).unwrap();
    assert_eq!(table(&e).number("columns", 0), 2);
    assert_eq!(e.table_context().unwrap().columns, 3);
    assert_ne!(inner, outer.id);
    e.undo();
    assert_eq!(table(&e), &outer);
    e.leave_table().unwrap();
    assert_eq!(e.table_context().unwrap().id, outer.id);
}
#[test]
fn removing_and_leaving_tables_keep_a_writable_document_and_body_tab_moves_focus() {
    let mut e = editor(1, 1);
    let before = e.document.clone();
    e.leave_table().unwrap();
    assert!(e.table_context().is_none());
    assert!(!e.tab(false).unwrap());
    select(&mut e, 0, 0);
    e.table_action(TableAction::Remove).unwrap();
    assert!(e.table_context().is_none());
    e.insert("After", 0).unwrap();
    assert!(e.text().ends_with("After"));
    e.undo();
    e.undo();
    assert_eq!(e.document, before);
    // A legacy document ending in a table still gets a real trailing paragraph.
    e.document.blocks.pop();
    select(&mut e, 0, 0);
    let before = e.document.clone();
    e.leave_table().unwrap();
    e.insert("End", 0).unwrap();
    e.undo();
    e.undo();
    assert_eq!(e.document, before);
}
