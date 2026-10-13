//! Real font/GPU layout, real SQLite and the same deferred multi-window routes.
use super::*;
use crate::rich_editor::RichEditor;
use std::{path::Path, sync::Arc};

fn draw(cx: &mut HeadlessAppContext, h: AnyWindowHandle) -> anyhow::Result<()> {
    cx.run_until_parked();
    let cleanup = h.update(cx, |_, w, cx| {
        w.refresh();
        w.draw(cx)
    })?;
    cx.update(|cx| cleanup.clear(cx));
    cx.run_until_parked();
    Ok(())
}
fn quick(
    cx: &mut HeadlessAppContext,
    shell: &Entity<Shell>,
) -> (AnyWindowHandle, String, Entity<RichEditor>) {
    cx.update(|cx| {
        shell.update(cx, |s, cx| {
            let (h, id) = s.windows.quick.clone().unwrap();
            let editor = s.live.as_ref().unwrap().notes[&id]
                .editor
                .as_ref()
                .unwrap()
                .clone();
            assert_eq!(s.real_status(&id, cx), "Ready when you are");
            (h, id, editor)
        })
    })
}
fn wait(
    cx: &mut HeadlessAppContext,
    shell: &Entity<Shell>,
    predicate: impl Fn(&Shell, &App) -> bool,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let h = cx.update(|cx| {
            let s = shell.read(cx);
            s.windows
                .main
                .or_else(|| s.windows.quick.as_ref().map(|q| q.0))
                .unwrap()
        });
        let done = h.update(cx, |_, w, cx| {
            shell.update(cx, |s, cx| {
                s.poll_storage(w, cx);
                predicate(s, cx)
            })
        })?;
        if done {
            return Ok(());
        }
        draw(cx, h)?;
        anyhow::ensure!(Instant::now() < deadline, "Quick Note workflow timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn text(
    cx: &mut HeadlessAppContext,
    h: AnyWindowHandle,
    editor: &Entity<RichEditor>,
    text: &str,
) -> anyhow::Result<()> {
    h.update(cx, |_, w, cx| {
        editor.update(cx, |e, cx| e.replace_text_in_range(None, text, w, cx))
    })?;
    draw(cx, h)
}
fn saved(s: &Shell, cx: &App) -> bool {
    s.live
        .as_ref()
        .unwrap()
        .notes
        .values()
        .all(|n| n.pending.is_none() && n.revision == n.saved && !n.has_pending_work(cx))
}

pub fn quick_note_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("library");
    let (worker, library) = notes_storage::Worker::start(root.clone(), false)?;
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
    // First launch creates only the small window and no durable note.
    let initial = cx.open_window(size(px(680.), px(660.)), |w, cx| {
        cx.new(|cx| QuickView::initial(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    let shell = initial.update(&mut cx, |q, _, _| q.shell.clone())?;
    let (q, blank, _) = quick(&mut cx, &shell);
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    draw(&mut cx, q)?;
    cx.capture_screenshot(q)?
        .save(directory.join("quick-empty.png"))?;
    // Opening main with no target must leave the quick draft where it is.
    cx.update(|cx| shell.update(cx, |s, cx| s.open_main(None, cx)));
    let main = cx.update(|cx| shell.read(cx).windows.main.unwrap());
    draw(&mut cx, main)?;
    q.update(&mut cx, |_, w, cx| {
        shell.update(cx, |s, cx| {
            s.close_quick(w, cx);
        })
    })?;
    cx.run_until_parked();
    cx.update(|cx| {
        assert!(
            !shell
                .read(cx)
                .live
                .as_ref()
                .unwrap()
                .notes
                .contains_key(&blank)
        )
    });
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    cx.update(|cx| shell.update(cx, |s, cx| s.open_quick(cx)));
    let (q, id, editor) = quick(&mut cx, &shell);
    draw(&mut cx, q)?;
    text(&mut cx, q, &editor, "A little room for a thought")?;
    cx.update(|cx| shell.update(cx, |s, cx| s.save_real_notes(cx)));
    // Submit revision N, edit N+1 before accepting the acknowledgement.
    q.update(&mut cx, |_, w, cx| {
        shell.update(cx, |s, cx| s.poll_storage(w, cx))
    })?;
    text(&mut cx, q, &editor, " — and a little more")?;
    wait(&mut cx, &shell, |s, _| {
        s.live.as_ref().unwrap().notes[&id].pending.is_none()
    })?;
    cx.update(|cx| {
        let n = &shell.read(cx).live.as_ref().unwrap().notes[&id];
        assert_eq!(n.saved, 1);
        assert_eq!(n.revision, 2);
        assert_eq!(n.note.revision, 1);
    });
    cx.update(|cx| shell.update(cx, |s, cx| s.save_real_notes(cx)));
    wait(&mut cx, &shell, saved)?;
    text(
        &mut cx,
        q,
        &editor,
        "\nKeep it close. Come back when you’re ready.",
    )?;
    draw(&mut cx, q)?;
    cx.capture_screenshot(q)?
        .save(directory.join("quick-dark.png"))?;
    // Composition prevents a move. This checks the adapter, not a real IME.
    q.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.replace_and_mark_text_in_range(None, "日本", Some(2..2), w, cx)
        })
    })?;
    main.update(&mut cx, |_, w, cx| {
        shell.update(cx, |s, cx| assert!(!s.take_quick_into_main(&id, w, cx)))
    })?;
    q.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| e.replace_text_in_range(None, "日本語", w, cx))
    })?;
    let (document, selection) = cx.update(|cx| {
        let e = editor.read(cx);
        (e.model.document.clone(), e.model.selection.clone())
    });
    draw(&mut cx, q)?;
    super::super::visual_keys(&mut cx, q, "ctrl-shift-o")?;
    cx.update(|cx| {
        let s = shell.read(cx);
        assert!(s.windows.quick.is_none());
        assert!(
            s.reading,
            "Search reset must not navigate away after handoff"
        );
        assert!(s.windows.notice.is_none());
        assert_eq!(
            s.live.as_ref().unwrap().notes[&id].editor.as_ref().unwrap(),
            &editor
        );
        assert_eq!(editor.read(cx).model.document, document);
        assert_eq!(editor.read(cx).model.selection, selection);
    });
    draw(&mut cx, main)?;
    super::super::visual_keys(&mut cx, main, "ctrl-z")?;
    cx.update(|cx| {
        assert!(
            !editor
                .read(cx)
                .model
                .document
                .plain_text()
                .contains("日本語")
        )
    });
    super::super::visual_keys(&mut cx, main, "ctrl-shift-z ctrl-s")?;
    wait(&mut cx, &shell, saved)?;
    // Internal find subscription must follow the moved editor to its new window.
    super::super::visual_keys(&mut cx, main, "ctrl-f")?;
    super::super::visual_keys(&mut cx, main, "escape")?;

    // A media-only draft persists a parent before import, on the same FIFO writer.
    cx.update(|cx| shell.update(cx, |s, cx| s.open_quick(cx)));
    let (q, media_id, media_editor) = quick(&mut cx, &shell);
    let attachment = temporary.path().join("A small reference.txt");
    std::fs::write(&attachment, "A reference worth keeping")?;
    q.update(&mut cx, |_, _, cx| {
        media_editor.update(cx, |e, cx| {
            e.queue_import(
                notes_storage::media::Import::Files {
                    paths: vec![attachment],
                    images: Some(false),
                },
                cx,
            )
        })
    })?;
    main.update(&mut cx, |_, w, cx| {
        shell.update(cx, |s, cx| {
            assert!(!s.take_quick_into_main(&media_id, w, cx))
        })
    })?;
    wait(&mut cx, &shell, |s, cx| {
        !s.live.as_ref().unwrap().notes[&media_id]
            .editor
            .as_ref()
            .unwrap()
            .read(cx)
            .has_pending_work()
    })?;
    cx.update(|cx| {
        assert_eq!(
            media_editor
                .read(cx)
                .model
                .document
                .references("attachment")
                .len(),
            1
        );
        shell.update(cx, |s, cx| s.save_real_notes(cx));
    });
    wait(&mut cx, &shell, saved)?;
    // Main may close without stopping the shared writer or stealing the Quick Note.
    main.update(&mut cx, |_, w, cx| {
        shell.update(cx, |s, cx| {
            s.request_close(w, cx);
        })
    })?;
    cx.run_until_parked();
    cx.update(|cx| assert!(shell.read(cx).windows.main.is_none()));
    text(&mut cx, q, &media_editor, " Notes beside a reference.")?;
    cx.update(|cx| shell.update(cx, |s, cx| s.save_real_notes(cx)));
    // Exercise the app-scoped polling task after its original window is gone.
    cx.advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    wait(&mut cx, &shell, saved)?;
    let expected_media = cx.update(|cx| {
        shell.read(cx).live.as_ref().unwrap().notes[&media_id]
            .note
            .body
            .clone()
    });
    cx.update(|cx| shell.update(cx, |s, cx| s.open_main(Some(media_id.clone()), cx)));
    let main = cx.update(|cx| shell.read(cx).windows.main.unwrap());
    draw(&mut cx, main)?;
    // A failed first save retains the new draft and holds Quick Note open.
    // Failure injection uses a separate library below so production flags stay immutable.
    main.update(&mut cx, |_, w, cx| {
        shell.update(cx, |s, cx| {
            s.request_close(w, cx);
        })
    })?;
    cx.run_until_parked();
    drop(editor);
    drop(media_editor);
    let weak = shell.downgrade();
    drop(shell);
    cx.update(|cx| assert!(cx.windows().is_empty()));
    cx.run_until_parked();
    cx.update(|cx| {
        cx.new(|_| ()).update(cx, |_, cx| cx.notify());
    });
    assert!(
        weak.upgrade().is_none(),
        "Closed window retained its library controller"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let store = loop {
        match notes_storage::Store::open(&root) {
            Ok(s) => break s,
            Err(e) if Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let notes = store.load()?.notes;
    assert_eq!(notes.len(), 2);
    assert!(!notes.iter().any(|n| n.id == blank));
    assert_eq!(
        notes.iter().find(|n| n.id == media_id).unwrap().body,
        expected_media
    );
    drop(store);

    for (name, width, height, theme, fail) in [
        ("quick-light", 680., 660., Appearance::Light, false),
        ("quick-small", 420., 420., Appearance::Reference, false),
        ("quick-error", 420., 420., Appearance::Reference, true),
    ] {
        let root = temporary.path().join(name);
        let (worker, library) = notes_storage::Worker::start(root.clone(), fail)?;
        let h = cx.open_window(size(px(width), px(height)), |w, cx| {
            cx.new(|cx| QuickView::initial(theme, root, worker, library, w, cx))
        })?;
        let shell = h.update(&mut cx, |q, _, _| q.shell.clone())?;
        let (q, id, editor) = quick(&mut cx, &shell);
        text(
            &mut cx,
            q,
            &editor,
            "A thought worth keeping\nStart small. Give it a little shape.",
        )?;
        cx.update(|cx| shell.update(cx, |s, cx| s.save_real_notes(cx)));
        if fail {
            wait(&mut cx, &shell, |s, _| {
                s.live.as_ref().unwrap().notes[&id].error.is_some()
            })?;
            q.update(&mut cx, |_, w, cx| {
                shell.update(cx, |s, cx| {
                    s.close_quick(w, cx);
                    assert!(s.windows.quick.is_some());
                })
            })?;
            cx.update(|cx| {
                assert!(
                    editor
                        .read(cx)
                        .model
                        .document
                        .plain_text()
                        .contains("worth keeping")
                )
            });
        } else {
            wait(&mut cx, &shell, saved)?;
        }
        draw(&mut cx, q)?;
        draw(&mut cx, q)?;
        q.update(&mut cx, |_, w, cx| {
            editor.update(cx, |e, cx| {
                let range = e.selected_text_range(false, w, cx).unwrap().range;
                let caret = e.bounds_for_range(range, Bounds::default(), w, cx).unwrap();
                assert!(
                    caret.bottom() <= w.viewport_size().height - px(42.),
                    "Save feedback must leave the writing caret visible"
                );
            })
        })?;
        cx.capture_screenshot(q)?
            .save(directory.join(format!("{name}.png")))?;
        q.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
    }
    println!(
        "PASS: Quick-only launch, untouched discard, single editor handoff with selection/undo, composition/insertion guards, revision-safe first save, find rebinding, first attachment, main close/reopen, failed-save retention and dark/light/compact GPU layouts."
    );
    Ok(())
}
