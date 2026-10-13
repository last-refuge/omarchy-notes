//! Snapshot-based PDF exports, independent from autosave/recovery state.
use super::*;
use notes_export::{Cancellation, Paper, Report};

#[derive(Default)]
pub(in crate::shell) struct State {
    pub open: bool,
    focus: Option<FocusHandle>,
    paper: Paper,
    operation: u64,
    job: Option<Job>,
    pub message: Option<String>,
    failed: bool,
    output: Option<PathBuf>,
}
struct Job {
    operation: u64,
    note: String,
    revision: u64,
    title: String,
    cancel: Cancellation,
    rendering: bool,
}
impl State {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    fn matches(&self, operation: u64) -> bool {
        self.job.as_ref().is_some_and(|j| j.operation == operation)
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.cancel();
        }
    }
}
impl Shell {
    pub(in crate::shell) fn pdf_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = &mut self.live else {
            return;
        };
        match command {
            Command::PdfPanel => {
                if !self
                    .library
                    .selected
                    .as_ref()
                    .is_some_and(|id| live.notes.contains_key(id))
                {
                    return;
                }
                live.pdf.open = true;
                // Put keyboard focus on the sheet; ordinary Tab follows its controls.
                live.pdf
                    .focus
                    .get_or_insert_with(|| cx.focus_handle())
                    .focus(window, cx);
                self.reading = true;
                self.settings_open = false;
                self.folders_open = false;
            }
            Command::PdfPaper(paper) if !live.pdf.busy() => live.pdf.paper = paper,
            Command::PdfCancel => {
                if let Some(job) = &live.pdf.job {
                    job.cancel.cancel();
                    live.pdf.message = Some("Cancelling export…".into());
                } else {
                    live.pdf.open = false;
                    self.focus_editor(window, cx);
                }
            }
            Command::PdfOpen => {
                if let Some(path) = &live.pdf.output {
                    cx.open_with_system(path);
                }
            }
            Command::PdfExport => {
                self.choose_pdf(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }
    fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self
            .library
            .selected
            .as_ref()
            .and_then(|id| self.live.as_ref()?.notes.get(id))
            .and_then(|s| s.active_editor())
        {
            session.read(cx).focus.clone().focus(window, cx);
        }
    }
    fn pdf_snapshot(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(u64, Document, PathBuf, Paper, String)> {
        let live = self.live.as_mut()?;
        if live.pdf.busy() {
            return None;
        }
        let id = self.library.selected.as_ref()?;
        let session = live.notes.get(id)?;
        if session.has_unobserved_edit(cx) {
            live.pdf.message = Some("Your latest edit is still being prepared. Try exporting again in a moment; your draft is retained.".into());
            live.pdf.failed = true;
            cx.notify();
            return None;
        }
        let editor = session.active_editor()?.read(cx);
        if editor.model.marked.is_some() || editor.has_pending_work() {
            live.pdf.message =
                Some("Finish inserting files or composing text, then export your note.".into());
            live.pdf.failed = true;
            cx.notify();
            return None;
        }
        let document = editor.model.committed().clone();
        let title = if document.title().is_empty() {
            "Untitled note".into()
        } else {
            document.title()
        };
        live.pdf.operation += 1;
        let operation = live.pdf.operation;
        live.pdf.job = Some(Job {
            operation,
            note: id.clone(),
            revision: session.revision,
            title: title.clone(),
            cancel: Cancellation::default(),
            rendering: false,
        });
        live.pdf.output = None;
        live.pdf.failed = false;
        live.pdf.message = Some(format!("Choose where to save “{title}”."));
        live.closing = false;
        let mut name = String::new();
        for ch in title
            .chars()
            .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
        {
            if name.len() + ch.len_utf8() > 200 {
                break;
            }
            name.push(ch);
        }
        let name = name.trim().trim_matches('.');
        let name = if name.is_empty() { "Note" } else { name };
        Some((
            operation,
            document,
            live.root.clone(),
            live.pdf.paper,
            format!("{name}.pdf"),
        ))
    }
    fn choose_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((operation, document, root, paper, name)) = self.pdf_snapshot(cx) else {
            return;
        };
        let directory = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.clone());
        let picker = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(live) = &mut this.live else { return; };
                if !live.pdf.matches(operation) { return; }
                match result {
                    Ok(Ok(Some(mut path))) => {
                        if !path.extension().is_some_and(|s| s.eq_ignore_ascii_case("pdf")) {
                            let mut name = path.file_name().unwrap_or_default().to_os_string(); name.push(".pdf"); path.set_file_name(name);
                        }
                        this.start_pdf(operation, document, root, path, paper, window, cx);
                    }
                    Ok(Ok(None)) => { live.pdf.job = None; live.pdf.message = Some("Export cancelled. Your note is unchanged.".into()); }
                    _ => { live.pdf.job = None; live.pdf.failed = true; live.pdf.message = Some("The system file picker is unavailable. Try exporting again when the desktop portal is running.".into()); }
                }
                cx.notify();
            });
        }).detach();
    }
    #[allow(clippy::too_many_arguments)]
    fn start_pdf(
        &mut self,
        operation: u64,
        document: Document,
        root: PathBuf,
        path: PathBuf,
        paper: Paper,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = &mut self.live else {
            return;
        };
        if !live.pdf.matches(operation) {
            return;
        }
        let job = live.pdf.job.as_mut().unwrap();
        job.rendering = true;
        live.pdf.message = Some(format!("Preparing “{}”… You can keep writing.", job.title));
        let cancel = job.cancel.clone();
        let target = path.clone();
        let work = cx.background_spawn(async move {
            notes_export::export(&root, &document, &target, paper, &cancel)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.finish_pdf(operation, path, result, cx);
            });
        })
        .detach();
        cx.notify();
    }
    fn finish_pdf(
        &mut self,
        operation: u64,
        path: PathBuf,
        result: anyhow::Result<Report>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = &mut self.live else {
            return;
        };
        if !live.pdf.matches(operation) {
            return;
        }
        let job = live.pdf.job.take().unwrap();
        match result {
            Ok(report) => {
                let newer = live
                    .notes
                    .get(&job.note)
                    .is_some_and(|s| s.revision != job.revision);
                live.pdf.message = Some(format!(
                    "“{}” exported · {} {}.{}{} Saved to {}",
                    job.title,
                    report.pages,
                    if report.pages == 1 { "page" } else { "pages" },
                    if newer {
                        " Newer edits stay in your note; export again to include them."
                    } else {
                        ""
                    },
                    if report.attachment_labels > 0 {
                        " Attached files are listed; their contents are not included."
                    } else {
                        ""
                    },
                    path.display()
                ));
                live.pdf.output = Some(path);
                live.pdf.failed = false;
            }
            Err(error) => {
                live.pdf.failed = true;
                live.pdf.message = Some(format!("{error:#}"));
            }
        }
        cx.notify();
    }
    pub(super) fn pdf_panel(&self, height: f32, cx: &mut Context<Self>) -> Div {
        let pdf = &self.live.as_ref().unwrap().pdf;
        let theme = self.theme;
        let busy = pdf.busy();
        let panel = div()
            .id("pdf-panel")
            .role(Role::Group)
            .aria_label("Export PDF")
            .when_some(pdf.focus.clone(), |d, focus| d.track_focus(&focus))
            .max_h(px((height - 280.).clamp(120., 290.)))
            .overflow_y_scroll()
            .flex_none()
            .px(px(22.))
            .py(px(18.))
            .bg(theme.overlay)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_family("Noto Serif")
                            .text_size(px(22.))
                            .child("A page worth sharing."),
                    )
                    .when(!busy, |d| {
                        d.child(self.icon_button(
                            "pdf-dismiss",
                            "Close PDF export",
                            "close",
                            Command::PdfCancel,
                            cx,
                        ))
                    }),
            )
            .child(
                div()
                    .mt(px(5.))
                    .text_size(px(12.))
                    .text_color(theme.muted)
                    .child("Beautiful type. Selectable text. A copy of this moment."),
            )
            .child(
                div()
                    .mt(px(13.))
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(px(8.))
                    .children([Paper::Letter, Paper::A4].into_iter().map(|paper| {
                        self.control(
                            format!("pdf-{}", paper.label()),
                            format!("{} paper", paper.label()),
                            Command::PdfPaper(paper),
                            cx,
                        )
                        .px(px(12.))
                        .py(px(7.))
                        .border_1()
                        .border_color(if pdf.paper == paper {
                            theme.accent
                        } else {
                            theme.border
                        })
                        .bg(if pdf.paper == paper {
                            theme.selected
                        } else {
                            theme.overlay
                        })
                        .opacity(if busy { 0.5 } else { 1. })
                        .child(paper.label())
                    }))
                    .when(!busy, |d| {
                        d.child(
                            self.control(
                                "pdf-save",
                                "Choose location and export PDF",
                                Command::PdfExport,
                                cx,
                            )
                            .px(px(14.))
                            .py(px(8.))
                            .bg(theme.accent)
                            .text_color(theme.on_accent)
                            .child("Export PDF…"),
                        )
                    })
                    .when(pdf.job.as_ref().is_some_and(|j| j.rendering), |d| {
                        d.child(
                            self.control("pdf-cancel", "Cancel PDF export", Command::PdfCancel, cx)
                                .px(px(12.))
                                .py(px(7.))
                                .child("Cancel"),
                        )
                    })
                    .when(pdf.output.is_some(), |d| {
                        d.child(
                            self.control("pdf-open", "Open exported PDF", Command::PdfOpen, cx)
                                .px(px(12.))
                                .py(px(7.))
                                .child("Open PDF"),
                        )
                    }),
            )
            .when_some(pdf.message.clone(), |d, message| {
                d.child(
                    div()
                        .id("pdf-status")
                        .role(Role::Status)
                        .aria_label(message.clone())
                        .mt(px(12.))
                        .text_size(px(12.))
                        .line_height(relative(1.5))
                        .text_color(if pdf.failed {
                            theme.accent
                        } else {
                            theme.muted
                        })
                        .child(message),
                )
            })
            .when(pdf.message.is_none(), |d| {
                d.child(
                    div()
                        .mt(px(10.))
                        .text_size(px(11.))
                        .text_color(theme.muted)
                        .child("Images travel with the page. Attachments appear as file labels."),
                )
            });
        div().flex_none().child(panel)
    }
}

#[cfg(feature = "visual-tests")]
pub fn pdf_ui_check(directory: &std::path::Path) -> anyhow::Result<()> {
    use std::sync::Arc;
    std::fs::create_dir_all(directory)?;
    let root =
        std::env::temp_dir().join(format!("omarchy-pdf-ui-{}", notes_core::document::new_id()));
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let mut store = notes_storage::Store::open(&root)?;
    let mut title = notes_core::document::Block::paragraph("A quieter place to think");
    title.set_kind("h1");
    let mut body = Document {
        blocks: vec![
            title,
            notes_core::document::Block::paragraph(
                "Some ideas deserve a little room. Take the thought out of your notebook and give it a page of its own.",
            ),
            notes_core::document::Block::paragraph(
                "Thoughtful margins. Type you can settle into. A small detail, made with care.",
            ),
        ],
        ..Default::default()
    };
    let mut check = notes_core::document::Block::paragraph("Make something worth sharing");
    check.set_kind("check");
    body.blocks.push(check);
    let note = store.create(body, None)?;
    drop(store);
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
    fn draw(
        cx: &mut HeadlessAppContext,
        h: WindowHandle<Shell>,
        directory: &std::path::Path,
        name: &str,
    ) -> anyhow::Result<()> {
        cx.run_until_parked();
        let cleanup = cx.update_window(h.into(), |_, w, cx| {
            w.refresh();
            w.draw(cx)
        })?;
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
        cx.capture_screenshot(h.into())?
            .save(directory.join(name))?;
        Ok(())
    }
    for (width, height, appearance, name) in [
        (1440., 860., Appearance::Reference, "pdf-dark.png"),
        (1440., 860., Appearance::Light, "pdf-light.png"),
        (420., 860., Appearance::Reference, "pdf-narrow.png"),
        (420., 420., Appearance::Reference, "pdf-small.png"),
    ] {
        let deadline = Instant::now() + Duration::from_secs(5);
        let store = loop {
            match notes_storage::Store::open(&root) {
                Ok(store) => break store,
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        let (worker, library) = Worker::from_store(store)?;
        let h = cx.open_window(size(px(width), px(height)), |w, cx| {
            cx.new(|cx| Shell::new_live(appearance, root.clone(), worker, library, w, cx))
        })?;
        h.update(&mut cx, |s, w, cx| {
            s.command(Command::Select(note.id.clone()), w, cx);
            s.command(Command::PdfPanel, w, cx);
        })?;
        draw(&mut cx, h, directory, name)?;
        if width == 1440. && appearance == Appearance::Reference {
            let output = directory.join("snapshot.pdf");
            h.update(&mut cx, |s, w, cx| {
                let (op, doc, root, paper, _) = s.pdf_snapshot(cx).unwrap();
                s.start_pdf(op, doc, root, output, paper, w, cx);
            })?;
            cx.run_until_parked();
            h.update(&mut cx, |s, _, _| {
                let pdf = &s.live.as_ref().unwrap().pdf;
                assert!(!pdf.busy());
                assert!(!pdf.failed, "{:?}", pdf.message);
            })?;
            draw(&mut cx, h, directory, "pdf-success.png")?;
            h.update(&mut cx, |s, w, cx| {
                let (op, doc, root, paper, _) = s.pdf_snapshot(cx).unwrap();
                s.start_pdf(op, doc, root, directory.join("snapshot.pdf"), paper, w, cx);
            })?;
            cx.run_until_parked();
            h.update(&mut cx, |s, _, _| {
                assert!(s.live.as_ref().unwrap().pdf.failed)
            })?;
            draw(&mut cx, h, directory, "pdf-error.png")?;
        }
        h.update(&mut cx, |_, w, _| w.remove_window())?;
        cx.run_until_parked();
    }
    println!(
        "Passed real-font GPU PDF sheet at 1440/420, dark/light, successful export and protected-file failure."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn native_picker_snapshot_cancel_no_overwrite_and_stale_results() {
        let root = tempfile::tempdir().unwrap();
        let mut store = notes_storage::Store::open(root.path()).unwrap();
        let note = store
            .create(
                Document {
                    blocks: vec![notes_core::document::Block::paragraph("Original snapshot")],
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        let other = store
            .create(
                Document {
                    blocks: vec![notes_core::document::Block::paragraph("A different note")],
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        drop(store);
        let (worker, library) = Worker::start(root.path().into(), false).unwrap();
        let mut app = TestApp::new();
        let mut window = app.open_window(|w, cx| {
            Shell::new_live(
                Appearance::Reference,
                root.path().into(),
                worker,
                library,
                w,
                cx,
            )
        });
        window.update(|s, w, cx| {
            s.command(Command::Select(note.id.clone()), w, cx);
            s.command(Command::PdfPanel, w, cx);
            s.command(Command::PdfExport, w, cx);
            assert!(!s.request_close(w, cx), "export must hold the window open");
        });
        assert!(app.did_prompt_for_new_path());
        window.update(|s, w, cx| {
            let editor = s.live.as_ref().unwrap().notes[&note.id]
                .editor
                .as_ref()
                .unwrap()
                .clone();
            editor.update(cx, |e, cx| {
                e.edit(
                    |e| {
                        e.select(e.text().len(), false);
                        e.insert(" NEWER", 0)
                    },
                    w,
                    cx,
                )
            });
            s.command(Command::Select(other.id.clone()), w, cx);
        });
        let path = root.path().join("snapshot.pdf");
        app.simulate_new_path_selection(|_| Some(path.clone()));
        app.run_until_parked();
        window.read(|s, _| {
            let pdf = &s.live.as_ref().unwrap().pdf;
            assert!(!pdf.busy());
            assert!(!pdf.failed, "{:?}", pdf.message);
            assert!(pdf.message.as_ref().unwrap().contains("Original snapshot"));
            assert!(pdf.message.as_ref().unwrap().contains("Newer edits"));
            assert_eq!(pdf.output.as_ref(), Some(&path));
            assert_eq!(s.library.selected.as_ref(), Some(&other.id));
        });
        let original = std::fs::read(&path).unwrap();
        window.update(|s, w, cx| s.command(Command::PdfExport, w, cx));
        app.simulate_new_path_selection(|_| Some(path.clone()));
        app.run_until_parked();
        window.read(|s, _| {
            let pdf = &s.live.as_ref().unwrap().pdf;
            assert!(pdf.failed);
            assert!(pdf.output.is_none());
        });
        assert_eq!(std::fs::read(&path).unwrap(), original);
        window.update(|s, w, cx| s.command(Command::PdfExport, w, cx));
        app.simulate_new_path_selection(|_| None);
        app.run_until_parked();
        window.update(|s, _, cx| {
            assert!(!s.live.as_ref().unwrap().pdf.busy());
            let message = s.live.as_ref().unwrap().pdf.message.clone();
            s.finish_pdf(
                0,
                path,
                Ok(Report {
                    pages: 99,
                    attachment_labels: 0,
                }),
                cx,
            );
            assert_eq!(s.live.as_ref().unwrap().pdf.message, message);
        });
        window.update(|_, w, _| w.remove_window());
    }
    #[::core::prelude::v1::test]
    fn cancellation_before_render_and_composition_block_export() {
        let root = tempfile::tempdir().unwrap();
        let mut store = notes_storage::Store::open(root.path()).unwrap();
        let note = store.create(Document::default(), None).unwrap();
        drop(store);
        let (worker, library) = Worker::start(root.path().into(), false).unwrap();
        let mut app = TestApp::new();
        let mut window = app.open_window(|w, cx| {
            Shell::new_live(
                Appearance::Reference,
                root.path().into(),
                worker,
                library,
                w,
                cx,
            )
        });
        let path = root.path().join("cancelled.pdf");
        window.update(|s, w, cx| {
            let (op, doc, root, paper, _) = s.pdf_snapshot(cx).unwrap();
            s.start_pdf(op, doc, root, path.clone(), paper, w, cx);
            s.command(Command::PdfCancel, w, cx);
        });
        window.read(|s, _| assert!(!s.live.as_ref().unwrap().pdf.busy()));
        assert!(!path.exists());
        window.update(|s, w, cx| {
            let editor = s.live.as_ref().unwrap().notes[&note.id]
                .editor
                .as_ref()
                .unwrap()
                .clone();
            editor.update(cx, |e, cx| {
                e.replace_and_mark_text_in_range(None, "日本", None, w, cx)
            });
            assert!(s.pdf_snapshot(cx).is_none());
            assert!(
                s.live
                    .as_ref()
                    .unwrap()
                    .pdf
                    .message
                    .as_ref()
                    .unwrap()
                    .contains("composing")
            );
        });
        window.update(|_, w, _| w.remove_window());
    }
}
