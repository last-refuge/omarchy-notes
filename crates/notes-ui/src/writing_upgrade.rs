//! Explicit library-level consent; work and cancellation live on the writer.
use super::*;
use notes_storage::migration::{Cancellation, Progress, Upgrade};

#[derive(Default)]
pub(in crate::shell) struct State {
    pub open: bool,
    details_focus: Option<FocusHandle>,
    pub(super) details_scroll: ScrollHandle,
    pub(super) destination: Option<PathBuf>,
    generation: u64,
    picking: bool,
    pub(super) pending: Option<(u64, Cancellation)>,
    pub message: Option<String>,
}
impl State {
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.picking
    }
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some((_, cancel)) = &self.pending {
            cancel.cancel();
        }
    }
}
impl Shell {
    pub(in crate::shell) fn upgrade_command(
        &mut self,
        command: Command,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match command {
            Command::UpgradePanel => {
                if live.notes.values().any(|s| s.input_is_composing(cx)) {
                    return;
                }
                if live.upgrade.destination.is_none() {
                    live.upgrade.destination = Some(default_destination(&live.root));
                }
                live.upgrade.open = true;
                live.upgrade
                    .details_focus
                    .get_or_insert_with(|| cx.focus_handle())
                    .focus(w, cx);
                live.upgrade
                    .details_scroll
                    .set_offset(point(px(0.), px(0.)));
                live.closing = false;
            }
            Command::UpgradeCancel => {
                if let Some((_, cancel)) = &live.upgrade.pending {
                    cancel.cancel();
                    live.upgrade.message =
                        Some("Cancelling before the format commit… Your draft stays here.".into());
                    live.upgrade.details_scroll.scroll_to_bottom();
                } else {
                    live.upgrade.generation += 1;
                    live.upgrade.picking = false;
                    live.upgrade.open = false;
                    if let Some(session) = self
                        .library
                        .selected
                        .as_ref()
                        .and_then(|id| live.notes.get(id))
                        .and_then(|s| s.active_editor())
                    {
                        session.read(cx).focus.clone().focus(w, cx);
                    }
                }
            }
            Command::UpgradeChoose if !live.upgrade.busy() && live.schema < 3 => {
                let current = live
                    .upgrade
                    .destination
                    .clone()
                    .unwrap_or_else(|| default_destination(&live.root));
                let parent = current.parent().unwrap_or(&live.root).to_owned();
                let name = current
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                live.upgrade.generation += 1;
                let generation = live.upgrade.generation;
                live.upgrade.picking = true;
                let picker = cx.prompt_for_new_path(&parent, Some(&name));
                cx.spawn_in(w,async move |this,cx| {
                    let result=picker.await;
                    let _=this.update_in(cx,|this,_,cx| {
                        let Some(live)=this.live.as_mut() else{return;};
                        if live.upgrade.generation!=generation || !live.upgrade.picking {return;}
                        live.upgrade.picking=false;
                        match result {
                            Ok(Ok(Some(path)))=>{live.upgrade.destination=Some(path);live.upgrade.message=None;}
                            Ok(Ok(None))=>{}
                            _=>live.upgrade.message=Some("The folder picker is unavailable. You can use the suggested backup location, or try again.".into()),
                        }
                        cx.notify();
                    });
                }).detach();
            }
            Command::UpgradeStart
                if live.upgrade.open && !live.upgrade.busy() && live.schema < 3 =>
            {
                if live.notes.values().any(|s| s.input_is_composing(cx)) {
                    live.upgrade.message =
                        Some("Finish composing text before enabling quotes.".into());
                    cx.notify();
                    return;
                }
                let Some(destination) = live.upgrade.destination.clone() else {
                    return;
                };
                live.operation += 1;
                let operation = live.operation;
                let cancel = Cancellation::default();
                match live.worker.requests.try_send(Request::Upgrade {
                    destination,
                    cancel: cancel.clone(),
                    operation,
                }) {
                    Ok(()) => {
                        live.upgrade.pending = Some((operation, cancel));
                        live.upgrade.message = Some(
                            "Backing up your saved library… Your draft stays in this window."
                                .into(),
                        );
                        live.closing = false;
                    }
                    Err(_) => {
                        live.upgrade.message = Some(
                            "Storage is busy. Your library and draft are unchanged; try again."
                                .into(),
                        )
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }
    pub(super) fn upgrade_progress(
        &mut self,
        operation: u64,
        progress: Progress,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !live
            .upgrade
            .pending
            .as_ref()
            .is_some_and(|(op, _)| *op == operation)
        {
            return;
        }
        live.upgrade.message = Some(
            match progress {
                Progress::BackingUp => "Backing up your saved library…",
                Progress::Verifying => "Your backup is verified. Preparing the upgrade…",
                Progress::Committing => "Finishing the library upgrade…",
            }
            .into(),
        );
        cx.notify();
    }
    pub(super) fn upgrade_finished(
        &mut self,
        operation: u64,
        result: Result<Upgrade, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        if !live
            .upgrade
            .pending
            .as_ref()
            .is_some_and(|(op, _)| *op == operation)
        {
            return;
        }
        live.upgrade.pending = None;
        match result {
            Ok(receipt) => {
                live.schema = 3;
                for s in live.notes.values() {
                    if let Some(editor) = s.active_editor() {
                        editor.update(cx, |e, _| e.model.quotes_enabled = true);
                    }
                }
                live.upgrade.destination = Some(receipt.backup);
                live.upgrade.message=Some("Quotes are enabled. Your draft will save automatically, and your pre-upgrade backup stays available below.".into());
                live.message = None;
                for s in live.notes.values_mut() {
                    if s.needs_upgrade {
                        s.needs_upgrade = false;
                        s.error = None;
                        s.last_edit = Instant::now() - Duration::from_secs(4);
                    }
                }
            }
            Err(error) => {
                live.upgrade.message = Some(format!(
                    "{error} Your draft is still here. If the backup folder already exists, choose a new destination before retrying."
                ))
            }
        }
        live.upgrade.details_scroll.scroll_to_bottom();
        cx.notify();
    }
    pub(in crate::shell) fn upgrade_panel(
        &self,
        height: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let live = self.live.as_ref().unwrap();
        let upgrade = &live.upgrade;
        let theme = self.theme;
        let busy = upgrade.busy();
        let done = live.schema >= 3;
        div().id("quote-upgrade-panel").role(Role::Group).aria_label("Enable quotes with a verified backup")
            .flex().flex_col().min_h_0().overflow_hidden().bg(theme.overlay).border_b_1().border_color(theme.border)
            .when(height < 620., |d|d.flex_1())
            .when(height >= 620., |d|d.flex_none().h(px(340.)))
            .child(div().id("quote-upgrade-explanation").role(Role::Group).aria_label("Upgrade details. Use arrow keys or Page Down to review the backup location.")
                .when_some(upgrade.details_focus.clone(),|d,f|d.track_focus(&f)).tab_stop(true)
                .track_scroll(&upgrade.details_scroll).flex_1().min_h_0().overflow_y_scroll().p(px(20.))
                .focus_visible(|d|d.border_l_2().border_color(theme.accent))
                .on_key_down(cx.listener(|s,event:&KeyDownEvent,_,cx|{
                    let Some(live)=s.live.as_mut()else{return;};let scroll=&live.upgrade.details_scroll;
                    let offset=scroll.offset();let limit=scroll.max_offset().y;
                    let y=match event.keystroke.key.as_str(){"down"=>offset.y-px(36.),"up"=>offset.y+px(36.),"pagedown"=>offset.y-px(120.),"pageup"=>offset.y+px(120.),"home"=>px(0.),"end"=>-limit,_=>return};
                    scroll.set_offset(point(offset.x,y));cx.stop_propagation();cx.notify();
                }))
            .child(div().font_family("Noto Serif").text_size(px(23.)).text_color(theme.text).child(if done {"A little more room for expression."} else {"Make room for a quotation."}))
            .when(!done,|d|d.child(div().mt(px(10.)).text_size(px(13.)).line_height(relative(1.6)).text_color(theme.muted)
                .child("We’ll make and verify a complete copy of your saved library, then enable the new quote format. Your current draft stays here until it can be saved."))
                .child(div().mt(px(8.)).text_size(px(12.)).line_height(relative(1.5)).text_color(theme.text)
                .child("Older versions cannot open the upgraded library. To use an older version, restore this earlier backup into a separate folder; keep later edits separately.")))
            .child(div().mt(px(12.)).text_size(px(11.)).text_color(theme.muted).child("PRE-UPGRADE BACKUP"))
            .child(div().id("quote-backup-path").role(Role::Label).aria_label(upgrade.destination.as_ref().map(|p|p.display().to_string()).unwrap_or_default())
                .mt(px(4.)).p(px(10.)).rounded(px(6.)).bg(theme.editor).text_size(px(12.)).text_color(theme.text).overflow_x_hidden()
                .child(upgrade.destination.as_ref().map(|p|p.display().to_string()).unwrap_or_default()))
            .when_some(upgrade.message.clone(),|d,message|d.child(div().id("quote-upgrade-status").role(Role::Status).aria_label(message.clone()).mt(px(10.)).text_size(px(12.)).line_height(relative(1.5)).text_color(theme.accent).child(message)))
            )
            .child(div().flex().flex_wrap().flex_none().gap(px(8.)).p(px(12.)).border_t_1().border_color(theme.border)
                .when(!busy && !done,|d|d.child(self.control("quote-upgrade-start","Back up and enable quotes",Command::UpgradeStart,cx).px(px(12.)).py(px(8.)).bg(theme.accent).text_color(theme.on_accent).child("Back up & enable quotes"))
                    .child(self.control("quote-upgrade-choose","Choose another backup location",Command::UpgradeChoose,cx).px(px(10.)).py(px(8.)).border_color(theme.border).child("Choose location…")))
                .child(self.control("quote-upgrade-cancel",if busy {"Cancel library upgrade"}else{"Return to writing"},Command::UpgradeCancel,cx).px(px(10.)).py(px(8.)).border_color(theme.border).child(if busy {"Cancel"}else if done {"Back to writing"}else{"Not now"})))
            .into_any_element()
    }
}
fn default_destination(root: &std::path::Path) -> PathBuf {
    root.parent().unwrap_or(root).join(format!(
        "Omarchy Notes before quotes {}-{}",
        notes_storage::now(),
        &notes_core::document::new_id()[..8]
    ))
}
