//! GPU media workflow check; only disposable libraries and project fixtures.
use super::*;
use notes_storage::media::Import;
use std::sync::Arc;
pub fn media_check(directory: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let root = std::env::temp_dir().join(format!(
        "omarchy-media-check-{}",
        notes_core::document::new_id()
    ));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::create_dir_all(&root)?;
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/qt-v1/blobs");
    let png = root.join("A little faster.png");
    let pdf = root.join("Release notes.pdf");
    std::fs::copy(
        fixture.join("bd/bdec1de53e3c856d6973c7324a5d719d6f6169e01a6f75ae3b4453f419d625e2"),
        &png,
    )?;
    std::fs::copy(
        fixture.join("d7/d7449961543b028f75a5778a6cbcfb8d3b7ea2dfe66da1efc60d9747f4fc4803"),
        &pdf,
    )?;
    let library_root = root.join("library");
    let (worker, library) = Worker::start(library_root.clone(), false)?;
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
        mut condition: impl FnMut(&mut Shell, &mut Window, &mut Context<Shell>) -> bool,
    ) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            draw(cx, h)?;
            if h.update(cx, |s, w, cx| {
                s.poll_storage(w, cx);
                condition(s, w, cx)
            })? {
                return Ok(());
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "Timed out waiting for media workflow"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| {
            Shell::new_live(
                Appearance::Reference,
                library_root.clone(),
                worker,
                library,
                w,
                cx,
            )
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-n")?;
    wait(&mut cx, h, |s, _, _| {
        s.live.as_ref().unwrap().creating.is_none()
    })?;
    let (id,editor)=h.update(&mut cx,|s,w,cx| {
        let id=s.library.selected.clone().unwrap(); let editor=s.live.as_ref().unwrap().notes[&id].editor.as_ref().unwrap().clone();
        editor.update(cx,|e,cx| {
            e.edit(|e| { e.insert("Small details. Better days.",0)?; e.enter(false)?; e.insert("A little faster, a little clearer. Keep the picture and the details together.",0)?; e.enter(false) },w,cx);
            e.queue_import(Import::Files{paths:vec![png.clone(),pdf.clone()],images:None},cx);
        }); (id,editor)
    })?;
    wait(&mut cx, h, |_, _, cx| !editor.read(cx).has_pending_work())?;
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            assert!(e.error.is_none(), "{:?}", e.error);
            assert_eq!(e.model.document.references("image").len(), 1);
            assert_eq!(e.model.document.references("attachment").len(), 1);
            // Select a real image, then verify the cached GPU preview exists.
            let at = e.model.text().find('\u{fffc}').unwrap();
            e.model.select(at, false);
            e.model.select(at + 3, true);
            cx.notify();
            w.invalidate_character_coordinates();
        })
    })?;
    wait(&mut cx, h, |_, _, cx| {
        editor
            .read(cx)
            .selected_media()
            .is_some_and(|m| m.image.is_some())
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-s")?;
    wait(&mut cx, h, |s, _, _| {
        let v = &s.live.as_ref().unwrap().notes[&id];
        v.pending.is_none() && v.revision == v.saved
    })?;
    draw(&mut cx, h)?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("media-dark.png"))?;
    h.update(&mut cx, |s, w, cx| {
        s.command(Command::Appearance(Appearance::Light), w, cx)
    })?;
    draw(&mut cx, h)?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("media-light.png"))?;
    // Removing a selected object is one undo operation; formatting/body survives.
    let expected = cx.update(|cx| editor.read(cx).model.document.clone());
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.edit(|m| m.insert("", 0), w, cx);
            assert!(e.model.document.references("image").is_empty());
            e.edit(
                |m| {
                    m.undo();
                    Ok(())
                },
                w,
                cx,
            );
            assert_eq!(e.model.document, expected);
        })
    })?;
    // Cursor changes during import require deliberate insertion at the new location.
    h.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.model.select(e.model.text().len(), false);
            e.queue_import(
                Import::Files {
                    paths: vec![pdf.clone()],
                    images: Some(false),
                },
                cx,
            );
            e.edit(|m| m.insert("Keep this thought. ", 0), w, cx);
        })
    })?;
    h.update(&mut cx, |s, w, cx| {
        assert!(!s.request_close(w, cx));
        let live = s.live.as_mut().unwrap();
        live.closing = false;
        live.message = None;
    })?;
    wait(&mut cx, h, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.insert_ready(w, cx);
            !e.has_pending_work()
        })
    })?;
    assert!(cx.update(|cx| editor.read(cx).model.text().contains("Keep this thought.")));
    assert_eq!(
        cx.update(|cx| editor
            .read(cx)
            .model
            .document
            .references("attachment")
            .len()),
        2
    );
    // Cancel then receive a late commit: no insertion into this or another note.
    let before = cx.update(|cx| editor.read(cx).model.document.clone());
    h.update(&mut cx, |_, _, cx| {
        editor.update(cx, |e, cx| {
            e.queue_import(
                Import::Files {
                    paths: vec![pdf],
                    images: Some(false),
                },
                cx,
            );
            e.cancel_transfer(cx);
        })
    })?;
    super::super::visual_keys(&mut cx, h.into(), "ctrl-s")?;
    wait(&mut cx, h, |s, _, _| {
        let v = &s.live.as_ref().unwrap().notes[&id];
        v.pending.is_none() && v.revision == v.saved
    })?;
    assert_eq!(
        cx.update(|cx| editor.read(cx).model.document.clone()),
        before
    );
    // A bad image leaves the draft intact and exposes a useful error.
    h.update(&mut cx, |_, _, cx| {
        editor.update(cx, |e, cx| {
            e.queue_import(
                Import::Image {
                    bytes: b"broken".to_vec(),
                    name: "Bad.png".into(),
                },
                cx,
            )
        })
    })?;
    wait(&mut cx, h, |_, _, cx| !editor.read(cx).has_pending_work())?;
    assert!(cx.update(|cx| editor.read(cx).error.is_some()));
    assert_eq!(
        cx.update(|cx| editor.read(cx).model.document.clone()),
        before
    );
    draw(&mut cx, h)?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("media-error.png"))?;
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    drop(editor);
    cx.run_until_parked();
    // The durable mixed document opens at compact widths and loads its media again.
    for width in [640., 420.] {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (worker, library) = loop {
            match Worker::start(library_root.clone(), false) {
                Ok(v) => break v,
                Err(e) => {
                    if Instant::now() > deadline {
                        return Err(e);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        };
        assert_eq!(library.notes[0].body, before);
        let h = cx.open_window(size(px(width), px(900.)), |w, cx| {
            cx.new(|cx| {
                Shell::new_live(
                    Appearance::Reference,
                    library_root.clone(),
                    worker,
                    library,
                    w,
                    cx,
                )
            })
        })?;
        let editor = h.update(&mut cx, |s, w, cx| {
            s.command(Command::Select(id.clone()), w, cx);
            s.live.as_ref().unwrap().notes[&id]
                .editor
                .as_ref()
                .unwrap()
                .clone()
        })?;
        h.update(&mut cx, |_, _, cx| {
            editor.update(cx, |e, cx| {
                let at = e.model.text().find('\u{fffc}').unwrap();
                e.model.select(at, false);
                e.model.select(at + 3, true);
                cx.notify();
            })
        })?;
        wait(&mut cx, h, |_, _, cx| {
            editor
                .read(cx)
                .selected_media()
                .is_some_and(|m| m.image.is_some())
        })?;
        draw(&mut cx, h)?;
        cx.capture_screenshot(h.into())?
            .save(directory.join(if width == 640. {
                "media-narrow.png"
            } else {
                "media-420.png"
            }))?;
        h.update(&mut cx, |_, w, _| w.remove_window())?;
        drop(editor);
        cx.run_until_parked();
    }
    println!(
        "Passed GPU media import, cached image rendering, attachment cards, selection/removal/undo, cursor-change guard, cancellation, failed import, save and compact reopen. Images: {}",
        directory.display()
    );
    Ok(())
}
