use super::*;
use notes_core::document::Block;

fn note(store: &mut Store, title: &str, body: &str) -> Note {
    store
        .create(
            Document::from_blocks(vec![Block::paragraph(title), Block::paragraph(body)]),
            None,
        )
        .unwrap()
}

#[test]
fn prefix_search_is_literal_scoped_live_only_and_updates_after_save() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let mut first = note(
        &mut store,
        "Research",
        &format!("{} benchmark café 日本語 findings", "context ".repeat(40)),
    );
    let suffix = note(&mut store, "Other", "workbench only");
    let q = Query {
        text: "bench cafe 日本".into(),
        ..Default::default()
    };
    let page = store.query(&q).unwrap();
    assert_eq!(
        page.rows.iter().map(|r| &r.id).collect::<Vec<_>>(),
        [&first.id]
    );
    assert!(
        page.rows[0].snippet.contains("benchmark"),
        "FTS snippet should show the match beyond the stored preview"
    );
    assert!(page.rows[0].snippet.encode_utf16().count() <= 160);
    assert_eq!(
        store
            .query(&Query {
                text: "bench".into(),
                ..Default::default()
            })
            .unwrap()
            .rows
            .len(),
        1
    );
    assert!(
        store
            .query(&Query {
                text: "Research OR Other".into(),
                ..Default::default()
            })
            .unwrap()
            .rows
            .is_empty(),
        "OR is a literal required word"
    );
    let folder = match store
        .organize(notes_core::organization::Mutation::CreateFolder {
            name: "Work".into(),
            parent: None,
        })
        .unwrap()
    {
        notes_core::organization::Change::Folder(folder) => folder.id,
        _ => unreachable!(),
    };
    store
        .organize(notes_core::organization::Mutation::MoveNote {
            id: first.id.clone(),
            folder: Some(folder.clone()),
        })
        .unwrap();
    assert!(
        store
            .query(&Query {
                scope: Scope::Folder(None),
                ..q.clone()
            })
            .unwrap()
            .rows
            .is_empty()
    );
    assert_eq!(
        store
            .query(&Query {
                scope: Scope::Folder(Some(folder)),
                ..q.clone()
            })
            .unwrap()
            .rows
            .len(),
        1
    );
    first.body = Document::from_blocks(vec![
        Block::paragraph("Renamed"),
        Block::paragraph("newneedle"),
    ]);
    let saved = store.save(&first).unwrap();
    first.revision = saved.revision;
    assert!(store.query(&q).unwrap().rows.is_empty());
    assert_eq!(
        store
            .query(&Query {
                text: "newnee".into(),
                ..Default::default()
            })
            .unwrap()
            .rows[0]
            .id,
        first.id
    );
    store
        .db
        .execute("UPDATE notes SET deleted_at=10 WHERE id=?1", [&first.id])
        .unwrap();
    assert!(
        store
            .query(&Query {
                text: "newnee".into(),
                ..Default::default()
            })
            .unwrap()
            .rows
            .is_empty()
    );
    assert_eq!(
        store
            .query(&Query {
                scope: Scope::Trash,
                ..Default::default()
            })
            .unwrap()
            .rows[0]
            .id,
        first.id
    );
    assert_eq!(
        store.query(&Query::default()).unwrap().rows[0].id,
        suffix.id
    );
    assert!(
        store
            .query(&Query {
                scope: Scope::Trash,
                text: "newnee".into(),
                ..Default::default()
            })
            .is_err()
    );
}

#[test]
fn browse_sorts_are_deterministic_and_pages_are_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    let alpha = note(&mut store, "Alpha", "a");
    let lower = note(&mut store, "alpha", "b");
    let pinned = note(&mut store, "Zebra", "c");
    let untitled = note(&mut store, "", "");
    for (n, created, edited, pin) in [
        (&alpha, 1, 4, false),
        (&lower, 2, 3, false),
        (&pinned, 0, 0, true),
        (&untitled, 3, 2, false),
    ] {
        store
            .db
            .execute(
                "UPDATE notes SET created_at=?2,updated_at=?3,pinned=?4 WHERE id=?1",
                params![n.id, created, edited, pin],
            )
            .unwrap();
    }
    for (sort, order) in [
        (
            Sort::Edited,
            [&pinned.id, &alpha.id, &lower.id, &untitled.id],
        ),
        (
            Sort::Created,
            [&pinned.id, &untitled.id, &lower.id, &alpha.id],
        ),
        (
            Sort::Title,
            [&pinned.id, &alpha.id, &lower.id, &untitled.id],
        ),
    ] {
        let page = store
            .query(&Query {
                sort,
                limit: 2,
                ..Default::default()
            })
            .unwrap();
        assert!(page.has_more);
        assert_eq!(
            page.rows.iter().map(|r| &r.id).collect::<Vec<_>>(),
            &order[..2]
        );
        let rest = store
            .query(&Query {
                sort,
                limit: 2,
                offset: 2,
                ..Default::default()
            })
            .unwrap();
        assert!(!rest.has_more);
        assert_eq!(
            rest.rows.iter().map(|r| &r.id).collect::<Vec<_>>(),
            &order[2..]
        );
    }
    assert!(
        store
            .query(&Query {
                limit: 201,
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        store
            .query(&Query {
                offset: 100_001,
                ..Default::default()
            })
            .is_err()
    );
    assert!(
        store
            .query(&Query {
                text: "x".repeat(4097),
                ..Default::default()
            })
            .is_err()
    );
    // Validation errors must not leave a connection hook that affects writes.
    note(&mut store, "Still writable", "after query errors");
}

#[test]
fn interrupted_query_clears_its_progress_hook_before_subsequent_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    for index in 0..200 {
        note(&mut store, &format!("Title {index}"), "prefix searchable");
    }
    let query = Query {
        text: "pre".into(),
        sort: Sort::Title,
        ..Default::default()
    };
    assert!(store.query_with_budget(&query, Duration::ZERO).is_err());
    note(&mut store, "Writable after interruption", "prefix newest");
    assert_eq!(store.query(&query).unwrap().rows.len(), 200);
}

#[test]
fn matched_long_tokens_have_bounded_unicode_snippets() {
    let tmp = tempfile::tempdir().unwrap();
    let mut store = Store::open(tmp.path()).unwrap();
    note(
        &mut store,
        "Long token",
        &format!("prefix{}", "𐐀".repeat(300)),
    );
    let page = store
        .query(&Query {
            text: "prefix".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].snippet.contains("prefix"));
    assert!(page.rows[0].snippet.ends_with('…'));
    assert!(page.rows[0].snippet.encode_utf16().count() <= 160);
}
