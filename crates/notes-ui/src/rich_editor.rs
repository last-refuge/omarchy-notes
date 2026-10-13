//! Native rich text surface: one model supplies editing, shaping and hit testing.
#[path = "editor_accessibility.rs"]
mod accessibility;
#[path = "editor_links.rs"]
mod links;
#[path = "editor_media.rs"]
mod media;
#[path = "editor_related.rs"]
mod related;
#[path = "editor_selection.rs"]
mod selection;
#[path = "editor_tables.rs"]
mod tables;
#[path = "editor_tools.rs"]
mod tools;
use crate::theme::Theme;
use gpui::{prelude::*, *};
pub(crate) use links::{LinkNote, OpenNote};
pub(crate) use media::{ClipboardPlatform, MediaRequest};
use notes_core::{
    document::{Block, Document},
    text_field::TextField,
};
use notes_editor::Editor;
use std::{ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    rich,
    [
        FocusRelated,
        NextRelated,
        PreviousRelated,
        OpenRelated,
        CloseRelated,
        SelectionTools,
        CloseSelectionTools,
        NextSelectionControl,
        PreviousSelectionControl,
        LinkAddress,
        LinkNotePicker,
        CloseLinks,
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        WordLeft,
        WordRight,
        Up,
        Down,
        SelectUp,
        SelectDown,
        Home,
        End,
        SelectHome,
        SelectEnd,
        SelectAll,
        Enter,
        SoftBreak,
        Copy,
        Cut,
        Paste,
        PastePlain,
        Undo,
        Redo,
        Bold,
        Italic,
        Underline,
        Heading,
        Heading2,
        Heading3,
        Strikethrough,
        Highlight,
        InlineCode,
        Numbered,
        ToggleCheck,
        ListIndent,
        ListOutdent,
        FormatTools,
        CloseFormatTools,
        NextFormatControl,
        PreviousFormatControl,
        FindInNote,
        NextMatch,
        PreviousMatch,
        CloseFind,
        Body,
        Quote,
        Checklist,
        Bullet,
        Indent,
        Outdent,
        InsertImage,
        AttachFile,
        OpenMedia,
        SaveMedia,
        TableTools,
        CloseTableTools,
        NextTableControl,
        PreviousTableControl,
        LeaveTable,
        CancelComposition
    ]
);
pub fn bind_keys(cx: &mut App) {
    macro_rules! keys {($($key:literal => $action:ident),*$(,)?)=>{cx.bind_keys([$(KeyBinding::new($key,$action,Some("RichNote"))),*]);};}
    keys!("backspace"=>Backspace,"delete"=>Delete,"left"=>Left,"right"=>Right,"shift-left"=>SelectLeft,"shift-right"=>SelectRight,"ctrl-left"=>WordLeft,"ctrl-right"=>WordRight,"up"=>Up,"down"=>Down,"shift-up"=>SelectUp,"shift-down"=>SelectDown,"home"=>Home,"end"=>End,"shift-home"=>SelectHome,"shift-end"=>SelectEnd,"ctrl-a"=>SelectAll,"enter"=>Enter,"shift-enter"=>SoftBreak,"ctrl-c"=>Copy,"ctrl-x"=>Cut,"ctrl-v"=>Paste,"ctrl-shift-v"=>PastePlain,"ctrl-z"=>Undo,"ctrl-shift-z"=>Redo,"ctrl-y"=>Redo,"ctrl-b"=>Bold,"ctrl-i"=>Italic,"ctrl-u"=>Underline,"ctrl-alt-1"=>Heading,"ctrl-alt-0"=>Body,"ctrl-shift-c"=>Checklist,"ctrl-shift-8"=>Bullet,"ctrl-shift-7"=>Numbered,"tab"=>Indent,"shift-tab"=>Outdent,"ctrl-shift-i"=>InsertImage,"ctrl-shift-a"=>AttachFile,"ctrl-enter"=>OpenMedia,"ctrl-shift-e"=>SaveMedia,"ctrl-alt-t"=>TableTools,"ctrl-shift-enter"=>LeaveTable,"escape"=>CancelComposition);
    keys!("ctrl-shift-x"=>Strikethrough,"ctrl-shift-y"=>Highlight,"ctrl-e"=>InlineCode,"ctrl-shift-t"=>Heading,"ctrl-shift-h"=>Heading2,"ctrl-shift-j"=>Heading3,"ctrl-alt-2"=>Heading2,"ctrl-alt-3"=>Heading3,"ctrl-shift-b"=>Body,"ctrl-shift-l"=>Checklist,"ctrl-shift-u"=>ToggleCheck,"ctrl-]"=>ListIndent,"ctrl-["=>ListOutdent);
    // Linux reports shifted punctuation as the symbol with Shift consumed.
    keys!("ctrl-&"=>Numbered,"ctrl-*"=>Bullet);
    keys!("ctrl-shift-q"=>Quote);
    keys!("ctrl-shift-k"=>LinkAddress,"ctrl-l"=>LinkNotePicker);
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-r", FocusRelated, Some("NoteTools")),
        KeyBinding::new("down", NextRelated, Some("RelatedNotes")),
        KeyBinding::new("up", PreviousRelated, Some("RelatedNotes")),
        KeyBinding::new("enter", OpenRelated, Some("RelatedNotes")),
        KeyBinding::new("escape", CloseRelated, Some("RelatedNotes")),
        KeyBinding::new("ctrl-shift-space", SelectionTools, Some("NoteTools")),
        KeyBinding::new("escape", CloseSelectionTools, Some("SelectionTools")),
        KeyBinding::new("tab", NextSelectionControl, Some("SelectionTools")),
        KeyBinding::new(
            "shift-tab",
            PreviousSelectionControl,
            Some("SelectionTools"),
        ),
        KeyBinding::new("escape", CloseLinks, Some("LinkPanel")),
        KeyBinding::new("ctrl-f", FindInNote, Some("NoteTools")),
        KeyBinding::new("ctrl-shift-m", FormatTools, Some("NoteTools")),
        KeyBinding::new("f3", NextMatch, Some("NoteTools")),
        KeyBinding::new("shift-f3", PreviousMatch, Some("NoteTools")),
        KeyBinding::new("shift-enter", PreviousMatch, Some("FindInNote")),
        KeyBinding::new("escape", CloseFormatTools, Some("FormatTools")),
        KeyBinding::new("tab", NextFormatControl, Some("FormatTools")),
        KeyBinding::new("shift-tab", PreviousFormatControl, Some("FormatTools")),
        KeyBinding::new("escape", CloseTableTools, Some("TableTools")),
        KeyBinding::new("tab", NextTableControl, Some("TableTools")),
        KeyBinding::new("shift-tab", PreviousTableControl, Some("TableTools")),
    ]);
}

pub struct Changed;
/// Composition changes update save status without creating a durable edit.
pub struct InputState;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WritingStyle {
    pub font: notes_core::preferences::NoteFont,
    pub size: u16,
}
impl Default for WritingStyle {
    fn default() -> Self {
        Self {
            font: notes_core::preferences::NoteFont::Serif,
            size: 19,
        }
    }
}
pub struct RichEditor {
    pub model: Editor,
    media: media::State,
    links: links::State,
    selection_tools: selection::State,
    related: related::State,
    pub focus: FocusHandle,
    table_focus: Vec<FocusHandle>,
    table_scroll: ScrollHandle,
    table_menu: bool,
    table_notice: Option<String>,
    find: notes_editor::Find,
    find_field: Entity<crate::search::SearchField>,
    find_open: bool,
    format_menu: bool,
    format_focus: Vec<FocusHandle>,
    format_scroll: ScrollHandle,
    _find_subscription: Subscription,
    pub theme: Theme,
    pub(crate) typography: WritingStyle,
    typography_anchor: Option<(usize, Pixels, bool)>,
    coordinates_dirty: bool,
    pub error: Option<String>,
    pub library_id: String,
    layout: Option<Rc<Layout>>,
    bounds: Bounds<Pixels>,
    scroll: ScrollHandle,
    dragging: bool,
    goal_x: Option<Pixels>,
    reveal: bool,
}
impl EventEmitter<Changed> for RichEditor {}
impl EventEmitter<InputState> for RichEditor {}
impl RichEditor {
    pub(crate) fn set_typography(&mut self, style: WritingStyle, cx: &mut Context<Self>) {
        if self.typography == style {
            return;
        }
        if self.typography_anchor.is_none()
            && let Some(layout) = &self.layout
        {
            let top = self.scroll.bounds().top() - self.bounds.top();
            let head = self.model.global(&self.model.selection.head);
            let (caret, height) = layout.caret(head);
            let caret_visible =
                caret.y >= top && caret.y + height <= top + self.scroll.bounds().size.height;
            let index = if caret_visible {
                head
            } else {
                layout.index(point(px(0.), top.max(px(0.))))
            };
            let (anchor, _) = layout.caret(index);
            self.typography_anchor = Some((index, anchor.y - top, caret_visible));
        }
        self.typography = style;
        self.layout = None;
        self.coordinates_dirty = true;
        self.goal_x = None;
        cx.notify();
    }
    pub(crate) fn reveal_caret(&mut self, cx: &mut Context<Self>) {
        self.reveal = true;
        cx.notify();
    }
    /// A note has one live editing entity. Moving its view to another window
    /// keeps the model/undo history while replacing window-bound callbacks and
    /// geometry. Composition and asynchronous insertions must settle first.
    pub(crate) fn rebind_window(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.input_is_composing(cx) || self.has_pending_work() {
            return false;
        }
        self._find_subscription =
            cx.subscribe_in(&self.find_field, window, |s, _, event, w, cx| {
                s.find_event(event, w, cx)
            });
        self.links.rebind(window, cx);
        self.deactivate_media(cx);
        self.layout = None;
        self.bounds = Bounds::default();
        self.reveal = true;
        self.dragging = false;
        window.invalidate_character_coordinates();
        cx.notify();
        true
    }
    pub fn new(
        document: Document,
        theme: Theme,
        library_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let find_field = cx.new(|cx| crate::search::SearchField::named("Find in note", theme, cx));
        let subscription = cx.subscribe_in(&find_field, window, |s, _, event, w, cx| {
            s.find_event(event, w, cx)
        });
        Self {
            model: Editor::new(document),
            typography: WritingStyle::default(),
            typography_anchor: None,
            coordinates_dirty: false,
            links: links::State::new(theme, window, cx),
            selection_tools: selection::State::new(cx),
            related: related::State::new(cx),
            find: notes_editor::Find::default(),
            find_field,
            find_open: false,
            format_menu: false,
            format_focus: (0..17).map(|_| cx.focus_handle()).collect(),
            format_scroll: ScrollHandle::new(),
            _find_subscription: subscription,
            media: media::State::default(),
            focus: cx.focus_handle(),
            table_focus: (0..9).map(|_| cx.focus_handle()).collect(),
            table_scroll: ScrollHandle::new(),
            table_menu: false,
            table_notice: None,
            theme,
            error: None,
            library_id,
            layout: None,
            bounds: Bounds::default(),
            scroll: ScrollHandle::new(),
            dragging: false,
            goal_x: None,
            reveal: false,
        }
    }
    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        if self.theme == theme {
            return;
        }
        self.theme = theme;
        self.links.theme(theme, cx);
        self.find_field.update(cx, |s, cx| {
            s.theme = theme;
            cx.notify();
        });
        self.layout = None;
        cx.notify();
    }
    pub fn edit(
        &mut self,
        op: impl FnOnce(&mut Editor) -> anyhow::Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table_notice = None;
        let before = self.model.revision;
        let selection_before = self.model.selection.clone();
        let composing = self.model.marked.is_some();
        self.error = op(&mut self.model).err().map(|e| e.to_string());
        if self.model.selection != selection_before {
            self.selection_tools.rearm();
        }
        self.layout = None;
        self.reveal = true;
        self.goal_x = None;
        if before != self.model.revision {
            self.find.refresh(self.model.committed());
            self.refresh_related();
            cx.emit(Changed);
        }
        if composing != self.model.marked.is_some() {
            cx.emit(InputState);
        }
        self.refresh_media(window, cx);
        window.invalidate_character_coordinates();
        cx.notify();
    }
    fn move_to(&mut self, byte: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.model.selection.clone();
        self.model.select(byte, extend);
        if self.model.selection != previous {
            self.selection_tools.rearm();
        }
        self.reveal = true;
        window.invalidate_character_coordinates();
        cx.notify();
    }
    fn point_index(&self, point: Point<Pixels>) -> usize {
        self.layout
            .as_ref()
            .map_or(0, |l| l.index(point - self.bounds.origin))
    }
    fn vertical(&mut self, down: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.marked.is_some() {
            cx.propagate();
            return;
        }
        if let Some(layout) = &self.layout {
            let (p, h) = layout.caret(self.model.global(&self.model.selection.head));
            let x = *self.goal_x.get_or_insert(p.x);
            let index = layout.index(point(x, p.y + if down { h + px(2.) } else { -px(2.) }));
            self.move_to(index, extend, window, cx);
        }
    }
    fn line_edge(&mut self, end: bool, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(layout) = &self.layout {
            let (p, h) = layout.caret(self.model.global(&self.model.selection.head));
            let index = layout.index(point(
                if end { px(100000.) } else { px(-100000.) },
                p.y + h / 2.,
            ));
            self.move_to(index, extend, window, cx);
        }
    }
    fn copy(&mut self, plain: bool, cx: &mut Context<Self>) -> bool {
        let range = self.model.range();
        if range.is_empty() {
            return false;
        }
        let text = self.model.text()[range].replace('\u{fffc}', "");
        match self.model.fragment() {
            Ok(blocks) if !plain=>cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(text,serde_json::json!({"omarchy_notes_fragment":1,"library":self.library_id,"blocks":blocks}))),
            _=>cx.write_to_clipboard(ClipboardItem::new_string(text)),
        }
        true
    }
    fn paste(&mut self, plain: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.marked.is_some() {
            cx.propagate();
            return;
        }
        if let Some(item) = cx.read_from_clipboard() {
            if !plain
                && let Some(metadata) = item
                    .metadata()
                    .and_then(|m| serde_json::from_str::<serde_json::Value>(m).ok())
                && metadata["omarchy_notes_fragment"] == 1
            {
                match serde_json::from_value::<Vec<Block>>(metadata["blocks"].clone()) {
                    Ok(blocks) => {
                        let document = Document::from_blocks(blocks);
                        let has_objects = !document.references("image").is_empty()
                            || !document.references("attachment").is_empty();
                        if has_objects && metadata["library"] != self.library_id {
                            self.error=Some("Copying attachments between libraries is not available yet. Use Paste as plain text for the text only.".into());
                            cx.notify();
                            return;
                        }
                        if has_objects {
                            self.queue_import(
                                notes_storage::media::Import::Fragment(notes_exchange::Fragment {
                                    document,
                                    bundle: Default::default(),
                                }),
                                cx,
                            );
                        } else {
                            self.edit(|e| e.paste_fragment(document.blocks), window, cx);
                        }
                        return;
                    }
                    Err(_) => {
                        self.error=Some("This rich clipboard content could not be read. Use Paste as plain text.".into());
                        cx.notify();
                        return;
                    }
                }
            }
            if let Some(text) = item.text() {
                self.edit(|e| e.insert(&text, notes_storage::now() as u64), window, cx);
            }
        }
    }
    fn tool(
        &self,
        id: &'static str,
        label: &'static str,
        action: Format,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let active = match &action {
            Format::Mark(mark) => self.model.mark_active(mark),
            Format::Style(style) => self.model.style_active(style),
            _ => false,
        };
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .when(matches!(action, Format::Mark(_) | Format::Style(_)), |d| {
                let state = match action {
                    Format::Mark(mark) => self.model.mark_state(mark),
                    _ => Some(active),
                };
                d.aria_toggled(match state {
                    Some(true) => accesskit::Toggled::True,
                    Some(false) => accesskit::Toggled::False,
                    None => accesskit::Toggled::Mixed,
                })
            })
            .aria_description(if active {
                "Active formatting"
            } else {
                "Apply formatting"
            })
            .tab_index(0)
            .cursor_pointer()
            .px(px(10.))
            .py(px(5.))
            .rounded(px(5.))
            .border_1()
            .border_color(transparent_black())
            .when(active, |d| d.bg(theme.selected).text_color(theme.accent))
            .hover(move |d| d.bg(theme.overlay))
            .focus_visible(move |d| d.border_color(theme.accent))
            .on_mouse_down(MouseButton::Left, |_, w, _| w.prevent_default())
            .on_click(cx.listener(move |s, _, w, cx| s.format(action, w, cx)))
            .font_weight(if id == "bold" {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .when(id == "italic", |d| d.italic())
            .when(id == "underline", |d| d.underline())
            .child(match id {
                "bold" => "B",
                "italic" => "I",
                "underline" => "U",
                _ => label,
            })
    }
    fn format(&mut self, format: Format, w: &mut Window, cx: &mut Context<Self>) {
        self.format_menu = false;
        self.edit(
            |e| match format {
                Format::Mark(m) => e.mark(m),
                Format::Style(s) => {
                    if matches!(s, "bullet" | "ordered" | "check") && e.style_active(s) {
                        e.style("paragraph")
                    } else {
                        e.style(s)
                    }
                }
                Format::Indent(outdent) => {
                    if e.document
                        .block(&e.selection.head.block)
                        .is_some_and(|b| b.kind == "list")
                    {
                        e.indent(outdent)
                    } else {
                        Ok(())
                    }
                }
                Format::ToggleCheck => {
                    let id = e.selection.head.block.clone();
                    e.toggle_check(&id);
                    Ok(())
                }
                Format::Undo => {
                    e.undo();
                    Ok(())
                }
                Format::Redo => {
                    e.redo();
                    Ok(())
                }
            },
            w,
            cx,
        );
        w.focus(&self.focus, cx);
    }
}
#[derive(Clone, Copy)]
enum Format {
    Mark(&'static str),
    Style(&'static str),
    Undo,
    Redo,
    Indent(bool),
    ToggleCheck,
}

impl EntityInputHandler for RichEditor {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.model.text();
        let a = TextField::byte_in(&text, r.start);
        let b = TextField::byte_in(&text, r.end).max(a);
        *actual = Some(text[..a].encode_utf16().count()..text[..b].encode_utf16().count());
        Some(text[a..b].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.model.text();
        let r = self.model.range();
        Some(UTF16Selection {
            range: text[..r.start].encode_utf16().count()..text[..r.end].encode_utf16().count(),
            reversed: self.model.global(&self.model.selection.head)
                < self.model.global(&self.model.selection.anchor),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let text = self.model.text();
        self.model
            .marked
            .as_ref()
            .map(|r| text[..r.start].encode_utf16().count()..text[..r.end].encode_utf16().count())
    }
    fn unmark_text(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.edit(
            |e| {
                e.commit_composition();
                Ok(())
            },
            w,
            cx,
        );
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if r.is_none() && self.model.marked.is_none() {
            self.edit(|e| e.insert(text, notes_storage::now() as u64), w, cx);
        } else {
            self.edit(|e| e.input(r, text, None), w, cx);
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit(|e| e.input(r, text, Some(selection)), w, cx);
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let index = TextField::byte_in(&self.model.text(), r.start);
        let (p, h) = self.layout.as_ref()?.caret(index);
        Some(Bounds::new(self.bounds.origin + p, size(px(2.), h)))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let text = self.model.text();
        Some(
            text[..self.point_index(p).min(text.len())]
                .encode_utf16()
                .count(),
        )
    }
}

impl Render for RichEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div().flex().flex_col().size_full().min_h_0().overflow_hidden().key_context("NoteTools")
            .on_action(cx.listener(|s,_:&FocusRelated,w,cx|s.focus_related(w,cx)))
            .on_action(cx.listener(|s,_:&SelectionTools,w,cx|s.focus_selection_tools(w,cx)))
            .on_action(cx.listener(|s,_:&FindInNote,w,cx|s.open_find(w,cx)))
            .on_action(cx.listener(|s,_:&NextMatch,w,cx|s.find_step(false,w,cx)))
            .on_action(cx.listener(|s,_:&PreviousMatch,w,cx|s.find_step(true,w,cx)))
            .on_action(cx.listener(|s,_:&FormatTools,w,cx|s.open_format_tools(w,cx)))
            .child(div().id("formatting-tools").role(Role::Toolbar).aria_label("Text formatting").flex().flex_none().flex_wrap().gap(px(2.)).px(px(20.)).py(px(6.)).border_b_1().border_color(theme.border).text_size(px(12.)).text_color(theme.muted).when(self.links.open, |d|d.hidden())
                .child(self.media_button("format-tools","Format",|s,w,cx|s.open_format_tools(w,cx),cx).aria_label("Formatting options, Control Shift M").aria_expanded(self.format_menu))
                .child(self.tool("bold","Bold",Format::Mark("bold"),cx))
                .child(self.tool("italic","Italic",Format::Mark("italic"),cx))
                .child(self.tool("underline","Underline",Format::Mark("underline"),cx))

                .child(self.tool("undo","Undo",Format::Undo,cx))
                .child(self.tool("redo","Redo",Format::Redo,cx))
                .child(self.media_button("table-tools","Table",|s,w,cx|s.table_tools(w,cx),cx).aria_label(if self.model.in_table(){"Table options"}else{"Insert a 2 by 2 table"}).when(self.model.in_table(),|d|d.bg(theme.selected).text_color(theme.accent)))
                .child(self.media_button("insert-image","Image",|s,w,cx|s.choose_files(true,w,cx),cx))
                .child(self.media_button("attach-file","Attach",|s,w,cx|s.choose_files(false,w,cx),cx))
                .child(self.media_button("link-tools","Link",|s,w,cx|s.open_link_panel(false,w,cx),cx).aria_label("Add or edit link, Control Shift K"))
                .child(self.media_button("find-note","Find",|s,w,cx|s.open_find(w,cx),cx).aria_label("Find in this note, Control F")))
            .child(self.format_controls(window,cx))
            .child(self.find_controls(cx))
            .child(self.table_controls(window,cx))
            .child(self.link_controls(window,cx))
            .child(self.media_controls(cx))
            .when_some(self.error.clone(),|d,message|d.child(div().id("editor-error").role(Role::Alert).aria_label(message.clone()).flex_none().px(px(24.)).py(px(10.)).text_size(px(12.)).text_color(theme.accent).child(message)))
            .child(div().id("rich-document-scroll").track_scroll(&self.scroll).overflow_y_scroll().flex_1().min_h_0()
                .child(accessibility::input(div().id("rich-note-input").role(Role::MultilineTextInput).aria_label("Note content").aria_description("Rich text note. Control B for bold, Control I for italic, Control S to save.").track_focus(&self.focus).tab_index(0).key_context("RichNote").cursor(CursorStyle::IBeam)
                    .max_w(px(910.)).mx_auto().px(px(30.)).pt(px(34.)).pb(px(100.))
                    .on_action(cx.listener(|s,_:&Backspace,w,cx|s.edit(|e|e.delete(false),w,cx)))
                    .on_action(cx.listener(|s,_:&Delete,w,cx|s.edit(|e|e.delete(true),w,cx)))
                    .on_action(cx.listener(|s,_:&Left,w,cx|s.edit(|e|{e.move_horizontal(false,false,false);Ok(())},w,cx)))
                    .on_action(cx.listener(|s,_:&Right,w,cx|s.edit(|e|{e.move_horizontal(true,false,false);Ok(())},w,cx)))
                    .on_action(cx.listener(|s,_:&SelectLeft,w,cx|s.edit(|e|{e.move_horizontal(false,true,false);Ok(())},w,cx)))
                    .on_action(cx.listener(|s,_:&SelectRight,w,cx|s.edit(|e|{e.move_horizontal(true,true,false);Ok(())},w,cx)))
                    .on_action(cx.listener(|s,_:&WordLeft,w,cx|s.edit(|e|{e.move_horizontal(false,false,true);Ok(())},w,cx)))
                    .on_action(cx.listener(|s,_:&WordRight,w,cx|s.edit(|e|{e.move_horizontal(true,false,true);Ok(())},w,cx)))
                    .on_action(cx.listener(|s,_:&Up,w,cx|s.vertical(false,false,w,cx)))
                    .on_action(cx.listener(|s,_:&Down,w,cx|s.vertical(true,false,w,cx)))
                    .on_action(cx.listener(|s,_:&SelectUp,w,cx|s.vertical(false,true,w,cx)))
                    .on_action(cx.listener(|s,_:&SelectDown,w,cx|s.vertical(true,true,w,cx)))
                    .on_action(cx.listener(|s,_:&Home,w,cx|s.line_edge(false,false,w,cx)))
                    .on_action(cx.listener(|s,_:&End,w,cx|s.line_edge(true,false,w,cx)))
                    .on_action(cx.listener(|s,_:&SelectHome,w,cx|s.line_edge(false,true,w,cx)))
                    .on_action(cx.listener(|s,_:&SelectEnd,w,cx|s.line_edge(true,true,w,cx)))
                    .on_action(cx.listener(|s,_:&SelectAll,w,cx|s.edit(|e|{e.select_all();Ok(())},w,cx)))
                    .on_action(cx.listener(|s,_:&Enter,w,cx|{if s.model.marked.is_none(){s.edit(|e|e.enter(false),w,cx);}}))
                    .on_action(cx.listener(|s,_:&SoftBreak,w,cx|s.edit(|e|e.enter(true),w,cx)))
                    .on_action(cx.listener(|s,_:&Copy,w,cx|s.copy_rich(false,w,cx)))
                    .on_action(cx.listener(|s,_:&Cut,w,cx|s.copy_rich(true,w,cx)))
                    .on_action(cx.listener(|s,_:&Paste,w,cx|s.paste_rich(false,w,cx)))
                    .on_action(cx.listener(|s,_:&PastePlain,w,cx|s.paste_rich(true,w,cx)))
                    .on_action(cx.listener(|s,_:&Undo,w,cx|s.format(Format::Undo,w,cx)))
                    .on_action(cx.listener(|s,_:&Redo,w,cx|s.format(Format::Redo,w,cx)))
                    .on_action(cx.listener(|s,_:&Bold,w,cx|s.format(Format::Mark("bold"),w,cx)))
                    .on_action(cx.listener(|s,_:&Italic,w,cx|s.format(Format::Mark("italic"),w,cx)))
                    .on_action(cx.listener(|s,_:&Underline,w,cx|s.format(Format::Mark("underline"),w,cx)))
                    .on_action(cx.listener(|s,_:&Heading,w,cx|s.format(Format::Style("h1"),w,cx)))
                    .on_action(cx.listener(|s,_:&Heading2,w,cx|s.format(Format::Style("h2"),w,cx)))
                    .on_action(cx.listener(|s,_:&Heading3,w,cx|s.format(Format::Style("h3"),w,cx)))
                    .on_action(cx.listener(|s,_:&Strikethrough,w,cx|s.format(Format::Mark("strikethrough"),w,cx)))
                    .on_action(cx.listener(|s,_:&Highlight,w,cx|s.format(Format::Mark("highlight"),w,cx)))
                    .on_action(cx.listener(|s,_:&InlineCode,w,cx|s.format(Format::Mark("code"),w,cx)))
                    .on_action(cx.listener(|s,_:&Numbered,w,cx|s.format(Format::Style("ordered"),w,cx)))
                    .on_action(cx.listener(|s,_:&ToggleCheck,w,cx|s.format(Format::ToggleCheck,w,cx)))
                    .on_action(cx.listener(|s,_:&ListIndent,w,cx|s.format(Format::Indent(false),w,cx)))
                    .on_action(cx.listener(|s,_:&ListOutdent,w,cx|s.format(Format::Indent(true),w,cx)))
                    .on_action(cx.listener(|s,_:&Body,w,cx|s.format(Format::Style("paragraph"),w,cx)))
                    .on_action(cx.listener(|s,_:&Quote,w,cx|s.format(Format::Style("quote"),w,cx)))
                    .on_action(cx.listener(|s,_:&Checklist,w,cx|s.format(Format::Style("check"),w,cx)))
                    .on_action(cx.listener(|s,_:&Bullet,w,cx|s.format(Format::Style("bullet"),w,cx)))
                    .on_action(cx.listener(|s,_:&Indent,w,cx|s.table_tab(false,w,cx)))
                    .on_action(cx.listener(|s,_:&Outdent,w,cx|s.table_tab(true,w,cx)))
                    .on_action(cx.listener(|s,_:&TableTools,w,cx|s.table_tools(w,cx)))
                    .on_action(cx.listener(|s,_:&LeaveTable,w,cx|s.leave_table(w,cx)))
                    .on_action(cx.listener(|s,_:&InsertImage,w,cx|s.choose_files(true,w,cx)))
                    .on_action(cx.listener(|s,_:&AttachFile,w,cx|s.choose_files(false,w,cx)))
                    .on_action(cx.listener(|s,_:&OpenMedia,w,cx|s.open_link_or_media(w,cx)))
                    .on_action(cx.listener(|s,_:&LinkAddress,w,cx|s.open_link_panel(false,w,cx)))
                    .on_action(cx.listener(|s,_:&LinkNotePicker,w,cx|s.open_link_panel(true,w,cx)))
                    .on_action(cx.listener(|s,_:&SaveMedia,w,cx|s.save_selected(w,cx)))
                    .on_drop::<ExternalPaths>(cx.listener(|s,paths:&ExternalPaths,_,cx|s.queue_import(notes_storage::media::Import::Files{paths:paths.paths().to_vec(),images:None},cx)))
                    .on_action(cx.listener(|s,_:&CancelComposition,w,cx|{if s.model.marked.is_some(){s.edit(|e|{e.cancel_composition();Ok(())},w,cx);}else if s.selection_tools.bounds.is_some() {s.dismiss_selection_tools(w,cx);}else if s.table_menu {s.close_table_tools(w,cx);}else if s.find_open {s.close_find(w,cx);}else{cx.propagate();}}))
                    .on_mouse_down(MouseButton::Left,cx.listener(|s,e:&MouseDownEvent,w,cx|{
                        if s.model.marked.is_some(){return;}
                        w.focus(&s.focus,cx);s.selection_tools.rearm();s.dragging=true;s.goal_x=None;
                        if e.modifiers.control && let Some(link)=s.link_at_point(e.position) {s.dragging=false;s.follow_href(&link.href,cx);return;}
                        let local=e.position-s.bounds.origin;
                        if let Some(id)=s.layout.as_ref().and_then(|l|l.checks.iter().find(|(_,b,_)|b.contains(&local)).map(|(id,_,_)|id.clone())) {s.edit(|e|{e.toggle_check(&id);Ok(())},w,cx);return;}
                        if let Some(start)=s.layout.as_ref().and_then(|l|l.objects.iter().find(|o|o.bounds.contains(&local)).map(|o|o.start)) {s.move_to(start,false,w,cx);s.move_to(start+3,true,w,cx);if e.click_count>=2{s.open_selected(w,cx);}return;}
                        let index=s.point_index(e.position);
                        if e.click_count>=3 {let p=s.model.position(index);let start=s.model.global(&notes_editor::Position{block:p.block.clone(),offset:0});let len=s.model.document.block(&p.block).unwrap().text().len();s.move_to(start,false,w,cx);s.move_to(start+len,true,w,cx);}
                        else if e.click_count==2 {let text=s.model.text();let r=text.unicode_word_indices().find(|(i,t)|*i<=index && *i+t.len()>=index).map(|(i,t)|i..i+t.len()).unwrap_or(index..index);s.move_to(r.start,false,w,cx);s.move_to(r.end,true,w,cx);}
                        else {s.move_to(index,e.modifiers.shift,w,cx);}
                    }))
                    .on_mouse_move(cx.listener(|s,e:&MouseMoveEvent,w,cx|{if s.dragging {let i=s.point_index(e.position);s.move_to(i,true,w,cx);let bounds=s.scroll.bounds();let delta=if e.position.y<bounds.top(){px(14.)}else if e.position.y>bounds.bottom(){px(-14.)}else{px(0.)};if delta!=px(0.){s.scroll.set_offset(s.scroll.offset()+point(px(0.),delta));cx.notify();}}}))
                    .on_mouse_up(MouseButton::Left,cx.listener(|s,_,_,cx|{s.dragging=false;cx.notify();}))
                    .on_mouse_up_out(MouseButton::Left,cx.listener(|s,_,_,cx|{s.dragging=false;cx.notify();}))
                    .child(TextSurface {editor:cx.entity()}), cx.entity()))
                .child(self.related_notes(window,cx)))
            .child(selection::Surface {editor:cx.entity()})
    }
}

struct Line {
    line: WrappedLine,
    origin: Point<Pixels>,
    height: Pixels,
    start: usize,
}
struct Object {
    key: String,
    start: usize,
    bounds: Bounds<Pixels>,
    image: bool,
    label: String,
}
struct Layout {
    objects: Vec<Object>,
    width: Pixels,
    height: Pixels,
    lines: Vec<Line>,
    checks: Vec<(String, Bounds<Pixels>, bool)>,
    borders: Vec<Bounds<Pixels>>,
    quotes: Vec<Bounds<Pixels>>,
    labels: Vec<(String, Point<Pixels>)>,
}
impl Layout {
    fn index(&self, p: Point<Pixels>) -> usize {
        if let Some(object) = self.objects.iter().find(|o| o.bounds.contains(&p)) {
            return object.start + if p.x > object.bounds.center().x { 3 } else { 0 };
        }
        let line = self.lines.iter().min_by(|a, b| {
            let distance = |l: &Line| {
                let bounds = Bounds::new(
                    l.origin,
                    size(l.line.width().max(px(1.)), l.line.size(l.height).height),
                );
                let dy = (bounds.top() - p.y).max(px(0.)).max(p.y - bounds.bottom());
                let dx = (bounds.left() - p.x).max(px(0.)).max(p.x - bounds.right());
                let y: f32 = dy.into();
                let x: f32 = dx.into();
                y * 10000. + x
            };
            distance(a).total_cmp(&distance(b))
        });
        line.map_or(0, |l| {
            l.start
                + l.line
                    .closest_index_for_position(p - l.origin, l.height)
                    .unwrap_or_else(|i| i)
                    .min(l.line.len())
        })
    }
    fn caret(&self, index: usize) -> (Point<Pixels>, Pixels) {
        if let Some(object) = self
            .objects
            .iter()
            .find(|o| index >= o.start && index <= o.start + 3)
        {
            return (
                point(
                    if index > object.start {
                        object.bounds.right()
                    } else {
                        object.bounds.left()
                    },
                    object.bounds.top(),
                ),
                object.bounds.size.height.min(px(32.)),
            );
        }
        let line = self
            .lines
            .iter()
            .find(|l| index >= l.start && index <= l.start + l.line.len())
            .or_else(|| self.lines.last());
        line.map_or((point(px(0.), px(0.)), px(30.)), |l| {
            (
                l.origin
                    + l.line
                        .position_for_index(
                            index.saturating_sub(l.start).min(l.line.len()),
                            l.height,
                        )
                        .unwrap_or_default(),
                l.height,
            )
        })
    }
}
fn make_layout(
    document: &Document,
    width: Pixels,
    theme: Theme,
    typography: WritingStyle,
    window: &Window,
) -> Layout {
    let mut layout = Layout {
        width,
        objects: vec![],
        height: px(0.),
        lines: vec![],
        checks: vec![],
        borders: vec![],
        quotes: vec![],
        labels: vec![],
    };
    fn layout_blocks(
        blocks: &[Block],
        origin: Point<Pixels>,
        width: Pixels,
        global: &mut usize,
        out: &mut Layout,
        style: (Theme, WritingStyle),
        w: &Window,
    ) -> Pixels {
        let (theme, typography) = style;
        let x = origin.x;
        let mut y = origin.y;
        let mut ordered = [0usize; 17];
        for block in blocks {
            if block.kind == "list" {
                let indent = block.number("indent", 0).min(16);
                ordered[indent] = if block.string("list") == "ordered" {
                    ordered[indent] + 1
                } else {
                    0
                };
                ordered[indent + 1..].fill(0);
            } else {
                ordered.fill(0);
            }
            if block.kind == "table" {
                let columns = block.number("columns", 1);
                let rows = block.number("rows", 1);
                let cell_w = width / columns as f32;
                let mut row_heights = vec![px(42.); rows];
                let mut cells = vec![];
                for cell in &block.cells {
                    let start = (
                        out.lines.len(),
                        out.checks.len(),
                        out.borders.len(),
                        out.labels.len(),
                        out.objects.len(),
                    );
                    let col_span = cell
                        .attrs
                        .get("colSpan")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as usize;
                    let row_span = cell
                        .attrs
                        .get("rowSpan")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(1) as usize;
                    let cell_x = x + cell_w * cell.col as f32;
                    let h = layout_blocks(
                        &cell.blocks,
                        point(cell_x + px(12.), px(12.)),
                        (cell_w * col_span as f32 - px(24.)).max(px(20.)),
                        global,
                        out,
                        style,
                        w,
                    ) + px(4.);
                    for height in &mut row_heights[cell.row..cell.row + row_span] {
                        *height = (*height).max(h / row_span as f32);
                    }
                    cells.push((
                        cell.row,
                        col_span,
                        row_span,
                        cell_x,
                        start,
                        (
                            out.lines.len(),
                            out.checks.len(),
                            out.borders.len(),
                            out.labels.len(),
                            out.objects.len(),
                        ),
                    ));
                }
                for (row, col_span, row_span, cell_x, start, end) in cells {
                    let top = y + row_heights[..row].iter().copied().sum::<Pixels>();
                    for l in &mut out.lines[start.0..end.0] {
                        l.origin.y += top;
                    }
                    for (_, b, _) in &mut out.checks[start.1..end.1] {
                        b.origin.y += top;
                    }
                    for object in &mut out.objects[start.4..end.4] {
                        object.bounds.origin.y += top;
                    }
                    for b in &mut out.borders[start.2..end.2] {
                        b.origin.y += top;
                    }
                    for (_, p) in &mut out.labels[start.3..end.3] {
                        p.y += top;
                    }
                    out.borders.push(Bounds::new(
                        point(cell_x, top),
                        size(
                            cell_w * col_span as f32,
                            row_heights[row..row + row_span].iter().copied().sum(),
                        ),
                    ));
                }
                y += row_heights.iter().copied().sum::<Pixels>() + px(22.);
                continue;
            }
            let quote_top = y;
            let indent = if block.kind == "quote" {
                px(24.)
            } else if block.kind == "list" {
                px(30. + block.number("indent", 0) as f32 * 22.)
            } else {
                px(0.)
            };
            let size = (if block.kind == "heading" {
                match block.number("level", 1) {
                    1 => 32.,
                    2 => 26.,
                    _ => 22.,
                }
            } else {
                19.
            }) * typography.size as f32
                / 19.;
            let height = px(size * 1.55);
            if block.kind == "list" {
                if block.string("list") == "check" {
                    out.checks.push((
                        block.id.clone(),
                        Bounds::new(
                            point(x + indent - px(28.), y + px(6.)),
                            size_fn(px(19.), px(19.)),
                        ),
                        block.checked(),
                    ));
                } else {
                    out.labels.push((
                        if block.string("list") == "ordered" {
                            format!("{}.", ordered[block.number("indent", 0).min(16)])
                        } else {
                            "•".into()
                        },
                        point(x + indent - px(24.), y),
                    ));
                }
            }
            let mut segments: Vec<Vec<notes_core::document::Span>> = vec![];
            let mut text_spans = vec![];
            for span in &block.spans {
                if span.is_text() {
                    text_spans.push(span.clone());
                } else {
                    if !text_spans.is_empty() {
                        segments.push(std::mem::take(&mut text_spans));
                    }
                    segments.push(vec![span.clone()]);
                }
            }
            if !text_spans.is_empty() || segments.is_empty() {
                segments.push(text_spans);
            }
            for spans in segments {
                if spans.first().is_some_and(|s| !s.is_text()) {
                    let span = &spans[0];
                    let is_image = span.0.get("image").is_some();
                    let key = if is_image {
                        format!("image:{}", span.0["image"].as_str().unwrap_or(""))
                    } else {
                        format!("attachment:{}", span.0["attachment"].as_str().unwrap_or(""))
                    };
                    let available = (width - indent).max(px(40.));
                    let (w, h) = if is_image {
                        let native_w = span
                            .0
                            .get("width")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(640.)
                            .clamp(1., 12000.) as f32;
                        let native_h = span
                            .0
                            .get("height")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(360.)
                            .clamp(1., 12000.) as f32;
                        let scale = (f32::from(available) / native_w)
                            .min(360. / native_h)
                            .min(1.);
                        (
                            px(native_w * scale).max(px(40.)),
                            px(native_h * scale).max(px(40.)),
                        )
                    } else {
                        (available.min(px(520.)), px(74.))
                    };
                    y += px(8.);
                    out.objects.push(Object {
                        key,
                        start: *global,
                        bounds: Bounds::new(point(x + indent, y), size_fn(w, h)),
                        image: is_image,
                        label: span
                            .0
                            .get("alt")
                            .and_then(|v| v.as_str())
                            .unwrap_or(if is_image { "Image" } else { "Attachment" })
                            .to_owned(),
                    });
                    y += h + px(12.);
                    *global += 3;
                    continue;
                }
                let text: String = spans.iter().map(|s| s.content()).collect();
                let mut runs = vec![];
                for span in &spans {
                    let mut font = font(if span.mark("code") {
                        "monospace"
                    } else {
                        typography.font.family()
                    });
                    if span.mark("bold") || block.kind == "heading" {
                        font.weight = FontWeight::BOLD;
                    }
                    if span.mark("italic") {
                        font.style = FontStyle::Italic;
                    }
                    let link = span.0.get("href").is_some();
                    runs.push(TextRun {
                        len: span.content().len(),
                        font,
                        color: if link { theme.accent } else { theme.text },
                        background_color: span.mark("highlight").then_some(theme.selected),
                        underline: (span.mark("underline") || link).then_some(UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: false,
                        }),
                        strikethrough: span.mark("strikethrough").then_some(StrikethroughStyle {
                            thickness: px(1.),
                            color: None,
                        }),
                    });
                }
                let display = if text.is_empty() {
                    " ".to_owned()
                } else {
                    text.clone()
                };
                if runs.is_empty() {
                    runs.push(TextRun {
                        len: display.len(),
                        font: font(typography.font.family()),
                        color: theme.text,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    });
                }
                let lines = w
                    .text_system()
                    .shape_text(
                        display.into(),
                        px(size),
                        &runs,
                        Some((width - indent).max(px(20.))),
                        None,
                    )
                    .unwrap_or_default();
                let mut local = 0;
                for line in lines {
                    let h = line.size(height).height;
                    let len = line.len();
                    out.lines.push(Line {
                        line,
                        origin: point(x + indent, y),
                        height,
                        start: *global + local,
                    });
                    y += h;
                    local += len + 1;
                }
                *global += text.len();
            }
            *global += 1;
            if block.kind == "quote" {
                out.quotes.push(Bounds::new(
                    point(x + px(2.), quote_top + px(3.)),
                    size_fn(px(2.), (y - quote_top - px(6.)).max(px(18.))),
                ));
            }
            y += if block.kind == "list" {
                px(4.)
            } else {
                px(16.)
            };
        }
        y
    }
    layout.height = layout_blocks(
        &document.blocks,
        point(px(0.), px(0.)),
        width,
        &mut 0,
        &mut layout,
        (theme, typography),
        window,
    )
    .max(px(220.));
    layout
}
fn size_fn(w: Pixels, h: Pixels) -> Size<Pixels> {
    size(w, h)
}

struct TextSurface {
    editor: Entity<RichEditor>,
}
impl IntoElement for TextSurface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for TextSurface {
    type RequestLayoutState = ();
    type PrepaintState = Rc<Layout>;
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
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let editor = self.editor.clone();
        (
            window.request_measured_layout(style, move |known, available, window, cx| {
                let width = known
                    .width
                    .unwrap_or(match available.width {
                        AvailableSpace::Definite(w) => w,
                        _ => px(700.),
                    })
                    .max(px(20.));
                editor.update(cx, |s, _| {
                    if s.layout.as_ref().is_none_or(|l| l.width != width) {
                        s.layout = Some(Rc::new(make_layout(
                            &s.model.document,
                            width,
                            s.theme,
                            s.typography,
                            window,
                        )));
                    }
                    size(width, s.layout.as_ref().unwrap().height)
                })
            }),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Rc<Layout> {
        self.editor.update(cx, |s, cx| {
            if s.layout
                .as_ref()
                .is_none_or(|l| l.width != bounds.size.width)
            {
                s.layout = Some(Rc::new(make_layout(
                    &s.model.document,
                    bounds.size.width,
                    s.theme,
                    s.typography,
                    window,
                )));
            }
            if s.bounds != bounds || s.coordinates_dirty {
                s.bounds = bounds;
                s.coordinates_dirty = false;
                window.invalidate_character_coordinates();
            }
            if let Some((index, mut offset, caret)) = s.typography_anchor.take() {
                let (position, height) = s.layout.as_ref().unwrap().caret(index);
                if caret {
                    offset = offset
                        .min((s.scroll.bounds().size.height - height - px(12.)).max(px(0.)))
                        .max(px(0.));
                }
                let now = bounds.top() + position.y - s.scroll.bounds().top();
                s.scroll
                    .set_offset(s.scroll.offset() + point(px(0.), offset - now));
                cx.notify();
            }
            if s.reveal {
                s.reveal = false;
                let (p, h) = s
                    .layout
                    .as_ref()
                    .unwrap()
                    .caret(s.model.global(&s.model.selection.head));
                let caret = bounds.origin + p;
                let viewport = s.scroll.bounds();
                let delta = if caret.y < viewport.top() + px(12.) {
                    viewport.top() + px(12.) - caret.y
                } else if caret.y + h > viewport.bottom() - px(12.) {
                    viewport.bottom() - px(12.) - caret.y - h
                } else {
                    px(0.)
                };
                if delta != px(0.) && viewport.size.height > px(0.) {
                    s.scroll
                        .set_offset(s.scroll.offset() + point(px(0.), delta));
                    cx.notify();
                }
            }
            s.layout.clone().unwrap()
        })
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        layout: &mut Rc<Layout>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let s = self.editor.read(cx);
        let theme = s.theme;
        let focus = s.focus.clone();
        let range = s.model.range();
        let find_hits = if s.find_open && s.model.marked.is_none() {
            s.find.matches.clone()
        } else {
            Default::default()
        };
        let active_find = s.find_open && s.find.selected() == Some(&range);
        let head = s.model.global(&s.model.selection.head);
        let marked = s.model.marked.clone();
        let text_len = s.model.text().len();
        let empty = s.model.text().is_empty();
        let media = s.media.items.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for object in &layout.objects {
            let b = Bounds::new(bounds.origin + object.bounds.origin, object.bounds.size);
            let item = media.get(&object.key);
            let selected = range.start <= object.start && range.end >= object.start + 3;
            let border = if selected { theme.accent } else { theme.border };
            window.paint_quad(fill(b, theme.overlay).corner_radii(px(8.)));
            if object.image {
                if let Some(image) = item.and_then(|p| p.image.clone()) {
                    let image_size = image.size(0);
                    let iw = image_size.width.0 as f32;
                    let ih = image_size.height.0 as f32;
                    let scale = (f32::from(b.size.width) / iw).min(f32::from(b.size.height) / ih);
                    let image_bounds = Bounds::new(
                        b.center() - point(px(iw * scale / 2.), px(ih * scale / 2.)),
                        size_fn(px(iw * scale), px(ih * scale)),
                    );
                    let _ =
                        window.paint_image(b, image_bounds, Corners::all(px(8.)), image, 0, false);
                } else {
                    paint_label(
                        if item.is_some_and(|p| p.info.error.is_some()) {
                            "Image unavailable"
                        } else {
                            "Loading image…"
                        },
                        b.origin + point(px(16.), px(18.)),
                        theme.muted,
                        window,
                        cx,
                    );
                }
            } else {
                let title = item.map_or(object.label.as_str(), |p| p.info.name.as_str());
                // Fit titles to card width; the full filename is in the selected-media controls.
                let max = ((f32::from(b.size.width) - 60.) / 8.).max(4.) as usize;
                let badge = Bounds::new(
                    b.origin + point(px(14.), px(16.)),
                    size_fn(px(34.), px(42.)),
                );
                window.paint_quad(
                    outline(badge, theme.border, BorderStyle::Solid).corner_radii(px(5.)),
                );
                for (y, width) in [(13., 16.), (20., 16.), (27., 10.)] {
                    window.paint_quad(fill(
                        Bounds::new(
                            badge.origin + point(px(9.), px(y)),
                            size_fn(px(width), px(1.)),
                        ),
                        theme.muted,
                    ));
                }
                let mut title: String = title.chars().take(max.saturating_sub(4)).collect();
                if title.chars().count()
                    < item.map_or(object.label.chars().count(), |p| {
                        p.info.name.chars().count()
                    })
                {
                    title.push('…');
                }
                paint_label(
                    &title,
                    b.origin + point(px(64.), px(10.)),
                    theme.text,
                    window,
                    cx,
                );
                let detail = if let Some(p) = item {
                    if p.info.error.is_some() {
                        "File unavailable · select for details".into()
                    } else {
                        format!(
                            "{}  ·  {}",
                            media::file_kind(&p.info.mime),
                            media::bytes_label(p.info.size)
                        )
                    }
                } else {
                    "Loading attachment…".into()
                };
                let detail: String = detail.chars().take(max.saturating_sub(4)).collect();
                paint_label(
                    &detail,
                    b.origin + point(px(64.), px(38.)),
                    theme.muted,
                    window,
                    cx,
                );
            }
            window.paint_quad(outline(b, border, BorderStyle::Solid).corner_radii(px(8.)));
        }
        for b in &layout.borders {
            let b = Bounds::new(bounds.origin + b.origin, b.size);
            window.paint_quad(outline(b, theme.border, BorderStyle::Solid));
        }
        for b in &layout.quotes {
            window.paint_quad(
                fill(Bounds::new(bounds.origin + b.origin, b.size), theme.accent)
                    .corner_radii(px(1.)),
            );
        }
        for line in &layout.lines {
            let origin = bounds.origin + line.origin;
            let _ =
                line.line
                    .paint_background(origin, line.height, TextAlign::Left, None, window, cx);
            let paint_ranges =
                |r: &Range<usize>, underline: bool, color: Hsla, window: &mut Window| {
                    let start = r.start.saturating_sub(line.start).min(line.line.len());
                    let end = r.end.saturating_sub(line.start).min(line.line.len());
                    if start >= end {
                        return;
                    }
                    // Per-grapheme quads also handle reversed glyph positions in bidi runs.
                    for (byte, g) in line
                        .line
                        .text
                        .grapheme_indices(true)
                        .filter(|(i, g)| *i < end && *i + g.len() > start)
                    {
                        let a = line
                            .line
                            .position_for_index(byte, line.height)
                            .unwrap_or_default();
                        let b = line
                            .line
                            .position_for_index(byte + g.len(), line.height)
                            .unwrap_or(a);
                        let width = if a.y == b.y {
                            (b.x - a.x).abs().max(px(2.))
                        } else {
                            (layout.width - a.x).max(px(2.))
                        };
                        let x = if a.y == b.y { a.x.min(b.x) } else { a.x };
                        window.paint_quad(fill(
                            Bounds::new(
                                origin
                                    + point(
                                        x,
                                        a.y + if underline {
                                            line.height - px(2.)
                                        } else {
                                            px(0.)
                                        },
                                    ),
                                size(width, if underline { px(1.) } else { line.height }),
                            ),
                            color,
                        ));
                    }
                };
            // Hits are sorted. Visit only the matches intersecting this line.
            let first = find_hits.partition_point(|r| r.end <= line.start);
            for hit in find_hits[first..]
                .iter()
                .take_while(|r| r.start < line.start + line.line.len())
            {
                paint_ranges(hit, false, theme.accent.opacity(0.18), window);
            }
            paint_ranges(
                &range,
                false,
                if active_find {
                    theme.accent.opacity(0.3)
                } else {
                    theme.selected
                },
                window,
            );
            if active_find {
                paint_ranges(&range, true, theme.accent, window);
            }
            if let Some(marked) = &marked {
                paint_ranges(marked, true, theme.accent, window);
            }
            let _ = line
                .line
                .paint(origin, line.height, TextAlign::Left, None, window, cx);
        }
        for (_, b, checked) in &layout.checks {
            let b = Bounds::new(bounds.origin + b.origin, b.size);
            window.paint_quad(outline(
                b,
                if *checked { theme.accent } else { theme.muted },
                BorderStyle::Solid,
            ));
            if *checked {
                window.paint_quad(fill(b, theme.accent));
                paint_label(
                    "✓",
                    b.origin - px_point(0., 4.),
                    theme.on_accent,
                    window,
                    cx,
                );
            }
        }
        for (label, p) in &layout.labels {
            paint_label(label, bounds.origin + *p, theme.muted, window, cx);
        }
        if empty {
            paint_label(
                "A thought, a sentence, a small detail…",
                bounds.origin,
                theme.muted,
                window,
                cx,
            );
        }
        if focus.is_focused(window) && range.is_empty() {
            let (p, h) = layout.caret(head.min(text_len));
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + p + point(px(0.), px(3.)),
                    size(px(1.5), h - px(6.)),
                ),
                theme.accent,
            ));
        }
    }
}
fn px_point(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}
fn paint_label(text: &str, origin: Point<Pixels>, color: Hsla, w: &mut Window, cx: &mut App) {
    let run = TextRun {
        len: text.len(),
        font: font("Noto Sans"),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = w
        .text_system()
        .shape_line(text.to_owned().into(), px(12.), &[run], None);
    let _ = line.paint(origin, px(26.), TextAlign::Left, None, w, cx);
}
