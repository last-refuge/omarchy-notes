//! Selection popover and explicit relationships exercised in the actual shell.
use super::*;
use std::sync::Arc;
pub fn context_check(directory: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().join("library");
    let mut store = notes_storage::Store::open(&root)?;
    let target = store.create(
        Document {
            blocks: vec![notes_core::document::Block::paragraph(
                "The garden notebook",
            )],
            ..Default::default()
        },
        None,
    )?;
    let mut title = notes_core::document::Block::paragraph("Make room for a good idea");
    title.set_kind("h1");
    let mut link = notes_core::document::Span::text("The garden notebook");
    link.0["href"] = serde_json::json!(format!("note://{}", target.id));
    let mut missing = notes_core::document::Span::text("A thought to reconnect");
    missing.0["href"] = serde_json::json!("note://missing");
    let mut links = notes_core::document::Block::paragraph("");
    links.spans = vec![
        link.clone(),
        notes_core::document::Span::text(" · "),
        link,
        notes_core::document::Span::text(" · "),
        missing,
    ];
    let source=store.create(Document {blocks:vec![title,notes_core::document::Block::paragraph("Keep the small things close. A little emphasis helps the right words find their rhythm."),links],..Default::default()},None)?;
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
    fn open(root: &std::path::Path) -> anyhow::Result<notes_storage::Store> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match notes_storage::Store::open(root) {
                Ok(s) => return Ok(s),
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    for (name, width, height, appearance) in [
        ("dark", 1440., 900., Appearance::Reference),
        ("light", 1440., 900., Appearance::Light),
        ("narrow", 420., 820., Appearance::Reference),
        ("small", 420., 420., Appearance::Reference),
    ] {
        drop(open(&root)?);
        let (worker, library) = Worker::start(root.clone(), false)?;
        let h = cx.open_window(size(px(width), px(height)), |w, cx| {
            cx.new(|cx| Shell::new_live(appearance, root.clone(), worker, library, w, cx))
        })?;
        let editor = h.update(&mut cx, |s, w, cx| {
            s.command(Command::Select(source.id.clone()), w, cx);
            s.live.as_ref().unwrap().notes[&source.id].editor.clone()
        })?;
        h.update(&mut cx, |_, w, cx| {
            editor.update(cx, |s, cx| {
                s.edit(
                    |e| {
                        let i = e.text().find("small things").unwrap();
                        e.select(i, false);
                        e.select(i + 12, true);
                        Ok(())
                    },
                    w,
                    cx,
                )
            })
        })?;
        draw(&mut cx, h)?;
        draw(&mut cx, h)?;
        let (bubble, viewport, a, b) = h.update(&mut cx, |_, _, cx| {
            editor.read(cx).selection_test_geometry()
        })?;
        if let Some(bubble) = bubble {
            assert_eq!(bubble.intersect(&viewport), bubble);
            assert!(bubble.bottom() < a.y || bubble.top() > b.y);
        } else {
            anyhow::ensure!(name == "small", "popover absent at {name}");
        }
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("selection-{name}.png")))?;
        if name == "dark" {
            super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-space enter")?;
            h.update(&mut cx, |_, _, cx| {
                assert!(editor.read(cx).model.mark_active("bold"))
            })?;
            super::super::visual_keys(&mut cx, h.into(), "ctrl-z escape")?;
            draw(&mut cx, h)?;
            h.update(&mut cx, |_, _, cx| {
                assert_eq!(editor.read(cx).model.document, source.body);
                assert!(editor.read(cx).selection_test_geometry().0.is_none());
            })?;
            super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-space tab tab tab enter")?;
            h.update(&mut cx, |_, _, cx| {
                assert!(editor.read(cx).model.mark_active("highlight"))
            })?;
            super::super::visual_keys(&mut cx, h.into(), "ctrl-z ctrl-shift-space escape")?;
            draw(&mut cx, h)?;
            h.update(&mut cx, |_, _, cx| {
                assert!(editor.read(cx).selection_test_geometry().0.is_none())
            })?;
            // Routed mouse dragging suppresses the toolbar until release.
            cx.update_window(h.into(), |_, w, cx| {
                w.dispatch_event(
                    PlatformInput::MouseDown(MouseDownEvent {
                        button: MouseButton::Left,
                        position: a,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }),
                    cx,
                );
            })?;
            cx.update_window(h.into(), |_, w, cx| {
                w.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position: b,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Modifiers::default(),
                    }),
                    cx,
                );
            })?;
            draw(&mut cx, h)?;
            h.update(&mut cx, |_, _, cx| {
                assert!(editor.read(cx).selection_test_geometry().0.is_none())
            })?;
            cx.update_window(h.into(), |_, w, cx| {
                w.dispatch_event(
                    PlatformInput::MouseUp(MouseUpEvent {
                        button: MouseButton::Left,
                        position: b,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    }),
                    cx,
                );
            })?;
            draw(&mut cx, h)?;
            h.update(&mut cx, |_, _, cx| {
                assert!(
                    editor.read(cx).selection_test_geometry().0.is_some(),
                    "drag release reopens a previously dismissed selection"
                )
            })?;
            h.update(&mut cx, |_, w, cx| {
                editor.update(cx, |s, cx| {
                    s.edit(|e| e.input(None, "候補", Some(None)), w, cx)
                })
            })?;
            super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-space ctrl-shift-r")?;
            draw(&mut cx, h)?;
            h.update(&mut cx, |_, w, cx| {
                let e = editor.read(cx);
                assert!(e.focus.is_focused(w));
                assert!(e.selection_test_geometry().0.is_none());
            })?;
            super::super::visual_keys(&mut cx, h.into(), "escape")?;
        }
        super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-r")?;
        draw(&mut cx, h)?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("related-{name}.png")))?;
        super::super::visual_keys(&mut cx, h.into(), "enter")?;
        h.update(&mut cx, |s, w, cx| {
            assert_eq!(s.library.selected.as_ref(), Some(&target.id));
            s.command(Command::LinkBack, w, cx);
        })?;
        super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-r down enter")?;
        h.update(&mut cx, |_, w, cx| {
            let e = editor.read(cx);
            assert_eq!(e.model.current_link().unwrap().href, "note://missing");
            assert!(!e.focus.is_focused(w));
        })?;
        draw(&mut cx, h)?;
        if name == "dark" {
            cx.capture_screenshot(h.into())?
                .save(directory.join("related-repair.png"))?;
        }
        super::super::visual_keys(&mut cx, h.into(), "escape")?;
        h.update(&mut cx, |_, _, cx| {
            assert_eq!(editor.read(cx).model.document, source.body)
        })?;
        super::super::visual_keys(&mut cx, h.into(), "ctrl-s")?;
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
                    .all(|n| n.saved == n.revision && n.pending.is_none())
            })? {
                break;
            }
            anyhow::ensure!(Instant::now() < deadline, "save timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        h.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
        drop(editor);
    }
    assert_eq!(
        open(&root)?
            .load()?
            .notes
            .iter()
            .find(|n| n.id == source.id)
            .unwrap()
            .body,
        source.body
    );
    // The related-note list mounts only its visible rows even with 5,000 targets.
    let mut store = open(&root)?;
    let mut references = notes_core::document::Block::paragraph("");
    references.spans = (0..5000)
        .map(|i| {
            notes_core::document::Span(
                serde_json::json!({"text":format!("{i} "),"href":format!("note://target-{i}")}),
            )
        })
        .collect();
    let large = store.create(
        Document {
            blocks: vec![
                notes_core::document::Block::paragraph("Many connections"),
                references,
            ],
            ..Default::default()
        },
        None,
    )?;
    drop(store);
    let (worker, library) = Worker::start(root.clone(), false)?;
    let h = cx.open_window(size(px(1000.), px(720.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    let editor = h.update(&mut cx, |s, w, cx| {
        s.command(Command::Select(large.id.clone()), w, cx);
        s.live.as_ref().unwrap().notes[&large.id].editor.clone()
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-r up")?;
    draw(&mut cx, h)?;
    draw(&mut cx, h)?;
    let counts = h.update(&mut cx, |_, _, cx| editor.read(cx).related_test_counts())?;
    assert_eq!(counts.0, 5000);
    assert!(
        counts.1 > 0 && counts.1 <= 8,
        "mounted {} related rows",
        counts.1
    );
    super::super::visual_keys(&mut cx, h.into(), "enter")?;
    h.update(&mut cx, |_, w, cx| {
        assert_eq!(
            editor.read(cx).model.current_link().unwrap().href,
            "note://target-4999"
        );
        w.remove_window();
    })?;
    cx.run_until_parked();
    drop(editor);
    println!(
        "Related-note mounted-row check: {} of {} targets at 1000 × 720 logical pixels.",
        counts.1, counts.0
    );
    println!(
        "PASS: selection toolbar geometry, keyboard marks/undo/Escape, routed drag/composition suppression; explicit related navigation/return/repair, state preservation, responsive GPU views and SQLite reopen."
    );
    Ok(())
}
