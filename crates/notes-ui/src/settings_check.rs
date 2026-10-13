//! Real typography, settings focus, atomic persistence and retained editing state.
use super::*;
use anyhow::Context as _;
use notes_core::document::{Block, Document, Span};
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
            s.preferences.busy() || s.folder_view_busy()
        })?;
        draw(cx, h)?;
        if !busy {
            return Ok(());
        }
        anyhow::ensure!(Instant::now() < deadline, "Preference save timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn state(path: &Path, override_: Option<Appearance>) -> anyhow::Result<State> {
    let (worker, session) = notes_platform::preferences::Worker::start(path.to_owned())?;
    let mut state = State::memory(Appearance::Omarchy);
    state.worker = Some(worker);
    state.session = session;
    state.persistent = true;
    state.appearance_override = override_;
    Ok(state)
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
pub fn settings_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let tmp = tempfile::tempdir()?;
    let root = tmp.path().join("library");
    let path = tmp.path().join("config/preferences.json");
    let mut doc = Document::default();
    doc.blocks[0].set_kind("h1");
    doc.blocks[0].spans = vec![Span::text("A little room for a thought")];
    doc.blocks.push(Block::paragraph("The rhythm of a notebook is personal. Keep a sentence, a question, or a detail worth returning to."));
    for i in 0..30 {
        doc.blocks.push(Block::paragraph(&format!("Thought {i}. Let the next idea take its time. This paragraph wraps when the writing size grows.")));
    }
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
    let (worker, library) = notes_storage::Worker::start(root.clone(), false)
        .context("Start initial settings window")?;
    let preferences = state(&path, Some(Appearance::Reference))?;
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| {
            let mut s =
                Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx);
            s.configure_preferences(preferences, cx);
            s.configure_folder_view_in(root.clone(), Some(tmp.path().join("config")), cx);
            s.command(Command::Select(note.id.clone()), w, cx);
            s
        })
    })?;
    draw(&mut cx, h)?;
    let editor = h.update(&mut cx, |s, _, _| {
        s.live.as_ref().unwrap().notes[&note.id]
            .editor
            .as_ref()
            .unwrap()
            .clone()
    })?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, "One more thought. ", w, cx)
        })
    })?;
    draw(&mut cx, h)?;
    let (expected, selection, revision, caret) = h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            let range = e.selected_text_range(false, w, cx).unwrap().range;
            (
                e.model.document.clone(),
                e.model.selection.clone(),
                e.model.revision,
                e.bounds_for_range(range, Bounds::default(), w, cx).unwrap(),
            )
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-,")?;
    h.update(&mut cx, |s, w, cx| {
        assert!(s.settings_open);
        assert!(s.preferences.focus[0].is_focused(w));
        s.command(
            Command::Preference(Patch {
                note_font: Some(NoteFont::Sans),
                note_size: Some(24),
                ..Default::default()
            }),
            w,
            cx,
        );
        // A second edit while the first write is outstanding must survive its ack.
        s.command(
            Command::Preference(Patch {
                note_size: Some(26),
                ..Default::default()
            }),
            w,
            cx,
        );
    })?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| w.focus(&s.preferences.focus[8], cx))?;
    draw(&mut cx, h)?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(
            s.preferences.session.value.note_size, 27,
            "One Enter activates the stepper once"
        )
    })?;
    super::super::visual_keys(&mut cx, h.into(), "space")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(
            s.preferences.session.value.note_size, 28,
            "One Space activates the stepper once"
        );
        s.command(
            Command::Preference(Patch {
                note_size: Some(26),
                ..Default::default()
            }),
            w,
            cx,
        );
    })?;
    settle(&mut cx, h)?;
    let stored = notes_platform::preferences::load(&path)?;
    assert_eq!(stored.note_size, 26);
    assert_eq!(stored.note_font, NoteFont::Sans);
    assert_eq!(
        stored.appearance,
        Appearance::Omarchy,
        "CLI palette override must not leak into other saves"
    );
    h.update(&mut cx, |s, _, _| {
        assert_eq!(s.appearance, Appearance::Reference);
        assert!(s.preferences.session.error.is_none());
    })?;
    super::super::visual_keys(&mut cx, h.into(), "escape")?;
    draw(&mut cx, h)?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            assert!(e.focus.is_focused(w));
            assert_eq!(e.model.document, expected);
            assert_eq!(e.model.selection, selection);
            assert_eq!(e.model.revision, revision);
            let range = e.selected_text_range(false, w, cx).unwrap().range;
            let after = e.bounds_for_range(range, Bounds::default(), w, cx).unwrap();
            assert!(
                after.size.height > caret.size.height,
                "Real text metrics must change with writing size"
            );
            assert!(after.top() > px(54.) && after.bottom() < w.viewport_size().height);
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-z")?;
    h.update(&mut cx, |_, _, cx| {
        assert_eq!(editor.read(cx).model.document, note.body)
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-z")?;
    h.update(&mut cx, |_, _, cx| {
        assert_eq!(editor.read(cx).model.document, expected)
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-,")?;
    h.update(&mut cx, |s, w, cx| {
        s.command(Command::Appearance(Appearance::Light), w, cx)
    })?;
    settle(&mut cx, h)?;
    assert_eq!(
        notes_platform::preferences::load(&path)?.appearance,
        Appearance::Light
    );
    h.update(&mut cx, |s, _, cx| {
        s.preferences.dragging = true;
        s.sidebar_width = 281.;
        s.list_width = 409.;
        s.poll_preferences(cx);
    })?;
    assert_eq!(
        notes_platform::preferences::load(&path)?.sidebar_width,
        248,
        "Divider moves must not write before release"
    );
    h.update(&mut cx, |s, _, cx| s.finish_pane_resize(cx))?;
    settle(&mut cx, h)?;
    assert_eq!(notes_platform::preferences::load(&path)?.sidebar_width, 281);
    assert_eq!(notes_platform::preferences::load(&path)?.list_width, 409);
    h.update(&mut cx, |s, w, cx| {
        s.command(Command::Focus, w, cx);
        s.command(Command::Sort, w, cx);
        assert_eq!(s.query.sort, notes_core::library::Sort::Created);
        s.command(Command::Sort, w, cx);
        assert_eq!(s.query.sort, notes_core::library::Sort::Title);
        s.command(Command::Quick, w, cx);
    })?;
    cx.run_until_parked();
    let quick = h.update(&mut cx, |s, _, cx| {
        let (quick, id) = s.windows.quick.as_ref().unwrap();
        let e = s.live.as_ref().unwrap().notes[id]
            .editor
            .as_ref()
            .unwrap()
            .read(cx);
        assert_eq!(e.typography.size, 26);
        assert_eq!(e.typography.font, NoteFont::Sans);
        *quick
    })?;
    super::super::visual_keys(&mut cx, quick, "ctrl-w")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| assert!(s.windows.quick.is_none()))?;
    // Close immediately after another preference edit. The graceful close waits
    // for the latest preferences as well as dirty note revisions.
    h.update(&mut cx, |s, w, cx| {
        s.command(
            Command::Preference(Patch {
                note_size: Some(23),
                ..Default::default()
            }),
            w,
            cx,
        );
        s.request_close(w, cx);
    })?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while cx.update(|cx| !cx.windows().is_empty()) {
        if h.update(&mut cx, |s, w, cx| s.poll_storage(w, cx)).is_err() {
            break;
        }
        cx.run_until_parked();
        anyhow::ensure!(Instant::now() < deadline, "Close did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(editor);
    cx.update(|cx| {
        cx.new(|_| ()).update(cx, |_, cx| cx.notify());
    });
    assert_eq!(notes_platform::preferences::load(&path)?.note_size, 23);
    assert_eq!(
        store(&root)
            .context("Reopen after graceful settings close")?
            .load()?
            .notes[0]
            .body,
        expected
    );

    for (name, width, height, appearance) in [
        ("dark", 1440., 960., Appearance::Reference),
        ("light", 1440., 960., Appearance::Light),
        ("narrow", 640., 820., Appearance::Reference),
        ("small", 420., 420., Appearance::Reference),
    ] {
        // Window disposal drops the sender before the writer finishes closing
        // SQLite. Wait for that exact library lock to be released before reopen.
        let (worker, library) = notes_storage::Worker::from_store(store(&root)?)
            .with_context(|| format!("Start {name} settings window"))?;
        let preferences = state(&path, None)?;
        let h = cx.open_window(size(px(width), px(height)), |w, cx| {
            cx.new(|cx| {
                let mut s = Shell::new_live(appearance, root.clone(), worker, library, w, cx);
                s.configure_preferences(preferences, cx);
                s.configure_folder_view_in(root.clone(), Some(tmp.path().join("config")), cx);
                s
            })
        })?;
        settle(&mut cx, h)?;
        h.update(&mut cx, |s, w, cx| {
            assert_eq!(s.preferences.session.value.note_size, 23);
            assert_eq!(s.sidebar_width, 281.);
            assert_eq!(s.list_width, 409.);
            assert!(s.focused && s.library.sort_by_title);
            s.command(Command::Appearance(appearance), w, cx);
            s.toggle_settings(w, cx);
        })?;
        settle(&mut cx, h)?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("settings-{name}.png")))?;
        for index in (1..14).chain(std::iter::once(15)) {
            super::super::visual_keys(&mut cx, h.into(), "tab")?;
            draw(&mut cx, h)?;
            draw(&mut cx, h)?;
            h.update(&mut cx, |s,w,_| {
                assert!(s.preferences.focus[index].is_focused(w), "Settings Tab order at {index}");
                let extra=usize::from(s.preferences.appearance_override.is_some());
                let item=match index {1..=3=>4,4..=6=>6+extra,7..=8=>7+extra,9..=10=>10+extra,11..=12=>11+extra,15=>15+extra,_=>12+extra};
                let bounds=s.preferences.scroll.bounds_for_item(item).unwrap();
                let painted=bounds+s.preferences.scroll.offset();
                let viewport=s.preferences.scroll.bounds();
                assert!(painted.top()>=viewport.top()-px(1.) && painted.bottom()<=viewport.bottom()+px(1.), "Focused control group is clipped at {name}/{index}: {painted:?} vs {viewport:?}");
            })?;
        }
        if name == "small" {
            cx.capture_screenshot(h.into())?
                .save(directory.join("settings-small-keyboard.png"))?;
        }
        h.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
    }

    let repaired = std::fs::read(&path)?;
    let future = b"{\"version\":999,\"preferences\":{}}\n";
    std::fs::write(&path, future)?;
    let (worker, library) = notes_storage::Worker::from_store(store(&root)?)?;
    let preferences = state(&path, None)?;
    let h = cx.open_window(size(px(420.), px(420.)), |w, cx| {
        cx.new(|cx| {
            let mut s = Shell::new_live(Appearance::Omarchy, root.clone(), worker, library, w, cx);
            s.configure_preferences(preferences, cx);
            s.configure_folder_view_in(root.clone(), Some(tmp.path().join("config")), cx);
            s.toggle_settings(w, cx);
            s
        })
    })?;
    h.update(&mut cx, |s, w, cx| {
        assert!(s.preferences.session.error.is_some());
        s.command(Command::Appearance(Appearance::Light), w, cx);
    })?;
    settle(&mut cx, h)?;
    assert_eq!(std::fs::read(&path)?, future);
    super::super::visual_keys(&mut cx, h.into(), "shift-tab")?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |s, w, _| {
        assert!(s.preferences.focus[14].is_focused(w))
    })?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("settings-error-small.png"))?;
    std::fs::write(&path, repaired)?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    settle(&mut cx, h)?;
    h.update(&mut cx, |s, w, _| {
        assert!(s.preferences.session.error.is_none());
        assert_eq!(s.appearance, Appearance::Light);
        assert!(
            s.preferences.focus[0].is_focused(w),
            "A removed Retry button returns focus to Settings"
        );
        assert_eq!(s.preferences.session.value.note_size, 23);
        assert_eq!(s.preferences.session.value.note_font, NoteFont::Sans);
        assert!(s.focused && s.library.sort_by_title);
    })?;
    let p = notes_platform::preferences::load(&path)?;
    assert_eq!(p.appearance, Appearance::Light);
    assert_eq!(p.sidebar_width, 281);
    assert_eq!(p.list_width, 409);
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    println!(
        "PASS: preference save/reopen, CLI override isolation, revision-safe rapid edits, real typography, retained document/selection/undo, Quick Note inheritance, drag-end persistence, remembered focus/sort, graceful close, keyboard-visible controls, future-file preservation, repair/retry and dark/light/compact/error layouts."
    );
    Ok(())
}
