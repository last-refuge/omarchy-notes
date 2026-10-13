//! Links keep the editor selection anchored while the destination is chosen.
use super::*;
use crate::search::{SearchEvent, SearchField};
use notes_core::links::LinkTarget;
use std::sync::Arc;

pub(crate) use notes_core::links::LinkNote;
pub(crate) struct OpenNote(pub String);
impl EventEmitter<OpenNote> for RichEditor {}

pub(super) struct State {
    pub open: bool,
    notes_mode: bool,
    field: Entity<SearchField>,
    query: String,
    anchor: Option<(u64, notes_editor::Selection)>,
    pub(super) own_id: String,
    pub(super) catalog: Arc<[LinkNote]>,
    filtered: Vec<usize>,
    selected: usize,
    scroll: UniformListScrollHandle,
    _subscription: Subscription,
}
impl State {
    pub(super) fn rebind(&mut self, w: &mut Window, cx: &mut Context<RichEditor>) {
        self._subscription = cx.subscribe_in(&self.field, w, |s, _, event, w, cx| {
            s.link_event(event, w, cx)
        });
    }
    pub fn new(theme: Theme, w: &mut Window, cx: &mut Context<RichEditor>) -> Self {
        let field = cx.new(|cx| SearchField::named("Link destination", theme, cx));
        let subscription =
            cx.subscribe_in(&field, w, |s, _, event, w, cx| s.link_event(event, w, cx));
        Self {
            open: false,
            notes_mode: false,
            field,
            query: String::new(),
            anchor: None,
            own_id: String::new(),
            catalog: Arc::from([]),
            filtered: vec![],
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            _subscription: subscription,
        }
    }
    fn filter(&mut self) {
        let query = self.query.to_lowercase();
        self.filtered = self
            .catalog
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                n.id != self.own_id
                    && (n.title.to_lowercase().contains(&query)
                        || n.folder.to_lowercase().contains(&query))
            })
            .map(|(i, _)| i)
            .collect();
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
    pub fn composing(&self, cx: &App) -> bool {
        self.field.read(cx).composing()
    }
    pub fn theme(&mut self, theme: Theme, cx: &mut App) {
        self.field.update(cx, |s, cx| {
            s.theme = theme;
            cx.notify();
        });
    }
}
impl RichEditor {
    pub(crate) fn set_link_catalog(
        &mut self,
        own_id: String,
        catalog: Arc<[LinkNote]>,
        cx: &mut Context<Self>,
    ) {
        if self.links.own_id != own_id || self.links.catalog != catalog {
            self.links.own_id = own_id;
            self.links.catalog = catalog;
            self.links.filter();
            self.refresh_related();
            cx.notify();
        }
    }
    pub(crate) fn open_link_panel(&mut self, notes: bool, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        let existing = self.model.current_link();
        if !self.links.open {
            self.links.anchor = Some((self.model.revision, self.model.selection.clone()));
        }
        self.links.open = true;
        self.links.notes_mode = notes;
        self.error = None;
        self.table_menu = false;
        self.format_menu = false;
        self.find_open = false;
        self.links.query = if notes {
            String::new()
        } else {
            existing.map_or(String::new(), |l| l.href)
        };
        self.links.filter();
        let seed = self.links.query.clone();
        self.links
            .field
            .update(cx, |s, cx| s.set_text(&seed, w, cx));
        w.focus(&self.links.field.read(cx).focus.clone(), cx);
        cx.notify();
    }
    pub(crate) fn close_links(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.links.composing(cx) {
            return;
        }
        self.links.open = false;
        self.links.anchor = None;
        self.error = None;
        w.focus(&self.focus, cx);
        cx.notify();
    }
    fn link_anchor_valid(&mut self, cx: &mut Context<Self>) -> bool {
        if self.links.anchor.as_ref().is_some_and(|(rev, selection)| {
            *rev == self.model.revision && selection == &self.model.selection
        }) {
            return true;
        }
        self.error =
            Some("Your text or selection changed. Close Link and select the words again.".into());
        cx.notify();
        false
    }
    fn apply_link(&mut self, href: &str, label: &str, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) || !self.link_anchor_valid(cx) {
            return;
        }
        self.edit(|e| e.set_link(href, label), w, cx);
        if self.error.is_none() {
            self.close_links(w, cx);
        }
    }
    fn accept_link(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.links.notes_mode {
            if let Some(index) = self.links.filtered.get(self.links.selected) {
                let note = self.links.catalog[*index].clone();
                self.apply_link(&format!("note://{}", note.id), &note.title, w, cx);
            }
        } else {
            let href = self.links.query.clone();
            self.apply_link(&href, &href, w, cx);
        }
    }
    fn link_event(&mut self, event: &SearchEvent, w: &mut Window, cx: &mut Context<Self>) {
        if !self.links.open {
            return;
        }
        match event {
            SearchEvent::Changed(query) => {
                self.links.query = query.clone();
                self.links.filter();
                self.error = None;
            }
            SearchEvent::Accept => self.accept_link(w, cx),
            SearchEvent::Cancel => self.close_links(w, cx),
            SearchEvent::Next | SearchEvent::Previous
                if self.links.notes_mode && !self.links.filtered.is_empty() =>
            {
                let len = self.links.filtered.len();
                self.links.selected = if matches!(event, SearchEvent::Next) {
                    (self.links.selected + 1) % len
                } else {
                    (self.links.selected + len - 1) % len
                };
                self.links
                    .scroll
                    .scroll_to_item(self.links.selected, ScrollStrategy::Center);
            }
            _ => {}
        }
        cx.notify();
    }
    fn remove_current_link(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) || (self.links.open && !self.link_anchor_valid(cx)) {
            return;
        }
        self.edit(|e| e.remove_link(), w, cx);
        if self.error.is_none() {
            self.close_links(w, cx);
        }
    }
    pub(crate) fn follow_href(&mut self, href: &str, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        match LinkTarget::parse(href) {
            Ok(LinkTarget::Note(id)) => cx.emit(OpenNote(id)),
            Ok(LinkTarget::External(href)) => cx.open_url(&href),
            Err(error) => self.error = Some(format!("This link can’t be opened. {error}")),
        }
        cx.notify();
    }
    pub(crate) fn open_link_or_media(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        if let Some(link) = self.model.current_link() {
            self.follow_href(&link.href, cx);
        } else {
            self.open_selected(w, cx);
        }
    }
    pub(super) fn link_at_point(&self, point: Point<Pixels>) -> Option<notes_editor::Link> {
        let local = point - self.bounds.origin;
        self.layout.as_ref()?.lines.iter().find_map(|l| {
            if local.y < l.origin.y || local.y >= l.origin.y + l.line.size(l.height).height {
                return None;
            }
            let byte = l.line.index_for_position(local - l.origin, l.height).ok()?;
            self.model.link_at(l.start + byte)
        })
    }
    pub(super) fn link_controls(&self, w: &Window, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme;
        if !self.links.open {
            let Some(link) = self.model.current_link() else {
                return div().into_any_element();
            };
            let label = self
                .links
                .catalog
                .iter()
                .find(|n| link.href == format!("note://{}", n.id))
                .map_or(link.href, |n| n.title.clone());
            return div()
                .id("link-context")
                .role(Role::Group)
                .aria_label("Link actions")
                .flex_none()
                .flex()
                .flex_wrap()
                .items_center()
                .px(px(20.))
                .py(px(4.))
                .text_size(px(12.))
                .border_b_1()
                .border_color(t.border)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(80.))
                        .truncate()
                        .text_color(t.accent)
                        .child(label),
                )
                .child(
                    self.media_button(
                        "open-link",
                        "Open",
                        |s, w, cx| s.open_link_or_media(w, cx),
                        cx,
                    )
                    .aria_label("Open link, Control Enter"),
                )
                .child(
                    self.media_button(
                        "edit-link",
                        "Edit",
                        |s, w, cx| s.open_link_panel(false, w, cx),
                        cx,
                    )
                    .aria_label("Edit link"),
                )
                .child(
                    self.media_button(
                        "remove-link",
                        "Remove",
                        |s, w, cx| s.remove_current_link(w, cx),
                        cx,
                    )
                    .aria_label("Remove link, keep text"),
                )
                .into_any_element();
        }
        let count = self.links.filtered.len();
        let status = if count == 0 {
            "No matching notes".into()
        } else {
            let n = &self.links.catalog[self.links.filtered[self.links.selected]];
            format!("{} of {count} · {}", self.links.selected + 1, n.title)
        };
        div()
            .id("link-panel")
            .role(Role::Group)
            .aria_label("Link destination picker")
            .key_context("LinkPanel")
            .flex_none()
            .bg(t.overlay)
            .border_b_1()
            .border_color(t.border)
            .px(px(20.))
            .py(px(10.))
            .text_size(px(12.))
            .text_color(t.text)
            .on_action(cx.listener(|s, _: &CloseLinks, w, cx| s.close_links(w, cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Connect a thought"),
                    )
                    .child(
                        self.media_button(
                            "link-web",
                            "Address",
                            |s, w, cx| s.open_link_panel(false, w, cx),
                            cx,
                        )
                        .when(!self.links.notes_mode, |d| {
                            d.bg(t.selected).text_color(t.accent)
                        })
                        .aria_toggled(if self.links.notes_mode {
                            accesskit::Toggled::False
                        } else {
                            accesskit::Toggled::True
                        }),
                    )
                    .child(
                        self.media_button(
                            "link-notes",
                            "A note",
                            |s, w, cx| s.open_link_panel(true, w, cx),
                            cx,
                        )
                        .when(self.links.notes_mode, |d| {
                            d.bg(t.selected).text_color(t.accent)
                        })
                        .aria_toggled(if self.links.notes_mode {
                            accesskit::Toggled::True
                        } else {
                            accesskit::Toggled::False
                        }),
                    )
                    .child(self.media_button(
                        "link-cancel",
                        "Close",
                        |s, w, cx| s.close_links(w, cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .mt(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .bg(t.editor)
                            .border_1()
                            .border_color(t.border)
                            .rounded(px(6.))
                            .px(px(10.))
                            .py(px(5.))
                            .child(self.links.field.clone()),
                    )
                    .when(!self.links.notes_mode || count > 0, |d| {
                        d.child(
                            self.media_button(
                                "apply-link",
                                "Link",
                                |s, w, cx| s.accept_link(w, cx),
                                cx,
                            )
                            .text_color(t.accent)
                            .aria_label("Apply link"),
                        )
                    })
                    .when(self.model.current_link().is_some(), |d| {
                        d.child(
                            self.media_button(
                                "unlink-from-panel",
                                "Remove",
                                |s, w, cx| s.remove_current_link(w, cx),
                                cx,
                            )
                            .aria_label("Remove link, keep text"),
                        )
                    }),
            )
            .when(self.links.notes_mode, |d| {
                d.child(
                    div()
                        .mt(px(5.))
                        .h(((w.viewport_size().height * 0.28) - px(40.))
                            .clamp(px(48.), px(180.))
                            .min(px(count.max(1) as f32 * 48.)))
                        .child(
                            uniform_list(
                                "link-note-results",
                                count,
                                cx.processor(move |s, range: Range<usize>, _, cx| {
                                    range
                                        .map(|row| {
                                            let n = s.links.catalog[s.links.filtered[row]].clone();
                                            let id = n.id.clone();
                                            let title = n.title.clone();
                                            div()
                                                .id(("link-note", row))
                                                .role(Role::Button)
                                                .aria_label(format!(
                                                    "Link to {} in {}",
                                                    n.title, n.folder
                                                ))
                                                .aria_selected(row == s.links.selected)
                                                .h(px(48.))
                                                .w_full()
                                                .px(px(10.))
                                                .py(px(5.))
                                                .rounded(px(5.))
                                                .cursor_pointer()
                                                .when(row == s.links.selected, |d| d.bg(t.selected))
                                                .hover(move |d| d.bg(t.selected))
                                                .on_mouse_down(MouseButton::Left, |_, w, _| {
                                                    w.prevent_default()
                                                })
                                                .on_click(cx.listener(move |s, _, w, cx| {
                                                    s.apply_link(
                                                        &format!("note://{id}"),
                                                        &title,
                                                        w,
                                                        cx,
                                                    )
                                                }))
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_color(t.text)
                                                        .child(n.title),
                                                )
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_size(px(10.))
                                                        .text_color(t.muted)
                                                        .child(n.folder),
                                                )
                                        })
                                        .collect()
                                }),
                            )
                            .track_scroll(&self.links.scroll)
                            .size_full(),
                        ),
                )
            })
            .child(
                div()
                    .id("link-hint")
                    .mt(px(6.))
                    .text_size(px(11.))
                    .text_color(t.muted)
                    .when(self.links.notes_mode, |d| {
                        d.role(Role::Status)
                            .aria_label(status.clone())
                            .child(div().truncate().child(status))
                            .child("↑ ↓ to choose · Enter to link · Esc to close")
                    })
                    .when(!self.links.notes_mode, |d| {
                        d.child("https://, http:// or mailto: · Your selected words stay intact")
                    }),
            )
            .into_any_element()
    }
}
