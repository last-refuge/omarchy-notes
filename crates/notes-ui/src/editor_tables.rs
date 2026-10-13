//! Contextual table controls use the pure model's availability and transactions.
use super::*;
use notes_editor::TableAction;
const OPTIONS: [(&str, &str, TableAction); 8] = [
    ("table-row-after", "Row below", TableAction::RowAfter),
    (
        "table-column-after",
        "Column after",
        TableAction::ColumnAfter,
    ),
    ("table-row-before", "Row above", TableAction::RowBefore),
    (
        "table-column-before",
        "Column before",
        TableAction::ColumnBefore,
    ),
    ("table-delete-row", "Delete row", TableAction::DeleteRow),
    (
        "table-delete-column",
        "Delete column",
        TableAction::DeleteColumn,
    ),
    (
        "table-split-cell",
        "Split merged cell",
        TableAction::SplitCell,
    ),
    ("table-remove", "Remove table", TableAction::Remove),
];
impl RichEditor {
    pub(super) fn table_tools(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.input_is_composing(cx) {
            return;
        }
        if self.model.in_table() {
            if self.model.table_context().is_none() {
                self.error = Some("Place the caret in one cell to change this table.".into());
                cx.notify();
                return;
            }
            self.format_menu = false;
            self.find_open = false;
            self.links.open = false;
            self.table_menu = !self.table_menu;
            if self.table_menu {
                let context = self.model.table_context().unwrap();
                let index = OPTIONS
                    .iter()
                    .position(|(_, _, action)| context.allows(*action))
                    .unwrap_or(8);
                self.table_scroll.set_offset(point(px(0.), px(0.)));
                w.focus(&self.table_focus[index], cx);
            } else {
                w.focus(&self.focus, cx);
            }
            cx.notify();
        } else {
            self.edit(|e| e.insert_table(2, 2), w, cx);
            self.table_menu = false;
            w.focus(&self.focus, cx);
        }
    }
    fn move_table_focus(&mut self, backward: bool, w: &mut Window, cx: &mut Context<Self>) {
        let Some(context) = self.model.table_context() else {
            self.close_table_tools(w, cx);
            return;
        };
        let active: Vec<_> = OPTIONS
            .iter()
            .enumerate()
            .filter(|(_, (_, _, action))| context.allows(*action))
            .map(|(i, _)| i)
            .chain([8])
            .collect();
        let current = active
            .iter()
            .position(|i| self.table_focus[*i].is_focused(w))
            .unwrap_or(0);
        let next = active[if backward {
            (current + active.len() - 1) % active.len()
        } else {
            (current + 1) % active.len()
        }];
        if next < 8 {
            let item = active.iter().position(|i| *i == next).unwrap();
            self.table_scroll.scroll_to_item(item);
        }
        w.focus(&self.table_focus[next], cx);
        cx.notify();
    }
    pub(super) fn close_table_tools(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.table_menu = false;
        w.focus(&self.focus, cx);
        cx.notify();
    }
    pub(super) fn table_tab(&mut self, backward: bool, w: &mut Window, cx: &mut Context<Self>) {
        let mut handled = true;
        self.edit(
            |e| {
                handled = e.tab(backward)?;
                Ok(())
            },
            w,
            cx,
        );
        if !handled {
            cx.propagate();
        }
    }
    pub(super) fn leave_table(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if !self.model.in_table() {
            cx.propagate();
            return;
        }
        self.edit(|e| e.leave_table(), w, cx);
        self.table_menu = false;
        w.focus(&self.focus, cx);
    }
    fn change_table(&mut self, action: TableAction, w: &mut Window, cx: &mut Context<Self>) {
        self.edit(|e| e.table_action(action), w, cx);
        if self.error.is_none() {
            self.table_notice = match action {
                TableAction::DeleteRow => Some("Row removed. Ctrl Z to undo.".into()),
                TableAction::DeleteColumn => Some("Column removed. Ctrl Z to undo.".into()),
                TableAction::Remove => Some("Table removed. Ctrl Z to undo.".into()),
                _ => None,
            };
        }
        self.close_table_tools(w, cx);
    }
    fn table_button(
        &self,
        id: &'static str,
        label: &'static str,
        action: TableAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let enabled = self.model.marked.is_none()
            && self.model.table_context().is_some_and(|c| c.allows(action));
        let index = OPTIONS.iter().position(|(_, _, a)| *a == action).unwrap();
        let theme = self.theme;
        let button = div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .tab_index(if enabled { 0 } else { -1 })
            .track_focus(&self.table_focus[index])
            .min_w(px(125.))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(5.))
            .border_1()
            .border_color(theme.border)
            .text_color(if action == TableAction::Remove {
                theme.accent
            } else {
                theme.text
            })
            .opacity(if enabled { 1. } else { 0.4 })
            .when(enabled, |d| {
                d.cursor_pointer().hover(move |d| d.bg(theme.selected))
            })
            .focus_visible(move |d| d.border_color(theme.accent).bg(theme.selected))
            .on_mouse_down(MouseButton::Left, |_, w, _| w.prevent_default())
            .on_click(cx.listener(move |s, _, w, cx| {
                if enabled {
                    s.change_table(action, w, cx);
                }
            }))
            .child(label);
        button.into_any_element()
    }
    pub(super) fn table_controls(&self, w: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let context = self.model.table_context();
        let notice = self.table_notice.clone();
        div().flex_none().flex().flex_col()
            .when_some(context, |d, context| {
                d.child(div().id("table-context").role(Role::Group)
                    .aria_label(format!("Table, {} rows and {} columns. Row {}, column {}.",context.rows,context.columns,context.row+1,context.column+1))
                    .flex().flex_wrap().items_center().gap(px(6.)).px(px(24.)).py(px(3.))
                    .text_size(px(11.)).text_color(theme.muted).border_b_1().border_color(theme.border)
                    .child(div().flex_1().min_w(px(150.)).child(format!("Table {} × {}  ·  Row {} / Column {}",context.rows,context.columns,context.row+1,context.column+1)))
                    .child(self.media_button("leave-table", "After table", |s,w,cx|s.leave_table(w,cx),cx)
                        .aria_label("Continue writing after table, Control Shift Enter")))
                .when(self.table_menu, |d| d.child(
                    div().id("table-options").role(Role::Group).aria_label("Table options").key_context("TableTools")
                        .on_action(cx.listener(|s,_:&CloseTableTools,w,cx|s.close_table_tools(w,cx)))
                        .on_action(cx.listener(|s,_:&NextTableControl,w,cx|s.move_table_focus(false,w,cx)))
                        .on_action(cx.listener(|s,_:&PreviousTableControl,w,cx|s.move_table_focus(true,w,cx)))
                        .flex_none()
                        .bg(theme.overlay).border_b_1().border_color(theme.border)
                        .p(px(18.)).text_size(px(12.)).text_color(theme.text)
                        .child(div().flex().items_center().mb(px(10.)).child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child("Shape this table"))
                            .child(self.media_button("close-table-tools","Done",|s,w,cx|s.close_table_tools(w,cx),cx).track_focus(&self.table_focus[8])))
                        .child(div().id("table-options-scroll").track_scroll(&self.table_scroll)
                            .max_h(((w.viewport_size().height * 0.3).min(px(280.)) - px(70.)).max(px(48.)))
                            .overflow_y_scroll().flex().flex_wrap().gap(px(7.)).children(
                                OPTIONS.into_iter().filter(|(_,_,action)|context.allows(*action))
                                    .map(|(id,label,action)|self.table_button(id,label,action,cx))))
                        .child(div().mt(px(10.)).text_size(px(11.)).text_color(theme.muted).child(if w.viewport_size().height < px(600.) {"Tab for more options · Esc to return"} else {"Tab moves between cells and adds a row at the end. Every change can be undone."}))
                ))
            })
            .when_some(notice, |d,message|d.child(div().id("table-notice").role(Role::Status).aria_label(message.clone())
                .px(px(24.)).py(px(6.)).text_size(px(11.)).text_color(theme.muted).child(message)))
    }
}
