//! Recently Deleted is a read-only preview with explicit recovery actions.
//! The controller owns all draft barriers, requests, and durable receipts.
use super::*;

impl Shell {
    fn recovery_button(
        &self,
        id: &'static str,
        title: impl Into<SharedString>,
        action: Action,
        focus: Option<usize>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let title = title.into();
        div()
            .id(id)
            .role(if enabled { Role::Button } else { Role::Label })
            .aria_label(title.clone())
            .tab_index(if enabled { 0 } else { -1 })
            .flex_none()
            .px(px(12.))
            .py(px(7.))
            .border_1()
            .border_color(theme.border)
            .rounded(px(6.))
            .text_size(px(12.))
            .when(enabled, |d| {
                d.when_some(focus, |d, focus| {
                    d.track_focus(&self.library_recovery.focus[focus])
                })
                .cursor_pointer()
                .hover(move |s| s.bg(theme.overlay))
                .focus_visible(move |s| s.border_color(theme.accent))
                .on_click(
                    cx.listener(move |s, _, w, cx| {
                        s.library_recovery_action(action.clone(), w, cx)
                    }),
                )
            })
            .when(!enabled, |d| {
                d.opacity(0.5)
                    .aria_description("Unavailable until the current library operation finishes.")
            })
            .child(title)
    }

    pub(in crate::shell) fn recovery_controls(&self, cx: &mut Context<Self>) -> Div {
        let state = &self.library_recovery;
        let enabled = self.live.is_some() && !state.mutating() && state.confirm.is_none();
        let theme = self.theme;
        let menu = div()
            .id("note-actions-menu")
            .role(Role::Group)
            .aria_label("Note actions")
            .absolute()
            .top(px(40.))
            .right(px(0.))
            .w(px(230.))
            .p(px(8.))
            .border_1()
            .border_color(theme.border)
            .rounded(px(8.))
            .bg(theme.navigation)
            .occlude()
            .flex()
            .flex_col()
            .gap(px(6.))
            .on_mouse_down_out(
                cx.listener(|s, _, w, cx| s.library_recovery_action(Action::CloseActions, w, cx)),
            )
            .on_key_down(cx.listener(|s, event: &KeyDownEvent, w, cx| {
                if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                    s.library_recovery_action(Action::CloseActions, w, cx);
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .child(self.recovery_button(
                "duplicate-note",
                "Duplicate",
                Action::Duplicate,
                Some(2),
                enabled && self.library.selected_note().is_some(),
                cx,
            ))
            .child(self.recovery_button(
                "trash-note",
                "Delete note",
                Action::TrashNote,
                Some(3),
                enabled && self.library.selected_note().is_some(),
                cx,
            ))
            .when_some(
                match &self.library.scope {
                    Scope::FolderId(Some(id)) => Some(id.clone()),
                    _ => None,
                },
                |d, id| {
                    d.child(self.recovery_button(
                        "delete-folder",
                        "Delete folder…",
                        Action::DeleteFolder(id),
                        Some(5),
                        enabled,
                        cx,
                    ))
                },
            )
            .when(state.undo.is_some(), |d| {
                d.child(self.recovery_button(
                    "undo-note-deletion",
                    "Undo deletion",
                    Action::UndoTrash,
                    Some(4),
                    enabled,
                    cx,
                ))
            });
        div()
            .relative()
            .flex_none()
            .child(
                self.recovery_button(
                    "note-actions",
                    "⋯",
                    Action::ToggleActions,
                    None,
                    enabled,
                    cx,
                )
                .aria_label("Note actions")
                .aria_expanded(state.actions_open)
                .text_size(px(22.))
                .py(px(0.))
                .h(px(34.)),
            )
            .when(state.actions_open, |d| {
                d.child(gpui::deferred(menu).priority(1))
            })
    }

    pub(in crate::shell) fn recovery_status(&self) -> Div {
        let state = &self.library_recovery;
        let message = state.error.as_ref().or(state.message.as_ref());
        div()
            .text_size(px(12.))
            .text_color(self.theme.muted)
            .when_some(message.cloned(), |d, message| {
                d.child(
                    div()
                        .id("library-recovery-status")
                        .role(Role::Status)
                        .aria_label(message.clone())
                        .max_h(px(72.))
                        .overflow_y_scroll()
                        .child(message),
                )
            })
            .when(state.busy(), |d| {
                d.child(
                    div()
                        .id("library-recovery-progress")
                        .role(Role::Status)
                        .aria_label("Waiting for the library operation")
                        .child("Waiting for your library…"),
                )
            })
    }

    pub(in crate::shell) fn recovery_retained_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let enabled = !self.library_recovery.mutating();
        let theme = self.theme;
        div()
            .id("retained-draft-controls")
            .flex()
            .flex_col()
            .gap(px(6.))
            .max_h(px(120.))
            .overflow_y_scroll()
            .children(
                self.library_recovery
                    .retained_drafts
                    .iter()
                    .map(|(id, title)| {
                        let id_for_action = id.clone();
                        let title = format!("View retained draft: {title}");
                        div()
                            .id(SharedString::from(format!("retained-draft-{id}")))
                            .role(if enabled { Role::Button } else { Role::Label })
                            .aria_label(title.clone())
                            .tab_index(if enabled { 0 } else { -1 })
                            .px(px(10.))
                            .py(px(6.))
                            .border_1()
                            .border_color(theme.border)
                            .rounded(px(6.))
                            .text_size(px(12.))
                            .text_color(theme.accent)
                            .when(enabled, |d| {
                                d.cursor_pointer()
                                    .focus_visible(move |s| s.border_color(theme.accent))
                                    .on_click(cx.listener(move |s, _, w, cx| {
                                        s.library_recovery_action(
                                            Action::ViewRetainedDraft(id_for_action.clone()),
                                            w,
                                            cx,
                                        )
                                    }))
                            })
                            .child(title)
                    }),
            )
    }

    pub(in crate::shell) fn recovery_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.library_recovery.confirm.is_some() {
            return self.recovery_confirmation(cx);
        }
        let state = &self.library_recovery;
        let theme = self.theme;
        let enabled = !state.mutating();
        let selected = state.selected.as_ref();
        let ready = state
            .preview
            .as_ref()
            .is_some_and(|p| Some(&p.note.id) == selected);
        let list = div()
            .id("deleted-note-list")
            .role(Role::ListBox)
            .aria_label("Deleted notes")
            .aria_description("Use Up, Down, Home, or End to select a read-only preview.")
            .track_focus(&state.focus[1])
            .tab_index(0)
            .border_1()
            .border_color(theme.border)
            .focus_visible(move |s| s.border_color(theme.accent))
            .rounded(px(8.))
            .overflow_hidden()
            .flex_1()
            .max_h(px(260.))
            .min_h(px(64.))
            .when(state.rows.is_empty(), |d| {
                d.child(
                    div()
                        .p(px(18.))
                        .text_color(theme.muted)
                        .child(if state.loading {
                            "Loading deleted notes…"
                        } else if state.error.is_some() {
                            "Deleted notes could not be loaded. Your library has not changed."
                        } else {
                            "No notes in Recently Deleted."
                        }),
                )
            })
            .when(!state.rows.is_empty(), |d| {
                d.child(
                    uniform_list(
                        "deleted-note-rows",
                        state.rows.len(),
                        cx.processor(|s, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| s.deleted_note_row(index, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .track_scroll(&state.scroll)
                    .size_full(),
                )
            });
        let preview = div()
            .id("deleted-note-preview")
            .role(Role::Document)
            .aria_label(if ready {
                format!("Read-only preview of {}", state.preview_title)
            } else {
                "Read-only deleted note preview".into()
            })
            .flex_1()
            .min_h_0()
            .border_1()
            .border_color(theme.border)
            .rounded(px(8.))
            .overflow_y_scroll()
            .p(px(18.))
            .when(ready, |d| {
                d.child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD)
                    .mb(px(10.)).child(state.preview_title.clone()))
                    .child(div().font_family("Noto Serif").text_size(px(17.))
                        .line_height(relative(1.6)).child(state.preview_text.clone()))
                    .when(state.preview_truncated, |d| {
                        d.child(div().mt(px(12.)).text_size(px(11.)).text_color(theme.muted)
                            .child("This preview is shortened. Restoring keeps the complete note, formatting, and attachments."))
                    })
            })
            .when(!ready, |d| {
                d.child(div().text_color(theme.muted).child(if state.selected.is_some() {
                    "Loading the read-only preview…"
                } else {
                    "Select a deleted note to preview it."
                }))
            });
        div()
            .id("recently-deleted-page")
            .role(Role::Group)
            .aria_label("Recently Deleted")
            .key_context("LibraryRecovery")
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(theme.editor)
            .on_key_down(cx.listener(|s, event: &KeyDownEvent, w, cx| {
                if event.keystroke.modifiers.modified() || s.library_recovery.mutating() {
                    return;
                }
                let list_focused = s.library_recovery.focus[1].is_focused(w);
                let action = match event.keystroke.key.as_str() {
                    "escape" => Action::CloseTrash,
                    "up" if list_focused => Action::StepTrash(false),
                    "down" if list_focused => Action::StepTrash(true),
                    "home" if list_focused => Action::FirstTrash,
                    "end" if list_focused => Action::LastTrash,
                    _ => return,
                };
                s.library_recovery_action(action, w, cx);
                w.prevent_default();
                cx.stop_propagation();
            }))
            .child(div().h(px(54.)).flex_none().px(px(16.)).flex().items_center()
                .gap(px(12.)).border_b_1().border_color(theme.border)
                .child(self.recovery_button("close-recently-deleted", "Back", Action::CloseTrash, Some(0), enabled, cx))
                .child(div().font_family("Noto Serif").text_size(px(22.)).child("Recently Deleted")))
            .child(div().flex_1().min_h_0().flex().flex_col().p(px(14.)).gap(px(12.))
                .child(div().text_size(px(12.)).text_color(theme.muted)
                    .child("Deleted notes can be restored for 30 days, then are removed automatically. Restoring a note does not recreate deleted folders."))
                .child(list)
                .when(state.has_more, |d| d.child(self.recovery_button("more-deleted-notes", "Load more deleted notes", Action::MoreTrash, None, enabled, cx)))
                .child(preview)
                .child(self.recovery_status())
                .child(self.recovery_retained_controls(cx))
                .child(div().flex().flex_wrap().gap(px(8.))
                    .child(self.recovery_button("restore-deleted-note", "Restore note", Action::RestoreSelected, Some(2), enabled && ready && !state.busy(), cx)
                        .border_color(theme.accent).text_color(theme.accent))
                    .child(self.recovery_button("request-permanent-deletion", "Delete permanently…", Action::RequestPermanent, Some(3), enabled && ready && !state.busy(), cx))
                    .when(state.undo.is_some(), |d| d.child(self.recovery_button("undo-from-deleted-notes", "Undo deletion", Action::UndoTrash, Some(4), enabled, cx)))))
            .into_any_element()
    }

    fn deleted_note_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.library_recovery.rows[index];
        let title = if row.title.is_empty() {
            "Untitled note"
        } else {
            &row.title
        };
        let selected = self.library_recovery.selected.as_ref() == Some(&row.id);
        let id = row.id.clone();
        let theme = self.theme;
        div()
            .id(SharedString::from(format!("deleted-note-{}", row.id)))
            .role(Role::ListBoxOption)
            .aria_label(title.to_owned())
            .aria_description(row.snippet.clone())
            .aria_selected(selected)
            .when(selected, |d| d.aria_active_descendant())
            .h(px(72.))
            .w_full()
            .px(px(14.))
            .py(px(10.))
            .border_b_1()
            .border_color(theme.border)
            .bg(if selected { theme.selected } else { theme.list })
            .cursor_pointer()
            .hover(move |s| s.bg(theme.overlay))
            .on_click(cx.listener(move |s, _, w, cx| {
                s.library_recovery_action(Action::SelectTrash(id.clone()), w, cx)
            }))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(title.to_owned()),
            )
            .child(
                div()
                    .mt(px(4.))
                    .text_size(px(11.))
                    .text_color(theme.muted)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(row.snippet.clone()),
            )
            .into_any_element()
    }

    fn recovery_confirmation(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.library_recovery;
        let confirmation = state.confirm.as_ref().unwrap();
        let theme = self.theme;
        let enabled = !state.mutating();
        div()
            .id("library-deletion-confirmation")
            .role(Role::Dialog)
            .aria_label(confirmation.title.clone())
            .key_context("LibraryRecovery")
            .size_full()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.))
            .bg(theme.navigation)
            .on_key_down(cx.listener(|s, event: &KeyDownEvent, w, cx| {
                if event.keystroke.key == "escape"
                    && !event.keystroke.modifiers.modified()
                    && !s.library_recovery.mutating()
                {
                    s.library_recovery_action(Action::Cancel, w, cx);
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .w_full()
                    .max_w(px(600.))
                    .max_h_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .p(px(22.))
                    .gap(px(18.))
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(12.))
                    .bg(theme.editor)
                    .child(
                        div()
                            .font_family("Noto Serif")
                            .text_size(px(25.))
                            .child(confirmation.title.clone()),
                    )
                    .child(
                        div()
                            .id("deletion-consequences")
                            .role(Role::Label)
                            .aria_label(confirmation.detail.clone())
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .text_size(px(14.))
                            .line_height(relative(1.6))
                            .child(confirmation.detail.clone()),
                    )
                    .child(self.recovery_status())
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .flex_wrap()
                            .justify_end()
                            .gap(px(10.))
                            .child(self.recovery_button(
                                "cancel-library-deletion",
                                "Cancel",
                                Action::Cancel,
                                Some(6),
                                enabled,
                                cx,
                            ))
                            .child(
                                self.recovery_button(
                                    "confirm-library-deletion",
                                    confirmation.confirm_label.clone(),
                                    Action::Confirm,
                                    Some(5),
                                    enabled,
                                    cx,
                                )
                                .when(confirmation.destructive, |d| {
                                    d.border_color(theme.accent).text_color(theme.accent)
                                }),
                            ),
                    ),
            )
            .into_any_element()
    }
}
