//! AccessKit text backed by the same document, selection and shaped geometry as writing.
//! The wrapper delegates directly to its div so focus and text share one accessible node.
use super::*;
use crate::accessible_input::{self, Content, units};
use gpui::accesskit::{ActionData, Node, NodeId, Rect, TextDirection, TextPosition, TextSelection};
use std::collections::BTreeMap;

struct Run {
    key: (String, usize),
    start: usize,
    value: String,
    offsets: Vec<usize>, // Includes the exclusive end; offsets are relative to this run.
    bounds: Rect,
    rtl: bool,
    style: InlineStyle,
    color: accesskit::Color,
    positions: Vec<f32>,
    widths: Vec<f32>,
    words: Vec<u8>,
    id: NodeId,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    code: bool,
    link: bool,
}
impl InlineStyle {
    fn from_span(span: &notes_core::document::Span, heading: bool) -> Self {
        Self {
            bold: heading || span.mark("bold"),
            italic: span.mark("italic"),
            underline: span.mark("underline") || span.0.get("href").is_some(),
            strike: span.mark("strikethrough"),
            code: span.mark("code"),
            link: span.0.get("href").is_some(),
        }
    }
    fn write(self, node: &mut Node, color: accesskit::Color) {
        node.set_font_family(if self.code { "monospace" } else { "Noto Serif" });
        node.set_font_weight(if self.bold { 700. } else { 400. });
        if self.italic {
            node.set_italic();
        }
        if self.underline {
            node.set_underline(accesskit::TextDecoration {
                style: accesskit::TextDecorationStyle::Solid,
                color,
            });
        }
        if self.strike {
            node.set_strikethrough(accesskit::TextDecoration {
                style: accesskit::TextDecorationStyle::Solid,
                color,
            });
        }
    }
}
struct Control {
    key: String,
    node: Node,
    action: ControlAction,
    id: NodeId,
}
#[derive(Clone)]
enum ControlAction {
    Check(String),
    Media(usize),
}
struct Snapshot {
    revision: u64,
    selection: notes_editor::Selection,
    anchor: usize,
    head: usize,
    runs: Vec<Run>,
    controls: Vec<Control>,
}
pub(super) fn input(content: Content, editor: Entity<RichEditor>) -> impl IntoElement {
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
    input.on_paint(move |snapshot, w, _| snapshot.listen(&editor, w))
}
impl Snapshot {
    fn current(&self, s: &RichEditor) -> bool {
        s.model.revision == self.revision && s.model.marked.is_none()
    }
    fn act(
        &self,
        action: AccessibleAction,
        data: Option<&ActionData>,
        s: &mut RichEditor,
        w: &mut Window,
        cx: &mut Context<RichEditor>,
    ) {
        if !self.current(s) {
            return;
        }
        match (action, data) {
            (AccessibleAction::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
                let (Some(anchor), Some(head)) =
                    (self.byte(selection.anchor), self.byte(selection.focus))
                else {
                    return;
                };
                s.move_to(anchor, false, w, cx);
                s.move_to(head, true, w, cx);
                w.focus(&s.focus, cx);
            }
            (AccessibleAction::ReplaceSelectedText, Some(ActionData::Value(text)))
                if s.model.selection == self.selection =>
            {
                s.edit(|e| e.insert(text, notes_storage::now() as u64), w, cx);
                w.focus(&s.focus, cx);
            }
            _ => {}
        }
    }
    fn byte(&self, position: TextPosition) -> Option<usize> {
        let run = self.runs.iter().find(|r| r.id == position.node)?;
        Some(run.start + run.offsets.get(position.character_index)?)
    }
    fn position(&self, byte: usize) -> TextPosition {
        // At a hard break choose the preceding run's newline; after it choose the next run.
        let run = self
            .runs
            .iter()
            .find(|r| byte < r.start + r.value.len())
            .unwrap_or_else(|| {
                self.runs
                    .last()
                    .expect("even an empty document has a text run")
            });
        let local = byte.saturating_sub(run.start).min(run.value.len());
        let character_index = run
            .offsets
            .partition_point(|offset| *offset <= local)
            .saturating_sub(1);
        TextPosition {
            node: run.id,
            character_index,
        }
    }
}

fn physical(bounds: Bounds<Pixels>, origin: Point<Pixels>, scale: f32) -> Rect {
    let p = bounds.origin + origin;
    Rect::new(
        f64::from(f32::from(p.x) * scale),
        f64::from(f32::from(p.y) * scale),
        f64::from(f32::from(p.x + bounds.size.width) * scale),
        f64::from(f32::from(p.y + bounds.size.height) * scale),
    )
}
impl Snapshot {
    fn capture(s: &RichEditor, w: &Window) -> Self {
        let layout = s.layout.as_ref().expect("text prepaint builds layout");
        let scale = w.scale_factor();
        let mut lines: Vec<_> = layout.lines.iter().collect();
        lines.sort_by_key(|l| l.start);
        let wraps: Vec<Vec<_>> = lines
            .iter()
            .map(|line| {
                line.line
                    .wrap_boundaries
                    .iter()
                    .map(|b| line.line.unwrapped_layout.runs[b.run_ix].glyphs[b.glyph_ix].index)
                    .collect()
            })
            .collect();
        let objects: BTreeMap<_, _> = layout.objects.iter().map(|o| (o.start, o)).collect();
        let leaves = s.model.document.leaves();
        let mut runs: Vec<Run> = vec![];
        let mut start = 0;
        for (block_index, block) in leaves.iter().enumerate() {
            let mut text = block.text();
            if block_index + 1 < leaves.len() {
                text.push('\n');
            }
            let words: Vec<_> = text.split_word_bound_indices().map(|(i, _)| i).collect();
            let mut span_end = 0;
            let styles: Vec<_> = block
                .spans
                .iter()
                .map(|span| {
                    span_end += span.content().len();
                    (
                        span_end,
                        InlineStyle::from_span(span, block.kind == "heading"),
                    )
                })
                .collect();
            let mut parts = units(&text);
            if parts.is_empty() {
                parts.push((0, ""));
            }
            for (offset, value) in parts {
                let byte = start + offset;
                let style = styles
                    .get(styles.partition_point(|(end, _)| *end <= offset))
                    .or_else(|| styles.last())
                    .map(|(_, style)| *style)
                    .unwrap_or_default();
                let line_index = lines.partition_point(|l| l.start <= byte).saturating_sub(1);
                let (bounds, rtl) = if let Some(object) = objects.get(&byte) {
                    (object.bounds, false)
                } else if let Some(line) = lines.get(line_index) {
                    let local = byte.saturating_sub(line.start).min(line.line.len());
                    let end = (local + value.len()).min(line.line.len());
                    // Resolve soft-wrap boundaries to the following visual row.
                    let wraps = &wraps[line_index];
                    let row = wraps.partition_point(|i| *i <= local);
                    let row_start = if row == 0 { 0 } else { wraps[row - 1] };
                    let x = line.line.unwrapped_layout.x_for_index(local)
                        - line.line.unwrapped_layout.x_for_index(row_start);
                    let advance = if value == "\n" || value.is_empty() {
                        px(0.)
                    } else {
                        line.line.unwrapped_layout.x_for_index(end)
                            - line.line.unwrapped_layout.x_for_index(local)
                    };
                    (
                        Bounds::new(
                            line.origin + point(x + advance.min(px(0.)), line.height * row as f32),
                            size(advance.abs(), line.height),
                        ),
                        advance < px(0.),
                    )
                } else {
                    (
                        Bounds::new(point(px(0.), px(0.)), size(px(0.), px(30.))),
                        false,
                    )
                };
                let rect = physical(bounds, s.bounds.origin, scale);
                let new = runs.last().is_none_or(|r| {
                    r.key.0 != block.id
                        || r.bounds.y0 != rect.y0
                        || r.bounds.y1 != rect.y1
                        || r.rtl != rtl
                        || r.style != style
                        || r.offsets.len() > 255
                        || r.value.ends_with('\n')
                        || value == "\u{fffc}"
                        || r.value == "\u{fffc}"
                });
                if new {
                    let rgba: Rgba = if style.link {
                        s.theme.accent
                    } else {
                        s.theme.text
                    }
                    .into();
                    runs.push(Run {
                        key: (block.id.clone(), offset),
                        start: byte,
                        value: String::new(),
                        offsets: vec![0],
                        bounds: rect,
                        rtl,
                        style,
                        color: accesskit::Color {
                            red: (rgba.r * 255.) as u8,
                            green: (rgba.g * 255.) as u8,
                            blue: (rgba.b * 255.) as u8,
                            alpha: (rgba.a * 255.) as u8,
                        },
                        positions: vec![],
                        widths: vec![],
                        words: vec![],
                        id: NodeId(0),
                    });
                }
                let run = runs.last_mut().unwrap();
                if words.binary_search(&offset).is_ok() {
                    run.words.push((run.offsets.len() - 1) as u8);
                }
                run.value.push_str(value);
                if !value.is_empty() {
                    run.offsets.push(run.value.len());
                    // Keep absolute coordinates until the full run bounds are known.
                    run.positions
                        .push(if rtl { rect.x1 } else { rect.x0 } as f32);
                    run.widths.push(rect.width() as f32);
                    run.bounds = run.bounds.union(rect);
                }
            }
            start += text.len();
        }
        for run in &mut runs {
            for p in &mut run.positions {
                *p = if run.rtl {
                    run.bounds.x1 as f32 - *p
                } else {
                    *p - run.bounds.x0 as f32
                };
            }
        }
        let mut controls = vec![];
        for (id, bounds, checked) in &layout.checks {
            let mut node = Node::new(Role::CheckBox);
            node.set_label(
                s.model
                    .document
                    .block(id)
                    .map(|b| b.text())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Checklist item".into()),
            );
            node.set_toggled(if *checked {
                Toggled::True
            } else {
                Toggled::False
            });
            node.set_bounds(physical(*bounds, s.bounds.origin, scale));
            node.add_action(AccessibleAction::Click);
            controls.push(Control {
                key: format!("check:{id}"),
                node,
                action: ControlAction::Check(id.clone()),
                id: NodeId(0),
            });
        }
        for object in &layout.objects {
            let mut node = Node::new(if object.image {
                Role::Image
            } else {
                Role::Button
            });
            let label = s
                .media
                .items
                .get(&object.key)
                .map(|m| m.info.name.clone())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| object.label.clone());
            node.set_label(if object.image {
                format!("Image: {}", object.label)
            } else {
                format!("Attachment: {label}")
            });
            node.set_description(
                "Activate to select. Control Enter opens; Control Shift E saves a copy.",
            );
            node.set_bounds(physical(object.bounds, s.bounds.origin, scale));
            node.add_action(AccessibleAction::Click);
            controls.push(Control {
                key: format!("media:{}", object.start),
                node,
                action: ControlAction::Media(object.start),
                id: NodeId(0),
            });
        }
        Self {
            revision: s.model.revision,
            selection: s.model.selection.clone(),
            anchor: s.model.global(&s.model.selection.anchor),
            head: s.model.global(&s.model.selection.head),
            runs,
            controls,
        }
    }
}

impl accessible_input::Snapshot for Snapshot {
    fn populate(&mut self, builder: &mut A11ySubtreeBuilder) {
        let snapshot = self;
        for run in &mut snapshot.runs {
            run.id = builder.synthetic_node_id(("text", &run.key));
        }
        for (index, run) in snapshot.runs.iter().enumerate() {
            let mut node = Node::new(Role::TextRun);
            node.set_value(run.value.clone());
            node.set_character_lengths(
                run.offsets
                    .windows(2)
                    .map(|p| (p[1] - p[0]) as u8)
                    .collect::<Vec<_>>(),
            );
            node.set_character_positions(run.positions.clone());
            node.set_character_widths(run.widths.clone());
            node.set_word_starts(run.words.clone());
            node.set_text_direction(if run.rtl {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            });
            node.set_bounds(run.bounds);
            run.style.write(&mut node, run.color);
            let same_line =
                |other: &&Run| other.key.0 == run.key.0 && other.bounds.y0 == run.bounds.y0;
            if let Some(previous) = index
                .checked_sub(1)
                .and_then(|i| snapshot.runs.get(i))
                .filter(same_line)
            {
                node.set_previous_on_line(previous.id);
            }
            if let Some(next) = snapshot.runs.get(index + 1).filter(same_line) {
                node.set_next_on_line(next.id);
            }
            builder.push_child(run.id, node);
        }
        for control in &mut snapshot.controls {
            control.id = builder.synthetic_node_id(&control.key);
            builder.push_child(control.id, control.node.clone());
        }
        builder.parent_node().set_text_selection(TextSelection {
            anchor: snapshot.position(snapshot.anchor),
            focus: snapshot.position(snapshot.head),
        });
    }
}
impl Snapshot {
    fn listen(self: &Rc<Self>, editor: &Entity<RichEditor>, w: &mut Window) {
        let snapshot = self;
        for control in &snapshot.controls {
            let saved = snapshot.clone();
            let action = control.action.clone();
            let editor = editor.downgrade();
            w.on_a11y_action(control.id, AccessibleAction::Click, move |_, w, cx| {
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                editor.update(cx, |s, cx| {
                    if !saved.current(s) {
                        return;
                    }
                    match &action {
                        ControlAction::Check(id) => s.edit(
                            |e| {
                                e.toggle_check(id);
                                Ok(())
                            },
                            w,
                            cx,
                        ),
                        ControlAction::Media(start) => {
                            s.move_to(*start, false, w, cx);
                            s.move_to(*start + 3, true, w, cx);
                        }
                    }
                    w.focus(&s.focus, cx);
                });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn grapheme_lengths_never_overflow_and_reassemble_exactly() {
        for text in [
            "e\u{301}👩‍👩‍👧‍👦🇺🇸 中文\n".to_string(),
            format!("a{}", "\u{301}".repeat(200)),
        ] {
            let parts = units(&text);
            assert_eq!(parts.iter().map(|(_, s)| *s).collect::<String>(), text);
            for (offset, value) in parts {
                assert!(value.len() <= 255);
                assert_eq!(&text[offset..offset + value.len()], value);
            }
        }
        assert_eq!(units("e\u{301}👩‍👩‍👧‍👦🇺🇸").len(), 3);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    fn capture(s: &mut RichEditor, w: &Window) -> Snapshot {
        s.layout = Some(Rc::new(make_layout(
            &s.model.document,
            px(220.),
            s.theme,
            s.typography,
            w,
        )));
        let mut snapshot = Snapshot::capture(s, w);
        for (i, run) in snapshot.runs.iter_mut().enumerate() {
            run.id = NodeId(i as u64 + 1);
        }
        snapshot
    }
    #[::core::prelude::v1::test]
    fn unicode_newlines_empty_paragraphs_and_long_runs_round_trip() {
        let mut app = TestApp::new();
        let document = Document {
            blocks: ["", "e\u{301} 👩‍👩‍👧‍👦 🇺🇸 中文 العربية", "", &"x".repeat(600), ""]
                .into_iter()
                .map(Block::paragraph)
                .collect(),
            ..Default::default()
        };
        let mut window = app.open_window(|w, cx| {
            RichEditor::new(document, Theme::reference(), String::new(), w, cx)
        });
        window.update(|s, w, _| {
            let snapshot = capture(s, w);
            assert_eq!(
                snapshot
                    .runs
                    .iter()
                    .map(|r| r.value.as_str())
                    .collect::<String>(),
                s.model.text()
            );
            for run in &snapshot.runs {
                assert_eq!(run.offsets.len() - 1, run.positions.len());
                assert_eq!(run.positions.len(), run.widths.len());
                assert!(run.words.iter().all(|i| (*i as usize) < run.offsets.len()));
                for offset in &run.offsets {
                    let byte = run.start + offset;
                    assert_eq!(snapshot.byte(snapshot.position(byte)), Some(byte));
                }
            }
            assert_eq!(
                snapshot.byte(TextPosition {
                    node: NodeId(u64::MAX),
                    character_index: 0
                }),
                None
            );
            assert_eq!(
                snapshot.byte(TextPosition {
                    node: snapshot.runs[0].id,
                    character_index: usize::MAX
                }),
                None
            );
        });
    }

    #[::core::prelude::v1::test]
    fn accessible_actions_share_undo_and_reject_stale_or_composing_edits() {
        let mut app = TestApp::new();
        let mut window = app.open_window(|w, cx| {
            RichEditor::new(
                Document {
                    blocks: vec![Block::paragraph("First 👩‍👩‍👧‍👦 last")],
                    ..Default::default()
                },
                Theme::reference(),
                String::new(),
                w,
                cx,
            )
        });
        window.update(|s, w, cx| {
            let snapshot = capture(s, w);
            let selection = TextSelection {
                anchor: snapshot.position("First 👩‍👩‍👧‍👦".len()),
                focus: snapshot.position(6),
            };
            snapshot.act(
                AccessibleAction::SetTextSelection,
                Some(&ActionData::SetTextSelection(selection)),
                s,
                w,
                cx,
            );
            assert_eq!(s.model.range(), 6.."First 👩‍👩‍👧‍👦".len());
            assert!(
                s.model.global(&s.model.selection.anchor) > s.model.global(&s.model.selection.head)
            );
            // A replacement tied to an older selection must not replace a new selection.
            snapshot.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("wrong".into())),
                s,
                w,
                cx,
            );
            assert!(s.model.text().contains('👩'));
            let selected = capture(s, w);
            selected.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("family".into())),
                s,
                w,
                cx,
            );
            assert_eq!(s.model.text(), "First family last");
            selected.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("stale".into())),
                s,
                w,
                cx,
            );
            assert_eq!(s.model.text(), "First family last");
            s.model.undo();
            assert_eq!(s.model.text(), "First 👩‍👩‍👧‍👦 last");
            let snapshot = capture(s, w);
            s.model.input(None, "候補", Some(None)).unwrap();
            let text = s.model.text();
            snapshot.act(
                AccessibleAction::SetTextSelection,
                Some(&ActionData::SetTextSelection(selection)),
                s,
                w,
                cx,
            );
            snapshot.act(
                AccessibleAction::ReplaceSelectedText,
                Some(&ActionData::Value("wrong".into())),
                s,
                w,
                cx,
            );
            assert_eq!(s.model.text(), text);
            assert!(s.model.marked.is_some());
        });
    }
}
