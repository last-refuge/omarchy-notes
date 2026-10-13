//! Real-font GPU regression for lazy editor creation and safe LRU eviction.
use super::*;
use notes_core::document::{Block, Span};
use notes_storage::media::Import;
use std::{path::Path, sync::Arc};

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
fn opened(s: &Shell) -> usize {
    s.live
        .as_ref()
        .unwrap()
        .notes
        .values()
        .filter(|n| n.editor.is_some())
        .count()
}

fn list_anchor(s: &Shell) -> (String, Pixels) {
    let offset = s.list.logical_scroll_top();
    let row = s.library.visible[offset.item_ix];
    (s.library.notes[row].id.clone(), offset.offset_in_item)
}
fn preview_content(s: &Shell, id: &str) -> (String, String, String, Vec<String>) {
    let row = s.library.notes.iter().find(|n| n.id == id).unwrap();
    (
        row.title.clone(),
        row.summary.clone(),
        row.folder.clone(),
        row.blocks.iter().map(|b| b.text().to_owned()).collect(),
    )
}
fn document_edge_workflow(
    cx: &mut HeadlessAppContext,
    h: WindowHandle<Shell>,
    id: &str,
) -> anyhow::Result<()> {
    h.update(cx, |s, w, cx| {
        s.command(Command::Select(id.to_owned()), w, cx)
    })?;
    draw(cx, h)?;
    super::super::visual_keys(cx, h.into(), "ctrl-home")?;
    h.update(cx, |s, _, cx| {
        let e = s.live.as_ref().unwrap().notes[id]
            .active_editor()
            .unwrap()
            .read(cx);
        assert_eq!(
            e.model.range(),
            0..0,
            "Control Home reaches the first document position"
        );
    })?;
    super::super::visual_keys(cx, h.into(), "ctrl-shift-end")?;
    h.update(cx, |s, _, cx| {
        let e = s.live.as_ref().unwrap().notes[id]
            .active_editor()
            .unwrap()
            .read(cx);
        assert_eq!(
            e.model.range(),
            0..e.model.text().len(),
            "Control Shift End selects through offscreen blocks"
        );
        assert_eq!(e.model.global(&e.model.selection.anchor), 0);
    })?;
    super::super::visual_keys(cx, h.into(), "ctrl-end")?;
    h.update(cx, |s, _, cx| {
        let e = s.live.as_ref().unwrap().notes[id]
            .active_editor()
            .unwrap()
            .read(cx);
        let end = e.model.text().len();
        assert_eq!(
            e.model.range(),
            end..end,
            "Control End collapses at the final document position"
        );
    })?;
    super::super::visual_keys(cx, h.into(), "ctrl-shift-home")?;
    h.update(cx, |s, _, cx| {
        let e = s.live.as_ref().unwrap().notes[id]
            .active_editor()
            .unwrap()
            .read(cx);
        assert_eq!(e.model.range(), 0..e.model.text().len());
        assert_eq!(
            e.model.global(&e.model.selection.head),
            0,
            "Control Shift Home preserves reversed selection direction"
        );
        assert_eq!(
            e.model.global(&e.model.selection.anchor),
            e.model.text().len()
        );
    })?;
    super::super::visual_keys(cx, h.into(), "ctrl-home")?;
    Ok(())
}
fn evicted_reading_workflow(
    cx: &mut HeadlessAppContext,
    h: WindowHandle<Shell>,
    ids: &[String],
) -> anyhow::Result<()> {
    use notes_core::preferences::{NoteFont, Patch};
    let reader = &ids[0];
    for hidden_typography_first in [false, true] {
        h.update(cx, |s, w, cx| {
            s.change_preferences(
                Patch {
                    note_font: Some(NoteFont::Serif),
                    note_size: Some(19),
                    sidebar_width: Some(220),
                    list_width: Some(300),
                    ..Default::default()
                },
                cx,
            );
            s.command(Command::Select(reader.clone()), w, cx);
        })?;
        draw(cx, h)?;
        draw(cx, h)?;
        h.update(cx, |s, _, cx| {
            s.live.as_ref().unwrap().notes[reader]
                .active_editor()
                .unwrap()
                .update(cx, |e, cx| {
                    let mut state = e.view_state();
                    state.scroll = point(px(0.), px(-2_000.));
                    e.restore_view_state(state, cx);
                });
        })?;
        draw(cx, h)?;
        draw(cx, h)?;
        let (before, old_entity, body) = h.update(cx, |s, _, cx| {
            let editor = s.live.as_ref().unwrap().notes[reader]
                .active_editor()
                .unwrap();
            let state = editor.read(cx).view_state();
            assert!(state.reading.is_some() && state.scroll.y < px(-1_000.));
            (
                state,
                editor.entity_id(),
                editor.read(cx).model.document.clone(),
            )
        })?;
        h.update(cx, |s, w, cx| {
            s.command(Command::Select(ids[50].clone()), w, cx)
        })?;
        draw(cx, h)?;
        h.update(cx, |s, w, cx| {
            let new_style = Patch {
                note_font: Some(NoteFont::Sans),
                note_size: Some(28),
                sidebar_width: Some(300),
                list_width: Some(440),
                ..Default::default()
            };
            if hidden_typography_first {
                assert!(s.live.as_ref().unwrap().notes[reader].editor.is_some());
                s.change_preferences(new_style.clone(), cx);
                // No draw: the hidden editor still has the old actual geometry,
                // although its new font has invalidated layout for later reflow.
            }
            for id in &ids[51..59] {
                s.command(Command::Select(id.clone()), w, cx);
            }
            assert!(s.live.as_ref().unwrap().notes[reader].editor.is_none());
            if !hidden_typography_first {
                s.change_preferences(new_style, cx);
            }
            s.command(Command::Select(reader.clone()), w, cx);
            assert_ne!(
                s.live.as_ref().unwrap().notes[reader]
                    .active_editor()
                    .unwrap()
                    .entity_id(),
                old_entity
            );
        })?;
        draw(cx, h)?;
        draw(cx, h)?;
        h.update(cx, |s, _, cx| {
            let e = s.live.as_ref().unwrap().notes[reader].active_editor().unwrap().read(cx);
            let after = e.view_state();
            let original = before.reading.as_ref().unwrap();
            let current = after.reading.as_ref().unwrap();
            assert_eq!(current.position.block, original.position.block, "the first visible paragraph survives font/size/pane-width changes after eviction; hidden-change-first={hidden_typography_first}");
            assert!((current.offset - original.offset).abs() <= px(1.), "the visible paragraph keeps its relative offset: {:?} versus {:?}", current.offset, original.offset);
            assert_ne!(after.scroll, before.scroll, "changed line geometry must adjust pixels to preserve the logical reading position");
            assert_eq!(after.selection, before.selection);
            assert_eq!(e.model.document, body);
            assert!(!e.model.can_undo(), "an evicted clean editor starts fresh undo history");
        })?;
    }
    h.update(cx, |s, _, cx| {
        s.change_preferences(
            Patch {
                note_font: Some(NoteFont::Sans),
                note_size: Some(22),
                sidebar_width: Some(248),
                list_width: Some(352),
                ..Default::default()
            },
            cx,
        )
    })?;
    draw(cx, h)?;
    Ok(())
}
fn evicted_typing_workflow(cx: &mut HeadlessAppContext, root: &Path) -> anyhow::Result<()> {
    let mut store = notes_storage::Store::open(root)?;
    let styled = Span(serde_json::json!({
        "text": "Styled words",
        "marks": ["bold", "italic"],
        "future-style": {"color": "warm", "version": 3}
    }));
    let mut block = Block::paragraph("");
    block.spans.push(styled.clone());
    let document = Document {
        blocks: vec![block],
        ..Default::default()
    };
    let inside = store.create(document.clone(), None)?.id;
    let boundary = store.create(document, None)?.id;
    let empty = store.create(Document::default(), None)?.id;
    let mut fillers = vec![];
    for index in 0..8 {
        fillers.push(
            store
                .create(
                    Document {
                        blocks: vec![Block::paragraph(&format!("Other note {index}"))],
                        ..Default::default()
                    },
                    None,
                )?
                .id,
        );
    }
    let (worker, library) = Worker::from_store(store)?;
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| {
            Shell::new_live(
                Appearance::Reference,
                root.to_owned(),
                worker,
                library,
                w,
                cx,
            )
        })
    })?;
    for (id, caret, keys, bold, unknown_style) in [
        (&inside, 3, "", true, true),
        (&boundary, "Styled words".len(), "ctrl-b", false, true),
        (&empty, 0, "ctrl-b ctrl-i", true, false),
    ] {
        h.update(cx, |s, w, cx| {
            s.command(Command::Select(id.clone()), w, cx);
            s.live.as_ref().unwrap().notes[id]
                .active_editor()
                .unwrap()
                .update(cx, |e, cx| {
                    e.model.select(caret, false);
                    cx.notify();
                });
        })?;
        draw(cx, h)?;
        super::super::visual_keys(cx, h.into(), keys)?;
        let (old_entity, state, body) = h.update(cx, |s, _, cx| {
            let session = &s.live.as_ref().unwrap().notes[id];
            let editor = session.active_editor().unwrap();
            let e = editor.read(cx);
            assert_eq!(
                e.model.document, session.note.body,
                "caret formatting does not dirty the body"
            );
            assert_eq!(session.revision, session.saved);
            assert_eq!(e.model.mark_active("bold"), bold);
            assert!(e.model.mark_active("italic"));
            (editor.entity_id(), e.view_state(), e.model.document.clone())
        })?;
        h.update(cx, |s, w, cx| {
            for id in &fillers {
                s.command(Command::Select(id.clone()), w, cx);
            }
            assert!(
                s.live.as_ref().unwrap().notes[id].editor.is_none(),
                "a saved formatting context remains evictable"
            );
            s.command(Command::Select(id.clone()), w, cx);
        })?;
        draw(cx, h)?;
        h.update(cx, |s, w, cx| {
            let editor = s.live.as_ref().unwrap().notes[id].active_editor().unwrap();
            let e = editor.read(cx);
            assert_ne!(editor.entity_id(), old_entity);
            assert_eq!(e.model.selection, state.selection);
            assert_eq!(
                e.model.typing_state(),
                state.typing,
                "reopening retains explicit typing choices"
            );
            assert_eq!(e.model.document, body);
            assert_eq!(e.model.revision, 0);
            assert!(!e.model.can_undo(), "evicted undo history remains fresh");
            editor.update(cx, |e, cx| e.replace_text_in_range(None, "X", w, cx));
        })?;
        draw(cx, h)?;
        h.update(cx, |s, _, cx| {
            let e = s.live.as_ref().unwrap().notes[id]
                .active_editor()
                .unwrap()
                .read(cx);
            let inserted = e.model.document.blocks[0]
                .spans
                .iter()
                .find(|span| span.content().contains('X'))
                .unwrap();
            assert_eq!(inserted.mark("bold"), bold);
            assert!(inserted.mark("italic"));
            if unknown_style {
                assert_eq!(inserted.0["future-style"], styled.0["future-style"]);
            }
            assert_eq!(
                e.model.text(),
                format!(
                    "{}X{}",
                    &body.plain_text()[..caret],
                    &body.plain_text()[caret..]
                )
            );
            s.save_real_notes(cx);
        })?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let saved = h.update(cx, |s, w, cx| {
                s.poll_storage(w, cx);
                let session = &s.live.as_ref().unwrap().notes[id];
                session.pending.is_none() && session.revision == session.saved
            })?;
            if saved {
                break;
            }
            anyhow::ensure!(Instant::now() < deadline, "styled typing did not save");
            draw(cx, h)?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    h.update(cx, |s, w, cx| {
        s.request_close(w, cx);
        assert!(s.windows.main.is_none());
    })?;
    cx.run_until_parked();
    Ok(())
}
fn incremental_preview_workflow(
    cx: &mut HeadlessAppContext,
    h: WindowHandle<Shell>,
    ids: &[String],
) -> anyhow::Result<()> {
    let edited = &ids[121];
    let unrelated = &ids[122];
    h.update(cx, |s, w, cx| {
        s.command(Command::Select(edited.clone()), w, cx);
        s.list.scroll_to(ListOffset {
            item_ix: 120,
            offset_in_item: px(17.),
        });
    })?;
    draw(cx, h)?;
    let (anchor, visible, old_other) = h.update(cx, |s, _, _| {
        let anchor = list_anchor(s);
        assert!(s.list.logical_scroll_top().item_ix > 0);
        (
            anchor,
            s.library.visible.clone(),
            preview_content(s, unrelated),
        )
    })?;
    h.update(cx, |s, w, cx| {
        let editor = s.live.as_ref().unwrap().notes[edited]
            .active_editor()
            .unwrap()
            .clone();
        editor.update(cx, |e, cx| {
            e.edit(
                |m| {
                    m.select(0, false);
                    m.insert("A fresh title ", 0)?;
                    m.select(m.text().len(), false);
                    m.enter(false)?;
                    m.insert(
                        "AnchorSearchNeedle lives only in this new body paragraph.",
                        0,
                    )
                },
                w,
                cx,
            )
        });
    })?;
    draw(cx, h)?;
    h.update(cx, |s, _, _| {
        assert_eq!(
            s.library.visible, visible,
            "typing does not change edited-date ordering before commit"
        );
        assert_eq!(
            list_anchor(s),
            anchor,
            "typing preserves the offscreen list anchor and pixel offset"
        );
        assert_eq!(
            preview_content(s, unrelated),
            old_other,
            "unrelated preview content stays intact"
        );
        assert!(
            s.library
                .notes
                .iter()
                .find(|n| &n.id == edited)
                .unwrap()
                .title
                .starts_with("A fresh title")
        );
    })?;
    // Search routes use the freshly derived title/body while the note remains open.
    for query in ["A fresh title", "AnchorSearchNeedle"] {
        h.update(cx, |s, w, cx| {
            s.search.update(cx, |f, cx| {
                f.select_all(w, cx);
                f.replace_text_in_range(None, query, w, cx);
            });
        })?;
        draw(cx, h)?;
        h.update(cx, |s, _, _| {
            assert_eq!(s.library.visible.len(), 1);
            assert_eq!(&s.library.notes[s.library.visible[0]].id, edited);
            assert_eq!(
                s.library.selected.as_ref(),
                Some(edited),
                "search keeps the current draft selected"
            );
        })?;
    }
    h.update(cx, |s, w, cx| {
        s.search.update(cx, |f, cx| f.clear(w, cx));
    })?;
    draw(cx, h)?;
    h.update(cx, |s, _, cx| {
        assert_eq!(s.library.visible.len(), 1_000);
        s.change_preferences(
            notes_core::preferences::Patch {
                sort_by_title: Some(true),
                ..Default::default()
            },
            cx,
        );
        s.list.scroll_to(ListOffset {
            item_ix: 120,
            offset_in_item: px(17.),
        });
    })?;
    draw(cx, h)?;
    let (anchor, old_index) = h.update(cx, |s, _, _| {
        assert_eq!(s.list.logical_scroll_top().item_ix, 120);
        assert_eq!(s.list.logical_scroll_top().offset_in_item, px(17.));
        (list_anchor(s), s.list.logical_scroll_top().item_ix)
    })?;
    h.update(cx, |s, w, cx| {
        s.live.as_ref().unwrap().notes[edited]
            .active_editor()
            .unwrap()
            .update(cx, |e, cx| {
                e.edit(
                    |m| {
                        let end = m.document.blocks[0].plain_text().len();
                        m.select(0, false);
                        m.select(end, true);
                        m.insert("ZZZ moved title", 0)
                    },
                    w,
                    cx,
                )
            });
    })?;
    h.update(cx, |s, _, _| assert_eq!(list_anchor(s), anchor))?;
    draw(cx, h)?;
    h.update(cx, |s, _, _| {
        assert_eq!(
            list_anchor(s),
            anchor,
            "title reordering retains the same note anchor by identity"
        );
        assert_ne!(
            s.list.logical_scroll_top().item_ix,
            old_index,
            "the anchor actually changed list index"
        );
        assert_eq!(preview_content(s, unrelated), old_other);
    })?;
    h.update(cx, |s, _, cx| {
        s.change_preferences(
            notes_core::preferences::Patch {
                sort_by_title: Some(false),
                ..Default::default()
            },
            cx,
        );
    })?;
    draw(cx, h)?;
    h.update(cx, |s, w, cx| {
        s.save_real_notes(cx);
        s.poll_storage(w, cx);
    })?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let saved = h.update(cx, |s, w, cx| {
            s.poll_storage(w, cx);
            let session = &s.live.as_ref().unwrap().notes[edited];
            session.pending.is_none() && session.revision == session.saved
        })?;
        if saved {
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "preview workflow did not save");
        draw(cx, h)?;
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

/// Two real writer acknowledgments arrive together. Updating the pinned first
/// row must not hide a reorder among other notes before their row callback runs.
fn batch_save_anchor_workflow(
    cx: &mut HeadlessAppContext,
    h: WindowHandle<Shell>,
    pinned: &str,
) -> anyhow::Result<()> {
    h.update(cx, |s, w, cx| {
        s.command(Command::Select(pinned.to_owned()), w, cx);
        if !s.live.as_ref().unwrap().notes[pinned].note.pinned {
            s.command(Command::Pin, w, cx);
        }
        s.save_real_notes(cx);
        s.poll_storage(w, cx);
    })?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let saved = h.update(cx, |s, w, cx| {
            s.poll_storage(w, cx);
            let note = &s.live.as_ref().unwrap().notes[pinned];
            note.pending.is_none() && note.revision == note.saved
        })?;
        if saved {
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "pinned fixture did not save");
        draw(cx, h)?;
        std::thread::sleep(Duration::from_millis(5));
    }
    draw(cx, h)?;
    let (anchor_id, other) = h.update(cx, |s, _, _| {
        assert_eq!(s.library.notes[s.library.visible[0]].id, pinned);
        let a = s.library.notes[s.library.visible[120]].id.clone();
        let b = s.library.notes[s.library.visible[130]].id.clone();
        assert_ne!(a, b);
        (a, b)
    })?;
    for id in [pinned, other.as_str()] {
        h.update(cx, |s, w, cx| {
            s.command(Command::Select(id.to_owned()), w, cx);
            s.live.as_ref().unwrap().notes[id]
                .active_editor()
                .unwrap()
                .update(cx, |e, cx| {
                    e.edit(
                        |m| {
                            m.select(m.text().len(), false);
                            m.insert(" A batch edit to keep.", 0)
                        },
                        w,
                        cx,
                    )
                });
        })?;
    }
    // Measure B while it is an ordinary unpinned row. It will become the first
    // unpinned row and gain a group heading after its save acknowledgment.
    h.update(cx, |s, _, _| {
        s.list.scroll_to(ListOffset {
            item_ix: 130,
            offset_in_item: px(0.),
        })
    })?;
    draw(cx, h)?;
    let old_height = h.update(cx, |s, _, _| {
        s.list.bounds_for_item(130).unwrap().size.height
    })?;
    h.update(cx, |s, _, _| {
        s.list.scroll_to(ListOffset {
            item_ix: 120,
            offset_in_item: px(17.),
        })
    })?;
    draw(cx, h)?;
    let anchor = h.update(cx, |s, _, _| {
        assert_eq!(list_anchor(s), (anchor_id.clone(), px(17.)));
        let note = &s.live.as_ref().unwrap().notes[&other];
        assert!(note.pending.is_none() && note.revision != note.saved);
        list_anchor(s)
    })?;
    // Wait only for the clock to advance past the fixture's newest timestamp;
    // the assertions do not depend on how long either SQLite save takes.
    let newest = h.update(cx, |s, _, _| {
        s.live
            .as_ref()
            .unwrap()
            .notes
            .values()
            .map(|s| s.note.updated_at)
            .max()
            .unwrap()
    })?;
    while notes_storage::now() <= newest {
        std::thread::sleep(Duration::from_millis(1));
    }
    let fence = h.update(cx, |s, _, _| {
        let live = s.live.as_mut().unwrap();
        for id in [pinned, other.as_str()] {
            live.operation += 1;
            let operation = live.operation;
            let session = live.notes.get_mut(id).unwrap();
            let revision = session.revision;
            live.worker
                .requests
                .try_send(Request::Save {
                    note: session.note.clone(),
                    local_revision: revision,
                    operation,
                })
                .unwrap();
            session.pending = Some((operation, revision));
        }
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        live.worker
            .requests
            .try_send(Request::CaptureStatus {
                request_id: notes_core::document::new_id(),
                reply,
            })
            .unwrap();
        receive
    })?;
    // FIFO status reply proves both Saved events are already queued. Do not pump
    // the GUI executor between enqueueing the saves and applying this batch.
    assert!(
        fence
            .recv_timeout(Duration::from_secs(5))?
            .unwrap()
            .is_none()
    );
    h.update(cx, |s, w, cx| {
        s.poll_storage(w, cx);
        assert_eq!(s.library.notes[s.library.visible[0]].id, pinned, "P remains pinned at index zero");
        assert_eq!(s.library.notes[s.library.visible[1]].id, other, "B moved ahead of A in the same acknowledgment batch");
        assert_eq!(list_anchor(s), anchor, "the actual A identity and pixel offset survive a batch where P's own position did not change");
        assert_eq!(s.list.logical_scroll_top().item_ix, 121);
        for id in [pinned, other.as_str()] {
            let session = &s.live.as_ref().unwrap().notes[id];
            assert!(session.pending.is_none() && session.revision == session.saved);
        }
    })?;
    draw(cx, h)?;
    h.update(cx, |s, _, _| assert_eq!(list_anchor(s), anchor))?;
    h.update(cx, |s, _, _| {
        s.list.scroll_to(ListOffset {
            item_ix: 1,
            offset_in_item: px(0.),
        })
    })?;
    draw(cx, h)?;
    h.update(cx, |s, _, _| {
        let height = s.list.bounds_for_item(1).unwrap().size.height;
        assert!(height > old_height + px(16.), "the newly first unpinned row is measured with its group heading: {height:?} versus {old_height:?}");
    })?;
    Ok(())
}

/// A user edit and a close/export command can share a GPUI update before the
/// Changed subscription runs. Exercise the actual barriers and durable writer.
fn queued_edit_workflow(cx: &mut HeadlessAppContext, root: &Path) -> anyhow::Result<()> {
    let mut store = notes_storage::Store::open(root)?;
    let note = store.create(
        Document {
            blocks: vec![Block::paragraph("A thought to preserve")],
            ..Default::default()
        },
        None,
    )?;
    let (worker, library) = Worker::from_store(store)?;
    let h = cx.open_window(size(px(1024.), px(768.)), |w, cx| {
        cx.new(|cx| {
            Shell::new_live(
                Appearance::Reference,
                root.to_owned(),
                worker,
                library,
                w,
                cx,
            )
        })
    })?;
    draw(cx, h)?;
    h.update(cx, |s, w, cx| {
        let editor = s.live.as_ref().unwrap().notes[&note.id]
            .active_editor()
            .unwrap()
            .clone();
        editor.update(cx, |e, cx| e.edit(|m| m.insert("Latest draft. ", 0), w, cx));
        assert!(
            !s.can_close_recovered(cx),
            "the old saved revision is not recovery for the latest edit"
        );
        s.command(Command::CloseRecovered, w, cx);
        assert!(
            s.windows.main.is_some(),
            "recovery-close must retain the latest draft"
        );
        s.request_close(w, cx);
        assert!(
            s.windows.main.is_some(),
            "ordinary close must wait for the latest draft"
        );
        s.command(Command::ExportRecovery, w, cx);
        assert!(
            s.live
                .as_ref()
                .unwrap()
                .message
                .as_deref()
                .unwrap()
                .contains("latest edit")
        );
        s.command(Command::Recover, w, cx);
        assert!(
            s.live.as_ref().unwrap().creating.is_none(),
            "a copy cannot use the old body"
        );
        s.command(Command::PdfPanel, w, cx);
        s.command(Command::PdfExport, w, cx);
        assert!(
            !s.live.as_ref().unwrap().pdf.busy(),
            "an export cannot publish a mislabeled revision"
        );
        assert!(
            s.live
                .as_ref()
                .unwrap()
                .pdf
                .message
                .as_deref()
                .unwrap()
                .contains("latest edit")
        );
        s.command(Command::PdfCancel, w, cx);
        s.save_real_notes(cx);
        s.poll_storage(w, cx);
        assert!(
            s.windows.main.is_some(),
            "same-update poll cannot acknowledge the old draft"
        );
        assert!(
            s.live.as_ref().unwrap().notes[&note.id].pending.is_none(),
            "the writer must wait for Changed"
        );
    })?;
    draw(cx, h)?;
    h.update(cx, |s, w, cx| {
        assert!(
            s.live.as_ref().unwrap().notes[&note.id]
                .note
                .body
                .plain_text()
                .contains("Latest draft")
        );
        s.save_real_notes(cx);
        s.poll_storage(w, cx);
    })?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let saved = h.update(cx, |s, w, cx| {
            s.poll_storage(w, cx);
            let session = &s.live.as_ref().unwrap().notes[&note.id];
            session.pending.is_none() && session.revision == session.saved
        })?;
        if saved {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "latest draft did not save after Changed settled"
        );
        draw(cx, h)?;
        std::thread::sleep(Duration::from_millis(5));
    }
    h.update(cx, |s, w, cx| {
        assert!(s.can_close_recovered(cx));
        s.request_close(w, cx);
        assert!(
            s.windows.main.is_none(),
            "close completes only after the latest write commits"
        );
    })?;
    cx.run_until_parked();
    let deadline = Instant::now() + Duration::from_secs(5);
    let store = loop {
        match notes_storage::Store::open(root) {
            Ok(store) => break store,
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    let reopened = store.load()?;
    assert_eq!(reopened.notes.len(), 1);
    assert!(reopened.notes[0].body.plain_text().contains("Latest draft"));
    Ok(())
}

pub fn bounded_editor_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let scratch = tempfile::tempdir()?;
    let root = scratch.path().join("library");
    let mut store = notes_storage::Store::open(&root)?;
    let mut ids = vec![];
    for index in 0..1_000 {
        let mut body = Document {
            blocks: vec![Block::paragraph(&format!("Thought {index:04}"))],
            ..Default::default()
        };
        if index == 0 {
            for paragraph in 0..80 {
                body.blocks.push(Block::paragraph(&format!(
                    "Paragraph {paragraph}. Keep the words and the reading position close."
                )));
            }
        }
        ids.push(store.create(body, None)?.id);
    }
    let (worker, library) = Worker::from_store(store)?;
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
    let h = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(s.live.as_ref().unwrap().notes.len(), 1_000);
        assert_eq!(opened(s), 1, "startup creates only the visible editor");
    })?;
    draw(&mut cx, h)?;
    document_edge_workflow(&mut cx, h, &ids[0])?;
    h.update(&mut cx, |s, w, cx| {
        s.command(Command::Select(ids[0].clone()), w, cx)
    })?;
    draw(&mut cx, h)?;
    let (old_entity, state) = h.update(&mut cx, |s, _, cx| {
        let editor = s.live.as_ref().unwrap().notes[&ids[0]]
            .active_editor()
            .unwrap();
        let state = editor.update(cx, |e, cx| {
            e.model.select(20, false);
            e.model.select(38, true);
            let mut state = e.view_state();
            state.scroll = point(px(0.), px(-450.));
            e.restore_view_state(state.clone(), cx);
            state
        });
        (editor.entity_id(), state)
    })?;
    draw(&mut cx, h)?;
    let state = h.update(&mut cx, |s, _, cx| {
        let current = s.live.as_ref().unwrap().notes[&ids[0]]
            .active_editor()
            .unwrap()
            .read(cx)
            .view_state();
        assert_eq!(current.selection, state.selection);
        assert!(
            current.scroll.y < px(-100.),
            "the long note has a reading offset"
        );
        current
    })?;
    for id in &ids[1..18] {
        h.update(&mut cx, |s, w, cx| {
            s.command(Command::Select(id.clone()), w, cx);
            assert!(opened(s) <= IDLE_EDITOR_LIMIT + 1);
        })?;
    }
    h.update(&mut cx, |s, w, cx| {
        assert!(s.live.as_ref().unwrap().notes[&ids[0]].editor.is_none());
        s.command(Command::Appearance(Appearance::Light), w, cx);
        s.change_preferences(
            notes_core::preferences::Patch {
                note_size: Some(22),
                note_font: Some(notes_core::preferences::NoteFont::Sans),
                ..Default::default()
            },
            cx,
        );
        s.command(Command::Select(ids[0].clone()), w, cx);
        let editor = s.live.as_ref().unwrap().notes[&ids[0]]
            .active_editor()
            .unwrap();
        assert_ne!(
            editor.entity_id(),
            old_entity,
            "an evicted editor is recreated"
        );
        assert_eq!(editor.read(cx).view_state(), state);
        assert_eq!(editor.read(cx).typography.size, 22);
        assert_eq!(
            editor.read(cx).typography.font,
            notes_core::preferences::NoteFont::Sans
        );
        assert_eq!(editor.read(cx).theme, s.theme);
    })?;
    draw(&mut cx, h)?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("reopened-reading-position.png"))?;
    evicted_reading_workflow(&mut cx, h, &ids)?;
    incremental_preview_workflow(&mut cx, h, &ids)?;
    batch_save_anchor_workflow(&mut cx, h, &ids[121])?;

    // Dirty, pending-save, failed, composing and importing sessions are exempt.
    h.update(&mut cx, |s, w, cx| {
        let dirty = ids[20].clone();
        s.command(Command::Select(dirty.clone()), w, cx);
        let editor = s.live.as_ref().unwrap().notes[&dirty]
            .active_editor()
            .unwrap()
            .clone();
        editor.update(cx, |e, cx| {
            e.edit(|m| m.insert("Keep this unsaved thought. ", 0), w, cx)
        });
        let dirty_entity = editor.entity_id();
        let pending = ids[21].clone();
        s.command(Command::Select(pending.clone()), w, cx);
        // A queued revision is protected even before its acknowledgment arrives.
        s.live
            .as_mut()
            .unwrap()
            .notes
            .get_mut(&pending)
            .unwrap()
            .pending = Some((999, 0));
        let failed = ids[22].clone();
        s.command(Command::Select(failed.clone()), w, cx);
        s.live
            .as_mut()
            .unwrap()
            .notes
            .get_mut(&failed)
            .unwrap()
            .error = Some("Injected storage failure".into());
        let composing = ids[23].clone();
        s.command(Command::Select(composing.clone()), w, cx);
        s.live.as_ref().unwrap().notes[&composing]
            .active_editor()
            .unwrap()
            .update(cx, |e, cx| {
                e.replace_and_mark_text_in_range(None, "日本", Some(2..2), w, cx)
            });
        let importing = ids[24].clone();
        s.command(Command::Select(importing.clone()), w, cx);
        s.live.as_ref().unwrap().notes[&importing]
            .active_editor()
            .unwrap()
            .update(cx, |e, cx| {
                e.queue_import(
                    Import::Files {
                        paths: vec![root.join("missing.png")],
                        images: None,
                    },
                    cx,
                )
            });
        assert!(s.live.as_ref().unwrap().notes[&importing].has_pending_work(cx));
        for id in &ids[30..60] {
            s.command(Command::Select(id.clone()), w, cx);
        }
        let live = s.live.as_ref().unwrap();
        for (kind, id) in [
            ("dirty", &dirty),
            ("pending", &pending),
            ("failed", &failed),
            ("composing", &composing),
            ("importing", &importing),
        ] {
            assert!(
                live.notes[id].editor.is_some(),
                "unsafe {kind} session {id} must survive eviction"
            );
        }
        assert_eq!(
            live.notes[&dirty].active_editor().unwrap().entity_id(),
            dirty_entity
        );
        assert!(
            live.notes[&dirty]
                .active_editor()
                .unwrap()
                .read(cx)
                .model
                .committed()
                .plain_text()
                .contains("Keep this unsaved thought")
        );
        assert!(live.notes[&composing].input_is_composing(cx));
        assert!(opened(s) <= IDLE_EDITOR_LIMIT + 1 + 5);
        // Synthetic queued/failure states have no worker request to settle.
        s.live
            .as_mut()
            .unwrap()
            .notes
            .get_mut(&pending)
            .unwrap()
            .pending = None;
        s.live
            .as_mut()
            .unwrap()
            .notes
            .get_mut(&failed)
            .unwrap()
            .error = None;
    })?;
    draw(&mut cx, h)?;
    h.update(&mut cx, |s, _, _| {
        assert!(
            s.live.as_ref().unwrap().notes[&ids[20]]
                .note
                .body
                .plain_text()
                .contains("Keep this unsaved thought")
        );
    })?;

    // Background capture metadata must not create an editor or steal selection.
    h.update(&mut cx, |s, w, cx| {
        let count = opened(s);
        let selected = s.library.selected.clone();
        let captured = notes_storage::Note {
            id: notes_core::document::new_id(),
            folder: None,
            body: Document::default(),
            revision: 1,
            created_at: 1,
            updated_at: 1,
            pinned: false,
        };
        let id = captured.id.clone();
        s.attach_note(captured, w, cx);
        assert!(s.live.as_ref().unwrap().notes[&id].editor.is_none());
        assert_eq!(opened(s), count);
        assert_eq!(s.library.selected, selected);
    })?;
    h.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    queued_edit_workflow(&mut cx, &scratch.path().join("queued-edit-library"))?;
    evicted_typing_workflow(&mut cx, &scratch.path().join("typing-library"))?;
    println!(
        "Passed 1,000-note lazy startup, four-idle-editor LRU bound, recreated selection/scroll/typography/typing context, inherited and explicit empty/boundary formatting after eviction, incremental previews with stable list anchors and fresh title/body search, dirty/pending/error/composition/import protection, and same-update edit/close/copy/export barriers followed by durable save/close/reopen. Synthetic fault states and composition adapter checks do not establish real IME or crash-safety evidence. Images: {}",
        directory.display()
    );
    Ok(())
}
