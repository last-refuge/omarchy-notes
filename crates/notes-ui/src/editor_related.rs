//! Cached explicit relationships; the footer never parses a document.
use super::*;
use notes_core::links::{LinkTarget, RelatedNote, related_notes};
pub(super) struct State {
    items: Vec<RelatedNote>,
    selected: usize,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    #[cfg(feature = "visual-tests")]
    mounted: usize,
}
impl State {
    pub fn new(cx: &mut App) -> Self {
        Self {
            items: vec![],
            selected: 0,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            #[cfg(feature = "visual-tests")]
            mounted: 0,
        }
    }
}
impl RichEditor {
    #[cfg(feature = "visual-tests")]
    pub(crate) fn related_test_counts(&self) -> (usize, usize) {
        (self.related.items.len(), self.related.mounted)
    }

    pub(super) fn refresh_related(&mut self) {
        let previous = self
            .related
            .items
            .get(self.related.selected)
            .map(|n| n.id.clone());
        self.related.items = related_notes(
            self.model.committed(),
            &self.links.own_id,
            &self.links.catalog,
        );
        self.related.selected = previous
            .and_then(|id| self.related.items.iter().position(|n| n.id == id))
            .unwrap_or(0);
    }
    pub(crate) fn focus_related(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        if self.related.items.is_empty() {
            self.open_link_panel(true, w, cx);
            return;
        }
        self.format_menu = false;
        self.table_menu = false;
        self.find_open = false;
        self.links.open = false;
        self.scroll.scroll_to_item(1);
        self.related
            .scroll
            .scroll_to_item(self.related.selected, ScrollStrategy::Center);
        w.focus(&self.related.focus, cx);
        cx.notify();
    }
    fn step_related(&mut self, backward: bool, cx: &mut Context<Self>) {
        let count = self.related.items.len();
        if count == 0 {
            return;
        }
        self.related.selected = if backward {
            (self.related.selected + count - 1) % count
        } else {
            (self.related.selected + 1) % count
        };
        self.related
            .scroll
            .scroll_to_item(self.related.selected, ScrollStrategy::Center);
        cx.notify();
    }
    fn activate_related(&mut self, id: &str, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        let Some(item) = self.related.items.iter().find(|n| n.id == id) else {
            return;
        };
        if !item.missing {
            self.follow_href(&format!("note://{id}"), cx);
            return;
        }
        // Reveal the exact stored link so Edit and Remove repair the original text.
        let mut offset = 0;
        let mut found = None;
        'outer: for block in self.model.document.leaves() {
            for span in &block.spans {
                if span.is_text()
                    && span
                        .0
                        .get("href")
                        .and_then(|v| v.as_str())
                        .and_then(|s| LinkTarget::parse(s).ok())
                        == Some(LinkTarget::Note(id.to_owned()))
                {
                    found = Some(offset);
                    break 'outer;
                }
                offset += span.content().len();
            }
            offset += 1;
        }
        if let Some(offset) = found {
            self.move_to(offset, false, w, cx);
            if let Some(link) = self.model.current_link() {
                self.move_to(link.range.end, true, w, cx);
            }
            self.open_link_panel(false, w, cx);
        }
    }
    fn open_selected_related(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self
            .related
            .items
            .get(self.related.selected)
            .map(|n| n.id.clone())
        {
            self.activate_related(&id, w, cx);
        }
    }
    pub(super) fn related_notes(&self, w: &Window, cx: &mut Context<Self>) -> AnyElement {
        let count = self.related.items.len();
        if count == 0 {
            return div().into_any_element();
        }
        let theme = self.theme;
        let current = &self.related.items[self.related.selected];
        let status = format!(
            "{} of {count} · {}{}",
            self.related.selected + 1,
            current.title,
            if current.missing {
                " · Unavailable; Enter to edit link"
            } else {
                " · Enter to open"
            }
        );
        div().id("related-notes").role(Role::Group).aria_label("Related notes").key_context("RelatedNotes").max_w(px(910.)).mx_auto().px(px(30.)).pb(px(40.)).text_size(px(12.)).text_color(theme.muted)
            .on_action(cx.listener(|s,_:&NextRelated,_,cx|s.step_related(false,cx)))
            .on_action(cx.listener(|s,_:&PreviousRelated,_,cx|s.step_related(true,cx)))
            .on_action(cx.listener(|s,_:&OpenRelated,w,cx|s.open_selected_related(w,cx)))
            .on_action(cx.listener(|s,_:&CloseRelated,w,cx|{w.focus(&s.focus,cx);s.reveal=true;cx.notify();}))
            .child(div().border_t_1().border_color(theme.border).pt(px(14.)).pb(px(6.))
                .child(div().id("related-heading").role(Role::Button).aria_label(format!("Related notes, {count}. Control Shift R")).aria_description("Explicit links in this note. Up and Down choose a target; Enter opens or repairs it. Escape returns to writing.").track_focus(&self.related.focus).tab_index(0).flex().items_center().justify_between().rounded(px(4.)).border_1().border_color(transparent_black()).px(px(6.)).py(px(4.)).cursor_pointer().focus_visible(move|d|d.border_color(theme.accent)).on_click(cx.listener(|s,_,w,cx|s.focus_related(w,cx)))
                    .child("RELATED NOTES").child(format!("{count}"))))
            .child(div().h(((w.viewport_size().height*0.3)-px(40.)).clamp(px(48.),px(168.)).min(px(count as f32*48.)))
                .child(uniform_list("related-note-results",count,cx.processor(move|s,range:Range<usize>,w,cx|{
                    #[cfg(feature = "visual-tests")]
                    { s.related.mounted=range.len(); }
                    range.map(|row|{
                    let n=s.related.items[row].clone();let id=n.id.clone();
                    let label=if n.missing{format!("Edit unavailable link: {}",n.title)}else{format!("Open related note: {}",n.title)};
                    div().id(("related-note",row)).role(Role::Button).aria_label(label).aria_selected(row==s.related.selected).h(px(48.)).w_full().px(px(8.)).py(px(5.)).rounded(px(5.)).cursor_pointer().when(row==s.related.selected && s.related.focus.is_focused(w),|d|d.bg(theme.selected)).hover(move|d|d.bg(theme.selected)).on_mouse_down(MouseButton::Left,|_,w,_|w.prevent_default()).on_click(cx.listener(move|s,_,w,cx|s.activate_related(&id,w,cx)))
                        .child(div().truncate().text_color(theme.text).child(n.title))
                        .child(div().truncate().text_size(px(10.)).child(if n.missing{"Unavailable · Edit link".into()}else{n.folder}))
                }).collect()})).track_scroll(&self.related.scroll).size_full()))
            .when(self.related.focus.is_focused(w),|d|d.child(div().id("related-status").role(Role::Status).aria_label(status.clone()).mt(px(6.)).truncate().text_size(px(11.)).child(status)))
            .into_any_element()
    }
}
