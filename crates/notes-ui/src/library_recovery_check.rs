//! One real-writer recovery workflow, including edits arriving during deletion.
use super::*;
use anyhow::Context as _;
use notes_core::document::{Block, Document};
use notes_core::organization::{Change, Mutation as FolderMutation};
use std::{collections::HashSet, path::Path, sync::Arc};

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
fn settle(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        draw(cx, h.into())?;
        let busy = h.update(cx, |s, w, cx| {
            s.poll_storage(w, cx);
            if let Some(error) = &s.library_recovery.error {
                assert!(
                    s.live
                        .as_ref()
                        .unwrap()
                        .notes
                        .values()
                        .any(|n| n.recovery_blocked),
                    "{error}"
                );
            }
            s.library_recovery.busy()
                || s.query.loading
                || s.live.as_ref().unwrap().creating.is_some()
        })?;
        if !busy {
            draw(cx, h.into())?;
            return Ok(());
        }
        anyhow::ensure!(Instant::now() < deadline, "Recovery workflow timed out");
        std::thread::sleep(Duration::from_millis(3));
    }
}
fn action(
    cx: &mut HeadlessAppContext,
    h: WindowHandle<Shell>,
    action: Action,
) -> anyhow::Result<()> {
    h.update(cx, |s, w, cx| s.library_recovery_action(action, w, cx))?;
    settle(cx, h)
}
fn select(cx: &mut HeadlessAppContext, h: WindowHandle<Shell>, id: &str) -> anyhow::Result<()> {
    h.update(cx, |s, w, cx| {
        s.command(Command::Select(id.to_owned()), w, cx)
    })?;
    draw(cx, h.into())
}
fn store(root: &Path) -> anyhow::Result<notes_storage::Store> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match notes_storage::Store::open(root) {
            Ok(store) => return Ok(store),
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => std::thread::sleep(Duration::from_millis(3)),
        }
    }
}
fn folder(store: &mut notes_storage::Store, name: &str, parent: Option<String>) -> String {
    let Change::Folder(folder) = store
        .organize(FolderMutation::CreateFolder {
            name: name.into(),
            parent,
        })
        .unwrap()
    else {
        panic!("folder receipt")
    };
    folder.id
}
fn ids(doc: &Document) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut blocks: Vec<_> = doc.blocks.iter().collect();
    while let Some(block) = blocks.pop() {
        assert!(ids.insert(block.id.clone()));
        blocks.extend(block.cells.iter().flat_map(|cell| &cell.blocks));
    }
    ids
}

pub fn library_recovery_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("library");
    let pixel_path = temp.path().join("pixel.png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([20, 40, 60, 255])).save(&pixel_path)?;
    let pixel = std::fs::read(&pixel_path)?;
    let pdf = temp.path().join("report.pdf");
    std::fs::write(&pdf, b"%PDF-private-recovery-fixture")?;
    let mut initial = store(&root)?;
    let parent = folder(&mut initial, "Work", None);
    let child = folder(&mut initial, "Clients", Some(parent.clone()));
    let mut original = initial.create(Document::default(), Some(child.clone()))?;
    original.body = initial.import_media(
        &original.id,
        notes_storage::media::Import::Files {
            paths: vec![pixel_path, pdf],
            images: None,
        },
    )?;
    original
        .body
        .blocks
        .insert(0, Block::paragraph("Original notebook"));
    original
        .body
        .extra
        .insert("future-recovery".into(), serde_json::json!({"keep":true}));
    initial
        .save(&original)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let spare = initial.create(
        Document::from_blocks(vec![Block::paragraph("Spare notebook")]),
        None,
    )?;
    drop(initial);
    let (worker, library) = notes_storage::Worker::from_store(store(&root)?)?;
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
    settle(&mut cx, h)?;
    select(&mut cx, h, &original.id)?;
    // Request duplication in the same update as an edit. The source mirror has
    // not yet observed that editor event; the real save barrier must catch up.
    h.update(&mut cx, |s, w, cx| {
        let editor = s.live.as_ref().unwrap().notes[&original.id]
            .active_editor()
            .unwrap();
        editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, "Reviewed draft. ", w, cx)
        });
        s.library_recovery_action(Action::Duplicate, w, cx);
        assert!(
            s.library_recovery
                .pending
                .as_ref()
                .unwrap()
                .operation
                .is_none()
        );
    })?;
    settle(&mut cx, h)?;
    let (duplicate, original_body) = h.update(&mut cx, |s, _, _| {
        let live = s.live.as_ref().unwrap();
        let duplicate = live
            .notes
            .values()
            .find(|n| n.note.id != original.id && n.note.id != spare.id)
            .unwrap()
            .note
            .clone();
        let body = live.notes[&original.id].note.body.clone();
        assert_eq!(duplicate.body.plain_text(), body.plain_text());
        assert!(
            ids(&duplicate.body).is_disjoint(&ids(&body)),
            "Duplicate identities must be fresh"
        );
        assert_eq!(duplicate.body.extra, body.extra);
        assert_eq!(duplicate.body.references("image"), body.references("image"));
        assert_ne!(
            duplicate.body.references("attachment"),
            body.references("attachment")
        );
        (duplicate, body)
    })?;
    let image_hash = duplicate
        .body
        .references("image")
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(notes_storage::media::read_blob(&root, &image_hash)?, pixel);
    let duplicate_attachment = duplicate
        .body
        .references("attachment")
        .into_iter()
        .next()
        .unwrap();
    let info = notes_storage::media::attachment_info(&root, &duplicate_attachment)?;
    assert_eq!(
        notes_storage::media::read_blob(&root, &info.hash)?,
        b"%PDF-private-recovery-fixture"
    );
    h.update(&mut cx, |s, w, cx| {
        s.library_recovery_action(Action::ToggleActions, w, cx)
    })?;
    draw(&mut cx, h.into())?;
    h.update(&mut cx, |s, w, _| {
        assert!(s.library_recovery.actions_open);
        assert!(s.library_recovery.focus[2].is_focused(w));
    })?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("recovery-actions.png"))?;
    super::super::visual_keys(&mut cx, h.into(), "escape")?;
    assert!(!h.update(&mut cx, |s, _, _| s.library_recovery.actions_open)?);

    select(&mut cx, h, &duplicate.id)?;
    action(&mut cx, h, Action::TrashNote)?;
    assert!(!h.update(&mut cx, |s, _, _| {
        s.live.as_ref().unwrap().notes.contains_key(&duplicate.id)
    })?);
    assert!(h.update(&mut cx, |s, _, _| s.library_recovery.undo.is_some())?);
    action(&mut cx, h, Action::UndoTrash)?;
    assert_eq!(
        h.update(&mut cx, |s, _, _| s.live.as_ref().unwrap().notes
            [&duplicate.id]
            .note
            .body
            .clone())?,
        duplicate.body
    );
    select(&mut cx, h, &duplicate.id)?;
    action(&mut cx, h, Action::TrashNote)?;
    action(&mut cx, h, Action::OpenTrash)?;
    action(&mut cx, h, Action::SelectTrash(duplicate.id.clone()))?;
    h.update(&mut cx, |s, w, _| {
        assert_eq!(
            s.library_recovery.preview.as_ref().unwrap().note.body,
            duplicate.body
        );
        assert!(s.library_recovery.focus[1].is_focused(w));
        assert!(
            !s.live.as_ref().unwrap().notes.contains_key(&duplicate.id),
            "Preview cannot mount an editable deleted note"
        );
    })?;
    super::super::visual_keys(&mut cx, h.into(), "home end")?;
    settle(&mut cx, h)?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("recovery-preview.png"))?;
    action(&mut cx, h, Action::RestoreSelected)?;
    assert!(h.update(&mut cx, |s, _, _| {
        s.live.as_ref().unwrap().notes.contains_key(&duplicate.id)
    })?);
    action(&mut cx, h, Action::CloseTrash)?;

    action(&mut cx, h, Action::DeleteFolder(parent.clone()))?;
    h.update(&mut cx, |s, w, _| {
        let confirm = s.library_recovery.confirm.as_ref().unwrap();
        assert!(confirm.detail.contains("does not recreate"));
        assert!(
            s.library_recovery.focus[6].is_focused(w),
            "Cancellation is the initial focus"
        );
    })?;
    draw(&mut cx, h.into())?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("recovery-folder-confirm.png"))?;
    action(&mut cx, h, Action::Confirm)?;
    h.update(&mut cx, |s, _, _| {
        assert!(
            !s.live
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .any(|f| f.id == parent || f.id == child)
        );
        assert!(!s.live.as_ref().unwrap().notes.contains_key(&original.id));
    })?;
    action(&mut cx, h, Action::UndoTrash)?;
    h.update(&mut cx, |s, _, _| {
        assert_eq!(
            s.live.as_ref().unwrap().notes[&original.id].note.body,
            original_body
        );
        assert!(
            s.live.as_ref().unwrap().notes[&original.id]
                .note
                .folder
                .is_none()
        );
        assert!(
            !s.live
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .any(|f| f.id == parent || f.id == child)
        );
    })?;

    select(&mut cx, h, &duplicate.id)?;
    action(&mut cx, h, Action::TrashNote)?;
    action(&mut cx, h, Action::OpenTrash)?;
    action(&mut cx, h, Action::SelectTrash(duplicate.id.clone()))?;
    action(&mut cx, h, Action::RequestPermanent)?;
    h.update(&mut cx, |s, w, _| {
        assert!(
            s.library_recovery
                .confirm
                .as_ref()
                .unwrap()
                .detail
                .contains("cannot be undone")
        );
        assert!(s.library_recovery.focus[6].is_focused(w));
    })?;
    draw(&mut cx, h.into())?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("recovery-permanent-confirm.png"))?;
    super::super::visual_keys(&mut cx, h.into(), "escape")?;
    assert!(h.update(&mut cx, |s, _, _| s.library_recovery.confirm.is_none())?);
    action(&mut cx, h, Action::RequestPermanent)?;
    action(&mut cx, h, Action::Confirm)?;
    assert!(!h.update(&mut cx, |s, _, _| {
        s.library_recovery.rows.iter().any(|r| r.id == duplicate.id)
    })?);
    assert_eq!(
        notes_storage::media::read_blob(&root, &image_hash)?,
        pixel,
        "Live original still retains its image"
    );
    action(&mut cx, h, Action::CloseTrash)?;

    // The same Quick editor is promoted to Main. A real deletion request goes
    // to the writer, then a newer Quick edit arrives before its receipt.
    eprintln!("Recovery workflow stage: shared Main/Quick draft");
    h.update(&mut cx, |s, _, cx| s.open_quick(cx))
        .context("open shared Quick")?;
    cx.run_until_parked();
    let (quick, quick_id, editor) = h.update(&mut cx, |s, _, _| {
        let (window, id) = s.windows.quick.clone().unwrap();
        (
            window,
            id.clone(),
            s.live.as_ref().unwrap().notes[&id]
                .active_editor()
                .unwrap()
                .clone(),
        )
    })?;
    draw(&mut cx, quick).context("draw initial Quick")?;
    quick.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, "Shared saved draft.", w, cx)
        })
    })?;
    draw(&mut cx, quick).context("draw saved Quick draft")?;
    h.update(&mut cx, |s, _, cx| {
        s.save_real_notes(cx);
    })?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        settle(&mut cx, h)?;
        let saved = h.update(&mut cx, |s, _, cx| {
            let note = &s.live.as_ref().unwrap().notes[&quick_id];
            note.pending.is_none() && note.revision == note.saved && !note.has_unobserved_edit(cx)
        })?;
        if saved {
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "Shared draft save timed out");
    }
    // Explicit Select promotes Quick into Main and closes its small window.
    // Real list navigation reads the same note while leaving Quick available.
    let count = h.update(&mut cx, |s, w, cx| {
        s.focus.focus(w, cx);
        s.library.visible.len()
    })?;
    for _ in 0..count {
        super::super::visual_keys(&mut cx, h.into(), "up")?;
    }
    for _ in 0..=count {
        if h.update(&mut cx, |s, _, _| {
            s.library.selected.as_ref() == Some(&quick_id)
        })? {
            break;
        }
        super::super::visual_keys(&mut cx, h.into(), "down")?;
    }
    eprintln!("Recovery workflow stage: submit shared deletion before newer input");
    h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.library.selected.as_ref(), Some(&quick_id));
        s.library_recovery_action(Action::TrashNote, w, cx);
        assert!(
            s.library_recovery
                .pending
                .as_ref()
                .unwrap()
                .operation
                .is_some(),
            "Writer request must precede newer input"
        );
    })?;
    quick.update(&mut cx, |_, w, cx| {
        editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, " Newer held draft.", w, cx)
        })
    })?;
    draw(&mut cx, quick).context("draw newer Quick before deletion receipt")?;
    settle(&mut cx, h)?;
    let held = h.update(&mut cx, |s, _, cx| {
        let note = &s.live.as_ref().unwrap().notes[&quick_id];
        assert!(note.recovery_blocked && note.error.is_some());
        assert!(
            s.windows.quick.is_some(),
            "A newer Quick draft must retain its window"
        );
        assert!(
            s.library_recovery
                .retained_drafts
                .iter()
                .any(|(id, _)| id == &quick_id)
        );
        note.active_editor()
            .unwrap()
            .read(cx)
            .model
            .document
            .clone()
    })?;
    select(&mut cx, h, &spare.id)?;
    action(&mut cx, h, Action::ViewRetainedDraft(quick_id.clone()))?;
    assert_eq!(
        h.update(&mut cx, |s, _, _| s.library.selected.clone())?,
        Some(quick_id.clone())
    );
    h.update(&mut cx, |s, w, cx| s.command(Command::Save, w, cx))?;
    assert!(h.update(&mut cx, |s, _, _| {
        s.live.as_ref().unwrap().notes[&quick_id].recovery_blocked
    })?);
    eprintln!("Recovery workflow stage: restore newer held draft");
    action(&mut cx, h, Action::UndoTrash)?;
    h.update(&mut cx, |s, _, cx| {
        let note = &s.live.as_ref().unwrap().notes[&quick_id];
        assert!(!note.recovery_blocked);
        assert_eq!(
            note.active_editor().unwrap().read(cx).model.document,
            held,
            "Restore must not overwrite the newer held model"
        );
    })?;
    h.update(&mut cx, |s, w, cx| s.command(Command::Save, w, cx))?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        settle(&mut cx, h)?;
        if h.update(&mut cx, |s, _, _| {
            let note = &s.live.as_ref().unwrap().notes[&quick_id];
            note.error.is_none() && note.pending.is_none() && note.revision == note.saved
        })? {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Restored held draft save timed out"
        );
    }
    eprintln!("Recovery workflow stage: close acknowledged shared Quick");
    let remaining_quick = h.update(&mut cx, |s, _, _| s.windows.quick.as_ref().map(|(h, _)| *h))?;
    if let Some(remaining_quick) = remaining_quick {
        super::super::visual_keys(&mut cx, remaining_quick, "ctrl-w")
            .context("close remaining Quick through routed ctrl-w")?;
    }
    cx.run_until_parked();
    h.update(&mut cx, |s, w, cx| s.request_close(w, cx))?;
    cx.run_until_parked();
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx.update(|cx| !cx.windows().is_empty()) {
        let _ = h.update(&mut cx, |s, w, cx| s.poll_storage(w, cx));
        cx.run_until_parked();
        anyhow::ensure!(
            Instant::now() < deadline,
            "Owned recovery windows did not close"
        );
        std::thread::sleep(Duration::from_millis(3));
    }
    drop(editor);
    let reopened = store(&root)?;
    let library = reopened.load()?;
    assert!(!library.notes.iter().any(|n| n.id == duplicate.id));
    assert_eq!(
        library
            .notes
            .iter()
            .find(|n| n.id == quick_id)
            .unwrap()
            .body,
        held
    );
    assert_eq!(
        library
            .notes
            .iter()
            .find(|n| n.id == original.id)
            .unwrap()
            .body,
        original_body
    );
    assert!(
        library
            .notes
            .iter()
            .find(|n| n.id == original.id)
            .unwrap()
            .folder
            .is_none()
    );
    assert!(
        reopened.load_trash(&duplicate.id).is_err(),
        "Permanent deletion removes the saved note"
    );
    println!(
        "PASS: real-writer same-update duplicate save barrier, fresh document/attachment identities and bytes, ordinary trash/Undo/restore, keyboard preview, nested folder confirmation and unfiled Undo, explicit permanent confirmation/cancel, shared Main+Quick newer-draft receipt guard, explicit restore/save and durable restart."
    );
    Ok(())
}
