//! Real-font GPU table workflows with scratch storage, routed keys and reopen.
use super::*;
use notes_core::document::Block as RichBlock;
use std::sync::Arc;

pub fn table_check(directory: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let root = std::env::temp_dir().join(format!(
        "notes-tables-check-{}",
        notes_core::document::new_id()
    ));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
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
    fn settle(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            draw(cx, h)?;
            if h.update(cx, |s, w, cx| {
                s.poll_storage(w, cx);
                let live = s.live.as_ref().unwrap();
                live.creating.is_none()
                    && live
                        .notes
                        .values()
                        .all(|n| n.pending.is_none() && n.saved == n.revision)
            })? {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "Timed out saving table workflow");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn open_store(root: &std::path::Path) -> anyhow::Result<notes_storage::Store> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match notes_storage::Store::open(root) {
                Ok(store) => return Ok(store),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }
    let (worker, library) = Worker::start(root.clone(), false)?;
    let h = cx.open_window(size(px(1440.), px(900.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-n")?;
    settle(&mut cx, h)?;
    let (id, editor) = h.update(&mut cx, |s, w, cx| {
        let id = s.library.selected.clone().unwrap();
        let editor = s.live.as_ref().unwrap().notes[&id].editor.clone();
        editor.update(cx, |s, cx| {
            s.edit(
                |e| {
                    e.insert("Give the details a little structure", 0)?;
                    e.style("h1")?;
                    e.enter(false)?;
                    e.insert("A shared plan, one thoughtful step at a time.", 0)
                },
                w,
                cx,
            )
        });
        (id, editor)
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-alt-t")?;
    for (i, text) in [
        "Next step",
        "Who",
        "Build a calmer workspace",
        "The team",
        "Review the small details",
    ]
    .iter()
    .enumerate()
    {
        if i > 0 {
            super::super::visual_keys(&mut cx, h.into(), "tab")?;
        }
        h.update(&mut cx, |_, w, cx| {
            editor.update(cx, |s, cx| s.edit(|e| e.insert(text, 0), w, cx))
        })?;
    }
    h.update(&mut cx, |_, w, cx| {
        let e = editor.read(cx);
        assert_eq!(e.model.table_context().unwrap().rows, 3);
        assert!(e.focus.is_focused(w));
        assert!(e.error.is_none(), "{:?}", e.error);
    })?;
    super::super::visual_keys(&mut cx, h.into(), "shift-tab")?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |s, cx| {
            assert_eq!(&s.model.text()[s.model.range()], "The team");
            s.edit(|e| e.insert("Everyone", 0), w, cx);
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-z ctrl-y ctrl-alt-t enter")?;
    h.update(&mut cx, |_, w, cx| {
        let e = editor.read(cx);
        assert_eq!(e.model.table_context().unwrap().rows, 4);
        assert!(e.focus.is_focused(w));
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-z ctrl-shift-enter")?;
    h.update(&mut cx, |_, _, cx| {
        assert!(!editor.read(cx).model.in_table())
    })?;
    super::super::visual_keys(&mut cx, h.into(), "tab")?;
    h.update(&mut cx, |_, w, cx| {
        assert!(
            !editor.read(cx).focus.is_focused(w),
            "Tab in body must move keyboard focus"
        );
        w.focus(&editor.read(cx).focus.clone(), cx);
    })?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |s, cx| {
            s.edit(|e| e.insert("Room for the next idea.", 0), w, cx)
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-s")?;
    settle(&mut cx, h)?;
    let expected = h.update(&mut cx, |_, _, cx| editor.read(cx).model.document.clone())?;
    draw(&mut cx, h)?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("table-writing.png"))?;
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    drop(editor);
    let mut store = open_store(&root)?;
    assert_eq!(
        store
            .load()?
            .notes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .body,
        expected
    );
    let mut fixture =
        Document::from_json(include_str!("../../../tests/fixtures/qt-merged-table.json"))?;
    fixture.blocks[0].set_kind("h1");
    let merged = store.create(fixture, None)?.id;
    drop(store);
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
            s.command(Command::Select(merged.clone()), w, cx);
            let editor = s.live.as_ref().unwrap().notes[&merged].editor.clone();
            editor.update(cx, |s, cx| {
                s.edit(
                    |e| {
                        let table = e
                            .document
                            .blocks
                            .iter()
                            .find(|b| b.kind == "table")
                            .unwrap();
                        let block = table.cells[0]
                            .blocks
                            .iter()
                            .find(|b| b.kind != "table")
                            .unwrap();
                        let offset = e.global(&notes_editor::Position {
                            block: block.id.clone(),
                            offset: 0,
                        });
                        e.select(offset, false);
                        Ok(())
                    },
                    w,
                    cx,
                )
            });
        })?;
        super::super::visual_keys(&mut cx, h.into(), "ctrl-alt-t")?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("table-options-{name}.png")))?;
        if name == "small" {
            super::super::visual_keys(&mut cx, h.into(), "tab tab tab tab tab tab tab")?;
            draw(&mut cx, h)?;
            cx.capture_screenshot(h.into())?
                .save(directory.join("table-options-small-focused.png"))?;
        }
        super::super::visual_keys(&mut cx, h.into(), "escape")?;
        h.update(&mut cx, |s, w, cx| {
            let editor = &s.live.as_ref().unwrap().notes[&merged].editor;
            assert!(editor.read(cx).focus.is_focused(w));
        })?;
        // The writer sees the same schema after the open/close and menu cycle.
        h.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
    }
    let mut store = open_store(&root)?;
    let note = store.create(
        Document {
            blocks: vec![RichBlock::paragraph("Fault test")],
            ..Default::default()
        },
        None,
    )?;
    let mut e = notes_editor::Editor::new(note.body.clone());
    e.insert_table(2, 2)?;
    e.table_action(notes_editor::TableAction::ColumnAfter)?;
    let mut changed = note.clone();
    changed.body = e.document;
    store.fail_saves = true;
    assert!(store.save(&changed).is_err());
    drop(store);
    assert_eq!(
        notes_storage::Store::open(&root)?
            .load()?
            .notes
            .iter()
            .find(|n| n.id == note.id)
            .unwrap()
            .body,
        note.body
    );
    println!(
        "Passed GPU table insertion, cell Tab/Shift Tab, appended row, whole-cell replacement, undo/redo, keyboard table options/Escape, body focus, save/reopen and injected save failure. Images: {}",
        directory.display()
    );
    Ok(())
}
