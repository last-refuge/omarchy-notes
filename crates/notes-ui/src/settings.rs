//! Presentation preferences, their persistence feedback and keyboard controls.
use super::*;
use notes_core::preferences::{NoteFont, Patch, Preferences, Session};
#[cfg(feature = "visual-tests")]
#[path = "settings_check.rs"]
mod check;
#[cfg(feature = "visual-tests")]
pub use check::settings_check;

pub(crate) struct State {
    pub session: Session,
    worker: Option<notes_platform::preferences::Worker>,
    persistent: bool,
    pub appearance_override: Option<Appearance>,
    return_focus: Option<FocusHandle>,
    scroll: ScrollHandle,
    pub dragging: bool,
    pub(super) focus: Vec<FocusHandle>,
}
impl State {
    pub fn memory(appearance: Appearance) -> Self {
        Self {
            session: Session::new(
                Preferences {
                    appearance,
                    ..Default::default()
                },
                None,
            ),
            worker: None,
            persistent: false,
            appearance_override: None,
            return_focus: None,
            scroll: ScrollHandle::new(),
            dragging: false,
            focus: vec![],
        }
    }
    pub fn load(appearance_override: Option<Appearance>) -> Self {
        let mut state = Self::memory(Appearance::Omarchy);
        state.persistent = true;
        state.appearance_override = appearance_override;
        match notes_platform::preferences::default_path()
            .and_then(notes_platform::preferences::Worker::start)
        {
            Ok((worker, session)) => {
                state.worker = Some(worker);
                state.session = session;
            }
            Err(error) => state.session.error = Some(format!("Couldn’t open preferences. {error}")),
        }
        state
    }
    pub fn appearance(&self) -> Appearance {
        self.appearance_override
            .unwrap_or(self.session.value.appearance)
    }
    pub fn busy(&self) -> bool {
        self.worker.is_some() && self.session.busy()
    }
}
impl Shell {
    pub(crate) fn configure_preferences(&mut self, mut preferences: State, cx: &mut Context<Self>) {
        preferences.focus = std::mem::take(&mut self.preferences.focus);
        self.preferences = preferences;
        self.apply_preferences(cx);
        if let Some(root) = self.live.as_ref().map(|live| live.root.clone()) {
            self.configure_folder_view(root, cx);
        }
    }
    pub(super) fn poll_preferences(&mut self, cx: &mut Context<Self>) {
        let events: Vec<_> = self
            .preferences
            .worker
            .as_ref()
            .map(|w| w.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            if self
                .preferences
                .session
                .complete(event.operation, event.result)
            {
                self.apply_preferences(cx);
            }
        }
        if let Some(worker) = &self.preferences.worker
            && let Some(job) = self.preferences.session.begin_save()
        {
            let op = job.operation;
            if worker.requests.try_send(job).is_err() {
                self.preferences.session.complete(
                    op,
                    Err(
                        "Preferences couldn’t be saved. Try again; your notes are unaffected."
                            .into(),
                    ),
                );
            }
            cx.notify();
        }
    }
    pub(super) fn change_preferences(&mut self, patch: Patch, cx: &mut Context<Self>) {
        let unavailable = (self.preferences.persistent && self.preferences.worker.is_none())
            .then(|| self.preferences.session.error.clone())
            .flatten();
        if self.preferences.session.change(patch) {
            if let Some(error) = unavailable {
                self.preferences.session.error = Some(error);
            }
            self.apply_preferences(cx);
            self.poll_preferences(cx);
        }
    }
    pub(super) fn apply_preferences(&mut self, cx: &mut Context<Self>) {
        let previous_sort = self.query.sort;
        let p = &self.preferences.session.value;
        if !self.preferences.dragging {
            self.sidebar_width = p.sidebar_width as f32;
            self.list_width = p.list_width as f32;
        }
        self.focused = p.focus_mode;
        if !self.query.sort_loaded {
            self.query.sort = if p.sort_by_title {
                notes_core::library::Sort::Title
            } else {
                notes_core::library::Sort::Edited
            };
        }
        self.library.sort_by_title = self.query.sort == notes_core::library::Sort::Title;
        self.list.remeasure();
        let typography = crate::rich_editor::WritingStyle {
            font: p.note_font,
            size: p.note_size,
        };
        if let Some(live) = &self.live {
            for session in live.notes.values() {
                if let Some(editor) = session.active_editor() {
                    editor.update(cx, |e, cx| e.set_typography(typography, cx));
                }
            }
        }
        self.refresh_query_visibility();
        if previous_sort != self.query.sort {
            self.queue_library_query(false, cx);
        }
        self.appearance = self.preferences.appearance();
        self.apply_theme(cx);
        cx.notify();
    }
    pub(super) fn finish_pane_resize(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.preferences.dragging) {
            return;
        }
        self.change_preferences(
            Patch {
                sidebar_width: Some(self.sidebar_width.round() as u16),
                list_width: Some(self.list_width.round() as u16),
                ..Default::default()
            },
            cx,
        );
    }
    pub(super) fn toggle_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open {
            self.close_settings(window, cx);
            return;
        }
        if self
            .library
            .selected
            .as_ref()
            .and_then(|id| self.live.as_ref()?.notes.get(id))
            .is_some_and(|s| s.input_is_composing(cx))
        {
            return;
        }
        self.preferences.return_focus = window.focused(cx);
        self.settings_open = true;
        self.folders_open = false;
        window.focus(&self.preferences.focus[0], cx);
        cx.notify();
    }
    pub(super) fn move_settings_focus(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut active = (0..14).collect::<Vec<_>>();
        if self.live.is_some() {
            active.push(15);
        }
        if self.preferences.session.error.is_some() && self.preferences.worker.is_some() {
            active.push(14);
        }
        let current = active
            .iter()
            .position(|&i| self.preferences.focus[i].is_focused(window))
            .unwrap_or(0);
        let next_position = if backwards {
            (current + active.len() - 1) % active.len()
        } else {
            (current + 1) % active.len()
        };
        let next = active[next_position];
        let extra = usize::from(self.preferences.appearance_override.is_some());
        let item = match next {
            1..=3 => Some(4),
            4..=6 => Some(6 + extra),
            7..=8 => Some(7 + extra),
            9..=10 => Some(10 + extra),
            11..=12 => Some(11 + extra),
            13 => Some(12 + extra),
            15 => Some(15 + extra),
            _ => None,
        };
        if let Some(item) = item {
            self.preferences.scroll.scroll_to_item(item);
        }
        window.focus(&self.preferences.focus[next], cx);
        cx.notify();
    }
    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_open = false;
        if let Some(focus) = self.preferences.return_focus.take() {
            window.focus(&focus, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }
    fn setting_heading(&self, title: &'static str, detail: &'static str) -> Div {
        div()
            .mt(px(28.))
            .mb(px(12.))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .mt(px(5.))
                    .text_size(px(12.))
                    .text_color(self.theme.muted)
                    .child(detail),
            )
    }
    fn stepper(
        &self,
        id: &'static str,
        name: &'static str,
        value: u16,
        range: std::ops::RangeInclusive<u16>,
        change: fn(u16) -> Patch,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let t = self.theme;
        let focus_index = match id {
            "writing-size" => 7,
            "sidebar-width" => 9,
            _ => 11,
        };
        div()
            .id((id, 3usize))
            .role(Role::Group)
            .aria_label(format!("{name}: {value} pixels"))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .py(px(10.))
            .child(div().text_size(px(13.)).child(name))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        self.control(
                            (id, 0usize),
                            format!("Decrease {name}"),
                            Command::Preference(change(
                                value.saturating_sub(1).max(*range.start()),
                            )),
                            cx,
                        )
                        .track_focus(&self.preferences.focus[focus_index])
                        .size(px(36.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_color(t.border)
                        .child("−"),
                    )
                    .child(
                        div()
                            .id((id, 1usize))
                            .role(Role::Label)
                            .aria_label(format!("{name}: {value} pixels"))
                            .w(px(58.))
                            .text_center()
                            .text_size(px(13.))
                            .child(format!("{value} px")),
                    )
                    .child(
                        self.control(
                            (id, 2usize),
                            format!("Increase {name}"),
                            Command::Preference(change((value + 1).min(*range.end()))),
                            cx,
                        )
                        .track_focus(&self.preferences.focus[focus_index + 1])
                        .size(px(36.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_color(t.border)
                        .child("+"),
                    ),
            )
    }
    pub(super) fn settings(&self, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let t = self.theme;
        let p = &self.preferences.session.value;
        let compact = width < 720.;
        let palettes = [
            (
                Appearance::Reference,
                "Graphite",
                "Quiet, warm darkness.",
                Theme::reference(),
            ),
            (
                Appearance::Light,
                "Warm paper",
                "Room for a little daylight.",
                Theme::light(),
            ),
            (
                Appearance::Omarchy,
                "Your desktop",
                "Follow Omarchy, live.",
                self.omarchy,
            ),
        ];
        let mut cards = div().flex().gap(px(10.));
        for (index, (mode, title, detail, palette)) in palettes.into_iter().enumerate() {
            cards = cards.child(
                self.control(("appearance", index), title, Command::Appearance(mode), cx)
                    .track_focus(&self.preferences.focus[index + 1])
                    .aria_selected(self.appearance == mode)
                    .flex_1()
                    .min_w_0()
                    .p(px(14.))
                    .border_color(if self.appearance == mode {
                        t.accent
                    } else {
                        t.border
                    })
                    .bg(if self.appearance == mode {
                        t.selected
                    } else {
                        t.editor
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .mb(px(12.))
                            .child(
                                div().flex().gap(px(5.)).children(
                                    [palette.navigation, palette.editor, palette.accent]
                                        .into_iter()
                                        .map(|color| {
                                            div()
                                                .size(px(16.))
                                                .rounded_full()
                                                .bg(color)
                                                .border_1()
                                                .border_color(t.border)
                                        }),
                                ),
                            )
                            .child(
                                div()
                                    .text_color(t.accent)
                                    .child(if self.appearance == mode { "✓" } else { " " }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(5.))
                            .text_size(px(11.))
                            .text_color(t.muted)
                            .child(detail),
                    ),
            );
        }
        let mut fonts = div().flex().gap(px(8.));
        for (index, (font, title)) in [
            (NoteFont::Serif, "Serif"),
            (NoteFont::Sans, "Sans"),
            (NoteFont::Mono, "Mono"),
        ]
        .into_iter()
        .enumerate()
        {
            fonts = fonts.child(
                self.control(
                    ("note-font", index),
                    format!("Writing font: {title}"),
                    Command::Preference(Patch {
                        note_font: Some(font),
                        ..Default::default()
                    }),
                    cx,
                )
                .track_focus(&self.preferences.focus[index + 4])
                .aria_selected(p.note_font == font)
                .flex_1()
                .min_w_0()
                .text_center()
                .py(px(10.))
                .font_family(font.family())
                .border_color(if p.note_font == font {
                    t.accent
                } else {
                    t.border
                })
                .bg(if p.note_font == font {
                    t.selected
                } else {
                    t.editor
                })
                .child(title),
            );
        }
        let body = div().w_full().max_w(px(820.)).mx_auto().pb(px(32.))
            .child(div().text_size(px(11.)).text_color(t.accent).child("YOUR NOTEBOOK, YOUR WAY"))
            .child(div().font_family("Noto Serif").text_size(px(if compact { 28. } else { 36. })).mt(px(10.)).child("Make yourself at home."))
            .child(div().mt(px(8.)).text_color(t.muted).text_size(px(13.)).child("A few thoughtful choices for the way you write."))
            .child(self.setting_heading("Appearance", "Choose the atmosphere around your words."))
            .child(cards)
            .when(self.preferences.appearance_override.is_some(), |d| d.child(div().mt(px(8.)).text_size(px(11.)).text_color(t.muted).child("This launch has a palette override. Choose a palette above to remember it.")))
            .child(self.setting_heading("Writing", "Change how notes read, without changing their contents."))
            .child(fonts)
            .child(self.stepper("writing-size", "Writing size", p.note_size, 14..=28, |v| Patch { note_size: Some(v), ..Default::default() }, cx))
            .child(div().id("writing-preview").mt(px(6.)).p(px(if compact { 18. } else { 24. })).rounded(px(10.)).border_1().border_color(t.border).bg(t.navigation).font_family(p.note_font.family()).text_size(px(p.note_size as f32)).line_height(relative(1.55))
                .child("A little space to think.").child(div().mt(px(6.)).text_color(t.muted).child("Keep the details that matter. Let the next thought find its shape.")))
            .child(self.setting_heading("Workspace", "Drag the dividers, or adjust them here with your keyboard."))
            .child(self.stepper("sidebar-width", "Sidebar width", p.sidebar_width, 220..=300, |v| Patch { sidebar_width: Some(v), ..Default::default() }, cx))
            .child(self.stepper("note-list-width", "Note list width", p.list_width, 300..=440, |v| Patch { list_width: Some(v), ..Default::default() }, cx))
            .child(self.control("reset-layout", "Restore comfortable pane widths", Command::Preference(Patch { sidebar_width: Some(248), list_width: Some(352), ..Default::default() }), cx).track_focus(&self.preferences.focus[13]).px(px(12.)).py(px(8.)).text_size(px(12.)).border_color(t.border).child("Restore comfortable widths"))
            .child(div().mt(px(12.)).text_size(px(11.)).text_color(t.muted).child("Narrow windows adapt automatically. Your preferred widths return when there’s room."))
            .when(self.live.is_some(), |d| d.child(self.setting_heading("Your notes and files", "Copy sources, export portable notes or keep a verified library backup.")).child(self.portability_controls(cx)))
            .child(self.setting_heading("A calmer workspace", "Panels stay still, and the writing caret does not blink."))
            .child(div().text_size(px(12.)).line_height(relative(1.7)).text_color(t.muted).child("Ctrl ,  Settings    ·    Ctrl K  Search    ·    Ctrl \\  Focus\nCtrl N  New note    ·    Ctrl Shift N  Quick Note\nCtrl F  Find in note    ·    Ctrl S  Save    ·    Esc  Return"));
        let status = if let Some(error) = &self.preferences.session.error {
            format!("Preferences not saved · {error}")
        } else if !self.preferences.persistent {
            "Sample session · these choices reset when you close the demo.".into()
        } else if self.preferences.session.busy() {
            "Remembering your choices…".into()
        } else {
            "Your choices are remembered on this device.".into()
        };
        div()
            .id("settings-page")
            .role(Role::Group)
            .aria_label("Notebook settings")
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(t.editor)
            .child(
                div()
                    .flex_none()
                    .h(px(52.))
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(t.border)
                    .child(
                        self.icon_button(
                            "close-settings",
                            "Close settings",
                            "back",
                            Command::Settings,
                            cx,
                        )
                        .track_focus(&self.preferences.focus[0]),
                    )
                    .child(label("Notebook settings", t)),
            )
            .child(
                body.id("settings-scroll")
                    .track_scroll(&self.preferences.scroll)
                    .overflow_y_scroll()
                    .flex_1()
                    .min_h_0()
                    .px(px(if compact { 20. } else { 40. }))
                    .pt(px(26.)),
            )
            .child(
                div()
                    .flex_none()
                    .max_h(px(148.))
                    .px(px(20.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(t.border)
                    .bg(t.navigation)
                    .child(
                        div()
                            .id("preferences-status")
                            .role(Role::Status)
                            .aria_label(status.clone())
                            .max_h(px(72.))
                            .overflow_y_scroll()
                            .text_size(px(11.))
                            .text_color(if self.preferences.session.error.is_some() {
                                t.accent
                            } else {
                                t.muted
                            })
                            .child(status),
                    )
                    .when(
                        self.preferences.session.error.is_some()
                            && self.preferences.worker.is_some(),
                        |d| {
                            d.child(
                                self.control(
                                    "retry-preferences",
                                    "Retry saving preferences",
                                    Command::RetryPreferences,
                                    cx,
                                )
                                .track_focus(&self.preferences.focus[14])
                                .mt(px(6.))
                                .px(px(10.))
                                .py(px(5.))
                                .text_size(px(12.))
                                .border_color(t.border)
                                .child("Try again"),
                            )
                        },
                    ),
            )
    }
}
