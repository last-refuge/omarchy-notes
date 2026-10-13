//! Real storage, fonts and GPU organization workflows with routed text input.
use super::*;
use notes_core::document::{Block, Document};
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
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let busy = h.update(cx, |s, w, cx| {
            s.poll_storage(w, cx);
            s.organization.busy()
        })?;
        draw(cx, h)?;
        if !busy {
            return Ok(());
        }
        anyhow::ensure!(Instant::now() < deadline, "Organization save timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn input(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>, text: &str) -> anyhow::Result<()> {
    h.update(cx, |s, w, cx| {
        s.organization.field.update(cx, |field, cx| {
            field.select_all(w, cx);
            field.replace_text_in_range(None, text, w, cx);
        })
    })?;
    draw(cx, h)
}
fn store(root: &Path) -> anyhow::Result<notes_storage::Store> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match notes_storage::Store::open(root) {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}
fn create(
    cx: &mut HeadlessAppContext,
    h: WindowHandle<Shell>,
    name: &str,
) -> anyhow::Result<String> {
    super::super::visual_keys(cx, h.into(), "ctrl-alt-n")?;
    input(cx, h, name)?;
    super::super::visual_keys(cx, h.into(), "enter")?;
    settle(cx, h)?;
    h.update(cx, |s, _, _| {
        assert!(!s.organization.open(), "{:?}", s.organization.error);
        let Scope::FolderId(Some(id)) = &s.library.scope else {
            panic!("New folder must be selected")
        };
        id.clone()
    })
}
pub fn organization_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let tmp = tempfile::tempdir()?;
    let root = tmp.path().join("library");
    let mut doc = Document::default();
    doc.blocks[0] = Block::paragraph("A note to keep nearby");
    doc.blocks.push(Block::paragraph(
        "The best place for an idea is somewhere you can find it again.",
    ));
    let note = store(&root)?.create(doc, None)?;
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
    let (worker, library) = notes_storage::Worker::from_store(store(&root)?)?;
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    draw(&mut cx, h)?;
    let work = create(&mut cx, h, "Work")?;
    let clients = create(&mut cx, h, "Clients")?;
    h.update(&mut cx, |s, w, cx| {
        s.command(Command::Scope(Scope::All), w, cx)
    })?;
    let personal = create(&mut cx, h, "Personal")?;
    let other_clients = create(&mut cx, h, "Clients")?;
    assert_ne!(clients, other_clients);
    super::super::visual_keys(&mut cx, h.into(), "ctrl-n")?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while h.update(&mut cx, |s, w, cx| {
        s.poll_storage(w, cx);
        s.live.as_ref().unwrap().creating.is_some()
    })? {
        anyhow::ensure!(Instant::now() < deadline, "New note timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(
            s.library.scope,
            Scope::FolderId(Some(other_clients.clone()))
        );
        assert_eq!(
            s.library.selected_note().unwrap().folder_id.as_ref(),
            Some(&other_clients)
        );
        assert_eq!(s.library.visible.len(), 1);
        s.command(
            Command::Scope(Scope::FolderId(Some(clients.clone()))),
            w,
            cx,
        );
        assert!(
            s.library.visible.is_empty(),
            "Same-named folders must not share notes"
        );
        assert_eq!(s.organization.paths[&clients], "Work / Clients");
        assert_eq!(s.organization.paths[&other_clients], "Personal / Clients");
        s.command(Command::Scope(Scope::FolderId(Some(work.clone()))), w, cx);
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-alt-n")?;
    h.update(&mut cx, |s, w, cx| {
        s.organization.field.update(cx, |f, cx| {
            f.replace_and_mark_text_in_range(None, "日本", Some(2..2), w, cx)
        });
    })?;
    draw(&mut cx, h)?;
    super::super::visual_keys(&mut cx, h.into(), "enter escape")?;
    h.update(&mut cx, |s, w, cx| {
        assert!(s.organization.open() && !s.organization.busy());
        assert!(s.organization.field.read(cx).composing());
        s.organization
            .field
            .update(cx, |f, cx| f.unmark_text(w, cx));
    })?;
    input(&mut cx, h, "clients")?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert!(s.organization.error.as_ref().unwrap().contains("already"))
    })?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("folders-duplicate.png"))?;
    input(&mut cx, h, "Ideas")?;
    h.update(&mut cx, |s, _, _| {
        s.organization.collapsed.insert(work.clone());
        s.refresh_folder_rows();
    })?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        assert!(!s.organization.collapsed.contains(&work));
        assert!(s.organization.rows.iter().any(|r| r.folder.name == "Ideas"));
        s.command(
            Command::Scope(Scope::FolderId(Some(clients.clone()))),
            w,
            cx,
        )
    })?;
    super::super::visual_keys(&mut cx, h.into(), "f2")?;
    input(&mut cx, h, "Projects")?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.organization.paths[&clients], "Work / Projects");
        s.organize(Action::MoveFolder, w, cx);
    })?;
    input(&mut cx, h, "Personal")?;
    h.update(&mut cx, |s, _, _| {
        assert!(
            !s.organization
                .destinations
                .iter()
                .any(|d| d.id.as_ref() == Some(&clients))
        );
        assert_eq!(s.organization.destinations[0].id.as_ref(), Some(&personal));
    })?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.organization.paths[&clients], "Personal / Projects");
        s.command(Command::Scope(Scope::All), w, cx);
        s.command(Command::Select(note.id.clone()), w, cx);
    })?;
    let editor = h.update(&mut cx, |s, _, _| {
        s.live.as_ref().unwrap().notes[&note.id].editor.clone()
    })?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, "Keep this draft. ", w, cx)
        })
    })?;
    draw(&mut cx, h)?;
    let (expected, selection, revision) = h.update(&mut cx, |_, _, cx| {
        let e = editor.read(cx);
        (
            e.model.document.clone(),
            e.model.selection.clone(),
            e.model.revision,
        )
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-alt-m")?;
    input(&mut cx, h, "Projects")?;
    draw(&mut cx, h)?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("folders-move-dark.png"))?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        let e = editor.read(cx);
        assert_eq!(e.model.document, expected);
        assert_eq!(e.model.selection, selection);
        assert_eq!(e.model.revision, revision);
        assert!(e.focus.is_focused(w));
        assert_eq!(
            s.live.as_ref().unwrap().notes[&note.id]
                .note
                .folder
                .as_ref(),
            Some(&clients)
        );
        assert_eq!(s.library.visible.len(), 1);
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-z")?;
    h.update(&mut cx, |_, _, cx| {
        assert_eq!(editor.read(cx).model.document, note.body)
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-z ctrl-s")?;
    // A delayed unrelated result cannot rename the active folder or close a form.
    super::super::visual_keys(&mut cx, h.into(), "f2")?;
    h.update(&mut cx, |s, w, cx| {
        s.organized(
            u64::MAX,
            Ok(Change::NoteMoved {
                id: note.id.clone(),
                folder: None,
            }),
            w,
            cx,
        )
    })?;
    h.update(&mut cx, |s, _, _| assert!(s.organization.open()))?;
    super::super::visual_keys(&mut cx, h.into(), "escape")?;
    h.update(&mut cx, |s, w, cx| {
        s.request_close(w, cx);
    })?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while cx.update(|cx| !cx.windows().is_empty()) {
        if h.update(&mut cx, |s, w, cx| s.poll_storage(w, cx)).is_err() {
            break;
        }
        cx.run_until_parked();
        anyhow::ensure!(Instant::now() < deadline, "Close timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(editor);
    cx.update(|cx| {
        cx.new(|_| ()).update(cx, |_, cx| cx.notify());
    });
    let loaded = store(&root)?.load()?;
    let reopened = loaded.notes.iter().find(|n| n.id == note.id).unwrap();
    assert_eq!(reopened.body, expected);
    assert_eq!(reopened.folder.as_ref(), Some(&clients));
    assert_eq!(
        loaded
            .folders
            .iter()
            .find(|f| f.id == clients)
            .unwrap()
            .parent
            .as_ref(),
        Some(&personal)
    );
    for (name, width, height, appearance) in [
        ("dark", 1440., 960., Appearance::Reference),
        ("light", 1440., 960., Appearance::Light),
        ("narrow", 640., 820., Appearance::Reference),
        ("small", 420., 420., Appearance::Reference),
    ] {
        let (worker, library) = notes_storage::Worker::from_store(store(&root)?)?;
        let h = cx.open_window(size(px(width), px(height)), |w, cx| {
            cx.new(|cx| Shell::new_live(appearance, root.clone(), worker, library, w, cx))
        })?;
        h.update(&mut cx, |s, w, cx| {
            s.command(Command::Select(note.id.clone()), w, cx)
        })?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("folders-library-{name}.png")))?;
        super::super::visual_keys(&mut cx, h.into(), "ctrl-alt-m")?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("folders-move-{name}.png")))?;
        super::super::visual_keys(&mut cx, h.into(), "tab down")?;
        h.update(&mut cx, |s, w, _| {
            assert!(s.organization.focus[1].is_focused(w));
            assert_eq!(s.organization.selected, 1);
        })?;
        super::super::visual_keys(&mut cx, h.into(), "escape ctrl-alt-n")?;
        input(&mut cx, h, "A home for the next idea")?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("folders-create-{name}.png")))?;
        super::super::visual_keys(&mut cx, h.into(), "escape")?;
        h.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
    }
    // A large in-memory hierarchy checks real list layout/search, independently
    // of SQLite throughput. This is not a whole-library performance benchmark.
    let (worker, mut library) = notes_storage::Worker::from_store(store(&root)?)?;
    library
        .folders
        .extend((0..10_000).map(|i| notes_storage::Folder {
            id: format!("large-{i}"),
            parent: None,
            name: format!("Collection {i:05}"),
            sort_key: 0,
        }));
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |s, _, cx| {
        let measured = s.organization.rendered_rows.borrow().len();
        assert!(
            measured > 0 && measured < 64,
            "Measured {measured} of 10,005 folder rows"
        );
        let last = s.organization.rows.len() - 1;
        s.organization.rendered_rows.borrow_mut().clear();
        s.organization
            .nav
            .scroll_to_item(last, ScrollStrategy::Nearest);
        cx.notify();
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        let last = s.organization.rows.len() - 1;
        assert!(s.organization.rendered_rows.borrow().contains(&last));
        assert!(s.organization.rendered_rows.borrow().len() < 64);
        assert_eq!(s.organization.nav.is_scrolled_to_end(), Some(true));
        s.command(Command::Select(note.id.clone()), w, cx);
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-alt-m")?;
    input(&mut cx, h, "Collection 09999")?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(s.organization.destinations.len(), 1);
        assert_eq!(
            s.organization.destinations[0].id.as_deref(),
            Some("large-9999")
        );
    })?;
    super::super::visual_keys(&mut cx, h.into(), "escape")?;
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    println!(
        "PASS: nested create, duplicate-name scope isolation, new note in selected folder, sibling-name rejection/retry, rename identity, folder move, dirty-note move, retained selection/undo, stable result routing, durable reopen, bounded 10,005-folder layout/search, and dark/light/compact organization layouts."
    );
    Ok(())
}
