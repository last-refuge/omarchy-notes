#[cfg(feature = "visual-tests")]
#[path = "bounded_editor_check.rs"]
mod bounded_editor_check;
#[cfg(feature = "visual-tests")]
pub use bounded_editor_check::bounded_editor_check;
#[cfg(feature = "visual-tests")]
#[path = "writing_context_check.rs"]
mod context_check;
#[cfg(feature = "visual-tests")]
pub use context_check::context_check;
#[cfg(feature = "visual-tests")]
#[path = "writing_link_check.rs"]
mod link_check;
#[cfg(feature = "visual-tests")]
pub use link_check::link_check;
#[cfg(feature = "visual-tests")]
#[path = "writing_tools_check.rs"]
mod tools_check;
#[cfg(feature = "visual-tests")]
pub use tools_check::writing_tools_check;
#[cfg(feature = "visual-tests")]
#[path = "writing_table_check.rs"]
mod table_check;
#[cfg(feature = "visual-tests")]
pub use pdf::pdf_ui_check;
#[cfg(feature = "visual-tests")]
pub use table_check::table_check;
#[cfg(feature = "visual-tests")]
#[path = "writing_media_check.rs"]
mod media_check;
#[path = "writing_pdf.rs"]
mod pdf;
#[cfg(feature = "visual-tests")]
#[path = "writing_quote_check.rs"]
mod quote_check;
#[path = "writing_upgrade.rs"]
mod upgrade;
#[cfg(feature = "visual-tests")]
pub use media_check::media_check;
#[cfg(feature = "visual-tests")]
pub use quote_check::quote_check;
// Per-note editing sessions and revision-guarded background persistence.
use super::*;
use crate::rich_editor::{
    Changed, EditorViewState, InputState, LinkNote, MediaRequest, OpenNote, RichEditor,
};
use notes_core::document::Document;
use notes_storage::{Event, Folder, Note, Request, SaveError, Worker};
use std::{path::PathBuf, time::Instant};

pub(super) struct Session {
    pub note: Note,
    pub editor: Option<Entity<RichEditor>>,
    view_state: Option<EditorViewState>,
    last_opened: u64,
    pub(super) model_revision: u64,
    pub(super) revision: u64,
    pub(super) saved: u64,
    pub(super) pending: Option<(u64, u64)>,
    pub(super) last_edit: Instant,
    pub(super) dirty_since: Instant,
    pub(super) error: Option<String>,
    pub(super) recovery_blocked: bool,
    pub(super) needs_upgrade: bool,
    pub(super) owner: AnyWindowHandle,
    _subscriptions: Vec<Subscription>,
}

// Keep four recently visited clean editors, plus visible and unsafe sessions.
// This bounds cross-note undo retention without discarding any unsaved model.
const IDLE_EDITOR_LIMIT: usize = 4;
impl Session {
    pub(super) fn active_editor(&self) -> Option<&Entity<RichEditor>> {
        self.editor.as_ref()
    }
    pub(super) fn input_is_composing(&self, cx: &App) -> bool {
        self.active_editor()
            .is_some_and(|e| e.read(cx).input_is_composing(cx))
    }
    pub(super) fn has_pending_work(&self, cx: &App) -> bool {
        self.active_editor()
            .is_some_and(|e| e.read(cx).has_pending_work())
    }
    /// Editor events settle after an entity update; until then the stored draft
    /// and its local revision are not a valid snapshot of the editor model.
    pub(super) fn has_unobserved_edit(&self, cx: &App) -> bool {
        self.active_editor()
            .is_some_and(|e| e.read(cx).model.revision != self.model_revision)
    }
    fn safe_to_evict(&self, cx: &App) -> bool {
        self.revision == self.saved
            && self.pending.is_none()
            && self.error.is_none()
            && !self.needs_upgrade
            && !self.input_is_composing(cx)
            && !self.has_pending_work(cx)
            && !self.has_unobserved_edit(cx)
            && self
                .active_editor()
                .is_some_and(|e| e.read(cx).error.is_none())
    }
}

/// Real GPU rendering and routed keyboard input against a disposable SQLite library.
#[cfg(feature = "visual-tests")]
pub fn writing_check(directory: &std::path::Path) -> anyhow::Result<()> {
    use std::sync::Arc;
    std::fs::create_dir_all(directory)?;
    let root = std::env::temp_dir().join(format!(
        "omarchy-writing-check-{}",
        notes_core::document::new_id()
    ));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let (worker, library) = Worker::start(root.clone(), false)?;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::assets::load_fonts(cx).unwrap();
        super::bind_keys(cx);
        crate::search::bind_keys(cx);
    });
    let handle = cx.open_window(size(px(1440.), px(960.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    fn draw(cx: &mut HeadlessAppContext, handle: WindowHandle<Shell>) -> anyhow::Result<()> {
        cx.run_until_parked();
        let cleanup = cx.update_window(handle.into(), |_, w, cx| {
            w.refresh();
            w.draw(cx)
        })?;
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
        Ok(())
    }
    fn type_text(
        cx: &mut HeadlessAppContext,
        handle: WindowHandle<Shell>,
        text: &str,
    ) -> anyhow::Result<()> {
        for ch in text.chars() {
            let key = ch.to_string();
            let cleanup = cx.update_window(handle.into(), |_, w, cx| {
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
    fn settle(cx: &mut HeadlessAppContext, handle: WindowHandle<Shell>) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let done = handle.update(cx, |s, w, cx| {
                s.poll_storage(w, cx);
                let live = s.live.as_ref().unwrap();
                live.creating.is_none()
                    && !s.query.loading
                    && live
                        .notes
                        .values()
                        .all(|s| s.pending.is_none() && s.revision == s.saved)
            })?;
            draw(cx, handle)?;
            if done {
                return Ok(());
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "Timed out settling durable editor state."
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    draw(&mut cx, handle)?;
    cx.capture_screenshot(handle.into())?
        .save(directory.join("empty.png"))?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-n")?;
    settle(&mut cx, handle)?;
    type_text(&mut cx, handle, "A quieter place to write")?;
    super::visual_keys(&mut cx, handle.into(), "enter")?;
    type_text(&mut cx, handle, "Make room for the next ")?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-b")?;
    type_text(&mut cx, handle, "good idea")?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-b")?;
    type_text(
        &mut cx,
        handle,
        ". Keep the words close and the page clear.",
    )?;
    super::visual_keys(&mut cx, handle.into(), "enter ctrl-shift-c")?;
    type_text(&mut cx, handle, "Capture the thought")?;
    super::visual_keys(&mut cx, handle.into(), "enter")?;
    type_text(&mut cx, handle, "Give it a little shape")?;
    let first = handle.update(&mut cx, |s, _, cx| {
        let id = s.library.selected.clone().unwrap();
        let session = &s.live.as_ref().unwrap().notes[&id];
        let model = &session.editor.as_ref().unwrap().read(cx).model;
        assert_eq!(model.document.blocks.len(), 4);
        assert_eq!(model.document.blocks[0].kind, "heading");
        assert!(
            model.document.blocks[1]
                .spans
                .iter()
                .any(|s| s.content() == "good idea" && s.mark("bold"))
                || model.document.blocks[1]
                    .spans
                    .iter()
                    .filter(|s| s.mark("bold"))
                    .map(|s| s.content())
                    .collect::<String>()
                    == "good idea"
        );
        assert_eq!(model.document.blocks[3].string("list"), "check");
        id
    })?;
    // Platform composition must not create a paragraph or acknowledge pre-edit text.
    handle.update(&mut cx, |s, w, cx| {
        let editor = s.live.as_ref().unwrap().notes[&first]
            .editor
            .as_ref()
            .unwrap()
            .clone();
        editor.update(cx, |e, cx| {
            e.replace_and_mark_text_in_range(None, "日本", Some(2..2), w, cx)
        });
    })?;
    draw(&mut cx, handle)?;
    super::visual_keys(&mut cx, handle.into(), "enter")?;
    handle.update(&mut cx, |s, w, cx| {
        assert_eq!(s.real_status(&first, cx), "Composing…");
        let editor = s.live.as_ref().unwrap().notes[&first]
            .editor
            .as_ref()
            .unwrap()
            .clone();
        editor.update(cx, |e, cx| {
            assert_eq!(e.model.document.blocks.len(), 4);
            assert!(!e.model.committed().plain_text().contains("日本"));
            assert!(e.bounds_for_range(0..0, Bounds::default(), w, cx).is_some());
            e.replace_text_in_range(None, "日本語", w, cx);
        });
    })?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-z")?;
    // Copy/paste preserves rich styles inside this GPUI process, with fresh block IDs.
    super::visual_keys(&mut cx, handle.into(), "ctrl-a ctrl-c")?;
    let copied = handle.update(&mut cx, |s, _, cx| {
        s.live.as_ref().unwrap().notes[&first]
            .editor
            .as_ref()
            .unwrap()
            .read(cx)
            .model
            .document
            .clone()
    })?;
    super::visual_keys(&mut cx, handle.into(), "end ctrl-v")?;
    handle.update(&mut cx, |s, _, cx| {
        let model = &s.live.as_ref().unwrap().notes[&first]
            .editor
            .as_ref()
            .unwrap()
            .read(cx)
            .model;
        assert!(model.document.blocks.len() > copied.blocks.len());
        model.document.validate().unwrap();
    })?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-z end")?;
    // Submit revision N, then type N+1 before applying the write acknowledgment.
    super::visual_keys(&mut cx, handle.into(), "ctrl-s")?;
    handle.update(&mut cx, |s, w, cx| s.poll_storage(w, cx))?;
    type_text(&mut cx, handle, " — then come back")?;
    std::thread::sleep(Duration::from_millis(20));
    handle.update(&mut cx, |s, w, cx| {
        s.poll_storage(w, cx);
        let session = &s.live.as_ref().unwrap().notes[&first];
        assert_ne!(
            session.revision, session.saved,
            "old acknowledgment must not mark newer edits saved"
        );
    })?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-s")?;
    settle(&mut cx, handle)?;
    draw(&mut cx, handle)?;
    cx.capture_screenshot(handle.into())?
        .save(directory.join("writing-dark.png"))?;
    // Undo is scoped to this note and survives navigating to another note.
    super::visual_keys(&mut cx, handle.into(), "ctrl-n")?;
    settle(&mut cx, handle)?;
    type_text(&mut cx, handle, "Another thought")?;
    handle.update(&mut cx, |s, w, cx| {
        s.command(Command::Select(first.clone()), w, cx)
    })?;
    draw(&mut cx, handle)?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-z ctrl-shift-z ctrl-s")?;
    settle(&mut cx, handle)?;
    handle.update(&mut cx, |s, w, cx| {
        s.command(Command::Appearance(Appearance::Light), w, cx)
    })?;
    draw(&mut cx, handle)?;
    cx.capture_screenshot(handle.into())?
        .save(directory.join("writing-light.png"))?;
    let expected = handle.update(&mut cx, |s, _, _| {
        s.live.as_ref().unwrap().notes[&first].note.body.clone()
    })?;
    handle.update(&mut cx, |_, w, _| w.remove_window())?;
    cx.run_until_parked();
    let deadline = Instant::now() + Duration::from_secs(5);
    let store = loop {
        match notes_storage::Store::open(&root) {
            Ok(s) => break s,
            Err(e) => {
                if Instant::now() > deadline {
                    return Err(e);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };
    let reopened = store.load()?;
    assert_eq!(reopened.notes.len(), 2);
    assert_eq!(
        reopened.notes.iter().find(|n| n.id == first).unwrap().body,
        expected
    );
    drop(store);
    // Reopen the durable document at compact size with a failing writer.
    let (worker, library) = Worker::start(root.clone(), true)?;
    let handle = cx.open_window(size(px(640.), px(820.)), |w, cx| {
        cx.new(|cx| Shell::new_live(Appearance::Reference, root.clone(), worker, library, w, cx))
    })?;
    handle.update(&mut cx, |s, w, cx| {
        s.command(Command::Select(first.clone()), w, cx)
    })?;
    draw(&mut cx, handle)?;
    cx.capture_screenshot(handle.into())?
        .save(directory.join("writing-narrow.png"))?;
    type_text(&mut cx, handle, "Unsaved ")?;
    super::visual_keys(&mut cx, handle.into(), "ctrl-s")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let failed = handle.update(&mut cx, |s, w, cx| {
            s.poll_storage(w, cx);
            s.live.as_ref().unwrap().notes[&first].error.is_some()
        })?;
        if failed {
            break;
        }
        anyhow::ensure!(Instant::now() < deadline, "Expected simulated save failure");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.update(&mut cx, |s, w, cx| {
        assert!(!s.request_close(w, cx));
        assert_ne!(
            s.live.as_ref().unwrap().notes[&first].revision,
            s.live.as_ref().unwrap().notes[&first].saved
        );
    })?;
    draw(&mut cx, handle)?;
    cx.capture_screenshot(handle.into())?
        .save(directory.join("save-error.png"))?;
    // Explicit recovery export through the same writer queue, with current unsaved content.
    let recovery = root.join("recovery-test");
    handle.update(&mut cx, |s, _, _| {
        let live = s.live.as_mut().unwrap();
        live.operation += 1;
        let operation = live.operation;
        let notes = live.notes.values().map(|s| s.note.clone()).collect();
        let revs = live
            .notes
            .values()
            .map(|s| (s.note.id.clone(), s.revision))
            .collect();
        live.worker
            .requests
            .try_send(Request::Export {
                notes,
                destination: recovery.clone(),
                operation,
            })
            .unwrap();
        live.exporting = Some((operation, revs));
    })?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let exported = handle.update(&mut cx, |s, w, cx| {
            s.poll_storage(w, cx);
            s.can_close_recovered(cx)
        })?;
        if exported {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Recovery export did not complete"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(recovery.join("recovery.json"))?)?;
    assert!(json["notes"].as_array().unwrap().iter().any(|n| {
        Document::from_json(&n["body"].to_string())
            .unwrap()
            .plain_text()
            .contains("Unsaved")
    }));
    handle.update(&mut cx, |s, w, cx| {
        s.command(Command::CloseRecovered, w, cx)
    })?;
    cx.run_until_parked();
    // Use authentic Qt documents to exercise table layout/editing at two widths.
    for width in [1024., 420.] {
        let fixture_root = root.join(format!("fixture-{width}"));
        std::fs::create_dir_all(&fixture_root)?;
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/qt-v1/library.sqlite3"
            ),
            fixture_root.join("library.sqlite3"),
        )?;
        let (worker, library) = Worker::start(fixture_root.clone(), false)?;
        let id = library
            .notes
            .iter()
            .find(|n| n.body.title().starts_with("Q3"))
            .unwrap()
            .id
            .clone();
        let handle = cx.open_window(size(px(width), px(820.)), |w, cx| {
            cx.new(|cx| {
                Shell::new_live(Appearance::Reference, fixture_root, worker, library, w, cx)
            })
        })?;
        handle.update(&mut cx, |s, w, cx| {
            s.command(Command::Select(id.clone()), w, cx);
            let editor = s.live.as_ref().unwrap().notes[&id]
                .editor
                .as_ref()
                .unwrap()
                .clone();
            editor.update(cx, |e, cx| {
                e.edit(
                    |e| {
                        let cell = e
                            .document
                            .blocks
                            .iter()
                            .find(|b| b.kind == "table")
                            .unwrap()
                            .cells[3]
                            .blocks[0]
                            .id
                            .clone();
                        let offset = e.global(&notes_editor::Position {
                            block: cell,
                            offset: 0,
                        });
                        e.select(offset, false);
                        e.insert("Dr. ", 0)
                    },
                    w,
                    cx,
                )
            });
        })?;
        super::visual_keys(&mut cx, handle.into(), "ctrl-s")?;
        settle(&mut cx, handle)?;
        draw(&mut cx, handle)?;
        cx.capture_screenshot(handle.into())?
            .save(directory.join(format!("table-{width}.png")))?;
        handle.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
    }
    println!(
        "Passed GPU rich input, composition adapter, rich clipboard, table editing, per-note undo, revision-safe saves, reopen, failure/close guard, and recovery export. Images: {}",
        directory.display()
    );
    Ok(())
}
#[cfg(feature = "performance-diagnostics")]
#[derive(Default)]
pub(super) struct DerivedTimings {
    pub changed_calls: u64,
    pub changed_ns: u64,
    pub snapshot_ns: u64,
    pub row_ns: u64,
    pub catalog_ns: u64,
}
pub(super) struct Writing {
    #[cfg(feature = "performance-diagnostics")]
    pub derived_timings: DerivedTimings,
    pub schema: u32,
    pub(super) upgrade: upgrade::State,
    pub notes: HashMap<String, Session>,
    pub folders: Vec<Folder>,
    pub(super) worker: Worker,
    pub root: PathBuf,
    pub(super) operation: u64,
    pub(super) creating: Option<(u64, Option<(String, u64)>)>,
    pub(super) closing: bool,
    pub message: Option<String>,
    exporting: Option<(u64, HashMap<String, u64>)>,
    exported: Option<HashMap<String, u64>>,
    pub(super) pdf: pdf::State,
    link_history: Vec<String>,
    editor_clock: u64,
}
impl Shell {
    pub fn new_live(
        appearance: Appearance,
        _root: PathBuf,
        worker: Worker,
        library: notes_storage::Library,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_live_window(appearance, _root, worker, library, true, window, cx)
    }
    pub(super) fn new_live_window(
        appearance: Appearance,
        _root: PathBuf,
        worker: Worker,
        library: notes_storage::Library,
        main: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        crate::rich_editor::bind_keys(cx);
        let mut s = Self::new(appearance, window, cx);
        s.library.notes.clear();
        s.library.selected = None;
        s.library.refresh();
        s.list.reset(0);
        s.live = Some(Writing {
            #[cfg(feature = "performance-diagnostics")]
            derived_timings: DerivedTimings::default(),
            schema: library.schema,
            upgrade: upgrade::State::default(),
            notes: HashMap::new(),
            folders: library.folders,
            worker,
            root: library.root,
            operation: 0,
            creating: None,
            closing: false,
            message: None,
            exporting: None,
            exported: None,
            pdf: pdf::State::default(),
            link_history: vec![],
            editor_clock: 0,
        });
        s.refresh_folder_rows();
        for note in library.notes {
            s.attach_note(note, window, cx);
        }
        s.refresh_real_library(cx);
        s.library.selected = s.library.notes.first().map(|n| n.id.clone());
        if main {
            s.bind_main(window, cx);
        } else {
            s.windows.main = None;
        }
        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(75))
                    .await;
                let Ok(Some(handle)) = this.update(cx, |s, _| {
                    s.windows
                        .main
                        .or_else(|| s.windows.quick.as_ref().map(|q| q.0))
                }) else {
                    break;
                };
                if handle
                    .update(cx, |_, w, cx| {
                        this.update(cx, |s, cx| s.poll_storage(w, cx))
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        s._write_task = Some(task);
        s
    }
    pub(super) fn attach_note(&mut self, note: Note, window: &mut Window, _: &mut Context<Self>) {
        let now = Instant::now();
        self.live.as_mut().unwrap().notes.insert(
            note.id.clone(),
            Session {
                note,
                editor: None,
                view_state: None,
                last_opened: 0,
                model_revision: 0,
                revision: 0,
                saved: 0,
                pending: None,
                last_edit: now,
                dirty_since: now,
                error: None,
                recovery_blocked: false,
                needs_upgrade: false,
                owner: window.window_handle(),
                _subscriptions: vec![],
            },
        );
    }
    /// Ensure an editor before selecting, focusing or rendering it, never in render.
    pub(super) fn ensure_editor(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<RichEditor>> {
        let live = self.live.as_mut()?;
        live.editor_clock += 1;
        let session = live.notes.get_mut(id)?;
        session.last_opened = live.editor_clock;
        if let Some(editor) = &session.editor {
            return Some(editor.clone());
        }
        let library_id = live.root.to_string_lossy().to_string();
        let body = session.note.body.clone();
        let state = session.view_state.take();
        let editor = cx.new(|cx| RichEditor::new(body, self.theme, library_id, window, cx));
        let typography = crate::rich_editor::WritingStyle {
            font: self.preferences.session.value.note_font,
            size: self.preferences.session.value.note_size,
        };
        editor.update(cx, |e, cx| {
            e.set_typography(typography, cx);
            if let Some(state) = state {
                e.restore_view_state(state, cx);
            }
        });
        let media_id = id.to_string();
        let media_subscription = cx.subscribe(&editor, move |s, _, event: &MediaRequest, cx| {
            s.request_media(&media_id, event, cx);
        });
        let link_source = id.to_string();
        let link_subscription = cx.subscribe(&editor, move |s, _, event: &OpenNote, cx| {
            s.route_note_link(&link_source, &event.0, cx);
        });
        let changed_id = id.to_string();
        let changed = cx.subscribe(&editor, move |s, _, _: &Changed, cx| {
            s.real_note_changed(&changed_id, cx)
        });
        let input = cx.subscribe(&editor, |_, _, _: &InputState, cx| cx.notify());
        let session = self.live.as_mut()?.notes.get_mut(id)?;
        session.owner = window.window_handle();
        session.model_revision = 0;
        session.editor = Some(editor.clone());
        session._subscriptions = vec![media_subscription, link_subscription, changed, input];
        Some(editor)
    }
    pub(super) fn trim_idle_editors(&mut self, cx: &mut Context<Self>) {
        let Some(live) = &mut self.live else {
            return;
        };
        let main = self.windows.main.and(self.library.selected.as_deref());
        let quick = self.windows.quick.as_ref().map(|q| q.1.as_str());
        let mut idle: Vec<_> = live
            .notes
            .iter()
            .filter(|(id, s)| {
                Some(id.as_str()) != main && Some(id.as_str()) != quick && s.safe_to_evict(cx)
            })
            .map(|(id, s)| (s.last_opened, id.clone()))
            .collect();
        idle.sort();
        let evict = idle.len().saturating_sub(IDLE_EDITOR_LIMIT);
        for (_, id) in idle.into_iter().take(evict) {
            let session = live.notes.get_mut(&id).unwrap();
            if let Some(editor) = session.editor.take() {
                session.view_state = Some(editor.read(cx).view_state());
                editor.update(cx, |e, cx| e.deactivate_media(cx));
                session._subscriptions.clear();
            }
        }
    }
    pub(super) fn sync_link_catalog(&mut self, cx: &mut Context<Self>) {
        #[cfg(feature = "performance-diagnostics")]
        let started = Instant::now();
        let catalog: std::sync::Arc<[LinkNote]> = self
            .library
            .notes
            .iter()
            .map(|n| LinkNote {
                id: n.id.clone(),
                title: n.title.clone(),
                folder: n.folder.clone(),
            })
            .collect();
        let mut ids: Vec<_> = self.library.selected.iter().cloned().collect();
        if let Some((_, id)) = &self.windows.quick {
            ids.push(id.clone());
        }
        for id in ids {
            if let Some(editor) = self
                .live
                .as_ref()
                .and_then(|l| l.notes.get(&id))
                .and_then(|s| s.active_editor().cloned())
            {
                editor.update(cx, |s, cx| s.set_link_catalog(id, catalog.clone(), cx));
            }
        }
        #[cfg(feature = "performance-diagnostics")]
        if let Some(live) = &mut self.live {
            live.derived_timings.catalog_ns += started.elapsed().as_nanos() as u64;
        }
    }
    pub(super) fn follow_note_link(
        &mut self,
        source: &str,
        target: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.library.selected.as_deref() != Some(source)
            && self.windows.quick.as_ref().is_none_or(|q| q.1 != source)
        {
            return;
        }
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !live.notes.contains_key(target) {
            if let Some(editor) = live.notes.get(source).and_then(Session::active_editor) {
                editor.update(cx, |s,cx| {s.error=Some("This note isn’t available in this library. It may have been deleted or belong to another notebook. You can edit or remove the link.".into());cx.notify();});
            }
            return;
        }
        if source != target {
            live.link_history.push(source.to_string());
            if live.link_history.len() > 100 {
                live.link_history.remove(0);
            }
        }
        self.command(Command::Select(target.into()), w, cx);
    }
    pub(super) fn back_from_link(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        while let Some(id) = live.link_history.pop() {
            if live.notes.contains_key(&id) {
                self.command(Command::Select(id), w, cx);
                break;
            }
        }
    }
    pub(super) fn activate_selected_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.library.selected.clone()
            && self.windows.quick.as_ref().is_none_or(|q| q.1 != id)
        {
            self.ensure_editor(&id, window, cx);
        }
        self.sync_link_catalog(cx);
        if let Some(live) = &mut self.live {
            for (id, s) in &mut live.notes {
                if self.windows.quick.as_ref().is_some_and(|q| &q.1 == id) {
                    continue;
                }
                let Some(editor) = s.active_editor().cloned() else {
                    continue;
                };
                if Some(id) == self.library.selected.as_ref() {
                    if s.owner != window.window_handle() {
                        if !editor.update(cx, |e, cx| e.rebind_window(window, cx)) {
                            continue;
                        }
                        s.owner = window.window_handle();
                    }
                    editor.update(cx, |e, cx| e.activate_media(window, cx));
                } else {
                    editor.update(cx, |e, cx| e.deactivate_media(cx));
                }
            }
        }
        self.trim_idle_editors(cx);
    }
    fn real_note_changed(&mut self, id: &str, cx: &mut Context<Self>) {
        #[cfg(feature = "performance-diagnostics")]
        let changed_started = Instant::now();
        if self.windows.quick.as_ref().is_some_and(|q| q.1 == id) {
            self.windows.cancel_quick_close();
        }
        if let Some(live) = &mut self.live
            && let Some(s) = live.notes.get_mut(id)
        {
            if s.revision == s.saved {
                s.dirty_since = Instant::now();
            }
            s.revision += 1;
            s.last_edit = Instant::now();
            if let Some(editor) = s.active_editor() {
                #[cfg(feature = "performance-diagnostics")]
                let snapshot_started = Instant::now();
                let model = &editor.read(cx).model;
                let revision = model.revision;
                s.note.body.clone_from(model.committed());
                s.model_revision = revision;
                #[cfg(feature = "performance-diagnostics")]
                {
                    live.derived_timings.snapshot_ns +=
                        snapshot_started.elapsed().as_nanos() as u64;
                }
            }
            if s.needs_upgrade && s.note.body.schema == 1 {
                s.needs_upgrade = false;
                s.error = None;
                live.message = None;
            }
            // A failed/conflicted draft stays blocked until an explicit retry/recovery.
            if live.closing {
                live.message = None;
            }
            live.closing = false;
        }
        self.refresh_real_note(id, false, cx);
        #[cfg(feature = "performance-diagnostics")]
        if let Some(live) = &mut self.live {
            live.derived_timings.changed_calls += 1;
            live.derived_timings.changed_ns += changed_started.elapsed().as_nanos() as u64;
        }
        cx.notify();
    }
    pub(super) fn pin_real_note(&mut self, cx: &mut Context<Self>) {
        if let Some(note) = self.library.selected_note()
            && let Some(live) = &mut self.live
            && let Some(s) = live.notes.get_mut(&note.id)
        {
            if s.saved == s.revision {
                s.dirty_since = Instant::now();
            }
            s.note.pinned = note.pinned;
            s.revision += 1;
            s.last_edit = Instant::now();
            cx.notify();
        }
    }
    pub(super) fn refresh_real_library(&mut self, cx: &mut Context<Self>) {
        let offset = self.list.logical_scroll_top();
        let anchor = self
            .library
            .visible
            .get(offset.item_ix)
            .map(|&i| self.library.notes[i].id.clone());
        let Some(live) = &self.live else {
            return;
        };
        let mut notes: Vec<_> = live
            .notes
            .values()
            .filter(|s| s.note.revision > 0 || s.revision > 0)
            .collect();
        notes.sort_by(|a, b| {
            b.note
                .pinned
                .cmp(&a.note.pinned)
                .then(b.note.updated_at.cmp(&a.note.updated_at))
                .then(a.note.id.cmp(&b.note.id))
        });
        self.library.notes = notes
            .into_iter()
            .map(|s| {
                let title = s.note.body.title();
                let folder = s
                    .note
                    .folder
                    .as_ref()
                    .and_then(|id| self.organization.paths.get(id))
                    .map_or("Notes", String::as_str);
                notes_core::preview::Note {
                    id: s.note.id.clone(),
                    title: if title.is_empty() {
                        "Untitled note".into()
                    } else {
                        title
                    },
                    summary: s.note.body.snippet(),
                    folder: folder.into(),
                    folder_id: s.note.folder.clone(),
                    day: "Your notes".into(),
                    time: if s.revision != s.saved {
                        "Unsaved".into()
                    } else {
                        String::new()
                    },
                    pinned: s.note.pinned,
                    blocks: vec![],
                    related: None,
                }
            })
            .collect();
        self.invalidate_query_indices();
        self.refresh_query_visibility();
        if let Some(anchor) = anchor
            && let Some(item_ix) = self
                .library
                .visible
                .iter()
                .position(|&i| self.library.notes[i].id == anchor)
        {
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: offset.offset_in_item,
            });
        }
        self.queue_library_query(false, cx);
        self.sync_link_catalog(cx);
        cx.notify();
    }
    /// A keystroke changes one note's derived row, not the entire notebook.
    /// Keep the list's measured rows and reading anchor when its order is stable.
    fn refresh_real_note(&mut self, id: &str, reorder: bool, cx: &mut Context<Self>) {
        #[cfg(feature = "performance-diagnostics")]
        let row_started = Instant::now();
        let Some(session) = self.live.as_ref().and_then(|l| l.notes.get(id)) else {
            return;
        };
        if session.note.revision == 0 && session.revision == 0 {
            return;
        }
        let title = session.note.body.title();
        let row = notes_core::preview::Note {
            id: id.to_owned(),
            title: if title.is_empty() {
                "Untitled note".into()
            } else {
                title
            },
            summary: session.note.body.snippet(),
            folder: session
                .note
                .folder
                .as_ref()
                .and_then(|folder| self.organization.paths.get(folder))
                .map_or("Notes", String::as_str)
                .into(),
            folder_id: session.note.folder.clone(),
            day: "Your notes".into(),
            time: if session.revision != session.saved {
                "Unsaved".into()
            } else {
                String::new()
            },
            pinned: session.note.pinned,
            blocks: vec![],
            related: None,
        };
        let previous = self.library.visible.clone();
        // A batch applies all commit dates before refreshing individual rows.
        // Sorting for an unchanged pinned row can reorder other rows, so their
        // identities (not only this row's index) determine whether to reset.
        let previous_order = reorder.then(|| {
            previous
                .iter()
                .map(|&index| self.library.notes[index].id.clone())
                .collect::<Vec<_>>()
        });
        let offset = self.list.logical_scroll_top();
        let anchor = previous
            .get(offset.item_ix)
            .map(|&i| self.library.notes[i].id.clone());
        let previous_position = self.library.notes.iter().position(|n| n.id == id);
        let catalog_changed =
            if let Some(existing) = self.library.notes.iter_mut().find(|n| n.id == id) {
                let changed = existing.title != row.title || existing.folder != row.folder;
                *existing = row;
                changed
            } else {
                self.library.notes.push(row);
                true
            };
        if reorder {
            let live = self.live.as_ref().unwrap();
            self.library.notes.sort_by(|a, b| {
                b.pinned
                    .cmp(&a.pinned)
                    .then_with(|| {
                        live.notes
                            .get(&b.id)
                            .map(|s| s.note.updated_at)
                            .cmp(&live.notes.get(&a.id).map(|s| s.note.updated_at))
                    })
                    .then(a.id.cmp(&b.id))
            });
        }
        let mut ordering_changed =
            previous_position != self.library.notes.iter().position(|n| n.id == id);
        if reorder || previous_position.is_none() {
            self.invalidate_query_indices();
        }
        self.refresh_query_visibility();
        ordering_changed |= previous_order.is_some_and(|order| {
            !order.iter().map(String::as_str).eq(self
                .library
                .visible
                .iter()
                .map(|&index| self.library.notes[index].id.as_str()))
        });
        if !ordering_changed && previous == self.library.visible {
            if let Some(index) = self
                .library
                .visible
                .iter()
                .position(|&i| self.library.notes[i].id == id)
            {
                self.list.remeasure_items(index..index + 1);
            }
        } else {
            self.list.reset(self.library.visible.len());
            if let Some(anchor) = anchor
                && let Some(index) = self
                    .library
                    .visible
                    .iter()
                    .position(|&i| self.library.notes[i].id == anchor)
            {
                self.list.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: offset.offset_in_item,
                });
            }
        }
        if catalog_changed {
            self.sync_link_catalog(cx);
        }
        #[cfg(feature = "performance-diagnostics")]
        if let Some(live) = &mut self.live {
            live.derived_timings.row_ns += row_started.elapsed().as_nanos() as u64;
        }
    }
    pub(super) fn create_real_note(
        &mut self,
        recover: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = &mut self.live else {
            return;
        };
        if live.creating.is_some() {
            return;
        }
        let source = if recover {
            self.library.selected.clone()
        } else {
            None
        };
        let body = if let Some(id) = &source {
            let Some(s) = live.notes.get(id) else {
                return;
            };
            if s.has_unobserved_edit(cx) {
                live.message = Some("Your latest edit is still being prepared. Try saving a copy again in a moment; your draft is retained.".into());
                cx.notify();
                return;
            }
            if s.input_is_composing(cx) || s.has_pending_work(cx) {
                live.message = Some(
                    "Finish the current insertion or text composition before saving a copy.".into(),
                );
                cx.notify();
                return;
            }
            s.note.body.clone()
        } else {
            let mut body = Document::default();
            body.blocks[0].set_kind("h1");
            body
        };
        let folder = match &self.library.scope {
            Scope::FolderId(id) => id.clone(),
            _ => None,
        };
        live.operation += 1;
        let operation = live.operation;
        match live.worker.requests.try_send(Request::Create {
            body,
            folder,
            operation,
        }) {
            Ok(()) => {
                live.creating = Some((
                    operation,
                    source.map(|id| {
                        let rev = live.notes[&id].revision;
                        (id, rev)
                    }),
                ));
                live.message = None;
            }
            Err(_) => live.message = Some("Storage is busy. Try creating the note again.".into()),
        }
        let _ = window;
        cx.notify();
    }
    pub(super) fn save_real_notes(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = &mut self.live {
            for s in live.notes.values_mut() {
                if !s.recovery_blocked {
                    s.error = None;
                }
                s.last_edit = Instant::now() - Duration::from_secs(4);
            }
            live.message = None;
        }
        cx.notify();
    }
    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let folder_view_busy = self.folder_view_busy();
        let library_busy = self.library_recovery.busy() || self.portability.busy();
        let Some(live) = &mut self.live else {
            return true;
        };
        if live.upgrade.busy() {
            live.upgrade.open = true;
            live.upgrade.message =
                Some("Finish or cancel the library upgrade before closing.".into());
            cx.notify();
            return false;
        }
        if live.pdf.busy() {
            live.pdf.open = true;
            live.pdf.message = Some("Finish or cancel PDF export before closing.".into());
            cx.notify();
            return false;
        }
        let quick_id = self.windows.quick.as_ref().map(|q| q.1.as_str());
        let pending = self.organization.busy()
            || library_busy
            || folder_view_busy
            || self.preferences.busy()
            || live.creating.is_some()
            || live
                .notes
                .values()
                .filter(|s| Some(s.note.id.as_str()) != quick_id)
                .any(|s| {
                    s.revision != s.saved
                        || s.has_unobserved_edit(cx)
                        || s.pending.is_some()
                        || (s.input_is_composing(cx) || s.has_pending_work(cx))
                });
        if !pending {
            self.remove_main(window, cx);
            return false;
        }
        live.closing = true;
        live.message =
            Some("Saving before closing… If a save fails, your window stays open.".into());
        for s in live.notes.values_mut() {
            s.last_edit = Instant::now() - Duration::from_secs(4);
        }
        cx.notify();
        false
    }
    pub(super) fn poll_storage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.poll_preferences(cx);
        self.poll_portability(window, cx);
        if let Some(sort) = self.poll_folder_view(cx) {
            self.query.sort = sort;
            self.query.sort_loaded = true;
            self.library.sort_by_title = sort == notes_core::library::Sort::Title;
            self.queue_library_query(false, cx);
        }
        // A successful retry removes its button. Keep focus on a mounted
        // Settings control so Escape and app shortcuts retain their key context.
        if self.settings_open
            && self.preferences.session.error.is_none()
            && self.preferences.focus[14].is_focused(window)
        {
            window.focus(&self.preferences.focus[0], cx);
        }
        self.poll_launches(cx);
        let Some(live) = &mut self.live else {
            return;
        };
        let events: Vec<_> = live.worker.events.try_iter().collect();
        let mut mutations = Vec::with_capacity(events.len());
        for event in events {
            let Some(event) = self.portability_event(event, window, cx) else {
                continue;
            };
            let Some(event) = self.library_recovery_event(event, window, cx) else {
                continue;
            };
            if matches!(event, Event::Queried { .. }) {
                self.library_query_event(event, cx);
            } else {
                mutations.push(event);
            }
        }
        let events = mutations;
        let changed = !events.is_empty();
        let only_note_updates = changed
            && events
                .iter()
                .all(|e| matches!(e, Event::Saved { .. } | Event::Captured { .. }));
        let saved_rows: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Saved { id, .. } => Some(id.clone()),
                Event::Captured { note } => Some(note.id.clone()),
                _ => None,
            })
            .collect();
        for event in events {
            match event {
                Event::Organized { operation, result } => {
                    self.organized(operation, result, window, cx)
                }
                Event::Captured { note } => {
                    // A background capture must not steal focus, selection or an
                    // existing draft. Only newly committed notes arrive here.
                    self.attach_note(note, window, cx);
                }
                Event::UpgradeProgress {
                    operation,
                    progress,
                } => self.upgrade_progress(operation, progress, cx),
                Event::Upgraded { operation, result } => {
                    self.upgrade_finished(operation, result, cx)
                }
                Event::Imported {
                    id,
                    operation,
                    result,
                } => {
                    self.deliver_import(&id, operation, result, cx);
                }
                Event::Exported { operation, result } => {
                    let live = self.live.as_mut().unwrap();
                    if live
                        .exporting
                        .as_ref()
                        .is_some_and(|(op, _)| *op == operation)
                    {
                        let (_, revisions) = live.exporting.take().unwrap();
                        match result {
                            Ok(path) => {
                                live.exported = Some(revisions);
                                live.message = Some(format!(
                                    "Recovery bundle saved to {}. You can close with recovery saved, or keep editing.",
                                    path.display()
                                ));
                            }
                            Err(error) => {
                                live.message = Some(format!("Recovery export failed: {error}"));
                            }
                        }
                    }
                }
                Event::Saved {
                    id,
                    local_revision,
                    operation,
                    result,
                } => {
                    let live = self.live.as_mut().unwrap();
                    if let Some(s) = live.notes.get_mut(&id)
                        && s.pending == Some((operation, local_revision))
                    {
                        s.pending = None;
                        match result {
                            Ok(saved)=>{s.saved=local_revision;s.note.revision=saved.revision;s.note.updated_at=saved.updated_at;if s.note.created_at == 0 { s.note.created_at = saved.updated_at; }s.error=None;s.needs_upgrade=false;}
                            Err(SaveError::UpgradeRequired)=>{
                                s.needs_upgrade = s.note.body.schema > 1 && live.schema < 3;
                                s.error=s.needs_upgrade.then(||"This quotation needs the new library format. Enable quotes with a verified backup, or undo the quote edit. Your draft is still here.".into());
                            }
                            Err(SaveError::Conflict(_))=>s.error=Some("This note changed elsewhere. Your draft is intact. Save a copy to keep both versions.".into()),
                            Err(SaveError::Failed(error))=>s.error=Some(format!("Couldn’t save. Your changes are still here. {error}")),
                        }
                        if s.error.is_some() {
                            if let Some(editor) = s.active_editor() {
                                editor.update(cx, |e, cx| e.reveal_caret(cx));
                            }
                            live.closing = false;
                            live.message=Some("Some changes need attention. Retry saving or export your drafts before closing.".into());
                        }
                    }
                }
                Event::Created { operation, result } => {
                    let launch_result = result.as_ref().map(|n| n.id.clone()).map_err(Clone::clone);
                    let live = self.live.as_mut().unwrap();
                    if live
                        .creating
                        .as_ref()
                        .is_none_or(|(op, _)| *op != operation)
                    {
                        continue;
                    }
                    let source = live.creating.take().and_then(|(_, source)| source);
                    match result {
                        Ok(note) => {
                            let id = note.id.clone();
                            let destination_scope = match &note.folder {
                                Some(id) => Scope::FolderId(Some(id.clone())),
                                None if matches!(self.library.scope, Scope::FolderId(None)) => {
                                    Scope::FolderId(None)
                                }
                                None => Scope::All,
                            };
                            // The successful copy is durable before the conflicted session is removed.
                            // The original database record remains unchanged and reappears on reopen.
                            if let Some((source, revision)) = source
                                && live.notes.get(&source).is_some_and(|s| {
                                    s.revision == revision
                                        && !s.has_unobserved_edit(cx)
                                        && (!s.input_is_composing(cx) && !s.has_pending_work(cx))
                                })
                            {
                                live.notes.remove(&source);
                            }
                            self.attach_note(note, window, cx);
                            self.library.scope = destination_scope;
                            self.library.query.clear();
                            self.search.update(cx, |s, cx| s.clear_silently(window, cx));
                            self.library.selected = Some(id.clone());
                            self.reading = true;
                            self.settings_open = false;
                            self.folders_open = false;
                            self.activate_selected_media(window, cx);
                            if let Some(editor) =
                                self.live.as_ref().unwrap().notes[&id].active_editor()
                            {
                                window.focus(&editor.read(cx).focus.clone(), cx);
                            }
                        }
                        Err(error) => {
                            live.closing = false;
                            live.message = Some(format!("Couldn’t create the note. {error}"));
                        }
                    }
                    self.created_launch(operation, launch_result, cx);
                }
                _ => {}
            }
        }
        let folder_view_busy = self.folder_view_busy();
        let library_busy = self.library_recovery.busy() || self.portability.busy();
        let live = self.live.as_mut().unwrap();
        let mut submitted = false;
        for s in live.notes.values_mut() {
            if s.recovery_blocked
                || self.library_recovery.blocks_save(&s.note.id)
                || s.revision == s.saved
                || s.pending.is_some()
                || s.error.is_some()
                || s.has_unobserved_edit(cx)
                || (s.input_is_composing(cx) || s.has_pending_work(cx))
            {
                continue;
            }
            if s.last_edit.elapsed() < Duration::from_millis(700)
                && s.dirty_since.elapsed() < Duration::from_secs(3)
            {
                continue;
            }
            live.operation += 1;
            let op = live.operation;
            let request = Request::Save {
                note: s.note.clone(),
                local_revision: s.revision,
                operation: op,
            };
            match live.worker.requests.try_send(request) {
                Ok(()) => {
                    s.pending = Some((op, s.revision));
                    submitted = true;
                }
                Err(_) => {
                    s.error = Some(
                        "Storage is busy or unavailable. Your draft remains here; use Retry."
                            .into(),
                    );
                    live.closing = false;
                    submitted = true;
                }
            }
        }
        if live.closing
            && !live.upgrade.busy()
            && !live.pdf.busy()
            && live.creating.is_none()
            && !self.preferences.busy()
            && !self.organization.busy()
            && !folder_view_busy
            && !library_busy
            && live
                .notes
                .values()
                .filter(|s| self.windows.quick.as_ref().is_none_or(|q| q.1 != s.note.id))
                .all(|s| {
                    s.revision == s.saved
                        && !s.has_unobserved_edit(cx)
                        && s.pending.is_none()
                        && (!s.input_is_composing(cx) && !s.has_pending_work(cx))
                })
        {
            self.remove_main(window, cx);
            return;
        }
        self.finish_quick_close(cx);
        self.trim_idle_editors(cx);
        if only_note_updates {
            for id in saved_rows {
                self.refresh_real_note(&id, true, cx);
            }
            self.queue_library_query(false, cx);
        } else if changed {
            self.refresh_real_library(cx);
        }
        if changed || submitted {
            cx.notify();
        }
        self.submit_library_query(cx);
        self.poll_library_recovery(window, cx);
    }
    pub(super) fn real_status(&self, id: &str, cx: &App) -> String {
        let live = self.live.as_ref().unwrap();
        let s = &live.notes[id];
        let status = if s.has_pending_work(cx) {
            "Preparing selection…".into()
        } else if s.input_is_composing(cx) {
            "Composing…".into()
        } else if s.error.is_some() {
            "Not saved · draft retained".into()
        } else if s.pending.is_some() {
            "Saving…".into()
        } else if s.saved != s.revision || s.has_unobserved_edit(cx) {
            "Unsaved changes".into()
        } else if s.note.revision == 0 {
            "Ready when you are".into()
        } else {
            "Saved on this device".into()
        };
        if s.active_editor()
            .is_some_and(|e| e.read(cx).model.history_usage().limit_reached)
        {
            format!("{status} · Undo keeps recent edits within 32 MiB")
        } else {
            status
        }
    }
    pub(super) fn real_editor(&self, narrow: bool, height: f32, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let live = self.live.as_ref().unwrap();
        let base = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .h_full()
            .bg(theme.editor);
        let Some(session) = self
            .library
            .selected
            .as_ref()
            .and_then(|id| live.notes.get(id))
        else {
            return base.child(div().m_auto().max_w(px(460.)).p(px(32.))
                .child(div().font_family("Noto Serif").text_size(px(34.)).child("A little room for your thoughts."))
                .child(div().mt(px(16.)).text_size(px(15.)).line_height(relative(1.6)).text_color(theme.muted).child("Start with a sentence. Shape it as you go. Your notes stay here, on your device."))
                .when_some(live.message.clone(), |d, message| d.child(div().mt(px(16.)).text_size(px(13.)).text_color(theme.accent).child(message)))
                .child(self.control("first-note","Create your first note",Command::New,cx).mt(px(24.)).px(px(18.)).py(px(12.)).bg(theme.accent).text_color(theme.on_accent).child("Create a note  ·  Ctrl N")));
        };
        if self
            .windows
            .quick
            .as_ref()
            .is_some_and(|q| q.1 == session.note.id)
        {
            return base.child(
                div()
                    .m_auto()
                    .p(px(32.))
                    .child("This note is open in Quick Note.")
                    .when_some(self.windows.notice.clone(), |d, text| {
                        d.child(div().mt(px(12.)).text_color(theme.muted).child(text))
                    })
                    .child(
                        self.control(
                            "take-quick-note",
                            "Continue this note here",
                            Command::Select(session.note.id.clone()),
                            cx,
                        )
                        .mt(px(16.))
                        .px(px(12.))
                        .child("Continue here"),
                    ),
            );
        }
        let dirty = live
            .notes
            .values()
            .filter(|s| s.revision != s.saved)
            .count();
        let error = if self.can_close_recovered(cx) {
            live.message.clone().or_else(|| session.error.clone())
        } else {
            session.error.clone().or_else(|| live.message.clone()).or_else(|| (live.schema < 3 && session.note.body.schema > 1).then(||"Your quotation is ready. Enable quotes with a verified backup to save it, or undo this edit to keep the older library format.".into()))
        };
        base.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(60.))
                .flex_none()
                .px(px(20.))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .flex_1()
                        .min_w_0()
                        .when(narrow || self.focused, |d| {
                            d.child(self.icon_button(
                                "back-writing",
                                "Back to notes",
                                "back",
                                Command::Back,
                                cx,
                            ))
                        })
                        .when(!live.link_history.is_empty(), |d| {
                            d.child(self.icon_button(
                                "back-from-link",
                                "Return to previous linked note",
                                "back",
                                Command::LinkBack,
                                cx,
                            ))
                        })
                        .child(icon("note", theme.muted, 18.))
                        .child(
                            label(
                                self.library
                                    .selected_note()
                                    .map_or_else(|| "Your notebook".into(), |n| n.folder.clone()),
                                theme,
                            )
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis(),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(self.recovery_controls(cx))
                        .child(
                            self.control(
                                "export-pdf",
                                "Export PDF · Ctrl Shift P",
                                Command::PdfPanel,
                                cx,
                            )
                            .px(px(9.))
                            .child("PDF"),
                        )
                        .child(self.icon_button(
                            "move-note",
                            "Move note to a folder (Ctrl Alt M)",
                            "folder",
                            Command::Organize(super::organization::Action::MoveNote),
                            cx,
                        ))
                        .child(self.icon_button(
                            "pin-real",
                            "Pin or unpin note",
                            "pin",
                            Command::Pin,
                            cx,
                        ))
                        .child(self.icon_button(
                            "focus-real",
                            "Focus on this note",
                            "focus",
                            Command::Focus,
                            cx,
                        ))
                        .child(self.icon_button(
                            "appearance-real",
                            "Notebook settings (Ctrl ,)",
                            "sun",
                            Command::Settings,
                            cx,
                        )),
                ),
        )
        .when(live.pdf.open && !live.upgrade.open, |d| {
            d.child(self.pdf_panel(height, cx))
        })
        .when(live.upgrade.open, |d| {
            d.child(self.upgrade_panel(height, cx))
        })
        .when_some(error.filter(|_| !live.upgrade.open), |d, message| {
            d.child(
                div()
                    .flex_none()
                    .px(px(22.))
                    .py(px(12.))
                    .bg(theme.selected)
                    .text_size(px(12.))
                    .line_height(relative(1.6))
                    .child(message)
                    .child(
                        div()
                            .flex()
                            .when(live.schema < 3 && session.note.body.schema > 1, |d| {
                                d.child(
                                    self.control(
                                        "enable-quotes",
                                        "Enable quotes with a backup",
                                        Command::UpgradePanel,
                                        cx,
                                    )
                                    .px(px(10.))
                                    .py(px(5.))
                                    .bg(theme.accent)
                                    .text_color(theme.on_accent)
                                    .child("Enable quotes…"),
                                )
                            })
                            .flex_wrap()
                            .gap(px(8.))
                            .mt(px(8.))
                            .child(
                                self.control("retry-save", "Retry saving", Command::Save, cx)
                                    .px(px(10.))
                                    .py(px(5.))
                                    .border_color(theme.border)
                                    .child("Retry"),
                            )
                            .child(
                                self.control(
                                    "save-copy",
                                    "Save draft as a new note",
                                    Command::Recover,
                                    cx,
                                )
                                .px(px(10.))
                                .py(px(5.))
                                .border_color(theme.border)
                                .child("Save a copy"),
                            )
                            .when(self.can_close_recovered(cx), |d| {
                                d.child(
                                    self.control(
                                        "close-recovered",
                                        "Close with recovery saved",
                                        Command::CloseRecovered,
                                        cx,
                                    )
                                    .px(px(10.))
                                    .py(px(5.))
                                    .border_color(theme.border)
                                    .child("Close with recovery saved"),
                                )
                            })
                            .child(
                                self.control(
                                    "export-drafts",
                                    "Export unsaved drafts",
                                    Command::ExportRecovery,
                                    cx,
                                )
                                .px(px(10.))
                                .py(px(5.))
                                .border_color(theme.border)
                                .child("Export drafts"),
                            ),
                    ),
            )
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .children(session.active_editor().cloned())
                .when(live.upgrade.open && height < 620., |d| d.hidden()),
        )
        .child(
            div()
                .h(px(42.))
                .flex_none()
                .px(px(22.))
                .flex()
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(theme.border)
                .child(
                    div()
                        .id("real-save-status")
                        .role(Role::Status)
                        .aria_label(self.real_status(&session.note.id, cx))
                        .text_size(px(11.))
                        .text_color(theme.muted)
                        .child(self.real_status(&session.note.id, cx)),
                )
                .child(label(
                    if dirty > 0 {
                        format!("{dirty} unsaved")
                    } else {
                        "Ctrl S  Save".into()
                    },
                    theme,
                )),
        )
    }
    pub(super) fn can_close_recovered(&self, cx: &App) -> bool {
        self.live.as_ref().is_some_and(|l| {
            !l.pdf.busy()
                && !l.upgrade.busy()
                && l.creating.is_none()
                && l.notes.values().all(|s| {
                    s.pending.is_none()
                        && !s.has_unobserved_edit(cx)
                        && (!s.input_is_composing(cx) && !s.has_pending_work(cx))
                        && (s.saved == s.revision
                            || l.exported
                                .as_ref()
                                .is_some_and(|revs| revs.get(&s.note.id) == Some(&s.revision)))
                })
        })
    }
    pub(super) fn export_recovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(live) = &mut self.live else {
            return;
        };
        if live.exporting.is_some() {
            return;
        }
        if live.notes.values().any(|s| s.has_unobserved_edit(cx)) {
            live.message = Some("Your latest edit is still being prepared. Try exporting again in a moment; your draft is retained.".into());
            cx.notify();
            return;
        }
        if live
            .notes
            .values()
            .any(|s| s.input_is_composing(cx) || s.has_pending_work(cx))
        {
            live.message = Some(
                "Finish the current insertion or text composition before exporting your drafts."
                    .into(),
            );
            cx.notify();
            return;
        }
        let notes: Vec<_> = live
            .notes
            .values()
            .filter(|s| s.revision != s.saved)
            .map(|s| s.note.clone())
            .collect();
        let revisions = live
            .notes
            .values()
            .map(|s| (s.note.id.clone(), s.revision))
            .collect();
        let request = cx.prompt_for_new_path(&live.root, Some("Notes recovery"));
        cx.spawn_in(window,async move|this,cx|{
            let result=request.await;
            let _=this.update_in(cx,|this,_,cx|{
                let Some(live)=&mut this.live else{return;};
                match result {
                    Ok(Ok(Some(destination)))=>{
                        live.operation+=1;let operation=live.operation;
                        match live.worker.requests.try_send(Request::Export{notes,destination,operation}) {
                            Ok(())=>{live.exporting=Some((operation,revisions));live.message=Some("Exporting your drafts and attachments…".into());},
                            Err(_)=>live.message=Some("Storage is busy. Try exporting again.".into()),
                        }
                    }
                    Ok(Ok(None))=>{},
                    _=>live.message=Some("The system file picker is unavailable. Keep this window open and retry saving or saving a copy.".into()),
                }
                cx.notify();
            });
        }).detach();
    }
}
