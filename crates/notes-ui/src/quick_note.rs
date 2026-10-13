//! A small writing window over the same library and editor entities as Notes.
//! Only one window may render a session; moving it keeps its model and history.
use super::*;
use crate::rich_editor::MediaRequest;
use notes_storage::{Note, Request};
use std::time::Instant;
#[cfg(feature = "visual-tests")]
#[path = "quick_note_check.rs"]
mod check;
#[cfg(feature = "visual-tests")]
pub use check::quick_note_check;

#[derive(Default)]
pub(super) struct Windows {
    pub main: Option<AnyWindowHandle>,
    pub quick: Option<(AnyWindowHandle, String)>,
    pub notice: Option<String>,
    opening: bool,
    closing: bool,
}
impl Windows {
    pub(super) fn cancel_quick_close(&mut self) {
        self.closing = false;
    }
}

pub(crate) struct QuickView {
    shell: Entity<Shell>,
    focus: FocusHandle,
    _subscription: Subscription,
}

pub(super) fn options(cx: &App, quick: bool) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            if quick {
                size(px(680.), px(660.))
            } else {
                size(px(1440.), px(960.))
            },
            cx,
        ))),
        titlebar: None,
        app_id: Some("org.omarchy.Notes.Rust".into()),
        window_background: WindowBackgroundAppearance::Opaque,
        window_min_size: Some(size(px(420.), px(420.))),
        ..Default::default()
    }
}

impl QuickView {
    pub(crate) fn configure_launches(
        &self,
        receiver: notes_platform::launch::Receiver,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shell.update(cx, |s, cx| {
            s.configure_launches(receiver, notes_core::launch::Intent::Quick, w, cx)
        });
    }
    pub(crate) fn configure_preferences(
        &self,
        preferences: super::settings::State,
        cx: &mut Context<Self>,
    ) {
        self.shell
            .update(cx, |s, cx| s.configure_preferences(preferences, cx));
    }
    pub(crate) fn initial(
        appearance: Appearance,
        root: std::path::PathBuf,
        worker: notes_storage::Worker,
        library: notes_storage::Library,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let shell =
            cx.new(|cx| Shell::new_live_window(appearance, root, worker, library, false, w, cx));
        shell.update(cx, |s, _| s.windows.main = None);
        Self::new(shell, w, cx)
    }
    pub(crate) fn new(shell: Entity<Shell>, w: &mut Window, cx: &mut Context<Self>) -> Self {
        shell.update(cx, |s, cx| {
            let mut body = notes_core::document::Document::default();
            body.blocks[0].set_kind("h1");
            let id = notes_core::document::new_id();
            s.attach_note(
                Note {
                    id: id.clone(),
                    folder: None,
                    body,
                    revision: 0,
                    created_at: 0,
                    updated_at: 0,
                    pinned: false,
                },
                w,
                cx,
            );
            s.windows.quick = Some((w.window_handle(), id.clone()));
            s.windows.opening = false;
            s.windows.closing = false;
            s.windows.notice = None;
            let editor = s.ensure_editor(&id, w, cx).unwrap();
            s.sync_link_catalog(cx);
            editor.update(cx, |e, cx| {
                e.activate_media(w, cx);
                w.focus(&e.focus, cx);
            });
            s.trim_idle_editors(cx);
            cx.notify();
        });
        w.set_window_title("Quick Note — Omarchy Notes");
        let weak = shell.downgrade();
        w.on_window_should_close(cx, move |w, cx| {
            weak.update(cx, |s, cx| s.close_quick(w, cx))
                .unwrap_or(true)
        });
        let subscription = cx.observe(&shell, |_, _, cx| cx.notify());
        Self {
            shell,
            focus: cx.focus_handle(),
            _subscription: subscription,
        }
    }
    fn command(&mut self, action: QuickAction, w: &mut Window, cx: &mut Context<Self>) {
        self.shell.update(cx, |s, cx| match action {
            QuickAction::Open => {
                if let Some((_, id)) = &s.windows.quick {
                    s.open_main(Some(id.clone()), cx);
                }
            }
            QuickAction::Save => s.save_real_notes(cx),
            QuickAction::Close => {
                s.close_quick(w, cx);
            }
            QuickAction::Minimize => w.minimize_window(),
        });
    }
    fn button(
        &self,
        id: &'static str,
        name: &'static str,
        action: QuickAction,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let t = self.shell.read(cx).theme;
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(name)
            .tab_index(0)
            .cursor_pointer()
            .px(px(10.))
            .py(px(8.))
            .rounded(px(6.))
            .border_1()
            .border_color(transparent_black())
            .hover(move |s| s.bg(t.overlay))
            .focus_visible(move |s| s.border_color(t.accent))
            .on_click(cx.listener(move |s, _, w, cx| s.command(action, w, cx)))
    }
}

pub(super) fn ensure_main(
    shell: &Entity<Shell>,
    focus: bool,
    cx: &mut App,
) -> anyhow::Result<AnyWindowHandle> {
    if let Some(handle) = shell.read(cx).windows.main {
        return Ok(handle);
    }
    let mut options = options(cx, false);
    options.focus = focus;
    let h = cx.open_window(options, |w, cx| {
        w.set_window_title("Omarchy Notes — Rust edition");
        shell.update(cx, |s, cx| s.bind_main(w, cx));
        shell.clone()
    })?;
    Ok(h.into())
}
pub(super) fn ensure_quick(
    shell: &Entity<Shell>,
    focus: bool,
    cx: &mut App,
) -> anyhow::Result<AnyWindowHandle> {
    if let Some((handle, _)) = shell.read(cx).windows.quick {
        shell.update(cx, |s, _| {
            s.windows.closing = false;
            s.windows.opening = false;
        });
        return Ok(handle);
    }
    let mut options = options(cx, true);
    options.focus = focus;
    Ok(cx
        .open_window(options, |w, cx| {
            cx.new(|cx| QuickView::new(shell.clone(), w, cx))
        })?
        .into())
}

#[derive(Clone, Copy)]
enum QuickAction {
    Open,
    Save,
    Close,
    Minimize,
}

impl Render for QuickView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.shell.read(cx);
        let t = s.theme;
        let note = s
            .windows
            .quick
            .as_ref()
            .and_then(|q| s.live.as_ref()?.notes.get(&q.1));
        let editor = note.and_then(|n| n.active_editor().cloned());
        let status = note
            .map(|n| s.real_status(&n.note.id, cx))
            .unwrap_or_default();
        let error = note
            .and_then(|n| n.error.clone())
            .or_else(|| s.windows.notice.clone())
            .or_else(|| s.live.as_ref()?.message.clone());
        let error_height = (f32::from(window.viewport_size().height) * 0.25).clamp(56., 120.);
        div()
            .id("quick-note-window")
            .role(Role::Group)
            .aria_label("Quick Note")
            .key_context("QuickNote")
            .size_full()
            .flex()
            .flex_col()
            .bg(t.editor)
            .text_color(t.text)
            .font_family("Noto Sans")
            .text_size(px(13.))
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|s, _: &OpenQuickInNotes, w, cx| s.command(QuickAction::Open, w, cx)),
            )
            .on_action(cx.listener(|s, _: &SaveNotes, w, cx| s.command(QuickAction::Save, w, cx)))
            .on_action(cx.listener(|s, _: &CloseNotes, w, cx| s.command(QuickAction::Close, w, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_none()
                    .h(px(62.))
                    .px(px(18.))
                    .border_b_1()
                    .border_color(t.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .on_mouse_down(MouseButton::Left, |_, w, _| w.start_window_move())
                            .child(icon("note", t.accent, 19.))
                            .child(
                                div()
                                    .font_family("Noto Serif")
                                    .text_size(px(20.))
                                    .child("Quick Note"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .child(
                                self.button(
                                    "quick-open",
                                    "Open in Notes · Ctrl Shift O",
                                    QuickAction::Open,
                                    cx,
                                )
                                .child("Open in Notes"),
                            )
                            .child(
                                self.button(
                                    "quick-minimize",
                                    "Minimize Quick Note",
                                    QuickAction::Minimize,
                                    cx,
                                )
                                .child(icon("minus", t.muted, 16.)),
                            )
                            .child(
                                self.button(
                                    "quick-close",
                                    "Save and close Quick Note · Ctrl W",
                                    QuickAction::Close,
                                    cx,
                                )
                                .child(icon("close", t.muted, 16.)),
                            ),
                    ),
            )
            .when_some(error, |d, message| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(22.))
                        .py(px(12.))
                        .bg(t.selected)
                        .text_size(px(12.))
                        .line_height(relative(1.5))
                        .child(
                            div()
                                .id("quick-error-message")
                                .max_h(px(error_height))
                                .overflow_y_scroll()
                                .child(message),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .mt(px(8.))
                                .child(
                                    self.button(
                                        "quick-retry",
                                        "Retry saving Quick Note",
                                        QuickAction::Save,
                                        cx,
                                    )
                                    .child("Retry"),
                                )
                                .child(
                                    self.button(
                                        "quick-recovery",
                                        "Open note and recovery tools",
                                        QuickAction::Open,
                                        cx,
                                    )
                                    .child("Open in Notes…"),
                                ),
                        ),
                )
            })
            .child(div().flex_1().min_h_0().overflow_hidden().children(editor))
            .child(
                div()
                    .flex_none()
                    .h(px(42.))
                    .px(px(22.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(t.border)
                    .child(
                        div()
                            .id("quick-save-status")
                            .role(Role::Status)
                            .aria_label(status.clone())
                            .text_size(px(11.))
                            .text_color(t.muted)
                            .child(status),
                    )
                    .child(label("Ctrl Shift O  Open in Notes", t)),
            )
    }
}

impl Shell {
    pub(super) fn open_quick(&mut self, cx: &mut Context<Self>) {
        if self.live.is_none() || self.windows.opening {
            return;
        }
        if let Some((handle, _)) = self.windows.quick {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, w, _| w.activate_window());
            });
            return;
        }
        self.windows.opening = true;
        let shell = cx.entity();
        cx.defer(move |cx| {
            if let Err(error) = ensure_quick(&shell, true, cx) {
                shell.update(cx, |s, cx| {
                    s.windows.opening = false;
                    s.live.as_mut().unwrap().message =
                        Some(format!("Couldn’t open Quick Note. {error}"));
                    cx.notify();
                });
            }
        });
    }
    pub(super) fn open_main(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let shell = cx.entity();
        cx.defer(move |cx| {
            let handle = match ensure_main(&shell, true, cx) {
                Ok(handle) => handle,
                Err(error) => {
                    shell.update(cx, |s, cx| {
                        s.live.as_mut().unwrap().message =
                            Some(format!("Couldn’t open Notes. {error}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = handle.update(cx, |_, w, cx| {
                shell.update(cx, |s, cx| {
                    if let Some(id) = id {
                        s.library.scope = Scope::All;
                        s.library.query.clear();
                        s.search.update(cx, |f, cx| f.clear_silently(w, cx));
                        s.queue_library_query(false, cx);
                        s.command(Command::Select(id), w, cx);
                    }
                });
                w.activate_window();
            });
        });
    }
    pub(super) fn take_quick_into_main(
        &mut self,
        id: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((quick, quick_id)) = &self.windows.quick else {
            return true;
        };
        if quick_id != id {
            return true;
        }
        let quick = *quick;
        let live = self.live.as_mut().unwrap();
        let session = live.notes.get_mut(id).unwrap();
        let Some(editor) = session.active_editor() else {
            return false;
        };
        if !editor.update(cx, |e, cx| e.rebind_window(w, cx)) {
            self.windows.notice = Some("Finish the current insertion or text composition before moving this note. Your draft is still in Quick Note.".into());
            cx.notify();
            return false;
        }
        session.owner = w.window_handle();
        // Explicitly keeping an untouched Quick Note in the notebook creates it.
        if session.note.revision == 0 && session.revision == 0 {
            session.revision = 1;
        }
        self.windows.quick = None;
        self.windows.closing = false;
        self.windows.notice = None;
        self.cancel_activation_for(quick, cx);
        self.refresh_real_library(cx);
        cx.defer(move |cx| {
            let _ = quick.update(cx, |_, w, _| w.remove_window());
        });
        true
    }
    pub(super) fn remove_main(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.cancel_activation_for(w.window_handle(), cx);
        self.organization.close_with_window();
        self.windows.main = None;
        self.live.as_mut().unwrap().closing = false;
        w.remove_window();
    }
    fn close_quick(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some((_, id)) = &self.windows.quick {
            let s = self.live.as_mut().unwrap().notes.get_mut(id).unwrap();
            if s.input_is_composing(cx) {
                self.windows.notice =
                    Some("Finish composing text before closing. Your draft stays here.".into());
                cx.notify();
                return false;
            }
            self.windows.notice = None;
            s.last_edit = Instant::now() - Duration::from_secs(4);
            self.windows.closing = true;
            self.finish_quick_close(cx);
            cx.notify();
        }
        false
    }
    pub(super) fn finish_quick_close(&mut self, cx: &mut Context<Self>) {
        if !self.windows.closing {
            return;
        }
        let Some((handle, id)) = &self.windows.quick else {
            return;
        };
        let handle = *handle;
        let folder_view_busy = self.windows.main.is_none() && self.folder_view_busy();
        let library_busy = self.library_recovery.busy() || self.portability.busy();
        let live = self.live.as_mut().unwrap();
        let session = &live.notes[id];
        if session.error.is_some() {
            self.windows.closing = false;
            return;
        }
        if self.preferences.busy()
            || library_busy
            || folder_view_busy
            || session.pending.is_some()
            || session.revision != session.saved
            || session.has_unobserved_edit(cx)
            || session.input_is_composing(cx)
            || session.has_pending_work(cx)
        {
            return;
        }
        if session.note.revision == 0 {
            live.notes.remove(id);
        }
        self.windows.quick = None;
        self.windows.closing = false;
        self.cancel_activation_for(handle, cx);
        self.refresh_real_library(cx);
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        });
    }
    /// Only the already verified clean recovery path calls this before removing
    /// its session, so the shared Quick view never renders a removed editor.
    pub(super) fn close_recovery_quick(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((handle, quick_id)) = self.windows.quick.as_ref() else {
            return;
        };
        if quick_id != id {
            return;
        }
        let handle = *handle;
        self.windows.quick = None;
        self.windows.closing = false;
        self.windows.notice = None;
        self.cancel_activation_for(handle, cx);
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        });
    }
    pub(super) fn request_media(&mut self, id: &str, event: &MediaRequest, cx: &mut Context<Self>) {
        let live = self.live.as_mut().unwrap();
        let session = live.notes.get_mut(id).unwrap();
        if session.has_unobserved_edit(cx) {
            let id = id.to_string();
            let event = MediaRequest {
                operation: event.operation,
                content: event.content.clone(),
            };
            let shell = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = shell.update(cx, |s, cx| s.request_media(&id, &event, cx));
            });
            return;
        }
        // A first media insertion needs a durable parent. FIFO on the sole writer
        // ensures creation precedes import; failure leaves both draft and undo intact.
        if session.note.revision == 0 && session.pending.is_none() {
            live.operation += 1;
            let op = live.operation;
            if live
                .worker
                .requests
                .try_send(Request::Save {
                    note: session.note.clone(),
                    local_revision: session.revision,
                    operation: op,
                })
                .is_err()
            {
                self.deliver_import(
                    id,
                    event.operation,
                    Err("Storage is busy. Try adding the files again.".into()),
                    cx,
                );
                return;
            }
            session.pending = Some((op, session.revision));
        }
        if live
            .worker
            .requests
            .try_send(Request::Import {
                id: id.into(),
                operation: event.operation,
                content: event.content.clone(),
            })
            .is_err()
        {
            self.deliver_import(
                id,
                event.operation,
                Err("Storage is busy. Try adding the files again.".into()),
                cx,
            );
        }
    }
    pub(super) fn deliver_import(
        &self,
        id: &str,
        operation: u64,
        result: Result<notes_core::document::Document, String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(s) = self.live.as_ref().and_then(|l| l.notes.get(id)) {
            let owner = s.owner;
            let Some(editor) = s.active_editor().cloned() else {
                return;
            };
            cx.defer(move |cx| {
                let _ = owner.update(cx, |_, w, cx| {
                    editor.update(cx, |e, cx| e.import_result(operation, result, w, cx))
                });
            });
        }
    }
    pub(super) fn route_note_link(&mut self, source: &str, target: &str, cx: &mut Context<Self>) {
        if self.windows.quick.as_ref().is_some_and(|q| q.1 == source) {
            if self.live.as_ref().unwrap().notes.contains_key(target) {
                self.open_main(Some(target.into()), cx);
            } else if let Some(editor) = self
                .live
                .as_ref()
                .unwrap()
                .notes
                .get(source)
                .and_then(|s| s.active_editor())
            {
                editor.update(cx, |e, cx| { e.error = Some("This note isn’t available in this library. You can edit or remove the link.".into()); cx.notify(); });
            }
            return;
        }
        let Some(handle) = self.windows.main else {
            return;
        };
        let shell = cx.weak_entity();
        let source = source.to_string();
        let target = target.to_string();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, w, cx| {
                shell.update(cx, |s, cx| s.follow_note_link(&source, &target, w, cx))
            });
        });
    }
}
