//! Window requests run on the UI thread, over the existing library controller.
use super::*;
use notes_core::launch::{Activation, Admission, Intent, Opened, Recent, Reply};
use notes_platform::launch::{Receiver, Request};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::time::Instant;
#[cfg(feature = "visual-tests")]
#[path = "window_launch_check.rs"]
mod check;
#[cfg(feature = "visual-tests")]
pub use check::window_launch_check;

struct Activating {
    native: notes_platform::activation::Pending,
    request: Request,
    handle: AnyWindowHandle,
    note_id: Option<String>,
    deadline: Instant,
}

#[derive(Default)]
pub(super) struct State {
    receiver: Option<Receiver>,
    recent: Recent,
    busy: bool,
    creating: Option<(u64, Request)>,
    activating: Option<Activating>,
    _shutdown: Option<Subscription>,
}

impl Shell {
    pub(crate) fn configure_launches(
        &mut self,
        receiver: Receiver,
        intent: Intent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.launches.receiver = Some(receiver);
        self.launches._shutdown = Some(cx.on_app_quit(move |s, _| {
            // GPUI's pinned App::shutdown invokes this synchronously BEFORE it
            // clears windows or tears down the platform display. Drop our
            // borrowed backend here, not in the returned shutdown future.
            s.launches.activating = None;
            async {}
        }));
        match intent {
            Intent::New => self.create_real_note(false, w, cx),
            Intent::Note { id } => self.command(Command::Select(id), w, cx),
            Intent::Main | Intent::Quick => {}
        }
    }
    pub(super) fn poll_launches(&mut self, cx: &mut Context<Self>) {
        if self.launches.activating.is_some() {
            let shell = cx.entity();
            cx.defer(move |cx| poll_activation(&shell, cx));
            return;
        }
        if self.launches.busy
            || self.organization.open()
            || self
                .live
                .as_ref()
                .is_some_and(|l| l.creating.is_some() || l.closing)
        {
            return;
        }
        let Some(request) = self
            .launches
            .receiver
            .as_ref()
            .and_then(|r| r.try_recv().ok())
        else {
            return;
        };
        if Instant::now() >= request.expires {
            let _ = request.reply.try_send(Err(
                "The window request expired before it could run. No window was opened for it."
                    .into(),
            ));
            return;
        }
        let admission = self.launches.recent.admit(
            request.id.clone(),
            request.intent.clone(),
            request.token.clone(),
        );
        match admission {
            Admission::Replay(reply) => {
                let _ = request.reply.try_send(reply);
                return;
            }
            Admission::Pending => {
                let _ = request.reply.try_send(Err(
                    "This window request is already running. Check the app before repeating it."
                        .into(),
                ));
                return;
            }
            Admission::Different => {
                let _ = request.reply.try_send(Err(
                    "This request ID was already used for a different window action.".into(),
                ));
                return;
            }
            Admission::Full => {
                let _ = request.reply.try_send(Err(
                    "Window requests are busy. This request was not started.".into(),
                ));
                return;
            }
            Admission::Start => {}
        }
        self.launches.busy = true;
        let shell = cx.entity();
        cx.defer(move |cx| dispatch(&shell, request, cx));
    }
    fn finish_launch(&mut self, request: Request, result: Reply, cx: &mut Context<Self>) {
        self.launches.recent.complete(&request.id, result.clone());
        let _ = request.reply.try_send(result);
        self.launches.busy = false;
        cx.notify();
    }
    pub(super) fn cancel_activation_for(
        &mut self,
        handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        if self
            .launches
            .activating
            .as_ref()
            .is_some_and(|a| a.handle == handle)
        {
            let active = self.launches.activating.take().unwrap();
            // Drop while the native target (and hence its display) still lives.
            drop(active.native);
            self.finish_launch(active.request, Err("The target window closed before activation was confirmed. Check the app before repeating --new.".into()), cx);
        }
    }
    pub(super) fn created_launch(
        &mut self,
        operation: u64,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .launches
            .creating
            .as_ref()
            .is_none_or(|p| p.0 != operation)
        {
            return;
        }
        let (_, request) = self.launches.creating.take().unwrap();
        let id = match result {
            Ok(id) => id,
            Err(error) => {
                self.finish_launch(
                    request,
                    Err(format!("Couldn’t create the note. {error}")),
                    cx,
                );
                return;
            }
        };
        let Some(handle) = self.windows.main else {
            self.finish_launch(
                request,
                Err(
                    "The note was created, but its window closed before opening was confirmed."
                        .into(),
                ),
                cx,
            );
            return;
        };
        let shell = cx.entity();
        cx.defer(move |cx| activate(&shell, request, handle, Some(id), cx));
    }
}

fn dispatch(shell: &Entity<Shell>, request: Request, cx: &mut App) {
    let changing_note = matches!(request.intent, Intent::New | Intent::Note { .. });
    let composing = shell
        .read(cx)
        .library
        .selected
        .as_ref()
        .and_then(|id| shell.read(cx).live.as_ref()?.notes.get(id))
        .is_some_and(|s| s.input_is_composing(cx));
    if changing_note && composing {
        shell.update(cx, |s, cx| s.finish_launch(request, Err("Finish the current text composition before opening another note. Your draft is unchanged.".into()), cx));
        return;
    }
    // Validate before changing the current selection or creating a main window.
    if let Intent::Note { id } = &request.intent {
        let available = shell.read(cx).live.as_ref().unwrap().notes.contains_key(id);
        if !available {
            shell.update(cx, |s, cx| s.finish_launch(request, Err("That note is not available in this library. The current note is unchanged.".into()), cx));
            return;
        }
    }
    let handle = match &request.intent {
        // Defer activation until the requested action has succeeded. In
        // particular, a supplied token must not trigger an implicit fallback.
        Intent::Quick => quick::ensure_quick(shell, false, cx),
        _ => quick::ensure_main(shell, false, cx),
    };
    let handle = match handle {
        Ok(h) => h,
        Err(e) => {
            shell.update(cx, |s, cx| {
                s.finish_launch(request, Err(format!("Couldn’t open the window. {e}")), cx)
            });
            return;
        }
    };
    let mut request = Some(request);
    let result = handle.update(cx, |_, w, cx| shell.update(cx, |s, cx| {
        let intent = request.as_ref().unwrap().intent.clone();
        match intent {
            Intent::New => {
                if s.live.as_ref().unwrap().creating.is_some() { return Err("Another note is being created. This request was not started.".into()); }
                s.create_real_note(false, w, cx);
                if let Some((op, _)) = s.live.as_ref().unwrap().creating {
                    s.launches.creating = Some((op, request.take().unwrap()));
                } else { return Err("Storage is busy. No new note was created.".into()); }
                Ok(None)
            }
            Intent::Note { id } => {
                if !s.take_quick_into_main(&id, w, cx) { return Err("Finish the current insertion or composition before moving this Quick Note into Notes.".into()); }
                s.library.scope = Scope::All; s.library.query.clear();
                s.search.update(cx, |s, cx| s.clear_silently(w, cx));
                s.queue_library_query(false, cx);
                s.command(Command::Select(id.clone()), w, cx);
                Ok(Some(id))
            }
            Intent::Quick => Ok(s.windows.quick.as_ref().map(|q| q.1.clone())),
            Intent::Main => Ok(s.library.selected.clone()),
        }
    }));
    // A New Note reply waits for the existing writer's committed creation event.
    let Some(request) = request else {
        return;
    };
    match result {
        Ok(Ok(id)) => activate(shell, request, handle, id, cx),
        Ok(Err(error)) => shell.update(cx, |s, cx| s.finish_launch(request, Err(error), cx)),
        Err(error) => shell.update(cx, |s, cx| {
            s.finish_launch(
                request,
                Err(format!(
                    "The window closed before opening was confirmed. {error}"
                )),
                cx,
            )
        }),
    }
}

fn activate(
    shell: &Entity<Shell>,
    request: Request,
    handle: AnyWindowHandle,
    note_id: Option<String>,
    cx: &mut App,
) {
    if request.token.is_some() {
        let native = handle.update(cx, |_, w, _| {
            let display = HasDisplayHandle::display_handle(w)?.as_raw();
            // SAFETY: only UI-owned windows reach this path. Every removal
            // cancels first; configure_launches also drops on app quit BEFORE
            // GPUI clears windows. This app never changes display backends.
            unsafe { notes_platform::activation::Pending::new(display) }
        });
        if let Ok(Ok(native)) = native {
            shell.update(cx, |s, _| {
                s.launches.activating = Some(Activating {
                    native,
                    request,
                    handle,
                    note_id,
                    deadline: Instant::now() + Duration::from_secs(2),
                })
            });
            return;
        }
    }
    let activation = if request.token.is_some() {
        Activation::Unavailable
    } else {
        Activation::Requested
    };
    let result = handle.update(cx, |_, w, _| {
        if request.token.is_none() {
            w.activate_window();
        }
    });
    let result = result
        .map(|()| Opened {
            intent: request.intent.clone(),
            note_id,
            activation,
        })
        .map_err(|_| "The window closed before opening was confirmed.".into());
    shell.update(cx, |s, cx| s.finish_launch(request, result, cx));
}

fn poll_activation(shell: &Entity<Shell>, cx: &mut App) {
    let Some(mut active) = shell.update(cx, |s, _| s.launches.activating.take()) else {
        return;
    };
    let result = active.handle.update(cx, |_, w, _| -> anyhow::Result<bool> {
        let display = HasDisplayHandle::display_handle(w)?.as_raw();
        let surface = HasWindowHandle::window_handle(w)?.as_raw();
        // SAFETY: native handles are borrowed only for this live window update;
        // the display lifetime is guarded at construction and all close paths.
        unsafe {
            active
                .native
                .poll(display, surface, active.request.token.as_ref().unwrap())
        }
    });
    let activation = match result {
        Err(_) => {
            drop(active.native);
            shell.update(cx, |s, cx| {
                s.finish_launch(
                    active.request,
                    Err("The target window closed before opening was confirmed.".into()),
                    cx,
                )
            });
            return;
        }
        Ok(Ok(true)) => Activation::TokenRequested,
        Ok(Ok(false)) if Instant::now() < active.deadline => {
            shell.update(cx, |s, _| s.launches.activating = Some(active));
            return;
        }
        _ => Activation::Unavailable,
    };
    drop(active.native);
    let opened = Opened {
        intent: active.request.intent.clone(),
        note_id: active.note_id,
        activation,
    };
    shell.update(cx, |s, cx| s.finish_launch(active.request, Ok(opened), cx));
}
