//! Real GPUI event routing + SQLite. This does not certify desktop focus or IME.
use super::*;
use notes_core::document::new_id;
use std::sync::{Arc, mpsc};

fn tick(cx: &mut HeadlessAppContext, shell: &Entity<Shell>) -> anyhow::Result<()> {
    let handle = cx.update(|cx| {
        let s = shell.read(cx);
        s.windows
            .main
            .or_else(|| s.windows.quick.as_ref().map(|q| q.0))
            .unwrap()
    });
    let cleanup = handle.update(cx, |_, w, cx| {
        shell.update(cx, |s, cx| s.poll_storage(w, cx));
        w.refresh();
        w.draw(cx)
    })?;
    cx.update(|cx| cleanup.clear(cx));
    cx.run_until_parked();
    Ok(())
}
fn send(
    sender: &notes_platform::launch::Sender,
    id: &str,
    intent: Intent,
    expired: bool,
) -> mpsc::Receiver<Reply> {
    let (reply, receiver) = mpsc::sync_channel(1);
    sender
        .try_send(Request {
            id: id.into(),
            intent,
            token: None,
            expires: if expired {
                Instant::now() - Duration::from_secs(1)
            } else {
                Instant::now() + Duration::from_secs(10)
            },
            reply,
        })
        .unwrap();
    receiver
}
fn receive(
    cx: &mut HeadlessAppContext,
    shell: &Entity<Shell>,
    receiver: mpsc::Receiver<Reply>,
) -> anyhow::Result<Reply> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        tick(cx, shell)?;
        if let Ok(reply) = receiver.try_recv() {
            return Ok(reply);
        }
        anyhow::ensure!(Instant::now() < deadline, "Window request timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub fn window_launch_check() -> anyhow::Result<()> {
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
    for fail in [false, true] {
        let root = temporary
            .path()
            .join(if fail { "failure" } else { "normal" });
        let (worker, library) = notes_storage::Worker::start(root.clone(), fail)?;
        let (sender, receiver) = notes_platform::launch::channel();
        let h = cx.open_window(size(px(1000.), px(740.)), |w, cx| {
            cx.new(|cx| {
                let mut s =
                    Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx);
                s.configure_launches(receiver, Intent::Main, w, cx);
                s
            })
        })?;
        let shell = h.entity(&cx)?;
        let first = new_id();
        let response = receive(&mut cx, &shell, send(&sender, &first, Intent::New, false))?;
        assert_eq!(response.is_err(), fail);
        assert_eq!(
            receive(&mut cx, &shell, send(&sender, &first, Intent::New, false))?,
            response
        );
        assert!(receive(&mut cx, &shell, send(&sender, &first, Intent::Quick, false))?.is_err());
        assert!(receive(&mut cx, &shell, send(&sender, &new_id(), Intent::New, true))?.is_err());
        if !fail {
            let note = response.unwrap().note_id.unwrap();
            let editor = cx.update(|cx| {
                let s = shell.read(cx);
                assert_eq!(s.live.as_ref().unwrap().notes.len(), 1);
                assert!(s.live.as_ref().unwrap().notes[&note].note.revision > 0);
                s.live.as_ref().unwrap().notes[&note].editor.clone()
            });
            h.update(&mut cx, |_, w, cx| {
                editor.update(cx, |e, cx| {
                    e.replace_text_in_range(None, "A draft worth keeping", w, cx)
                })
            })?;
            tick(&mut cx, &shell)?;
            let before = cx.update(|cx| editor.read(cx).model.selection.clone());
            h.update(&mut cx, |s, w, cx| {
                s.organize(super::super::organization::Action::New, w, cx);
            })?;
            let queued = send(&sender, &new_id(), Intent::Main, false);
            tick(&mut cx, &shell)?;
            assert!(matches!(queued.try_recv(), Err(mpsc::TryRecvError::Empty)));
            cx.update(|cx| {
                let s = shell.read(cx);
                assert!(s.organization.open());
                assert_eq!(s.library.selected.as_ref(), Some(&note));
            });
            super::super::visual_keys(&mut cx, h.into(), "escape")?;
            assert!(receive(&mut cx, &shell, queued)?.is_ok());
            assert_eq!(
                receive(
                    &mut cx,
                    &shell,
                    send(&sender, &new_id(), Intent::Main, false)
                )?
                .unwrap()
                .note_id,
                Some(note.clone())
            );
            assert!(
                receive(
                    &mut cx,
                    &shell,
                    send(&sender, &new_id(), Intent::Note { id: new_id() }, false)
                )?
                .is_err()
            );
            cx.update(|cx| {
                assert_eq!(editor.read(cx).model.selection, before);
                assert_eq!(
                    editor.read(cx).model.document.plain_text(),
                    "A draft worth keeping"
                );
            });
            // No duplicate after the caller drops its reply receiver.
            let lost = new_id();
            drop(send(&sender, &lost, Intent::New, false));
            let response =
                receive(&mut cx, &shell, send(&sender, &lost, Intent::New, false))?.unwrap();
            assert_ne!(response.note_id.as_ref(), Some(&note));
            cx.update(|cx| assert_eq!(shell.read(cx).live.as_ref().unwrap().notes.len(), 2));
            let quick = receive(
                &mut cx,
                &shell,
                send(&sender, &new_id(), Intent::Quick, false),
            )?
            .unwrap();
            assert_eq!(
                receive(
                    &mut cx,
                    &shell,
                    send(&sender, &new_id(), Intent::Quick, false)
                )?
                .unwrap()
                .note_id,
                quick.note_id
            );
            assert_eq!(cx.update(|cx| cx.windows().len()), 2);
            let (q, quick_id, quick_editor) = cx.update(|cx| {
                let s = shell.read(cx);
                let (q, id) = s.windows.quick.clone().unwrap();
                (
                    q,
                    id.clone(),
                    s.live.as_ref().unwrap().notes[&id].editor.clone(),
                )
            });
            q.update(&mut cx, |_, w, cx| {
                quick_editor.update(cx, |e, cx| {
                    e.replace_and_mark_text_in_range(None, "日本", Some(2..2), w, cx)
                })
            })?;
            assert!(
                receive(
                    &mut cx,
                    &shell,
                    send(
                        &sender,
                        &new_id(),
                        Intent::Note {
                            id: quick_id.clone()
                        },
                        false
                    )
                )?
                .is_err()
            );
            assert_eq!(cx.update(|cx| cx.windows().len()), 2);
            q.update(&mut cx, |_, w, cx| {
                quick_editor.update(cx, |e, cx| e.replace_text_in_range(None, "日本語", w, cx))
            })?;
            receive(
                &mut cx,
                &shell,
                send(
                    &sender,
                    &new_id(),
                    Intent::Note {
                        id: quick_id.clone(),
                    },
                    false,
                ),
            )?
            .unwrap();
            cx.update(|cx| {
                let s = shell.read(cx);
                assert!(s.windows.quick.is_none());
                assert_eq!(s.library.selected.as_ref(), Some(&quick_id));
                assert_eq!(
                    s.live.as_ref().unwrap().notes[&quick_id].editor,
                    quick_editor
                );
            });
            // Close main while Quick keeps the controller alive. Missing targets
            // must not create a main window, then default launch must reopen it.
            receive(
                &mut cx,
                &shell,
                send(&sender, &new_id(), Intent::Quick, false),
            )?
            .unwrap();
            h.update(&mut cx, |s, w, cx| {
                // Closing a naming page must not leave an invisible form
                // blocking future launches while Quick Note owns the process.
                s.organize(super::super::organization::Action::New, w, cx);
                assert!(s.organization.open());
                s.save_real_notes(cx);
                s.request_close(w, cx);
            })?;
            let deadline = Instant::now() + Duration::from_secs(5);
            while cx.update(|cx| shell.read(cx).windows.main.is_some()) {
                tick(&mut cx, &shell)?;
                anyhow::ensure!(Instant::now() < deadline, "main close timed out");
            }
            assert!(
                receive(
                    &mut cx,
                    &shell,
                    send(&sender, &new_id(), Intent::Note { id: new_id() }, false)
                )?
                .is_err()
            );
            assert_eq!(cx.update(|cx| cx.windows().len()), 1);
            receive(
                &mut cx,
                &shell,
                send(&sender, &new_id(), Intent::Main, false),
            )?
            .unwrap();
            assert_eq!(cx.update(|cx| cx.windows().len()), 2);
        } else {
            cx.update(|cx| {
                let s = shell.read(cx);
                assert!(s.live.as_ref().unwrap().notes.is_empty());
                assert!(
                    s.live
                        .as_ref()
                        .unwrap()
                        .message
                        .as_ref()
                        .unwrap()
                        .contains("Couldn’t create")
                );
            });
        }
        cx.update(|cx| {
            for h in cx.windows() {
                h.update(cx, |_, w, cx| {
                    shell.update(cx, |s, cx| s.cancel_activation_for(h, cx));
                    w.remove_window();
                })
                .unwrap();
            }
        });
        drop(shell);
        cx.update(|cx| {
            cx.new(|_| ()).update(cx, |_, cx| cx.notify());
        });
        cx.run_until_parked();
        let deadline = Instant::now() + Duration::from_secs(5);
        let store = loop {
            match notes_storage::Store::open(&root) {
                Ok(s) => break s,
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        let notes = store.load()?.notes;
        assert_eq!(notes.len(), if fail { 0 } else { 3 });
        if !fail {
            assert!(
                notes
                    .iter()
                    .any(|n| n.body.plain_text() == "A draft worth keeping")
            );
            assert!(notes.iter().any(|n| n.body.plain_text() == "日本語"));
        }
    }
    println!(
        "PASS: committed new-note replies, replay/lost-reply deduplication, failed creation, expired requests, draft/caret preservation, one Quick window, guarded composition/handoff, main reopening and persisted notes. Native activation is checked separately."
    );
    Ok(())
}
