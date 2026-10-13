//! Actual shell, real fonts and storage: consent, failures, quotes and recovery.
use super::*;
use std::sync::Arc;

pub fn quote_check(directory: &std::path::Path) -> anyhow::Result<()> {
    use notes_core::document::Block as RichBlock;
    std::fs::create_dir_all(directory)?;
    let temporary = tempfile::tempdir()?;
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
    fn wait(
        cx: &mut HeadlessAppContext,
        h: WindowHandle<Shell>,
        predicate: impl Fn(&Writing) -> bool,
    ) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let done = h.update(cx, |s, w, cx| {
                s.poll_storage(w, cx);
                predicate(s.live.as_ref().unwrap())
            })?;
            draw(cx, h)?;
            if done {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "Quote workflow timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
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
        let root = temporary.path().join(name);
        let mut store = open(&root)?;
        let mut title = RichBlock::paragraph("Make room for a good thought");
        title.set_kind("h1");
        let body = Document::from_blocks(vec![
            title,
            RichBlock::paragraph(
                "A notebook is a place to pay attention. To notice what matters, and give it a little room to grow.",
            ),
            RichBlock::paragraph("Keep the words close. Come back when you’re ready."),
        ]);
        let source = store.create(body.clone(), None)?;
        drop(store);
        let (worker, library) = Worker::start(root.clone(), false)?;
        let h = cx.open_window(size(px(width), px(height)), |w, cx| {
            cx.new(|cx| Shell::new_live(appearance, root.clone(), worker, library, w, cx))
        })?;
        let editor = h.update(&mut cx, |s, w, cx| {
            s.command(Command::Select(source.id.clone()), w, cx);
            s.live.as_ref().unwrap().notes[&source.id].editor.clone()
        })?;
        h.update(&mut cx, |_, w, cx| {
            editor.update(cx, |e, cx| {
                e.edit(
                    |e| {
                        e.select(body.blocks[0].text().len() + 1, false);
                        Ok(())
                    },
                    w,
                    cx,
                )
            })
        })?;
        super::super::visual_keys(&mut cx, h.into(), "ctrl-shift-q ctrl-s")?;
        wait(&mut cx, h, |l| l.notes[&source.id].needs_upgrade)?;
        h.update(&mut cx, |s, w, cx| {
            assert_eq!(s.live.as_ref().unwrap().schema, 2);
            s.command(Command::UpgradePanel, w, cx);
        })?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("upgrade-{name}.png")))?;
        if width < 500. {
            super::super::visual_keys(&mut cx, h.into(), "end")?;
            draw(&mut cx, h)?;
            h.update(&mut cx, |s, _, _| {
                assert!(s.live.as_ref().unwrap().upgrade.details_scroll.offset().y < px(0.))
            })?;
            cx.capture_screenshot(h.into())?
                .save(directory.join(format!("upgrade-details-{name}.png")))?;
        }
        super::super::visual_keys(&mut cx, h.into(), "escape")?;
        h.update(&mut cx, |s, w, cx| {
            assert!(!s.live.as_ref().unwrap().upgrade.open);
            assert!(s.live.as_ref().unwrap().notes[&source.id].needs_upgrade);
            s.command(Command::UpgradePanel, w, cx);
            // Existing destinations fail safely, and the same draft can retry.
            s.live.as_mut().unwrap().upgrade.destination = Some(root.clone());
            s.command(Command::UpgradeStart, w, cx);
        })?;
        wait(&mut cx, h, |l| l.upgrade.pending.is_none())?;
        h.update(&mut cx, |s, _, _| {
            let l = s.live.as_ref().unwrap();
            assert_eq!(l.schema, 2);
            assert!(
                l.upgrade
                    .message
                    .as_ref()
                    .unwrap()
                    .contains("already exists")
            );
        })?;
        if name == "dark" {
            cx.capture_screenshot(h.into())?
                .save(directory.join("upgrade-error.png"))?;
        }
        let backup = temporary.path().join(format!("before-{name}"));
        h.update(&mut cx, |s, w, cx| {
            s.live.as_mut().unwrap().upgrade.destination = Some(backup.clone());
            s.command(Command::UpgradeStart, w, cx);
            let operation = s.live.as_ref().unwrap().upgrade.pending.as_ref().unwrap().0;
            s.upgrade_finished(operation + 1, Err("stale result".into()), cx);
            assert!(s.live.as_ref().unwrap().upgrade.pending.is_some());
            assert!(!s.request_close(w, cx));
            // Newer edits made while the upgrade runs still get their own save.
            editor.update(cx, |e, cx| {
                e.edit(
                    |e| {
                        let offset = e.global(&notes_editor::Position {
                            block: body.blocks[1].id.clone(),
                            offset: body.blocks[1].text().len(),
                        });
                        e.select(offset, false);
                        e.insert(" One sentence at a time.", 1)
                    },
                    w,
                    cx,
                );
                w.focus(&e.focus, cx);
            });
        })?;
        wait(&mut cx, h, |l| {
            l.schema == 3
                && l.upgrade.pending.is_none()
                && l.notes
                    .values()
                    .all(|s| s.saved == s.revision && s.pending.is_none())
        })?;
        assert_eq!(
            notes_storage::backup::inspect(&backup)?
                .manifest
                .schema_version,
            2
        );
        h.update(&mut cx, |s, w, cx| {
            s.command(Command::UpgradeCancel, w, cx);
        })?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(format!("quotes-{name}.png")))?;
        h.update(&mut cx, |_, _, cx| {
            let doc = &editor.read(cx).model.document;
            assert_eq!(doc.schema, 2);
            assert_eq!(doc.blocks[1].kind, "quote");
            assert!(doc.blocks[1].text().ends_with("One sentence at a time."));
            doc.validate().unwrap();
        })?;
        // A full rich copy/paste retains the quote; one undo restores the note.
        super::super::visual_keys(&mut cx, h.into(), "ctrl-a ctrl-c end ctrl-v ctrl-z ctrl-s")?;
        wait(&mut cx, h, |l| {
            l.notes
                .values()
                .all(|s| s.saved == s.revision && s.pending.is_none())
        })?;
        let expected = h.update(&mut cx, |s, _, _| {
            s.live.as_ref().unwrap().notes[&source.id].note.body.clone()
        })?;
        h.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
        let store = open(&root)?;
        assert_eq!(store.load()?.notes[0].body, expected);
        drop(store);
        let rollback = temporary.path().join(format!("rollback-{name}"));
        notes_storage::backup::restore_to_new(&backup, &rollback)?;
        let restored = open(&rollback)?;
        assert_eq!(restored.schema()?, 2);
        assert_eq!(restored.load()?.notes[0].body, body);
    }
    println!(
        "PASS: routed quote editing, explicit upgrade/cancel/failure/retry, stale-result and close guards, edits during upgrade, rich clipboard, schema-3 save/reopen and schema-2 rollback; dark/light/compact GPU images."
    );
    Ok(())
}
