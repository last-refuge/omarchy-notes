//! Focused real-worker library search, sorting, and paging workflow.
use super::*;
use notes_core::{
    document::{Block, Document},
    organization::{Change, Mutation},
};
use std::{path::Path, sync::Arc, time::Instant};

fn draw(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>) -> anyhow::Result<()> {
    cx.run_until_parked();
    let cleanup = cx.update_window(h.into(), |_, w, cx| {
        w.refresh();
        w.draw(cx)
    })?;
    cx.update(|cx| cleanup.clear(cx));
    cx.run_until_parked();
    Ok(())
}

fn settle(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        draw(cx, h)?;
        let busy = h.update(cx, |s, w, cx| {
            s.poll_storage(w, cx);
            assert!(s.query.error.is_none(), "{:?}", s.query.error);
            if let Some((_, query)) = &s.query.in_flight {
                assert_eq!(query.limit, 100, "Every UI request is bounded to 100 rows");
            }
            if let Some(query) = &s.query.displayed {
                assert_eq!(query.limit, 100);
            }
            s.query.loading
                || s.query.in_flight.is_some()
                || s.query.wanted.is_some()
                || s.folder_view_busy()
        })?;
        if !busy {
            draw(cx, h)?;
            return Ok(());
        }
        if Instant::now() >= deadline {
            let state = h.update(cx, |s, _, _| {
                format!(
                    "generation={}, loading={}, in_flight={:?}, wanted={:?}, ready={:?}, rows={}, pending_rows={}, indices={}, folder_view_busy={}",
                    s.query.generation,
                    s.query.loading,
                    s.query.in_flight,
                    s.query.wanted,
                    s.query.ready,
                    s.query.ids.len(),
                    s.query.pending_ids.len(),
                    s.query.indices.len(),
                    s.folder_view_busy(),
                )
            })?;
            anyhow::bail!("Library query timed out: {state}");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn search(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>, text: &str) -> anyhow::Result<()> {
    h.update(cx, |s, w, cx| {
        s.search.update(cx, |f, cx| f.set_text(text, w, cx));
    })?;
    draw(cx, h)?;
    // GPUI headless uses a deterministic clock: wall-clock sleeps do not wake
    // the actual 45 ms search debounce. Advance that clock after registration.
    cx.advance_clock(Duration::from_millis(45));
    settle(cx, h)
}

fn visible(s: &Shell) -> Vec<String> {
    s.library
        .visible
        .iter()
        .map(|&i| s.library.notes[i].id.clone())
        .collect()
}

fn anchor(s: &Shell) -> String {
    s.library.notes[s.library.visible[s.list.logical_scroll_top().item_ix]]
        .id
        .clone()
}

fn ordered(notes: &[notes_storage::Note], sort: Sort) -> Vec<String> {
    let mut notes: Vec<_> = notes.iter().collect();
    notes.sort_by(|a, b| {
        b.pinned.cmp(&a.pinned).then_with(|| match sort {
            Sort::Edited => b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)),
            Sort::Created => b.created_at.cmp(&a.created_at).then(a.id.cmp(&b.id)),
            Sort::Title => a
                .body
                .title()
                .to_lowercase()
                .cmp(&b.body.title().to_lowercase())
                .then(b.updated_at.cmp(&a.updated_at))
                .then(a.id.cmp(&b.id)),
        })
    });
    notes.into_iter().map(|n| n.id.clone()).collect()
}

fn store(root: &Path) -> anyhow::Result<notes_storage::Store> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match notes_storage::Store::open(root) {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
}

pub fn library_query_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let tmp = tempfile::tempdir()?;
    let root = tmp.path().join("library");
    let mut storage = store(&root)?;
    let Change::Folder(folder) = storage.organize(Mutation::CreateFolder {
        name: "Evidence".into(),
        parent: None,
    })?
    else {
        anyhow::bail!("Expected a created folder")
    };
    let mut notes = Vec::with_capacity(225);
    for index in 0..225 {
        let title = match index {
            0 => "Zulu evidence".to_owned(),
            1 => "Alpha note".to_owned(),
            224 => "ZZZ writing draft".to_owned(),
            _ => format!("Entry {index:03}"),
        };
        let mut doc = Document::default();
        doc.blocks[0] = Block::paragraph(&title);
        let body = if index == 0 {
            format!(
                "{} Near the end, benchmarks reveal a useful result beside the quiet river.",
                "Background material before the useful passage. ".repeat(12)
            )
        } else {
            format!("Private fixture body for note {index:03}.")
        };
        doc.blocks.push(Block::paragraph(&body));
        notes.push(storage.create(doc, (index % 2 == 0).then(|| folder.id.clone()))?);
    }
    // Give the old prefix fixture a definitively newer edit timestamp while its
    // creation date stays old. This makes Edited vs Created observably distinct.
    std::thread::sleep(Duration::from_millis(2));
    let saved = storage
        .save(&notes[0])
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    notes[0].revision = saved.revision;
    notes[0].updated_at = saved.updated_at;
    let prefix = storage.query(&Query {
        text: "bench".into(),
        ..Default::default()
    })?;
    assert_eq!(prefix.rows.len(), 1);
    assert_eq!(prefix.rows[0].id, notes[0].id);
    assert!(prefix.rows[0].snippet.contains("benchmarks"));
    assert!(!notes[0].body.snippet().contains("benchmarks"));
    assert!(
        storage
            .query(&Query {
                text: "ench".into(),
                ..Default::default()
            })?
            .rows
            .is_empty(),
        "Interior substrings must not replace the FTS prefix route"
    );
    let scope_page = storage.query(&Query {
        text: "entry".into(),
        scope: QueryScope::Folder(Some(folder.id.clone())),
        limit: 100,
        ..Default::default()
    })?;
    let expected_edited = ordered(&notes, Sort::Edited);
    let expected_created = ordered(&notes, Sort::Created);
    let expected_title = ordered(&notes, Sort::Title);
    assert_eq!(expected_edited[0], notes[0].id);
    assert_ne!(expected_created[0], expected_edited[0]);
    assert_eq!(expected_title[0], notes[1].id);
    let prefix_id = notes[0].id.clone();
    let draft_id = notes[224].id.clone();
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::assets::load_fonts(cx).unwrap();
        super::super::bind_keys(cx);
        crate::search::bind_keys(cx);
    });
    let (worker, library) = notes_storage::Worker::from_store(storage)?;
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| {
            let mut s =
                Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx);
            s.configure_folder_view_in(root.clone(), Some(tmp.path().join("config")), cx);
            s
        })
    })?;
    println!("Query workflow stage: initial bounded page/private view load");
    settle(&mut cx, h)?;
    let view_key = notes_platform::library_view::LibraryKey::discover(&root)?;
    assert!(view_key.path_in(&tmp.path().join("config")).is_file());
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(visible(s), expected_edited[..100]);
        assert!(s.query.has_more);
        assert_eq!(s.query.next_offset, 100);
        s.command(Command::Select(draft_id.clone()), w, cx);
    })?;
    let editor = h.update(&mut cx, |s, _, _| {
        s.live.as_ref().unwrap().notes[&draft_id]
            .active_editor()
            .unwrap()
            .clone()
    })?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, "Draft retained. ", w, cx)
        })
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        // Control this workflow's explicit Save boundary rather than letting a
        // slower GPU autosave the title while sorting is being checked.
        let session = s.live.as_mut().unwrap().notes.get_mut(&draft_id).unwrap();
        session.last_edit = Instant::now() + Duration::from_secs(60);
        session.dirty_since = session.last_edit;
    })?;
    let (draft, selection, revision) = h.update(&mut cx, |_, _, cx| {
        let e = editor.read(cx);
        (
            e.model.document.clone(),
            e.model.selection.clone(),
            e.model.revision,
        )
    })?;
    println!("Query workflow stage: prefix and substring search");
    search(&mut cx, h, "bench")?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(visible(s).as_slice(), std::slice::from_ref(&prefix_id));
        assert_eq!(s.library.selected.as_ref(), Some(&draft_id));
        let snippet = &s.query.snippets[&prefix_id];
        assert!(snippet.contains("benchmarks"));
        assert!(snippet.encode_utf16().count() <= 161);
    })?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("library-prefix-snippet.png"))?;
    search(&mut cx, h, "ench")?;
    h.update(&mut cx, |s, _, _| {
        assert!(s.library.visible.is_empty());
        assert_eq!(s.library.selected.as_ref(), Some(&draft_id));
    })?;
    // Deliver a real old query's page after a newer search was requested. The
    // genuine worker reply remains in its channel and is independently ignored.
    println!("Query workflow stage: stale generation and folder replies");
    h.update(&mut cx, |s, _, cx| {
        s.library.query = "bench".into();
        s.queue_library_query(false, cx);
        let (operation, old_query) = s.query.in_flight.clone().unwrap();
        s.library.query = "entry".into();
        s.queue_library_query(false, cx);
        s.library_query_event(
            Event::Queried {
                operation,
                query: old_query,
                result: Ok(prefix.clone()),
            },
            cx,
        );
        assert_eq!(s.library.query, "entry");
        assert!(
            s.query.ids.is_empty(),
            "A stale page must not flash in the list"
        );
        assert_eq!(s.library.selected.as_ref(), Some(&draft_id));
    })?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.library.visible.len(), 100);
        s.command(
            Command::Scope(Scope::FolderId(Some(folder.id.clone()))),
            w,
            cx,
        );
        let (operation, old_scope) = s.query.in_flight.clone().unwrap();
        s.command(Command::Scope(Scope::FolderId(None)), w, cx);
        s.library_query_event(
            Event::Queried {
                operation,
                query: old_scope,
                result: Ok(scope_page.clone()),
            },
            cx,
        );
        assert_eq!(s.library.scope, Scope::FolderId(None));
        assert!(
            s.query.ids.is_empty(),
            "An old folder reply must stay hidden"
        );
        let e = editor.read(cx);
        assert_eq!(e.model.document, draft);
        assert_eq!(e.model.selection, selection);
        assert_eq!(e.model.revision, revision);
    })?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert!(!s.library.visible.is_empty());
        assert!(
            s.library
                .visible
                .iter()
                .all(|&i| s.library.notes[i].folder_id.is_none())
        );
    })?;
    search(&mut cx, h, "")?;
    h.update(&mut cx, |s, w, cx| {
        s.command(Command::Scope(Scope::All), w, cx)
    })?;
    settle(&mut cx, h)?;
    // Drive the production sort command, not a test-only comparator in the UI.
    println!("Query workflow stage: three sorts and bounded paging");
    h.update(&mut cx, |s, w, cx| s.command(Command::Sort, w, cx))?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(s.query.sort, Sort::Created);
        assert_eq!(visible(s), expected_created[..100]);
    })?;
    h.update(&mut cx, |s, w, cx| s.command(Command::Sort, w, cx))?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(s.query.sort, Sort::Title);
        assert_eq!(s.library.visible.len(), 100);
        // The local draft has a newer unsaved title than the SQL record. Query
        // ordering must use committed summaries until its actual Save receipt.
        assert_eq!(visible(s), expected_title[..100]);
    })?;
    h.update(&mut cx, |s, w, cx| s.command(Command::MoreResults, w, cx))?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(visible(s), expected_title[..200]);
        assert_eq!(s.query.next_offset, 200);
        assert!(s.query.has_more);
    })?;
    h.update(&mut cx, |s, w, cx| s.command(Command::MoreResults, w, cx))?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(visible(s), expected_title);
        assert_eq!(s.query.next_offset, 225);
        assert!(!s.query.has_more);
        s.list.scroll_to(ListOffset {
            item_ix: 150,
            offset_in_item: px(17.),
        });
    })?;
    draw(&mut cx, h)?;
    let saved_anchor = h.update(&mut cx, |s, _, _| {
        assert_eq!(s.list.logical_scroll_top().item_ix, 150);
        assert_eq!(s.list.logical_scroll_top().offset_in_item, px(17.));
        anchor(s)
    })?;
    // Refresh after a fresh edit/save changes the first-page title order. All
    // three loaded pages must refresh without reverting the second-page anchor.
    println!("Query workflow stage: acknowledged save and loaded-page anchor");
    h.update(&mut cx, |s, w, cx| {
        editor.update(cx, |e, cx| {
            e.edit(
                |m| {
                    let end = m.document.blocks[0].plain_text().len();
                    m.select(0, false);
                    m.select(end, true);
                    m.insert("A changed draft title", 0)
                },
                w,
                cx,
            )
        });
        s.command(Command::Save, w, cx);
    })?;
    draw(&mut cx, h)?;
    // Changed subscriptions are deferred; request Save again after delivery so
    // this checks an acknowledged revision rather than a same-turn stale body.
    h.update(&mut cx, |s, w, cx| s.command(Command::Save, w, cx))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        settle(&mut cx, h)?;
        let saved = h.update(&mut cx, |s, _, cx| {
            let session = &s.live.as_ref().unwrap().notes[&draft_id];
            assert!(session.error.is_none(), "{:?}", session.error);
            session.saved == session.revision
                && !session.has_unobserved_edit(cx)
                && session.pending.is_none()
        })?;
        if saved {
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "Draft save timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    let expected_saved = h.update(&mut cx, |s, _, cx| {
        assert_eq!(
            s.library.visible.len(),
            225,
            "Save refresh retains loaded pages"
        );
        assert_eq!(
            anchor(s),
            saved_anchor,
            "Save refresh retains the anchor ID"
        );
        assert_eq!(s.list.logical_scroll_top().item_ix, 151);
        assert_eq!(s.list.logical_scroll_top().offset_in_item, px(17.));
        assert_eq!(visible(s)[0], draft_id);
        editor.read(cx).model.document.clone()
    })?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("library-refreshed-pages.png"))?;
    h.update(&mut cx, |s, w, cx| s.command(Command::Sort, w, cx))?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.query.sort, Sort::Edited);
        assert_eq!(s.library.visible.len(), 100);
        assert_eq!(visible(s)[0], draft_id);
        s.request_close(w, cx);
    })?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx.update(|cx| !cx.windows().is_empty()) {
        if h.update(&mut cx, |s, w, cx| s.poll_storage(w, cx)).is_err() {
            break;
        }
        cx.run_until_parked();
        anyhow::ensure!(Instant::now() < deadline, "Close timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
    drop(editor);
    cx.run_until_parked();
    cx.update(|cx| {
        cx.new(|_| ()).update(cx, |_, cx| cx.notify());
    });
    let reopened = store(&root)?.load()?;
    assert_eq!(
        reopened
            .notes
            .iter()
            .find(|n| n.id == draft_id)
            .unwrap()
            .body,
        expected_saved
    );
    println!(
        "PASS: real SQLite prefix search (not interior substring), contextual FTS snippet, stale generation/folder replies with retained draft, Edited/Created/Title sorts, bounded 100→200→225 Load More pages, and durable Save refresh retaining loaded-page ID/pixel anchor."
    );
    Ok(())
}
