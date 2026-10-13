//! Links routed through real GPUI focus, editor state, navigation and SQLite.
use super::*;
use std::sync::Arc;

pub fn link_check(directory: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().join("library");
    let mut store = notes_storage::Store::open(&root)?;
    fn document(title: &str, text: &str) -> Document {
        let mut first = notes_core::document::Block::paragraph(title);
        first.set_kind("h1");
        Document {
            blocks: vec![first, notes_core::document::Block::paragraph(text)],
            ..Default::default()
        }
    }
    let source=store.create(document("A place for connected thoughts", "A good idea rarely arrives alone. Keep the research close, and leave a path back to the thought that started it."),None)?;
    let target = store.create(
        document(
            "The garden notebook",
            "Small observations grow into something worth keeping.",
        ),
        None,
    )?;
    let _third = store.create(
        document(
            "Reading for a slow Sunday",
            "A few pages, a little quiet, and room to think.",
        ),
        None,
    )?;
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
    let (worker, library) = Worker::start(root.clone(), false)?;
    let h = cx.open_window(size(px(1440.), px(900.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    let editor = h.update(&mut cx, |s, w, cx| {
        s.command(Command::Select(source.id.clone()), w, cx);
        s.live.as_ref().unwrap().notes[&source.id]
            .editor
            .as_ref()
            .unwrap()
            .clone()
    })?;
    // A selection spanning text with existing emphasis keeps both words and marks.
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |s, cx| {
            s.edit(
                |e| {
                    let start = e.text().find("good idea").unwrap();
                    e.select(start, false);
                    e.select(start + 9, true);
                    e.mark("bold")
                },
                w,
                cx,
            )
        })
    })?;
    let original = h.update(&mut cx, |_, _, cx| editor.read(cx).model.document.clone())?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-k")?;
    type_text(&mut cx, h, "https://example.com/research")?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    h.update(&mut cx, |_, w, cx| {
        let e = editor.read(cx);
        assert!(e.focus.is_focused(w));
        assert_eq!(
            e.model.current_link().unwrap().href,
            "https://example.com/research"
        );
        assert!(e.model.mark_active("bold"));
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-k tab tab enter")?;
    h.update(&mut cx, |_, _, cx| {
        assert_eq!(
            editor.read(cx).model.document,
            original,
            "keyboard removal keeps text and marks"
        )
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-z")?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-z")?;
    h.update(&mut cx, |_, _, cx| {
        assert_eq!(editor.read(cx).model.document, original)
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-y ctrl-shift-k")?;
    type_text(&mut cx, h, "javascript:bad")?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    h.update(&mut cx, |_, _, cx| {
        let e = editor.read(cx);
        assert!(e.error.as_ref().unwrap().contains("https://"));
        assert_eq!(
            e.model.current_link().unwrap().href,
            "https://example.com/research"
        );
    })?;
    // A document edit while the picker is open must not silently retarget.
    super::super::visual_keys(&mut cx, h.into(), "ctrl-a")?;
    type_text(&mut cx, h, "https://example.org")?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |s, cx| s.edit(|e| e.mark("italic"), w, cx))
    })?;
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    h.update(&mut cx, |_, _, cx| {
        assert!(
            editor
                .read(cx)
                .error
                .as_ref()
                .unwrap()
                .contains("selection changed")
        )
    })?;
    super::super::visual_keys(&mut cx, h.into(), "escape ctrl-z ctrl-l")?;
    type_text(&mut cx, h, "garden")?;
    super::super::visual_keys(&mut cx, h.into(), "down up enter")?;
    h.update(&mut cx, |_, _, cx| {
        assert_eq!(
            editor.read(cx).model.current_link().unwrap().href,
            format!("note://{}", target.id)
        )
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-enter")?;
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.library.selected.as_ref(), Some(&target.id));
        s.command(Command::LinkBack, w, cx);
        assert_eq!(s.library.selected.as_ref(), Some(&source.id));
        assert!(editor.read(cx).model.mark_active("bold"));
    })?;
    // Late requests from an inactive source cannot hijack navigation.
    h.update(&mut cx, |s, w, cx| {
        s.follow_note_link(&target.id, &target.id, w, cx);
        assert_eq!(s.library.selected.as_ref(), Some(&source.id));
    })?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |s, cx| {
            s.edit(|e| e.set_link("note://missing-target", ""), w, cx);
            s.open_link_or_media(w, cx);
        })
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |s, _, cx| {
        assert_eq!(s.library.selected.as_ref(), Some(&source.id));
        assert!(
            editor
                .read(cx)
                .error
                .as_ref()
                .unwrap()
                .contains("isn’t available")
        );
    })?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("link-missing.png"))?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-z")?;
    // Opening destinations must not steal a live text composition.
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |s, cx| {
            s.edit(|e| e.input(None, "候補", Some(None)), w, cx)
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-l ctrl-shift-k ctrl-enter")?;
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.library.selected.as_ref(), Some(&source.id));
        assert!(editor.read(cx).focus.is_focused(w));
        assert!(editor.read(cx).model.marked.is_some());
    })?;
    super::super::visual_keys(&mut cx, h.into(), "escape ctrl-s")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        draw(&mut cx, h)?;
        if h.update(&mut cx, |s, w, cx| {
            s.poll_storage(w, cx);
            s.live
                .as_ref()
                .unwrap()
                .notes
                .values()
                .all(|n| n.pending.is_none() && n.saved == n.revision)
        })? {
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "links save timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    let expected = h.update(&mut cx, |_, _, cx| editor.read(cx).model.document.clone())?;
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    drop(editor);
    assert_eq!(
        open_store(&root)?
            .load()?
            .notes
            .iter()
            .find(|n| n.id == source.id)
            .unwrap()
            .body,
        expected
    );
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
            s.command(Command::Select(source.id.clone()), w, cx);
        })?;
        super::super::visual_keys(&mut cx, h.into(), "ctrl-l")?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("link-picker-{name}.png")))?;
        super::super::visual_keys(&mut cx, h.into(), "escape ctrl-shift-k")?;
        type_text(&mut cx, h, "https://example.com/research")?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("link-address-{name}.png")))?;
        super::super::visual_keys(&mut cx, h.into(), "escape")?;
        h.update(&mut cx, |s, w, cx| {
            let e = s.live.as_ref().unwrap().notes[&source.id]
                .editor
                .as_ref()
                .unwrap()
                .read(cx);
            assert_eq!(e.model.document, expected);
            assert_eq!(e.model.revision, 0);
            w.remove_window();
        })?;
        cx.run_until_parked();
    }
    println!(
        "PASS: link add/retarget, invalid destination/stale picker rejection, marks and undo, note navigation/return, inactive-source guard, missing targets, composition guard, save/reopen and GPU layouts. External applications were not launched."
    );
    Ok(())
}
