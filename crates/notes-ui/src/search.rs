//! GPUI adapter for the pure single-line search model.
//! Input-handler/paint integration follows GPUI's Apache-2.0 input example;
//! see assets/NOTICE.md. Rich document editing lives in `rich_editor`.
use crate::theme::Theme;
use gpui::{prelude::*, *};
use notes_core::text_field::TextField;
#[path = "search_accessibility.rs"]
mod accessibility;
use std::{ops::Range, rc::Rc};

actions!(
    search,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        Home,
        End,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Accept,
        Next,
        Previous,
        Cancel
    ]
);
#[derive(Clone)]
pub enum SearchEvent {
    Changed(String),
    Accept,
    Next,
    Previous,
    Cancel,
}

pub struct SearchField {
    pub focus: FocusHandle,
    model: TextField,
    pub theme: Theme,
    line: Option<Rc<ShapedLine>>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Pixels,
    dragging: bool,
    label: &'static str,
}
impl EventEmitter<SearchEvent> for SearchField {}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("NoteSearch")),
        KeyBinding::new("delete", Delete, Some("NoteSearch")),
        KeyBinding::new("left", Left, Some("NoteSearch")),
        KeyBinding::new("right", Right, Some("NoteSearch")),
        KeyBinding::new("shift-left", SelectLeft, Some("NoteSearch")),
        KeyBinding::new("shift-right", SelectRight, Some("NoteSearch")),
        KeyBinding::new("home", Home, Some("NoteSearch")),
        KeyBinding::new("end", End, Some("NoteSearch")),
        KeyBinding::new("ctrl-a", SelectAll, Some("NoteSearch")),
        KeyBinding::new("ctrl-c", Copy, Some("NoteSearch")),
        KeyBinding::new("ctrl-x", Cut, Some("NoteSearch")),
        KeyBinding::new("ctrl-v", Paste, Some("NoteSearch")),
        KeyBinding::new("ctrl-insert", Copy, Some("NoteSearch")),
        KeyBinding::new("shift-delete", Cut, Some("NoteSearch")),
        KeyBinding::new("shift-insert", Paste, Some("NoteSearch")),
        KeyBinding::new("enter", Accept, Some("NoteSearch")),
        KeyBinding::new("down", Next, Some("NoteSearch")),
        KeyBinding::new("up", Previous, Some("NoteSearch")),
        KeyBinding::new("escape", Cancel, Some("NoteSearch")),
    ]);
}

impl SearchField {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            model: TextField::default(),
            theme: Theme::reference(),
            line: None,
            bounds: None,
            scroll: px(0.),
            dragging: false,
            label: "Search notes",
        }
    }
    pub fn named(label: &'static str, theme: Theme, cx: &mut Context<Self>) -> Self {
        Self {
            label,
            theme,
            ..Self::new(cx)
        }
    }
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.model = TextField::default();
        self.model.replace(None, text, None);
        self.select_all(window, cx);
        self.changed(window, cx);
    }
    pub fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model.select(0, false);
        self.model.select(self.model.text.len(), true);
        window.invalidate_character_coordinates();
        cx.notify();
    }
    pub fn composing(&self) -> bool {
        self.model.marked.is_some()
    }
    pub fn set_label(&mut self, label: &'static str, cx: &mut Context<Self>) {
        self.label = label;
        cx.notify();
    }
    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_silently(window, cx);
        cx.emit(SearchEvent::Changed(String::new()));
    }
    /// A controller-driven reset must not send a later user-search event that
    /// navigates away from the note the controller has just opened.
    pub fn clear_silently(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model = TextField::default();
        self.scroll = px(0.);
        window.invalidate_character_coordinates();
        cx.notify();
    }
    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.marked.is_none() {
            cx.emit(SearchEvent::Changed(self.model.text.clone()));
        }
        window.invalidate_character_coordinates();
        cx.notify();
    }
    fn motion(&mut self, forward: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.model.move_cursor(forward, extend);
        window.invalidate_character_coordinates();
        cx.notify();
    }
    fn delete(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.selection().is_empty() {
            self.model.move_cursor(forward, true);
        }
        self.model.replace(None, "", None);
        self.changed(window, cx);
    }
    fn index(&self, point: Point<Pixels>) -> usize {
        match (&self.line, self.bounds) {
            (Some(line), Some(bounds)) if line.text.as_ref() == self.model.text => self
                .model
                .clip(line.closest_index_for_x(point.x - bounds.left() + self.scroll)),
            _ => self.model.text.len(),
        }
    }
    fn copy(&self, cx: &mut Context<Self>) {
        let range = self.model.selection();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.model.text[range].to_owned()));
        }
    }
    fn navigation(&self, event: SearchEvent, cx: &mut Context<Self>) {
        // The platform IME owns navigation while pre-edit is active. Do not
        // forward it into parent shortcuts or GPUI's fallback text insertion.
        if self.model.marked.is_none() {
            cx.emit(event);
        }
    }
}

impl EntityInputHandler for SearchField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.model.byte_range(range);
        *actual = Some(self.model.utf16(range.start)..self.model.utf16(range.end));
        Some(self.model.text[range].to_owned())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = self.model.selection();
        Some(UTF16Selection {
            range: self.model.utf16(range.start)..self.model.utf16(range.end),
            reversed: self.model.head < self.model.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.model
            .marked
            .as_ref()
            .map(|r| self.model.utf16(r.start)..self.model.utf16(r.end))
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.model.marked = None;
        self.changed(window, cx);
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model.replace(range, text, None);
        self.changed(window, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model.replace(range, text, Some(selected));
        self.changed(window, cx);
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.line.as_ref()?;
        let bounds = self.bounds?;
        let range = self.model.byte_range(range);
        let x = (bounds.left() + line.x_for_index(range.start) - self.scroll)
            .clamp(bounds.left(), bounds.right());
        let end = (bounds.left() + line.x_for_index(range.end) - self.scroll)
            .clamp(bounds.left(), bounds.right());
        Some(Bounds::new(
            point(x, bounds.top()),
            size((end - x).max(px(1.)), bounds.size.height),
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.bounds?;
        Some(self.model.utf16(self.index(point)))
    }
}

impl Render for SearchField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let input = cx.entity();
        let paint_input = input.clone();
        let content = div()
            .id("search-field")
            .role(Role::TextInput)
            .aria_label(self.label)
            .aria_placeholder(self.label)
            .track_focus(&self.focus)
            .tab_index(0)
            .key_context("NoteSearch")
            .flex_1()
            .min_w_0()
            .h(px(28.))
            .cursor(CursorStyle::IBeam)
            .text_size(px(14.))
            .line_height(px(26.))
            .on_action(cx.listener(|s, _: &Backspace, w, cx| s.delete(false, w, cx)))
            .on_action(cx.listener(|s, _: &Delete, w, cx| s.delete(true, w, cx)))
            .on_action(cx.listener(|s, _: &Left, w, cx| s.motion(false, false, w, cx)))
            .on_action(cx.listener(|s, _: &Right, w, cx| s.motion(true, false, w, cx)))
            .on_action(cx.listener(|s, _: &SelectLeft, w, cx| s.motion(false, true, w, cx)))
            .on_action(cx.listener(|s, _: &SelectRight, w, cx| s.motion(true, true, w, cx)))
            .on_action(cx.listener(|s, _: &Home, w, cx| {
                s.model.select(0, false);
                w.invalidate_character_coordinates();
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &End, w, cx| {
                s.model.select(s.model.text.len(), false);
                w.invalidate_character_coordinates();
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &SelectAll, w, cx| {
                s.model.anchor = 0;
                s.model.head = s.model.text.len();
                w.invalidate_character_coordinates();
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &Copy, _, cx| s.copy(cx)))
            .on_action(cx.listener(|s, _: &Cut, w, cx| {
                s.copy(cx);
                s.model.replace(None, "", None);
                s.changed(w, cx);
            }))
            .on_action(cx.listener(|s, _: &Paste, w, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    s.model.replace(None, &text, None);
                    s.changed(w, cx);
                }
            }))
            .on_action(cx.listener(|s, _: &Accept, _, cx| s.navigation(SearchEvent::Accept, cx)))
            .on_action(cx.listener(|s, _: &Next, _, cx| s.navigation(SearchEvent::Next, cx)))
            .on_action(
                cx.listener(|s, _: &Previous, _, cx| s.navigation(SearchEvent::Previous, cx)),
            )
            .on_action(cx.listener(|s, _: &Cancel, _, cx| s.navigation(SearchEvent::Cancel, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, event: &MouseDownEvent, window, cx| {
                    window.focus(&s.focus, cx);
                    s.dragging = true;
                    s.model
                        .select(s.index(event.position), event.modifiers.shift);
                    window.invalidate_character_coordinates();
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|s, event: &MouseMoveEvent, window, cx| {
                if s.dragging {
                    s.model.select(s.index(event.position), true);
                    window.invalidate_character_coordinates();
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, _| s.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, _| s.dragging = false),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let s = input.read(cx);
                        let style = window.text_style();
                        let text: SharedString = if s.model.text.is_empty() {
                            s.label.into()
                        } else {
                            s.model.text.clone().into()
                        };
                        let run = TextRun {
                            len: text.len(),
                            font: style.font(),
                            color: if s.model.text.is_empty() {
                                theme.muted
                            } else {
                                theme.text
                            },
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let runs = if let Some(mark) = &s.model.marked {
                            vec![
                                TextRun {
                                    len: mark.start,
                                    ..run.clone()
                                },
                                TextRun {
                                    len: mark.len(),
                                    underline: Some(UnderlineStyle {
                                        color: Some(theme.accent),
                                        thickness: px(1.),
                                        wavy: false,
                                    }),
                                    ..run.clone()
                                },
                                TextRun {
                                    len: text.len() - mark.end,
                                    ..run
                                },
                            ]
                            .into_iter()
                            .filter(|r| r.len > 0)
                            .collect()
                        } else {
                            vec![run]
                        };
                        let line = window.text_system().shape_line(text, px(14.), &runs, None);
                        let caret = if s.model.text.is_empty() {
                            px(0.)
                        } else {
                            line.x_for_index(s.model.head)
                        };
                        let scroll = s
                            .scroll
                            .min(caret)
                            .max(caret - bounds.size.width + px(4.))
                            .max(px(0.));
                        let line = Rc::new(line);
                        input.update(cx, |s, _| {
                            s.line = Some(line.clone());
                            if s.bounds != Some(bounds) || s.scroll != scroll {
                                window.invalidate_character_coordinates();
                            }
                            s.bounds = Some(bounds);
                            s.scroll = scroll;
                        });
                        (line, scroll)
                    },
                    move |bounds, (line, scroll), window, cx| {
                        let s = paint_input.read(cx);
                        let focus = s.focus.clone();
                        let selection = s.model.selection();
                        let head = s.model.head;
                        let empty = s.model.text.is_empty();
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, paint_input.clone()),
                            cx,
                        );
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            let origin = point(bounds.left() - scroll, bounds.top());
                            if !selection.is_empty() && focus.is_focused(window) {
                                window.paint_quad(fill(
                                    Bounds::from_corners(
                                        point(
                                            origin.x + line.x_for_index(selection.start),
                                            bounds.top(),
                                        ),
                                        point(
                                            origin.x + line.x_for_index(selection.end),
                                            bounds.bottom(),
                                        ),
                                    ),
                                    theme.selected,
                                ));
                            }
                            if let Err(error) =
                                line.paint(origin, px(26.), TextAlign::Left, None, window, cx)
                            {
                                log::warn!("search paint: {error}");
                            }
                            if focus.is_focused(window) {
                                let x = if empty {
                                    px(0.)
                                } else {
                                    line.x_for_index(head)
                                };
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(origin.x + x, bounds.top() + px(3.)),
                                        size(px(1.5), px(20.)),
                                    ),
                                    theme.accent,
                                ));
                            }
                        });
                    },
                )
                .w_full()
                .h(px(26.)),
            );
        accessibility::input(content, cx.entity())
    }
}
