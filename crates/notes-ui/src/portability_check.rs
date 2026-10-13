//! One real-worker ownership workflow; synthetic paths bypass desktop choosers.
use super::*;
use notes_core::document::{Block, Document};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Instant};

fn draw(cx: &mut HeadlessAppContext, handle: AnyWindowHandle) -> anyhow::Result<()> {
    cx.run_until_parked();
    let cleanup = handle.update(cx, |_, w, cx| {
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
            assert!(!s.portability.failed, "{:?}", s.portability.message);
            s.portability.busy()
                || s.query.loading
                || s.library_recovery.busy()
                || s.live.as_ref().unwrap().creating.is_some()
        })?;
        if !busy {
            draw(cx, h.into())?;
            return Ok(());
        }
        anyhow::ensure!(Instant::now() < deadline, "Portability workflow timed out");
        std::thread::sleep(Duration::from_millis(3));
    }
}

fn route_paths(
    cx: &mut HeadlessAppContext,
    h: WindowHandle<Shell>,
    action: Action,
    paths: Vec<PathBuf>,
) -> anyhow::Result<()> {
    h.update(cx, |s, w, cx| {
        let snapshot = if matches!(action, Action::ExportNote(_)) {
            Some(
                s.portable_note_snapshot(cx)
                    .expect("settled selected draft"),
            )
        } else {
            None
        };
        let (operation, root) = s.begin_portability(Phase::Picker, cx).unwrap();
        // This is the same controller route used by an acknowledged chooser.
        s.portability_path(operation, root, action, paths, snapshot, w, cx);
    })?;
    settle(cx, h)
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

fn export_manifest(path: &Path, notes: usize, assets: usize) -> anyhow::Result<serde_json::Value> {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("export.json"))?)?;
    assert_eq!(manifest["format"], "omarchy-notes-portable");
    assert_eq!(manifest["notes"].as_object().unwrap().len(), notes);
    assert_eq!(manifest["assets"].as_object().unwrap().len(), assets);
    for file in manifest["notes"].as_object().unwrap().values() {
        assert!(path.join(file.as_str().unwrap()).is_file());
    }
    for file in manifest["assets"].as_object().unwrap().values() {
        assert!(path.join(file.as_str().unwrap()).is_file());
    }
    Ok(manifest)
}

pub fn portability_check(directory: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(directory)?;
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("library");
    let source = temp.path().join("Selected");
    std::fs::create_dir_all(source.join("Clients"))?;
    std::fs::create_dir_all(source.join("assets"))?;
    let first_source = "# First\n\n**Bold** and *italic*.\n\n![Pixel](../assets/pixel.png)\n\n[Report](../assets/report.pdf)\n\n[Second](b.md)\n\n![Remote chart](https://example.invalid/chart.png)";
    std::fs::write(source.join("Clients/a.md"), first_source)?;
    std::fs::write(source.join("Clients/b.md"), "# Second\n\nThe linked note.")?;
    std::fs::write(
        source.join("assets/report.pdf"),
        b"%PDF-private-portability-fixture",
    )?;
    image::RgbaImage::from_pixel(1, 1, image::Rgba([20, 40, 60, 255]))
        .save(source.join("assets/pixel.png"))?;
    let pixel = std::fs::read(source.join("assets/pixel.png"))?;
    let loose = temp.path().join("Loose.md");
    let loose_source = "# Loose source\n\nAn independent source copied into this library.";
    std::fs::write(&loose, loose_source)?;
    let mut body = Document::default();
    body.blocks[0] = Block::paragraph("Working notebook");
    body.extra.insert(
        "future-fixture".into(),
        serde_json::json!({"retained":true}),
    );
    let seed = store(&root)?.create(body, None)?;
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
    h.update(&mut cx, |s, w, cx| {
        s.portability_command(Action::Panel, w, cx)
    })?;
    draw(&mut cx, h.into())?;
    assert!(h.update(&mut cx, |s, _, _| s.portability.open)?);
    cx.capture_screenshot(h.into())?
        .save(directory.join("portability-panel.png"))?;

    route_paths(&mut cx, h, Action::ImportFiles, vec![loose.clone()])?;
    h.update(&mut cx, |s, w, cx| {
        let summary = s.portability.prepared.as_ref().unwrap().summary();
        assert_eq!(summary.files, 1);
        assert_eq!(
            s.live.as_ref().unwrap().notes.len(),
            1,
            "Review cannot create notes"
        );
        s.portability_command(Action::CommitImport, w, cx);
    })?;
    settle(&mut cx, h)?;
    route_paths(&mut cx, h, Action::ImportFolder, vec![source.clone()])?;
    h.update(&mut cx, |s, _, _| {
        let summary = s.portability.prepared.as_ref().unwrap().summary();
        assert_eq!((summary.files, summary.folders, summary.assets), (2, 2, 2));
        assert!(
            !summary.warnings.is_empty(),
            "Remote resource limitation must be reviewed"
        );
        assert_eq!(s.live.as_ref().unwrap().notes.len(), 2);
    })?;
    draw(&mut cx, h.into())?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("portability-review.png"))?;
    h.update(&mut cx, |s, w, cx| {
        s.portability_command(Action::CommitImport, w, cx)
    })?;
    settle(&mut cx, h)?;
    let imported = h.update(&mut cx, |s, w, cx| {
        assert_eq!(s.live.as_ref().unwrap().notes.len(), 4);
        let first = s
            .live
            .as_ref()
            .unwrap()
            .notes
            .values()
            .find(|session| session.note.body.title() == "First")
            .unwrap()
            .note
            .clone();
        assert_eq!(first.body.references("image").len(), 1);
        assert_eq!(first.body.references("attachment").len(), 1);
        let second = s
            .live
            .as_ref()
            .unwrap()
            .notes
            .values()
            .find(|session| session.note.body.title() == "Second")
            .unwrap();
        assert!(
            first
                .body
                .leaves()
                .iter()
                .flat_map(|b| &b.spans)
                .any(|span| span.0.get("href").and_then(|v| v.as_str())
                    == Some(format!("note://{}", second.note.id).as_str()))
        );
        s.command(Command::Select(first.id.clone()), w, cx);
        first
    })?;
    settle(&mut cx, h)?;
    let image_hash = imported
        .body
        .references("image")
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(notes_storage::media::read_blob(&root, &image_hash)?, pixel);
    let attachment = imported
        .body
        .references("attachment")
        .into_iter()
        .next()
        .unwrap();
    let info = notes_storage::media::attachment_info(&root, &attachment)?;
    assert_eq!(
        notes_storage::media::read_blob(&root, &info.hash)?,
        b"%PDF-private-portability-fixture"
    );

    for format in [portable::Format::Markdown, portable::Format::Html] {
        let target = temp.path().join(if format == portable::Format::Markdown {
            "note-md"
        } else {
            "note-html"
        });
        route_paths(&mut cx, h, Action::ExportNote(format), vec![target.clone()])?;
        let manifest = export_manifest(&target, 1, 2)?;
        let text = std::fs::read_to_string(
            target.join(manifest["notes"][&imported.id].as_str().unwrap()),
        )?;
        std::fs::write(
            directory.join(if format == portable::Format::Markdown {
                "exported-note.md"
            } else {
                "exported-note.html"
            }),
            &text,
        )?;
        assert!(
            text.contains("assets/") && text.contains("Bold") && text.contains("Report"),
            "Exported {format:?} text: {text}"
        );
        for (hash, file) in manifest["assets"].as_object().unwrap() {
            assert_eq!(
                std::fs::read(target.join(file.as_str().unwrap()))?,
                notes_storage::media::read_blob(&root, hash)?
            );
        }
    }
    let portable_library = temp.path().join("library-md");
    route_paths(
        &mut cx,
        h,
        Action::ExportLibrary,
        vec![portable_library.clone()],
    )?;
    let manifest = export_manifest(&portable_library, 4, 2)?;
    let first_text = std::fs::read_to_string(
        portable_library.join(manifest["notes"][&imported.id].as_str().unwrap()),
    )?;
    assert!(
        !first_text.contains("note://"),
        "Exported internal links must use portable note paths"
    );

    h.update(&mut cx, |s, _, cx| s.open_quick(cx))?;
    cx.run_until_parked();
    let (quick, quick_id, quick_editor) = h.update(&mut cx, |s, _, _| {
        let (window, id) = s.windows.quick.clone().unwrap();
        let editor = s.live.as_ref().unwrap().notes[&id]
            .active_editor()
            .unwrap()
            .clone();
        (window, id, editor)
    })?;
    draw(&mut cx, quick)?;
    quick.update(&mut cx, |_, w, cx| {
        quick_editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, "Quick draft included.", w, cx)
        })
    })?;
    let backup_path = temp.path().join("verified-backup");
    h.update(&mut cx, |s, w, cx| {
        s.command(Command::Select(seed.id.clone()), w, cx);
        let editor = s.live.as_ref().unwrap().notes[&seed.id]
            .active_editor()
            .unwrap();
        editor.update(cx, |e, cx| {
            e.replace_text_in_range(None, "Main draft included. ", w, cx)
        });
        let (operation, source_root) = s.begin_portability(Phase::Picker, cx).unwrap();
        s.portability_path(
            operation,
            source_root,
            Action::BackUp,
            vec![backup_path.clone()],
            None,
            w,
            cx,
        );
        assert!(
            matches!(
                s.portability.job.as_ref().unwrap().phase,
                Phase::SaveBarrier(_)
            ),
            "Backup must wait for same-update Main and Quick edits"
        );
    })?;
    settle(&mut cx, h)?;
    let expected: BTreeMap<_, _> =
        h.update(&mut cx, |s, _, cx| {
            let live = s.live.as_ref().unwrap();
            assert!(live.notes.values().all(|n| n.pending.is_none()
                && n.revision == n.saved
                && !n.has_unobserved_edit(cx)));
            let manifest = &s.portability.inspected.as_ref().unwrap().manifest;
            assert_eq!(manifest.notes, 5);
            for (id, session) in &live.notes {
                assert_eq!(
                    manifest.committed_revisions.get(id),
                    Some(&session.note.revision)
                );
            }
            assert!(
                live.notes[&seed.id]
                    .note
                    .body
                    .plain_text()
                    .contains("Main draft included.")
            );
            assert_eq!(
                live.notes[&quick_id].note.body.plain_text(),
                "Quick draft included."
            );
            live.notes
                .iter()
                .map(|(id, s)| (id.clone(), s.note.clone()))
                .collect()
        })?;
    route_paths(&mut cx, h, Action::Inspect, vec![backup_path.clone()])?;
    let restored = temp.path().join("restored-library");
    route_paths(&mut cx, h, Action::Restore, vec![restored.clone()])?;
    assert_eq!(
        h.update(&mut cx, |s, _, _| s.portability.restored.clone())?,
        Some(restored.clone())
    );
    let roundtrip = store(&restored)?.load()?;
    assert_eq!(roundtrip.notes.len(), expected.len());
    for note in &roundtrip.notes {
        assert_eq!(note.body, expected[&note.id].body);
        assert_eq!(note.revision, expected[&note.id].revision);
        assert_eq!(note.folder, expected[&note.id].folder);
    }
    assert_eq!(
        notes_storage::media::read_blob(&restored, &image_hash)?,
        pixel
    );
    let info = notes_storage::media::attachment_info(&restored, &attachment)?;
    assert_eq!(
        notes_storage::media::read_blob(&restored, &info.hash)?,
        b"%PDF-private-portability-fixture"
    );
    assert_eq!(std::fs::read_to_string(&loose)?, loose_source);
    assert_eq!(
        std::fs::read_to_string(source.join("Clients/a.md"))?,
        first_source
    );
    draw(&mut cx, h.into())?;
    cx.capture_screenshot(h.into())?
        .save(directory.join("portability-restored.png"))?;
    h.update(&mut cx, |s, w, cx| {
        s.portability_command(Action::Close, w, cx)
    })?;
    super::super::visual_keys(&mut cx, quick, "ctrl-w")?;
    // Closing main uses the normal production save barrier.
    h.update(&mut cx, |s, w, cx| s.request_close(w, cx))?;
    cx.run_until_parked();
    let deadline = Instant::now() + Duration::from_secs(5);
    while cx.update(|cx| !cx.windows().is_empty()) {
        let _ = h.update(&mut cx, |s, w, cx| s.poll_storage(w, cx));
        cx.run_until_parked();
        anyhow::ensure!(Instant::now() < deadline, "Owned windows did not close");
        std::thread::sleep(Duration::from_millis(3));
    }
    drop(quick_editor);
    let durable = store(&root)?.load()?;
    assert_eq!(durable.notes.len(), expected.len());
    for note in durable.notes {
        assert_eq!(note.body, expected[&note.id].body);
    }
    println!(
        "PASS: real-worker file/folder review and copy, warnings, local asset and note-link preservation, Markdown/HTML/library export, same-update Main+Quick backup barrier, committed revision manifest, controlled inspect/new-destination restore, exact model/assets and durable restart. Native library-window opening is excluded."
    );
    Ok(())
}
