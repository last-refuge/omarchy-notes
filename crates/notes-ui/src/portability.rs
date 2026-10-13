//! User-visible data ownership. Every picker and completion is stamped with
//! the source library and operation; publication never replaces a destination.
use super::*;
use notes_storage::{Event, Request, backup, file_import, portable};
use std::{path::PathBuf, sync::mpsc};
#[cfg(feature = "visual-tests")]
#[path = "portability_check.rs"]
mod check;
#[cfg(feature = "visual-tests")]
pub use check::portability_check;

#[derive(Clone)]
pub(super) enum Action {
    Panel,
    Close,
    Cancel,
    ImportFiles,
    ImportFolder,
    CommitImport,
    ExportNote(portable::Format),
    ExportLibrary,
    BackUp,
    Inspect,
    Restore,
    OpenLibrary,
    OpenRestored,
    OpenResult,
    EnableQuotes,
}
#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    focus: Vec<FocusHandle>,
    operation: u64,
    source: Option<PathBuf>,
    job: Option<Job>,
    prepared: Option<file_import::PreparedImport>,
    inspected: Option<backup::VerifiedBackup>,
    restored: Option<PathBuf>,
    output: Option<PathBuf>,
    message: Option<String>,
    scroll: ScrollHandle,
    failed: bool,
}
struct Job {
    phase: Phase,
    cancel: backup::Cancellation,
    import_cancel: file_import::Cancellation,
    progress: Option<mpsc::Receiver<backup::Progress>>,
    boundary: std::collections::HashMap<String, u64>,
}
enum Phase {
    Picker,
    Worker,
    Background,
    SaveBarrier(PathBuf),
}
impl State {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    fn matches(&self, operation: u64, root: &std::path::Path) -> bool {
        self.operation == operation && self.source.as_deref() == Some(root)
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(j) = &self.job {
            j.cancel.cancel();
            j.import_cancel.cancel();
        }
    }
}
enum WorkResult {
    Inspected(backup::VerifiedBackup),
    Restored(backup::VerifiedBackup),
    Opened(PathBuf),
}
impl Shell {
    pub(super) fn portability_command(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_none() {
            return;
        }
        match action {
            Action::Panel => {
                self.portability.open = true;
                if self.portability.focus.is_empty() {
                    self.portability.focus = (0..12).map(|_| cx.focus_handle()).collect();
                }
                window.focus(&self.portability.focus[0], cx);
                self.settings_open = false;
                self.folders_open = false;
            }
            Action::Close => {
                self.portability.open = false;
                if !self.portability.busy() {
                    self.portability.prepared = None;
                }
                window.focus(&self.focus, cx);
            }
            Action::Cancel => {
                if let Some(job) = &self.portability.job {
                    job.cancel.cancel();
                    job.import_cancel.cancel();
                }
                self.portability.message = Some("Cancelling… Close the system chooser if it is still open. Completed copies are retained.".into());
            }
            Action::EnableQuotes if !self.portability.busy() => {
                self.upgrade_command(Command::UpgradePanel, window, cx)
            }
            Action::OpenResult => {
                if let Some(path) = &self.portability.output {
                    cx.open_with_system(path);
                }
            }
            Action::CommitImport if !self.portability.busy() => self.commit_file_import(cx),
            Action::OpenRestored if !self.portability.busy() => {
                if let Some(path) = self.portability.restored.clone()
                    && let Some((op, root)) = self.begin_portability(Phase::Background, cx)
                {
                    self.background_open(op, root, path, window, cx);
                }
            }
            action if !self.portability.busy() => self.choose_portability(action, window, cx),
            _ => {}
        }
        cx.notify();
    }
    fn begin_portability(
        &mut self,
        phase: Phase,
        cx: &mut Context<Self>,
    ) -> Option<(u64, PathBuf)> {
        let live = self.live.as_mut()?;
        if self.portability.busy() {
            return None;
        }
        if live.upgrade.busy()
            || live.pdf.busy()
            || live.creating.is_some()
            || self.organization.busy()
            || live.closing
            || self.library_recovery.blocks_mutations()
        {
            self.portability.message =
                Some("Finish the current library operation before starting another copy.".into());
            self.portability.failed = true;
            cx.notify();
            return None;
        }
        live.operation += 1;
        self.portability.operation = live.operation;
        self.portability.source = Some(live.root.clone());
        self.portability.job = Some(Job {
            phase,
            cancel: backup::Cancellation::default(),
            import_cancel: file_import::Cancellation::default(),
            progress: None,
            boundary: Default::default(),
        });
        self.portability.prepared = None;
        self.portability.failed = false;
        self.portability.output = None;
        live.closing = false;
        cx.notify();
        Some((live.operation, live.root.clone()))
    }
    pub(super) fn portable_note_snapshot(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<notes_storage::Note> {
        let Some(session) = self
            .library
            .selected
            .as_ref()
            .and_then(|id| self.live.as_ref()?.notes.get(id))
        else {
            self.portability.message = Some("Select a note before exporting it.".into());
            cx.notify();
            return None;
        };
        if session.has_unobserved_edit(cx)
            || session.input_is_composing(cx)
            || session.has_pending_work(cx)
        {
            self.portability.failed = true;
            self.portability.message = Some("Finish the current edit or file insertion before exporting. Your draft is retained.".into());
            cx.notify();
            return None;
        }
        Some(session.note.clone())
    }
    fn choose_portability(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = if matches!(action, Action::ExportNote(_)) {
            let Some(note) = self.portable_note_snapshot(cx) else {
                return;
            };
            Some(note)
        } else {
            None
        };
        if matches!(action, Action::Restore) && self.portability.inspected.is_none() {
            self.portability.message =
                Some("Inspect a backup first, then restore it into a new folder.".into());
            return;
        }
        if matches!(action, Action::Inspect) {
            self.portability.inspected = None;
        }
        let Some((operation, root)) = self.begin_portability(Phase::Picker, cx) else {
            return;
        };
        let new_path = matches!(
            action,
            Action::ExportNote(_) | Action::ExportLibrary | Action::BackUp | Action::Restore
        );
        self.portability.message = Some(
            if new_path {
                "Choose a new destination folder name. Existing files are preserved."
            } else {
                "Choose the source on your device."
            }
            .into(),
        );
        if new_path {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.clone());
            let name = match action {
                Action::BackUp => "Notes backup",
                Action::Restore => "Restored notes",
                Action::ExportLibrary => "Notes Markdown",
                _ => "Note export",
            };
            let picker = cx.prompt_for_new_path(&home, Some(name));
            cx.spawn_in(window, async move |this, cx| {
                let result = picker.await;
                let _ = this.update_in(cx, |s, w, cx| {
                    if !s.portability_current(operation, &root) { return; }
                    match result {
                        Ok(Ok(Some(path))) => s.portability_path(operation, root, action, vec![path], snapshot, w, cx),
                        Ok(Ok(None)) => s.portability_failed("Chooser cancelled. Your source is unchanged.".into(), false, cx),
                        _ => s.portability_failed("The system chooser is unavailable. Retry when the desktop portal is running.".into(), true, cx),
                    }
                });
            }).detach();
        } else {
            let picker = cx.prompt_for_paths(PathPromptOptions {
                files: matches!(action, Action::ImportFiles),
                directories: !matches!(action, Action::ImportFiles),
                multiple: matches!(action, Action::ImportFiles),
                prompt: Some("Choose Notes source".into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = picker.await;
                let _ = this.update_in(cx, |s, w, cx| {
                    if !s.portability_current(operation, &root) { return; }
                    match result {
                        Ok(Ok(Some(paths))) if !paths.is_empty() => s.portability_path(operation, root, action, paths, snapshot, w, cx),
                        Ok(Ok(_)) => s.portability_failed("Chooser cancelled. Your source is unchanged.".into(), false, cx),
                        _ => s.portability_failed("The system chooser is unavailable. Retry when the desktop portal is running.".into(), true, cx),
                    }
                });
            }).detach();
        }
    }
    fn portability_current(&self, operation: u64, root: &std::path::Path) -> bool {
        self.live.as_ref().is_some_and(|l| l.root == root)
            && self.portability.matches(operation, root)
            && self.portability.busy()
    }
    #[allow(clippy::too_many_arguments)]
    fn portability_path(
        &mut self,
        operation: u64,
        root: PathBuf,
        action: Action,
        paths: Vec<PathBuf>,
        snapshot: Option<notes_storage::Note>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let job = self.portability.job.as_ref().unwrap();
        if job.cancel.check().is_err() {
            self.portability_failed(
                "Operation cancelled. Your source is unchanged.".into(),
                false,
                cx,
            );
            return;
        }
        let path = paths[0].clone();
        match action {
            Action::ImportFiles | Action::ImportFolder => {
                let request = Request::PrepareFileImport { paths, cancel: job.import_cancel.clone(), operation };
                self.portability_send(request, "Reading sources and checking local files…", cx);
            }
            Action::ExportNote(format) => self.portability_send(Request::PortableExport { selection: portable::Selection::Note(snapshot.unwrap()), destination: path, format, cancel: job.cancel.clone(), operation }, "Exporting your note and its files… You can keep writing.", cx),
            Action::ExportLibrary => self.portability_send(Request::PortableExport { selection: portable::Selection::Library, destination: path, format: portable::Format::Markdown, cancel: job.cancel.clone(), operation }, "Exporting saved notes as Markdown. Unsaved drafts and Recently Deleted are excluded; use a backup for revision history.", cx),
            Action::BackUp => {
                self.portability.job.as_mut().unwrap().phase = Phase::SaveBarrier(path);
                self.portability.message = Some("Saving main and Quick Note drafts before taking the backup…".into());
                self.save_real_notes(cx);
            }
            Action::Inspect => self.background_backup(operation, root, path, None, window, cx),
            Action::Restore => {
                let source = self.portability.inspected.as_ref().unwrap().path.clone();
                self.background_backup(operation, root, source, Some(path), window, cx);
            }
            Action::OpenLibrary => self.background_open(operation, root, path, window, cx),
            _ => {}
        }
        cx.notify();
    }
    fn portability_send(&mut self, request: Request, message: &str, cx: &mut Context<Self>) {
        if self
            .live
            .as_ref()
            .unwrap()
            .worker
            .requests
            .try_send(request)
            .is_err()
        {
            self.portability_failed(
                "Storage is busy or unavailable. Retry; your source is unchanged.".into(),
                true,
                cx,
            );
        } else {
            self.portability.job.as_mut().unwrap().phase = Phase::Worker;
            self.portability.message = Some(message.into());
            cx.notify();
        }
    }
    fn commit_file_import(&mut self, cx: &mut Context<Self>) {
        if self
            .live
            .as_ref()
            .is_some_and(|l| l.upgrade.busy() || l.pdf.busy() || l.creating.is_some())
            || self.organization.busy()
            || self.library_recovery.blocks_mutations()
        {
            self.portability.message = Some(
                "Finish the current library operation before importing the reviewed sources."
                    .into(),
            );
            cx.notify();
            return;
        }
        let Some(prepared) = self.portability.prepared.take() else {
            return;
        };
        if prepared.summary().needs_quotes && self.live.as_ref().unwrap().schema < 3 {
            self.portability.prepared = Some(prepared);
            self.portability.message = Some("Enable quotation support with a verified backup before importing these notes. Your sources are unchanged.".into());
            return;
        }
        let operation = self.portability.operation;
        self.portability.job = Some(Job {
            phase: Phase::Worker,
            cancel: backup::Cancellation::default(),
            import_cancel: file_import::Cancellation::default(),
            progress: None,
            boundary: Default::default(),
        });
        let cancel = self.portability.job.as_ref().unwrap().import_cancel.clone();
        let folder = match &self.library.scope {
            Scope::FolderId(id) => id.clone(),
            _ => None,
        };
        self.portability_send(
            Request::CommitFileImport {
                prepared,
                folder,
                accept_warnings: true,
                cancel,
                operation,
            },
            "Copying reviewed sources into your library…",
            cx,
        );
    }
    pub(super) fn poll_portability(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let phases = self
            .portability
            .job
            .as_ref()
            .and_then(|j| j.progress.as_ref())
            .map(|r| r.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for p in phases {
            self.portability_progress(p, cx);
        }
        let target = self.portability.job.as_ref().and_then(|j| {
            if let Phase::SaveBarrier(path) = &j.phase {
                Some(path.clone())
            } else {
                None
            }
        });
        let Some(destination) = target else {
            return;
        };
        if self
            .portability
            .job
            .as_ref()
            .unwrap()
            .cancel
            .check()
            .is_err()
        {
            self.portability_failed(
                "Backup cancelled before its snapshot. Your drafts remain here.".into(),
                false,
                cx,
            );
            return;
        }
        let live = self.live.as_ref().unwrap();
        if live
            .notes
            .values()
            .any(|s| s.error.is_some() || s.needs_upgrade)
        {
            self.portability_failed("Backup paused: a draft could not be saved. Resolve its error or export recovery before backing up; no incomplete backup was created.".into(), true, cx);
            return;
        }
        if live.creating.is_some()
            || self.organization.busy()
            || live.notes.values().any(|s| {
                s.pending.is_some()
                    || s.revision != s.saved
                    || s.has_unobserved_edit(cx)
                    || s.input_is_composing(cx)
                    || s.has_pending_work(cx)
            })
        {
            return;
        }
        let job = self.portability.job.as_mut().unwrap();
        job.boundary = live
            .notes
            .iter()
            .map(|(id, s)| (id.clone(), s.revision))
            .collect();
        let request = Request::BackUp {
            destination,
            cancel: job.cancel.clone(),
            operation: self.portability.operation,
        };
        self.portability_send(request, "Taking a verified snapshot of saved revisions… Newer edits remain available while the copy finishes.", cx);
    }
    pub(super) fn portability_event(
        &mut self,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Event> {
        let operation = match &event {
            Event::FileImportProgress { operation, .. }
            | Event::FileImportPrepared { operation, .. }
            | Event::FileImportCommitted { operation, .. }
            | Event::PortabilityProgress { operation, .. }
            | Event::PortableExported { operation, .. }
            | Event::BackedUp { operation, .. } => *operation,
            _ => return Some(event),
        };
        let root = self.live.as_ref().map(|l| l.root.clone())?;
        if !self.portability_current(operation, &root) {
            return None;
        }
        match event {
            Event::PortabilityProgress { progress, .. } => self.portability_progress(progress, cx),
            Event::FileImportProgress { progress, .. } => {
                self.portability.message = Some(format!(
                    "Import {:?} · {} of {}",
                    progress.phase, progress.completed, progress.total
                ));
            }
            Event::FileImportPrepared { result, .. } => {
                self.portability.job = None;
                match result {
                    Ok(prepared) => {
                        self.portability.message = Some("Review the notes and limitations below before copying them. Sources stay intact.".into());
                        self.portability.prepared = Some(prepared);
                    }
                    Err(e) => self.portability_failed(e, true, cx),
                }
            }
            Event::FileImportCommitted { result, .. } => {
                self.portability.job = None;
                match result {
                    Ok(report) => {
                        let count = report.notes.len();
                        self.live.as_mut().unwrap().folders.extend(report.folders);
                        for note in report.notes {
                            self.attach_note(note, window, cx);
                        }
                        self.refresh_folder_rows();
                        self.refresh_real_library(cx);
                        self.portability.message = Some(format!(
                            "Imported {count} notes. Original files remain intact."
                        ));
                    }
                    Err(e) => self.portability_failed(e, true, cx),
                }
            }
            Event::PortableExported { result, .. } => {
                self.portability.job = None;
                match result {
                    Ok(r) => {
                        self.portability.output = Some(r.path.clone());
                        self.portability.message = Some(format!(
                            "Exported {} notes and {} files to {}. {}",
                            r.notes,
                            r.assets,
                            r.path.display(),
                            r.warnings.join(" ")
                        ));
                    }
                    Err(e) => self.portability_failed(e, true, cx),
                }
            }
            Event::BackedUp { result, .. } => {
                let job = self.portability.job.take().unwrap();
                match result {
                    Ok(r) => {
                        let newer = self.live.as_ref().unwrap().notes.iter().any(|(id, s)| {
                            job.boundary.get(id) != Some(&s.revision) || s.has_unobserved_edit(cx)
                        });
                        self.portability.message = Some(format!(
                            "Verified backup: {} notes, {} files. Exact committed revisions are recorded in its manifest.{} Saved to {}",
                            r.manifest.notes,
                            r.manifest.blobs,
                            if newer {
                                " Newer drafts stay here; back up again to include them."
                            } else {
                                ""
                            },
                            r.path.display()
                        ));
                        self.portability.output = Some(r.path.clone());
                        self.portability.inspected = Some(r);
                    }
                    Err(e) => self.portability_failed(e, true, cx),
                }
            }
            _ => unreachable!(),
        }
        cx.notify();
        None
    }
    fn portability_failed(&mut self, message: String, failed: bool, cx: &mut Context<Self>) {
        self.portability.job = None;
        self.portability.message = Some(message);
        self.portability.failed = failed;
        cx.notify();
    }
    fn portability_progress(&mut self, phase: backup::Progress, cx: &mut Context<Self>) {
        self.portability.message = Some(
            match phase {
                backup::Progress::Snapshot => "Taking the committed database snapshot…",
                backup::Progress::Copying => "Copying and checking local files…",
                backup::Progress::Verifying => "Verifying the completed copy…",
                backup::Progress::Publishing => "Publishing the verified copy…",
            }
            .into(),
        );
        cx.notify();
    }
    fn background_backup(
        &mut self,
        operation: u64,
        root: PathBuf,
        source: PathBuf,
        destination: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (tx, rx) = mpsc::channel();
        let job = self.portability.job.as_mut().unwrap();
        job.progress = Some(rx);
        job.phase = Phase::Background;
        let cancel = job.cancel.clone();
        let work = cx.background_spawn(async move {
            if let Some(target) = destination {
                backup::restore_controlled(&source, &target, &cancel, |p| {
                    let _ = tx.send(p);
                })
                .map(WorkResult::Restored)
            } else {
                backup::inspect_controlled(&source, &cancel, |p| {
                    let _ = tx.send(p);
                })
                .map(WorkResult::Inspected)
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |s, _, cx| {
                s.finish_portability_work(operation, &root, result, cx);
            });
        })
        .detach();
    }
    fn background_open(
        &mut self,
        operation: u64,
        root: PathBuf,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cancel = self.portability.job.as_ref().unwrap().cancel.clone();
        self.portability.message = Some("Verifying and opening a separate library window. Your current library and Quick Note stay available.".into());
        let work = cx.background_spawn(async move {
            open_library_window(&path, &cancel).map(WorkResult::Opened)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |s, _, cx| {
                s.finish_portability_work(operation, &root, result, cx);
            });
        })
        .detach();
    }
    fn finish_portability_work(
        &mut self,
        operation: u64,
        root: &std::path::Path,
        result: anyhow::Result<WorkResult>,
        cx: &mut Context<Self>,
    ) {
        if !self.portability_current(operation, root) {
            return;
        }
        self.portability.job = None;
        match result {
            Ok(WorkResult::Inspected(r)) => {
                self.portability.message = Some(format!(
                    "Verified backup · {} notes · {} files · schema {} · {} bytes. Restore copies it into a new library.",
                    r.manifest.notes, r.manifest.blobs, r.manifest.schema_version, r.manifest.bytes
                ));
                self.portability.inspected = Some(r);
            }
            Ok(WorkResult::Restored(r)) => {
                self.portability.message = Some(format!(
                    "Restored and verified at {}. Use Open restored library to open it in a separate window; your current notes stay here.",
                    r.path.display()
                ));
                self.portability.output = Some(r.path.clone());
                self.portability.restored = Some(r.path);
            }
            Ok(WorkResult::Opened(path)) => {
                self.portability.message = Some(format!(
                    "Opened {} in its own library window. This notebook and its drafts remain available.",
                    path.display()
                ));
            }
            Err(e) => {
                self.portability.failed = true;
                self.portability.message = Some(format!("{e:#}"));
            }
        }
        cx.notify();
    }
    pub(super) fn portability_focus(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if backward {
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
    }
    pub(super) fn portability_controls(&self, cx: &mut Context<Self>) -> Div {
        div().child(
            self.control(
                "data-ownership",
                "Import, export and library backups",
                Command::Portability(Action::Panel),
                cx,
            )
            .track_focus(&self.preferences.focus[15])
            .px(px(12.))
            .py(px(10.))
            .border_color(self.theme.border)
            .child("Import, export & backups…"),
        )
    }
    pub(super) fn portability_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let busy = self.portability.busy();
        let buttons = [
            ("Import files…", Action::ImportFiles),
            ("Import folder…", Action::ImportFolder),
            (
                "Export note as Markdown…",
                Action::ExportNote(portable::Format::Markdown),
            ),
            (
                "Export note as HTML…",
                Action::ExportNote(portable::Format::Html),
            ),
            ("Export library as Markdown…", Action::ExportLibrary),
            ("Back Up Library…", Action::BackUp),
            ("Inspect Backup…", Action::Inspect),
            ("Restore inspected backup…", Action::Restore),
            ("Open Library…", Action::OpenLibrary),
        ];
        div().id("portability-page").role(Role::Group).aria_label("Your notes and files").flex().flex_col().size_full().flex_1().min_w_0().min_h_0().bg(t.editor)
            .child(div().flex_none().flex().items_center().justify_between().px(px(24.)).py(px(18.)).border_b_1().border_color(t.border)
                .child(div().font_family("Noto Serif").text_size(px(26.)).child("Your notes and files"))
                .child(self.control("close-data", "Back to notes", Command::Portability(Action::Close), cx).track_focus(&self.portability.focus[0]).px(px(12.)).py(px(8.)).child("Back")))
            .child(div().id("data-scroll").track_scroll(&self.portability.scroll).overflow_y_scroll().flex_1().min_h_0().p(px(24.))
                .child(div().text_color(t.muted).text_size(px(13.)).line_height(relative(1.6)).child("Import copies your sources. A note export captures its current draft when the chooser opens. Library exports include saved notes; backups save pending drafts and also keep Recently Deleted and revision history. Every destination must be a new folder."))
                .when_some(self.portability.inspected.as_ref(), |d, backup| d.child(div().mt(px(12.)).text_size(px(12.)).text_color(t.muted).child(format!("Selected verified backup: {}", backup.path.display()))))
                .when(!busy, |d| d.child(div().flex().flex_wrap().gap(px(10.)).mt(px(22.)).children(buttons.into_iter().enumerate().map(|(i,(title,action))| self.control(("data-action",i), title, Command::Portability(action), cx).track_focus(&self.portability.focus[i+1]).px(px(14.)).py(px(10.)).border_color(t.border).child(title)))))
                .when_some(self.portability.prepared.as_ref(), |d, prepared| {
                    let s = prepared.summary();
                    d.child(div().mt(px(24.)).p(px(18.)).border_1().border_color(t.border)
                        .child(format!("Ready to copy: {} notes · {} folders · {} local files · {} bytes",s.files,s.folders,s.assets,s.bytes))
                        .child(div().mt(px(10.)).text_size(px(12.)).line_height(relative(1.6)).children(s.warnings.iter().map(|w| div().mb(px(6.)).child(w.clone()))))
                        .when(s.needs_quotes && self.live.as_ref().is_some_and(|l|l.schema<3), |d| d.child("Quotation support needs a verified library upgrade before import.").child(self.control("import-enable-quotes", "Enable quotation support with a verified backup", Command::Portability(Action::EnableQuotes), cx).mt(px(10.)).px(px(12.)).py(px(8.)).child("Enable quotations…")))
                        .child(self.control("commit-import", "Accept the listed limitations and copy sources into this library", Command::Portability(Action::CommitImport), cx).track_focus(&self.portability.focus[10]).mt(px(12.)).px(px(14.)).py(px(10.)).bg(t.accent).text_color(t.on_accent).child("Accept & import")))
                })
                .when(self.portability.restored.is_some(), |d| d.child(self.control("open-restored", "Open verified restored library", Command::Portability(Action::OpenRestored), cx).track_focus(&self.portability.focus[11]).mt(px(16.)).px(px(14.)).py(px(10.)).child("Open restored library…")))
                .when(self.portability.output.is_some(), |d| d.child(self.control("open-data-output", "Show completed copy", Command::Portability(Action::OpenResult), cx).mt(px(16.)).px(px(14.)).py(px(10.)).child("Show completed copy"))))
            .when(self.live.as_ref().is_some_and(|l| l.upgrade.open), |d| d.child(self.upgrade_panel(720., cx)))
            .when(busy, |d| d.child(self.control("cancel-data-work", "Cancel the current copy before publication", Command::Portability(Action::Cancel), cx).px(px(24.)).py(px(12.)).child("Cancel operation")))
            .when_some(self.portability.message.clone(), |d,message| d.child(div().id("data-status").flex_none().role(Role::Status).aria_label(message.clone()).px(px(24.)).py(px(16.)).max_h(px(180.)).overflow_y_scroll().bg(t.navigation).text_size(px(12.)).line_height(relative(1.6)).text_color(if self.portability.failed { t.accent } else { t.muted }).child(message)))
    }
}
fn open_library_window(
    path: &std::path::Path,
    cancel: &backup::Cancellation,
) -> anyhow::Result<PathBuf> {
    use notes_platform::instance::{Endpoint, Message, Outcome};
    let root = path.canonicalize()?;
    let endpoint = Endpoint::discover(&root)?;
    let request = Message::launch(
        notes_core::document::new_id(),
        notes_core::launch::Intent::Main,
        None,
    );
    cancel.check()?;
    if let Some(reply) = endpoint.send(&request)? {
        anyhow::ensure!(
            matches!(reply.outcome, Outcome::Opened { .. }),
            "The library owner did not confirm a window. Your current library remains available."
        );
        return Ok(root);
    }
    backup::validate_library(&root, cancel)?;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("--data-dir")
        .arg(&root)
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!(
                "The new library process exited ({status}); your current notebook is still available."
            );
        }
        if let Some(reply) = endpoint.send(&request)? {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            anyhow::ensure!(
                matches!(reply.outcome, Outcome::Opened { .. }),
                "The new process did not confirm a library window. Its outcome is unconfirmed; your current notebook remains available."
            );
            return Ok(root);
        }
        // Once spawned, cancellation must not close a potentially mapped window
        // or terminate a process that may already own a library and user edits.
        if cancel.check().is_err() || std::time::Instant::now() >= deadline {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            anyhow::bail!(
                "Window opening is unconfirmed. The new process was left intact; your current notebook remains available."
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
