//! One focusable element owns both a painted input and its synthetic text nodes.
use gpui::{prelude::*, *};
use std::{cell::RefCell, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) type Content = Stateful<Div>;
pub(crate) trait Snapshot: 'static {
    fn populate(&mut self, builder: &mut A11ySubtreeBuilder);
}
type Capture<S> = Box<dyn Fn(&Window, &App) -> S>;
type Paint<S> = Box<dyn Fn(&Rc<S>, &mut Window, &mut App)>;
pub(crate) struct Input<S: Snapshot> {
    content: Content,
    capture: Capture<S>,
    paint: Option<Paint<S>>,
    snapshot: Rc<RefCell<Option<Rc<S>>>>,
}
impl<S: Snapshot> Input<S> {
    pub(crate) fn new(content: Content, capture: impl Fn(&Window, &App) -> S + 'static) -> Self {
        Self {
            content,
            capture: Box::new(capture),
            paint: None,
            snapshot: Rc::default(),
        }
    }
    pub(crate) fn on_action(
        mut self,
        action: AccessibleAction,
        mut listener: impl FnMut(&S, Option<&accesskit::ActionData>, &mut Window, &mut App) + 'static,
    ) -> Self {
        let state = self.snapshot.clone();
        self.content = self.content.on_a11y_action(action, move |data, w, cx| {
            let Some(snapshot) = state.borrow().clone() else {
                return;
            };
            listener(&snapshot, data, w, cx);
        });
        self
    }
    pub(crate) fn on_paint(
        mut self,
        paint: impl Fn(&Rc<S>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.paint = Some(Box::new(paint));
        self
    }
}
impl<S: Snapshot> IntoElement for Input<S> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
pub(crate) struct Prepaint<S> {
    content: <Content as Element>::PrepaintState,
    snapshot: Option<S>,
}
impl<S: Snapshot> Element for Input<S> {
    type RequestLayoutState = <Content as Element>::RequestLayoutState;
    type PrepaintState = Prepaint<S>;
    fn id(&self) -> Option<ElementId> {
        Element::id(&self.content)
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        self.content.source_location()
    }
    fn a11y_role(&self) -> Option<Role> {
        self.content.a11y_role()
    }
    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.content.write_a11y_info(node);
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.content.request_layout(id, inspector, w, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        w: &mut Window,
        cx: &mut App,
    ) -> Prepaint<S> {
        let content = self.content.prepaint(id, inspector, bounds, state, w, cx);
        let snapshot = w.is_a11y_active().then(|| (self.capture)(w, cx));
        Prepaint { content, snapshot }
    }
    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Prepaint<S>,
        builder: &mut A11ySubtreeBuilder,
    ) {
        let Some(mut snapshot) = prepaint.snapshot.take() else {
            return;
        };
        snapshot.populate(builder);
        *self.snapshot.borrow_mut() = Some(Rc::new(snapshot));
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Prepaint<S>,
        w: &mut Window,
        cx: &mut App,
    ) {
        self.content
            .paint(id, inspector, bounds, state, &mut prepaint.content, w, cx);
        if let (Some(paint), Some(snapshot)) = (&self.paint, self.snapshot.borrow().as_ref()) {
            paint(snapshot, w, cx);
        }
    }
}

// AccessKit stores a character's UTF-8 length in u8. An exceptionally large
// grapheme falls back to scalar boundaries; never truncate a byte length.
pub(crate) fn units(text: &str) -> Vec<(usize, &str)> {
    let mut parts = Vec::new();
    for (start, grapheme) in text.grapheme_indices(true) {
        if grapheme.len() <= u8::MAX as usize {
            parts.push((start, grapheme));
        } else {
            for (offset, ch) in grapheme.char_indices() {
                parts.push((start + offset, &grapheme[offset..offset + ch.len_utf8()]));
            }
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn accessible_units_preserve_graphemes_and_oversized_combining_text() {
        let ordinary = "é 👩‍👩‍👧‍👦 🇯🇵 中文";
        let oversized = format!("a{}", "\u{301}".repeat(300));
        let text = format!("{ordinary} {oversized}");
        let parts = units(&text);
        assert_eq!(
            parts.iter().map(|(_, part)| *part).collect::<String>(),
            text
        );
        assert!(parts.iter().all(|(offset, part)| {
            part.len() <= u8::MAX as usize && &text[*offset..*offset + part.len()] == *part
        }));
        assert!(parts.iter().any(|(_, part)| *part == "👩‍👩‍👧‍👦"));
        assert!(parts.iter().any(|(_, part)| *part == "é"));
        assert!(parts.iter().any(|(_, part)| *part == "🇯🇵"));
    }
}
