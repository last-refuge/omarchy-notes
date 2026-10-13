//! The selection toolbar is positioned from this frame's shaped text geometry.
use super::*;

pub(super) struct State {
    dismissed: Option<notes_editor::Selection>,
    focus: Vec<FocusHandle>,
    pub bounds: Option<Bounds<Pixels>>,
}
impl State {
    pub fn new(cx: &mut App) -> Self {
        Self {
            dismissed: None,
            focus: (0..7).map(|_| cx.focus_handle()).collect(),
            bounds: None,
        }
    }
    pub fn rearm(&mut self) {
        self.dismissed = None;
    }
    fn focused(&self, w: &Window) -> bool {
        self.focus.iter().any(|f| f.is_focused(w))
    }
}
#[derive(Clone, Copy)]
enum Choice {
    Format(Format),
    Link,
    More,
    Close,
}
const CHOICES: [(&str, &str, Choice); 7] = [
    ("selection-bold", "B", Choice::Format(Format::Mark("bold"))),
    (
        "selection-italic",
        "I",
        Choice::Format(Format::Mark("italic")),
    ),
    (
        "selection-underline",
        "U",
        Choice::Format(Format::Mark("underline")),
    ),
    (
        "selection-highlight",
        "Highlight",
        Choice::Format(Format::Mark("highlight")),
    ),
    ("selection-link", "Link", Choice::Link),
    ("selection-more", "More", Choice::More),
    ("selection-close", "×", Choice::Close),
];
impl RichEditor {
    fn selection_toolbar_available(&self, w: &Window, cx: &App) -> bool {
        let range = self.model.range();
        !range.is_empty()
            && !self.dragging
            && !self.input_is_composing(cx)
            && !self.format_menu
            && !self.table_menu
            && !self.find_open
            && !self.links.open
            && self.selection_tools.dismissed.as_ref() != Some(&self.model.selection)
            && (self.focus.is_focused(w) || self.selection_tools.focused(w))
            && self.model.text()[range]
                .chars()
                .any(|c| !matches!(c, '\n' | '\u{fffc}'))
    }
    pub(super) fn dismiss_selection_tools(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.selection_tools.dismissed = Some(self.model.selection.clone());
        self.selection_tools.bounds = None;
        w.focus(&self.focus, cx);
        cx.notify();
    }
    pub(crate) fn focus_selection_tools(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        self.selection_tools.dismissed = None;
        if self.model.range().is_empty()
            || self
                .selection_placement(size(px(300.), px(38.)), w)
                .is_none()
        {
            self.open_format_tools(w, cx);
            return;
        }
        self.format_menu = false;
        self.table_menu = false;
        self.find_open = false;
        self.links.open = false;
        w.focus(&self.selection_tools.focus[0], cx);
        cx.notify();
    }
    fn cycle_selection_tools(&mut self, backward: bool, w: &mut Window, cx: &mut Context<Self>) {
        let i = self
            .selection_tools
            .focus
            .iter()
            .position(|f| f.is_focused(w))
            .unwrap_or(0);
        let len = self.selection_tools.focus.len();
        w.focus(
            &self.selection_tools.focus[if backward {
                (i + len - 1) % len
            } else {
                (i + 1) % len
            }],
            cx,
        );
        cx.notify();
    }
    fn selection_placement(&self, size: Size<Pixels>, w: &Window) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let range = self.model.range();
        let selection = layout
            .lines
            .iter()
            .filter_map(|l| {
                let start = range.start.saturating_sub(l.start).min(l.line.len());
                let end = range.end.saturating_sub(l.start).min(l.line.len());
                if start >= end {
                    return None;
                }
                let a = l.line.position_for_index(start, l.height)?;
                let b = l.line.position_for_index(end, l.height)?;
                let left = if a.y == b.y { a.x.min(b.x) } else { px(0.) };
                let right = if a.y == b.y {
                    a.x.max(b.x)
                } else {
                    l.line.width()
                };
                Some(Bounds::new(
                    self.bounds.origin + l.origin + point(left, a.y.min(b.y)),
                    size_fn((right - left).max(px(2.)), (b.y - a.y).abs() + l.height),
                ))
            })
            .reduce(|a, b| a.union(&b))?;
        let viewport = self
            .scroll
            .bounds()
            .intersect(&Bounds::new(Point::default(), w.viewport_size()));
        place(selection, viewport, size)
    }
    #[cfg(feature = "visual-tests")]
    pub(crate) fn selection_test_geometry(
        &self,
    ) -> (
        Option<Bounds<Pixels>>,
        Bounds<Pixels>,
        Point<Pixels>,
        Point<Pixels>,
    ) {
        let layout = self.layout.as_ref().expect("editor shaped");
        let range = self.model.range();
        let (a, h) = layout.caret(range.start);
        let (b, _) = layout.caret(range.end);
        (
            self.selection_tools.bounds,
            self.scroll.bounds(),
            self.bounds.origin + a + point(px(0.), h / 2.),
            self.bounds.origin + b + point(px(0.), h / 2.),
        )
    }
    fn selection_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.theme;
        let revision = self.model.revision;
        let selection = self.model.selection.clone();
        div()
            .id("selection-formatting")
            .role(Role::Toolbar)
            .aria_label("Selection formatting")
            .aria_description("Tab to choose a format. Escape returns to your selected words.")
            .key_context("SelectionTools")
            .occlude()
            .flex()
            .items_center()
            .gap(px(2.))
            .p(px(4.))
            .h(px(38.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.overlay)
            .text_color(theme.text)
            .text_size(px(12.))
            .on_action(
                cx.listener(|s, _: &CloseSelectionTools, w, cx| s.dismiss_selection_tools(w, cx)),
            )
            .on_action(cx.listener(|s, _: &NextSelectionControl, w, cx| {
                s.cycle_selection_tools(false, w, cx)
            }))
            .on_action(cx.listener(|s, _: &PreviousSelectionControl, w, cx| {
                s.cycle_selection_tools(true, w, cx)
            }))
            .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                s.selection_tools.dismissed = Some(s.model.selection.clone());
                cx.notify();
            }))
            .children(CHOICES.iter().enumerate().map(|(i, (id, label, choice))| {
                let choice = *choice;
                let anchor = selection.clone();
                let name = match choice {
                    Choice::Format(Format::Mark("bold")) => "Bold selection",
                    Choice::Format(Format::Mark("italic")) => "Italic selection",
                    Choice::Format(Format::Mark("underline")) => "Underline selection",
                    Choice::Format(_) => "Highlight selection",
                    Choice::Link => "Link selected words",
                    Choice::More => "More formatting",
                    Choice::Close => "Dismiss selection formatting",
                };
                div()
                    .id(*id)
                    .role(Role::Button)
                    .aria_label(name)
                    .track_focus(&self.selection_tools.focus[i])
                    .tab_index(0)
                    .cursor_pointer()
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(transparent_black())
                    .hover(move |d| d.bg(theme.selected))
                    .focus_visible(move |d| d.border_color(theme.accent))
                    .when_some(
                        match choice {
                            Choice::Format(Format::Mark(mark)) => Some(self.model.mark_state(mark)),
                            _ => None,
                        },
                        |d, state| {
                            d.aria_toggled(match state {
                                Some(true) => accesskit::Toggled::True,
                                Some(false) => accesskit::Toggled::False,
                                None => accesskit::Toggled::Mixed,
                            })
                            .when(state == Some(true), |d| {
                                d.bg(theme.selected).text_color(theme.accent)
                            })
                        },
                    )
                    .when(i == 0, |d| d.font_weight(FontWeight::BOLD))
                    .when(i == 1, |d| d.italic())
                    .when(i == 2, |d| d.underline())
                    .on_mouse_down(MouseButton::Left, |_, w, cx| {
                        w.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.apply_selection_choice(choice, revision, &anchor, w, cx)
                    }))
                    .child(*label)
            }))
            .into_any_element()
    }
    fn apply_selection_choice(
        &mut self,
        choice: Choice,
        revision: u64,
        selection: &notes_editor::Selection,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_is_composing(cx)
            || revision != self.model.revision
            || selection != &self.model.selection
        {
            return;
        }
        match choice {
            Choice::Format(format) => self.format(format, w, cx),
            Choice::Link => self.open_link_panel(false, w, cx),
            Choice::More => self.open_format_tools(w, cx),
            Choice::Close => self.dismiss_selection_tools(w, cx),
        }
    }
}
/// Prefer above; otherwise below. Never cover the selected rows or editor chrome.
fn place(
    selection: Bounds<Pixels>,
    viewport: Bounds<Pixels>,
    size: Size<Pixels>,
) -> Option<Bounds<Pixels>> {
    let inset = px(6.);
    if selection.intersect(&viewport).is_empty() || size.width + inset * 2. > viewport.size.width {
        return None;
    }
    let top = if selection.top() - size.height - inset >= viewport.top() + inset {
        selection.top() - size.height - inset
    } else if selection.bottom() + size.height + inset <= viewport.bottom() - inset {
        selection.bottom() + inset
    } else {
        return None;
    };
    let x = (selection.center().x - size.width / 2.).clamp(
        viewport.left() + inset,
        viewport.right() - size.width - inset,
    );
    Some(Bounds::new(point(x, top), size))
}
pub(super) struct Surface {
    pub editor: Entity<RichEditor>,
}
impl IntoElement for Surface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Surface {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.editor
            .update(cx, |s, _| s.selection_tools.bounds = None);
        (
            w.request_layout(
                Style {
                    position: Position::Absolute,
                    ..Default::default()
                },
                [],
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        let available = self.editor.read(cx).selection_toolbar_available(w, cx);
        if !available {
            return;
        }
        let mut child = self.editor.update(cx, |s, cx| s.selection_toolbar(cx));
        let measured = child.layout_as_root(
            size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
            w,
            cx,
        );
        let placement = self.editor.read(cx).selection_placement(measured, w);
        if let Some(bounds) = placement {
            self.editor
                .update(cx, |s, _| s.selection_tools.bounds = Some(bounds));
            w.defer_draw(child, bounds.origin, 1, None);
        } else if self.editor.read(cx).selection_tools.focused(w) {
            self.editor.update(cx, |s, cx| {
                w.focus(&s.focus, cx);
                cx.notify();
            });
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn placement_avoids_selection_edges_and_clipped_rows() {
        let viewport = Bounds::new(point(px(100.), px(100.)), size(px(420.), px(400.)));
        let toolbar = size(px(280.), px(38.));
        for (y, h) in [(120., 20.), (280., 90.), (460., 30.)] {
            let selection = Bounds::new(point(px(490.), px(y)), size(px(30.), px(h)));
            let placed = place(selection, viewport, toolbar).unwrap();
            assert_eq!(placed.intersect(&viewport), placed);
            assert!(placed.intersect(&selection).is_empty());
        }
        assert!(place(viewport, viewport, toolbar).is_none());
        assert!(
            place(
                Bounds::new(point(px(100.), px(800.)), size(px(30.), px(30.))),
                viewport,
                toolbar
            )
            .is_none()
        );
        assert!(place(viewport, viewport, size(px(500.), px(38.))).is_none());
    }
}
