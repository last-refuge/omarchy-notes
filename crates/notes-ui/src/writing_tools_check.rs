//! Routed writing tools in the actual shell, with real fonts/GPU and SQLite.
use super::*;
use std::sync::Arc;

pub fn writing_tools_check(directory: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().join("library");
    let mut store = notes_storage::Store::open(&root)?;
    let mut doc = Document::default();
    doc.blocks[0].set_kind("h1");
    doc.blocks[0].spans = vec![notes_core::document::Span::text(
        "Give your thoughts a little shape",
    )];
    for text in [
        "Find the next idea. Keep a good idea close.",
        "A little emphasis helps the important words breathe.",
        "Café, 中文 and a family 👩‍👩‍👧‍👦 belong here too.",
    ] {
        doc.blocks
            .push(notes_core::document::Block::paragraph(text));
    }
    let note = store.create(doc, None)?;
    drop(store);
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
    fn type_text(
        cx: &mut HeadlessAppContext,
        h: WindowHandle<Shell>,
        text: &str,
    ) -> anyhow::Result<()> {
        for ch in text.chars() {
            let cleanup = cx.update_window(h.into(), |_, w, cx| {
                let key = ch.to_string();
                w.dispatch_keystroke(
                    Keystroke {
                        modifiers: Modifiers::default(),
                        key: key.clone(),
                        key_char: Some(key),
                    },
                    cx,
                );
                w.draw(cx)
            })?;
            cx.update(|cx| cleanup.clear(cx));
            cx.run_until_parked();
        }
        Ok(())
    }
    fn saved(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            draw(cx, h)?;
            if h.update(cx, |s, w, cx| {
                s.poll_storage(w, cx);
                s.live
                    .as_ref()
                    .unwrap()
                    .notes
                    .values()
                    .all(|n| n.pending.is_none() && n.saved == n.revision)
            })? {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "save did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let (worker, library) = Worker::start(root.clone(), false)?;
    let h = cx.open_window(size(px(1440.), px(900.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    let editor = h.update(&mut cx, |s, w, cx| {
        s.command(Command::Select(note.id.clone()), w, cx);
        s.live.as_ref().unwrap().notes[&note.id]
            .editor
            .as_ref()
            .unwrap()
            .clone()
    })?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |s, cx| {
            s.edit(|e| e.input(None, "候補", Some(None)), w, cx)
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-f ctrl-shift-m")?;
    h.update(&mut cx, |_, w, cx| {
        let e = editor.read(cx);
        assert!(e.model.marked.is_some());
        assert!(e.focus.is_focused(w));
    })?;
    super::super::visual_keys(&mut cx, h.into(), "escape")?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-f")?;
    type_text(&mut cx, h, "idea")?;
    let first = h.update(&mut cx, |_, _, cx| {
        let e = editor.read(cx);
        assert_eq!(&e.model.text()[e.model.range()], "idea");
        e.model.range()
    })?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    h.update(&mut cx, |_, _, cx| {
        assert!(editor.read(cx).model.range().start > first.start)
    })?;
    super::super::visual_keys(&mut cx, h.into(), "shift-enter")?;
    h.update(&mut cx, |_, _, cx| {
        assert_eq!(editor.read(cx).model.range(), first)
    })?;
    super::super::visual_keys(&mut cx, h.into(), "escape ctrl-shift-x ctrl-shift-y ctrl-e")?;
    h.update(&mut cx, |_, w, cx| {
        let e = editor.read(cx);
        assert!(e.focus.is_focused(w));
        for mark in ["strikethrough", "highlight", "code"] {
            assert!(e.model.mark_active(mark));
        }
    })?;
    super::super::visual_keys(
        &mut cx,
        h.into(),
        "ctrl-z ctrl-y ctrl-shift-m tab tab enter",
    )?;
    h.update(&mut cx, |_, _, cx| {
        assert!(editor.read(cx).model.style_active("h2"))
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-&")?;
    h.update(&mut cx, |_, _, cx| {
        assert!(editor.read(cx).model.style_active("ordered"))
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-7")?;
    h.update(&mut cx, |_, _, cx| {
        assert!(editor.read(cx).model.style_active("paragraph"))
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-*")?;
    h.update(&mut cx, |_, _, cx| {
        assert!(editor.read(cx).model.style_active("bullet"))
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-8")?;
    h.update(&mut cx, |_, _, cx| {
        assert!(editor.read(cx).model.style_active("paragraph"))
    })?;

    // Keep the finished screenshot quiet; show find on ordinary readable text.
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-x ctrl-shift-y ctrl-e ctrl-s")?;
    saved(&mut cx, h)?;
    let expected = h.update(&mut cx, |_, _, cx| editor.read(cx).model.document.clone())?;
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    drop(editor);
    fn open_store(root: &std::path::Path) -> anyhow::Result<notes_storage::Store> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match notes_storage::Store::open(root) {
                Ok(store) => return Ok(store),
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    assert_eq!(
        open_store(&root)?
            .load()?
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap()
            .body,
        expected
    );
    // Open each viewport explicitly: the headless Window::resize method does
    // not dispatch a platform resize event to update the logical viewport.
    for (name, width, height, appearance) in [
        ("dark", 1440., 900., Appearance::Reference),
        ("light", 1440., 900., Appearance::Light),
        ("narrow", 420., 820., Appearance::Reference),
        ("small", 420., 420., Appearance::Reference),
    ] {
        drop(open_store(&root)?);
        let (worker, library) = Worker::start(root.clone(), false)?;
        let h = cx.open_window(size(px(width), px(height)), |w, cx| {
            cx.new(|cx| Shell::new_live(appearance, root.clone(), worker, library, w, cx))
        })?;
        h.update(&mut cx, |s, w, cx| {
            assert_eq!(w.viewport_size(), size(px(width), px(height)));
            s.command(Command::Select(note.id.clone()), w, cx);
        })?;
        super::super::visual_keys(&mut cx, h.into(), "ctrl-f")?;
        type_text(&mut cx, h, "idea")?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("find-{name}.png")))?;
        super::super::visual_keys(&mut cx, h.into(), "escape ctrl-shift-m")?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("format-{name}.png")))?;
        super::super::visual_keys(
            &mut cx,
            h.into(),
            "tab tab tab tab tab tab tab tab tab tab tab tab",
        )?;
        draw(&mut cx, h)?;
        if name == "small" {
            cx.capture_screenshot(h.into())?
                .save(directory.join("format-small-focused.png"))?;
        }
        super::super::visual_keys(&mut cx, h.into(), "escape ctrl-f ctrl-a")?;
        type_text(&mut cx, h, "unfindable")?;
        if name == "dark" {
            draw(&mut cx, h)?;
            cx.capture_screenshot(h.into())?
                .save(directory.join("find-empty.png"))?;
        }
        super::super::visual_keys(&mut cx, h.into(), "escape")?;
        h.update(&mut cx, |s, w, cx| {
            let e = s.live.as_ref().unwrap().notes[&note.id]
                .editor
                .as_ref()
                .unwrap()
                .read(cx);
            assert_eq!(e.model.revision, 0, "search must never create saved edits");
            assert_eq!(e.model.document, expected);
            assert!(e.focus.is_focused(w));
            w.remove_window();
        })?;
        cx.run_until_parked();
    }
    assert_eq!(
        open_store(&root)?
            .load()?
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap()
            .body,
        expected
    );
    println!(
        "PASS: routed find/wrap, formatting menu/shortcuts, list toggling, undo/redo, search leaves persistence untouched; GPU layouts saved in {}",
        directory.display()
    );
    Ok(())
}
