//! Single-line search uses the same focus/synthetic-node wrapper as rich writing.
use super::*;
use crate::accessible_input::{self, Content, units};
use accesskit::{ActionData, Node, NodeId, Rect, TextPosition, TextSelection};
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn input(content: Content, editor: Entity<SearchField>) -> impl IntoElement {
    let capture = editor.clone();
    let mut input =
        accessible_input::Input::new(content, move |w, cx| Snapshot::capture(capture.read(cx), w));
    for action in [
        AccessibleAction::SetTextSelection,
        AccessibleAction::ReplaceSelectedText,
    ] {
        let entity = editor.downgrade();
        input = input.on_action(action, move |snapshot, data, w, cx| {
            if let Some(entity) = entity.upgrade() {
                entity.update(cx, |s, cx| snapshot.act(action, data, s, w, cx));
            }
        });
    }
    input
}
struct Run {
    offsets: Vec<usize>, // Global byte offsets, including the exclusive end.
    node: Node,
    id: NodeId,
}
struct Snapshot {
    model: TextField,
    runs: Vec<Run>,
}
impl Snapshot {
    fn capture(s: &SearchField, w: &Window) -> Self {
        let line = s.line.as_ref().expect("search prepaint shapes its line");
        let bounds = s.bounds.expect("search prepaint sets bounds");
        let scale = w.scale_factor();
        let all_words: Vec<_> = s
            .model
            .text
            .split_word_bound_indices()
            .map(|(i, _)| i)
            .collect();
        let mut parts = units(&s.model.text);
        if parts.is_empty() {
            parts.push((0, ""));
        }
        let mut runs = vec![];
        for chunk in parts.chunks(255) {
            let start = chunk[0].0;
            let end = chunk
                .last()
                .map(|(i, text)| i + text.len())
                .unwrap_or(start);
            let mut offsets: Vec<_> = chunk
                .iter()
                .filter(|(_, text)| !text.is_empty())
                .map(|(i, _)| *i)
                .collect();
            offsets.push(end);
            let x0 = line.x_for_index(start);
            let x1 = line.x_for_index(end);
            let x = bounds.left() - s.scroll + x0;
            let mut node = Node::new(Role::TextRun);
            node.set_value(s.model.text[start..end].to_string());
            node.set_character_lengths(
                offsets
                    .windows(2)
                    .map(|p| (p[1] - p[0]) as u8)
                    .collect::<Vec<_>>(),
            );
            node.set_character_positions(
                offsets[..offsets.len() - 1]
                    .iter()
                    .map(|i| f32::from(line.x_for_index(*i) - x0) * scale)
                    .collect::<Vec<_>>(),
            );
            node.set_character_widths(
                offsets
                    .windows(2)
                    .map(|p| {
                        f32::from(line.x_for_index(p[1]) - line.x_for_index(p[0])).abs() * scale
                    })
                    .collect::<Vec<_>>(),
            );
            node.set_word_starts(
                offsets[..offsets.len() - 1]
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| all_words.binary_search(i).is_ok())
                    .map(|(i, _)| i as u8)
                    .collect::<Vec<_>>(),
            );
            node.set_text_direction(accesskit::TextDirection::LeftToRight);
            node.set_bounds(Rect::new(
                f64::from(f32::from(x) * scale),
                f64::from(f32::from(bounds.top()) * scale),
                f64::from(f32::from(x + (x1 - x0).max(px(0.))) * scale),
                f64::from(f32::from(bounds.bottom()) * scale),
            ));
            runs.push(Run {
                offsets,
                node,
                id: NodeId(0),
            });
        }
        Self {
            model: s.model.clone(),
            runs,
        }
    }
    fn byte(&self, pos: TextPosition) -> Option<usize> {
        self.runs
            .iter()
            .find(|r| r.id == pos.node)?
            .offsets
            .get(pos.character_index)
            .copied()
    }
    fn position(&self, byte: usize) -> TextPosition {
        let run = self
            .runs
            .iter()
            .find(|r| r.offsets.last().is_some_and(|end| byte < *end))
            .unwrap_or_else(|| self.runs.last().unwrap());
        TextPosition {
            node: run.id,
            character_index: run
                .offsets
                .partition_point(|offset| *offset <= byte)
                .saturating_sub(1),
        }
    }
    fn act(
        &self,
        action: AccessibleAction,
        data: Option<&ActionData>,
        s: &mut SearchField,
        w: &mut Window,
        cx: &mut Context<SearchField>,
    ) {
        if s.model.text != self.model.text || s.model.marked.is_some() {
            return;
        }
        match (action, data) {
            (AccessibleAction::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
                let (Some(anchor), Some(head)) =
                    (self.byte(selection.anchor), self.byte(selection.focus))
                else {
                    return;
                };
                s.model.select(anchor, false);
                s.model.select(head, true);
                w.invalidate_character_coordinates();
                w.focus(&s.focus, cx);
                cx.notify();
            }
            (AccessibleAction::ReplaceSelectedText, Some(ActionData::Value(text)))
                if s.model.anchor == self.model.anchor && s.model.head == self.model.head =>
            {
                s.model.replace(None, text, None);
                s.changed(w, cx);
                w.focus(&s.focus, cx);
            }
            _ => {}
        }
    }
}
impl accessible_input::Snapshot for Snapshot {
    fn populate(&mut self, builder: &mut A11ySubtreeBuilder) {
        for (i, run) in self.runs.iter_mut().enumerate() {
            run.id = builder.synthetic_node_id(("text", i));
        }
        for (i, run) in self.runs.iter().enumerate() {
            let mut node = run.node.clone();
            if let Some(previous) = i.checked_sub(1).and_then(|i| self.runs.get(i)) {
                node.set_previous_on_line(previous.id);
            }
            if let Some(next) = self.runs.get(i + 1) {
                node.set_next_on_line(next.id);
            }
            builder.push_child(run.id, node);
        }
        builder.parent_node().set_text_selection(TextSelection {
            anchor: self.position(self.model.anchor),
            focus: self.position(self.model.head),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn capture(s: &mut SearchField, w: &Window) -> Snapshot {
        let text: SharedString = s.model.text.clone().into();
        s.line = Some(Rc::new(w.text_system().shape_line(
            text.clone(),
            px(14.),
            &[TextRun {
                len: text.len(),
                font: font("Noto Sans"),
                color: s.theme.text,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )));
        s.bounds = Some(Bounds::new(
            point(px(10.), px(20.)),
            size(px(200.), px(26.)),
        ));
        let mut snapshot = Snapshot::capture(s, w);
        for (i, run) in snapshot.runs.iter_mut().enumerate() {
            run.id = NodeId(i as u64 + 1);
        }
        snapshot
    }
    #[::core::prelude::v1::test]
    fn empty_unicode_and_long_search_text_have_exact_positions() {
        let mut app = TestApp::new();
        let mut window = app.open_window(|_, cx| SearchField::new(cx));
        window.update(|s, w, _| {
            for text in [
                String::new(),
                "e\u{301} 👩‍👩‍👧‍👦 中文".into(),
                "search ".repeat(150),
            ] {
                s.model.replace(Some(0..usize::MAX), &text, None);
                let snapshot = capture(s, w);
                assert_eq!(
                    snapshot
                        .runs
                        .iter()
                        .map(|r| r.node.value().unwrap())
                        .collect::<String>(),
                    text
                );
                for run in &snapshot.runs {
                    for offset in &run.offsets {
                        assert_eq!(snapshot.byte(snapshot.position(*offset)), Some(*offset));
                    }
                    assert_eq!(
                        run.node.character_lengths().len(),
                        run.node.character_positions().unwrap().len()
                    );
                }
                assert_eq!(
                    snapshot.byte(TextPosition {
                        node: NodeId(u64::MAX),
                        character_index: 0
                    }),
                    None
                );
            }
        });
    }
    #[::core::prelude::v1::test]
    fn search_actions_preserve_reversal_and_guard_new_queries_and_preedit() {
        let mut app = TestApp::new();
        let mut window = app.open_window(|_, cx| SearchField::new(cx));
        window.update(|s, w, cx| {
            s.model.replace(None, "Find 👩‍👩‍👧‍👦", None);
            let snapshot = capture(s, w);
            let selection = TextSelection {
                anchor: snapshot.position(s.model.text.len()),
                focus: snapshot.position(5),
            };
            snapshot.act(
                AccessibleAction::SetTextSelection,
                Some(&ActionData::SetTextSelection(selection)),
                s,
                w,
                cx,
            );
            assert!(s.model.head < s.model.anchor);
            snapshot.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("old selection".into())),
                s,
                w,
                cx,
            );
            assert!(s.model.text.contains('👩'));
            let selected = capture(s, w);
            selected.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("new\nquery".into())),
                s,
                w,
                cx,
            );
            assert_eq!(s.model.text, "Find new query");
            selected.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("stale".into())),
                s,
                w,
                cx,
            );
            assert_eq!(s.model.text, "Find new query");
            s.model.replace(None, "候補", Some(None));
            let composing = capture(s, w);
            let text = s.model.text.clone();
            composing.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("interrupt".into())),
                s,
                w,
                cx,
            );
            assert_eq!(s.model.text, text);
            assert!(s.model.marked.is_some());
        });
    }
}
