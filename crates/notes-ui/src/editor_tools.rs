//! Small, keyboard-first writing panels; document state stays in notes-editor.
use super::*;
use crate::search::SearchEvent;

const FORMATS: [(&str, &str, Format); 16] = [
    ("format-body", "Body", Format::Style("paragraph")),
    ("format-h1", "Title", Format::Style("h1")),
    ("format-h2", "Heading", Format::Style("h2")),
    ("format-h3", "Subheading", Format::Style("h3")),
    ("format-bullet", "Bulleted list", Format::Style("bullet")),
    ("format-numbered", "Numbered list", Format::Style("ordered")),
    ("format-check", "Checklist", Format::Style("check")),
    ("format-bold", "Bold", Format::Mark("bold")),
    ("format-italic", "Italic", Format::Mark("italic")),
    ("format-underline", "Underline", Format::Mark("underline")),
    (
        "format-strike",
        "Strikethrough",
        Format::Mark("strikethrough"),
    ),
    ("format-highlight", "Highlight", Format::Mark("highlight")),
    ("format-code", "Inline code", Format::Mark("code")),
    ("format-quote", "Quote", Format::Style("quote")),
    ("format-indent", "Indent list", Format::Indent(false)),
    ("format-outdent", "Outdent list", Format::Indent(true)),
];

impl RichEditor {
    pub(crate) fn input_is_composing(&self, cx: &App) -> bool {
        self.model.marked.is_some()
            || self.find_field.read(cx).composing()
            || self.links.composing(cx)
    }
    pub(super) fn open_format_tools(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        if self.format_menu {
            self.close_format_tools(w, cx);
            return;
        }
        self.table_menu = false;
        self.links.open = false;
        self.find_open = false;
        self.format_menu = true;
        self.format_scroll.set_offset(point(px(0.), px(0.)));
        w.focus(&self.format_focus[0], cx);
        cx.notify();
    }
    pub(super) fn close_format_tools(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.format_menu = false;
        w.focus(&self.focus, cx);
        cx.notify();
    }
    fn list_selected(&self) -> bool {
        self.model
            .document
            .block(&self.model.selection.head.block)
            .is_some_and(|b| b.kind == "list")
    }
    fn move_format_focus(&mut self, backward: bool, w: &mut Window, cx: &mut Context<Self>) {
        let mut active: Vec<usize> = (0..if self.list_selected() { 16 } else { 14 }).collect();
        active.push(16);
        let i = active
            .iter()
            .position(|i| self.format_focus[*i].is_focused(w))
            .unwrap_or(0);
        let next = active[if backward {
            (i + active.len() - 1) % active.len()
        } else {
            (i + 1) % active.len()
        }];
        if next < 16 {
            self.format_scroll.scroll_to_item(next);
        }
        w.focus(&self.format_focus[next], cx);
        cx.notify();
    }
    pub(super) fn format_controls(&self, w: &Window, cx: &mut Context<Self>) -> AnyElement {
        if !self.format_menu {
            return div().into_any_element();
        }
        let theme = self.theme;
        div()
            .id("format-panel")
            .role(Role::Group)
            .aria_label("Formatting options")
            .key_context("FormatTools")
            .flex_none()
            .bg(theme.overlay)
            .border_b_1()
            .border_color(theme.border)
            .p(px(16.))
            .text_size(px(12.))
            .text_color(theme.text)
            .on_action(cx.listener(|s, _: &CloseFormatTools, w, cx| s.close_format_tools(w, cx)))
            .on_action(
                cx.listener(|s, _: &NextFormatControl, w, cx| s.move_format_focus(false, w, cx)),
            )
            .on_action(
                cx.listener(|s, _: &PreviousFormatControl, w, cx| s.move_format_focus(true, w, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .mb(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Make it read beautifully"),
                    )
                    .child(
                        self.media_button(
                            "format-done",
                            "Done",
                            |s, w, cx| s.close_format_tools(w, cx),
                            cx,
                        )
                        .track_focus(&self.format_focus[16]),
                    ),
            )
            .child(
                div()
                    .id("format-options-scroll")
                    .track_scroll(&self.format_scroll)
                    .max_h(((w.viewport_size().height * 0.35) - px(70.)).clamp(px(48.), px(200.)))
                    .overflow_y_scroll()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .children(
                        FORMATS
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i < 14 || self.list_selected())
                            .map(|(i, (id, label, action))| {
                                self.tool(id, label, *action, cx)
                                    .track_focus(&self.format_focus[i])
                                    .min_w(px(110.))
                                    .text_color(theme.text)
                                    .border_color(theme.border)
                            }),
                    ),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(11.))
                    .text_color(theme.muted)
                    .child("Tab to explore · Esc to return to your words"),
            )
            .into_any_element()
    }
    pub(super) fn open_find(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        self.format_menu = false;
        self.table_menu = false;
        self.links.open = false;
        self.find_open = true;
        let range = self.model.range();
        let text = self.model.text();
        let seed = &text[range];
        if !seed.is_empty() && seed.len() <= 256 && !seed.contains(['\n', '\u{fffc}']) {
            self.find_field
                .update(cx, |field, cx| field.set_text(seed, w, cx));
        } else {
            self.find_field
                .update(cx, |field, cx| field.select_all(w, cx));
        }
        w.focus(&self.find_field.read(cx).focus.clone(), cx);
        cx.notify();
    }
    pub(super) fn close_find(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.find_field.read(cx).composing() {
            return;
        }
        self.find_open = false;
        w.focus(&self.focus, cx);
        cx.notify();
    }
    fn select_find_match(&mut self, range: Range<usize>, w: &mut Window, cx: &mut Context<Self>) {
        self.move_to(range.start, false, w, cx);
        self.move_to(range.end, true, w, cx);
    }
    pub(super) fn find_step(&mut self, backward: bool, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        if !self.find_open {
            self.open_find(w, cx);
            return;
        }
        if let Some(range) = self
            .find
            .step(backward, self.model.global(&self.model.selection.head))
        {
            self.select_find_match(range, w, cx);
        }
        cx.notify();
    }
    pub(super) fn find_event(
        &mut self,
        event: &SearchEvent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchEvent::Changed(query) => {
                let start = self.model.range().start;
                self.find
                    .set_query(self.model.committed(), query.clone(), start);
                if self.model.marked.is_none()
                    && let Some(range) = self.find.selected().cloned()
                {
                    self.select_find_match(range, w, cx);
                }
            }
            SearchEvent::Accept | SearchEvent::Next => self.find_step(false, w, cx),
            SearchEvent::Previous => self.find_step(true, w, cx),
            SearchEvent::Cancel => self.close_find(w, cx),
        }
        cx.notify();
    }
    pub(super) fn find_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        if !self.find_open {
            return div().into_any_element();
        }
        let theme = self.theme;
        let status = if let Some(error) = &self.find.error {
            error.clone()
        } else if self.find.query.is_empty() {
            "Search this note".into()
        } else if self.find.matches.is_empty() {
            "No matches".into()
        } else {
            let count = self.find.matches.len();
            let suffix = if self.find.truncated {
                "+ · Narrow your search"
            } else {
                ""
            };
            match self.find.current {
                Some(i) => format!("{} of {count}{suffix}", i + 1),
                None => format!("{count} matches{suffix}"),
            }
        };
        div()
            .id("find-in-note")
            .role(Role::Group)
            .aria_label("Find in this note")
            .key_context("FindInNote")
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .px(px(20.))
            .py(px(8.))
            .text_size(px(12.))
            .bg(theme.overlay)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w(px(150.))
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(6.))
                    .px(px(8.))
                    .py(px(3.))
                    .child(self.find_field.clone()),
            )
            .child(
                div()
                    .id("find-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_color(theme.muted)
                    .child(status),
            )
            .when(!self.find.matches.is_empty(), |d| {
                d.child(
                    self.media_button(
                        "find-previous",
                        "Previous",
                        |s, w, cx| s.find_step(true, w, cx),
                        cx,
                    )
                    .aria_label("Previous match, Shift F3"),
                )
                .child(
                    self.media_button(
                        "find-next",
                        "Next",
                        |s, w, cx| s.find_step(false, w, cx),
                        cx,
                    )
                    .aria_label("Next match, F3"),
                )
            })
            .child(
                self.media_button("find-done", "Done", |s, w, cx| s.close_find(w, cx), cx)
                    .aria_label("Close find, Escape"),
            )
            .into_any_element()
    }
}
