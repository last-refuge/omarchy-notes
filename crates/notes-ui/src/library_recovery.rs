//! Revision-tagged recovery on the library's existing serialized writer.
use super::*;
use notes_core::library::{Page, Query, Scope as QueryScope, Summary};
use notes_storage::{Event, Note, Request, TrashEntry, TrashReceipt, TrashToken};
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
#[cfg(feature = "visual-tests")]
#[path = "library_recovery_check.rs"]
mod check;
#[path = "library_recovery_view.rs"]
mod view;
#[cfg(feature = "visual-tests")]
pub use check::library_recovery_check;

#[derive(Clone)]
pub(super) enum Action {
    ToggleActions,
    CloseActions,
    OpenTrash,
    CloseTrash,
    SelectTrash(String),
    StepTrash(bool),
    FirstTrash,
    LastTrash,
    MoreTrash,
    Duplicate,
    TrashNote,
    DeleteFolder(String),
    UndoTrash,
    RestoreSelected,
    RequestPermanent,
    ViewRetainedDraft(String),
    Confirm,
    Cancel,
}
pub(super) struct Confirmation {
    pub title: String,
    pub detail: String,
    pub confirm_label: String,
    pub destructive: bool,
    kind: Mutation,
}
pub(super) struct State {
    pub open: bool,
    pub actions_open: bool,
    pub loading: bool,
    pub rows: Vec<Summary>,
    pub has_more: bool,
    pub selected: Option<String>,
    pub preview: Option<TrashEntry>,
    pub preview_text: String,
    pub preview_title: String,
    pub preview_truncated: bool,
    pub error: Option<String>,
    pub message: Option<String>,
    pub confirm: Option<Confirmation>,
    pub undo: Option<TrashReceipt>,
    pub retained_drafts: Vec<(String, String)>,
    pub focus: Vec<FocusHandle>,
    pub scroll: UniformListScrollHandle,
    pending: Option<Pending>,
    page_generation: u64,
    page_flight: Option<(u64, Query)>,
    wanted_page: Option<Query>,
    next_offset: u32,
    preview_generation: u64,
    preview_flight: Option<(u64, String, u64)>,
    wanted_preview: Option<String>,
    preview_task: Option<Task<()>>,
    purge: Option<(u64, PathBuf)>,
    next_purge: Instant,
    return_focus: Option<FocusHandle>,
}
impl State {
    pub fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            open: false,
            actions_open: false,
            loading: false,
            rows: vec![],
            has_more: false,
            selected: None,
            preview: None,
            preview_text: String::new(),
            preview_title: String::new(),
            preview_truncated: false,
            error: None,
            message: None,
            confirm: None,
            undo: None,
            retained_drafts: vec![],
            focus: (0..7).map(|_| cx.focus_handle()).collect(),
            scroll: UniformListScrollHandle::new(),
            pending: None,
            page_generation: 0,
            page_flight: None,
            wanted_page: None,
            next_offset: 0,
            preview_generation: 0,
            preview_flight: None,
            wanted_preview: None,
            preview_task: None,
            purge: None,
            next_purge: Instant::now() + Duration::from_secs(60),
            return_focus: None,
        }
    }
    pub fn mutating(&self) -> bool {
        self.pending.is_some()
    }
    pub fn busy(&self) -> bool {
        self.mutating()
            || self.purge.is_some()
            || self.page_flight.is_some()
            || self.wanted_page.is_some()
            || self.preview_flight.is_some()
            || self.wanted_preview.is_some()
            || self.preview_task.is_some()
    }
    pub fn blocks_mutations(&self) -> bool {
        self.mutating() || self.purge.is_some()
    }
    pub fn blocks_save(&self, id: &str) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.operation.is_some() && p.tags.contains_key(id))
    }
}
#[derive(Clone)]
enum Mutation {
    Duplicate(String),
    Trash(Vec<String>),
    Folder {
        id: String,
        folder_ids: Vec<String>,
        ids: Vec<String>,
    },
    Restore(Vec<TrashToken>),
    Undo(TrashReceipt),
    Permanent(Vec<TrashToken>),
}
struct Tag {
    local: Option<u64>,
    model: u64,
    database: i64,
}
struct Pending {
    root: PathBuf,
    kind: Mutation,
    tags: BTreeMap<String, Tag>,
    operation: Option<u64>,
}

impl Shell {
    pub(super) fn library_recovery_action(
        &mut self,
        action: Action,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.live.is_none() {
            return;
        }
        if self.library_recovery.mutating() {
            self.library_recovery.error =
                Some("Wait for the current recovery operation. Your draft stays available.".into());
            cx.notify();
            return;
        }
        match action {
            Action::ToggleActions => {
                self.library_recovery.actions_open = !self.library_recovery.actions_open;
                if self.library_recovery.actions_open {
                    w.focus(&self.library_recovery.focus[2], cx);
                } else {
                    w.focus(&self.focus, cx);
                }
            }
            Action::CloseActions => {
                self.library_recovery.actions_open = false;
                w.focus(&self.focus, cx);
            }
            Action::OpenTrash => {
                self.library_recovery.actions_open = false;
                self.library_recovery.return_focus = w.focused(cx);
                self.library_recovery.open = true;
                self.library_recovery.confirm = None;
                self.settings_open = false;
                self.folders_open = false;
                self.refresh_trash();
                w.focus(&self.library_recovery.focus[1], cx);
            }
            Action::CloseTrash => {
                self.library_recovery.open = false;
                self.library_recovery.confirm = None;
                self.library_recovery.page_generation += 1;
                self.library_recovery.preview_generation += 1;
                self.library_recovery.wanted_page = None;
                self.library_recovery.wanted_preview = None;
                self.library_recovery.preview_task = None;
                self.library_recovery.loading = false;
                let focus = self.library_recovery.return_focus.take();
                w.focus(focus.as_ref().unwrap_or(&self.focus), cx);
            }
            Action::SelectTrash(id) => self.select_trash(id, w, cx),
            Action::StepTrash(forward) => {
                let state = &self.library_recovery;
                if !state.rows.is_empty() {
                    let current = state
                        .rows
                        .iter()
                        .position(|r| Some(&r.id) == state.selected.as_ref())
                        .unwrap_or(0);
                    let next = if forward {
                        (current + 1).min(state.rows.len() - 1)
                    } else {
                        current.saturating_sub(1)
                    };
                    self.select_trash(state.rows[next].id.clone(), w, cx);
                }
            }
            Action::FirstTrash | Action::LastTrash => {
                let state = &self.library_recovery;
                let row = if matches!(action, Action::FirstTrash) {
                    state.rows.first()
                } else {
                    state.rows.last()
                };
                if let Some(row) = row {
                    self.select_trash(row.id.clone(), w, cx);
                }
            }
            Action::MoreTrash => {
                let state = &mut self.library_recovery;
                if state.has_more && !state.loading {
                    state.loading = true;
                    state.wanted_page = Some(Query {
                        generation: state.page_generation,
                        scope: QueryScope::Trash,
                        offset: state.next_offset,
                        limit: 100,
                        ..Default::default()
                    });
                }
            }
            Action::Duplicate | Action::TrashNote => {
                if let Some(id) = self.library.selected.clone() {
                    let kind = if matches!(action, Action::Duplicate) {
                        Mutation::Duplicate(id)
                    } else {
                        Mutation::Trash(vec![id])
                    };
                    self.begin_recovery(kind, cx);
                }
            }
            Action::DeleteFolder(id) => {
                let live = self.live.as_ref().unwrap();
                match notes_core::organization::descendants(&live.folders, &id) {
                    Ok(folder_ids) => {
                        let ids: Vec<_> = live
                            .notes
                            .values()
                            .filter(|s| {
                                !s.recovery_blocked
                                    && s.note
                                        .folder
                                        .as_ref()
                                        .is_some_and(|id| folder_ids.contains(id))
                            })
                            .map(|s| s.note.id.clone())
                            .collect();
                        let title = live
                            .folders
                            .iter()
                            .find(|f| f.id == id)
                            .map(|f| f.name.clone())
                            .unwrap_or_else(|| "this folder".into());
                        self.library_recovery.confirm = Some(Confirmation {
                            title: format!("Delete {title}?"),
                            detail: format!(
                                "Remove this folder and its {} descendant folders. Move {} notes to Recently Deleted after their current drafts are saved. Undo restores those notes to Notes; it does not recreate the folder tree.",
                                folder_ids.len().saturating_sub(1),
                                ids.len()
                            ),
                            confirm_label: "Delete folder".into(),
                            destructive: true,
                            kind: Mutation::Folder {
                                id,
                                folder_ids,
                                ids,
                            },
                        });
                        self.library_recovery.actions_open = false;
                        w.focus(&self.library_recovery.focus[6], cx);
                    }
                    Err(e) => self.library_recovery.error = Some(e.to_string()),
                }
            }
            Action::UndoTrash => {
                if let Some(receipt) = self.library_recovery.undo.clone() {
                    self.begin_recovery(Mutation::Undo(receipt), cx);
                }
            }
            Action::RestoreSelected | Action::RequestPermanent => {
                if let Some(entry) = self.library_recovery.preview.as_ref()
                    && self.library_recovery.selected.as_ref() == Some(&entry.note.id)
                    && !self.library_recovery.busy()
                {
                    let token = TrashToken {
                        id: entry.note.id.clone(),
                        revision: entry.note.revision,
                        deleted_at: entry.deleted_at,
                    };
                    if matches!(action, Action::RestoreSelected) {
                        self.begin_recovery(Mutation::Restore(vec![token]), cx);
                    } else {
                        self.library_recovery.confirm = Some(Confirmation {
                            title: "Delete this note permanently?".into(),
                            detail: "The saved note and its retained history will be removed. This cannot be undone. Any newer draft held in a Notes window remains available for recovery.".into(),
                            confirm_label: "Delete permanently".into(),
                            destructive: true,
                            kind: Mutation::Permanent(vec![token]),
                        });
                        w.focus(&self.library_recovery.focus[6], cx);
                    }
                }
            }
            Action::Confirm => {
                if let Some(confirm) = self.library_recovery.confirm.take() {
                    self.begin_recovery(confirm.kind, cx);
                }
            }
            Action::ViewRetainedDraft(id) => {
                if self
                    .live
                    .as_ref()
                    .is_some_and(|l| l.notes.contains_key(&id))
                {
                    self.library_recovery.open = false;
                    self.library_recovery.confirm = None;
                    self.library_recovery.actions_open = false;
                    self.library_recovery.page_generation += 1;
                    self.library_recovery.preview_generation += 1;
                    self.library_recovery.wanted_page = None;
                    self.library_recovery.wanted_preview = None;
                    self.library_recovery.preview_task = None;
                    self.library.scope = Scope::All;
                    self.library.selected = Some(id.clone());
                    self.command(Command::Select(id), w, cx);
                }
            }
            Action::Cancel => {
                self.library_recovery.confirm = None;
                self.library_recovery.actions_open = false;
                w.focus(
                    if self.library_recovery.open {
                        &self.library_recovery.focus[1]
                    } else {
                        &self.focus
                    },
                    cx,
                );
            }
        }
        self.submit_recovery_mutation(cx);
        self.submit_trash_reads(cx);
        cx.notify();
    }

    fn begin_recovery(&mut self, kind: Mutation, cx: &mut Context<Self>) {
        let live = self.live.as_mut().unwrap();
        if self.portability.busy()
            || self.library_recovery.blocks_mutations()
            || live.upgrade.busy()
            || live.pdf.busy()
            || live.creating.is_some()
            || self.organization.busy()
            || live.closing
        {
            self.library_recovery.error =
                Some("Finish the current library operation before changing recovery state.".into());
            return;
        }
        let ids: Vec<_> = match &kind {
            Mutation::Duplicate(id) => vec![id.clone()],
            Mutation::Trash(ids) | Mutation::Folder { ids, .. } => ids.clone(),
            Mutation::Restore(tokens) | Mutation::Permanent(tokens) => {
                tokens.iter().map(|t| t.id.clone()).collect()
            }
            Mutation::Undo(receipt) => receipt.entries.iter().map(|t| t.id.clone()).collect(),
        };
        if ids.len() > notes_storage::recovery::MAX_MUTATION_NOTES {
            self.library_recovery.error =
                Some("Choose at most 10,000 notes for one recovery operation.".into());
            return;
        }
        let restoring = matches!(
            kind,
            Mutation::Restore(_) | Mutation::Undo(_) | Mutation::Permanent(_)
        );
        if matches!(kind, Mutation::Permanent(_))
            && ids.iter().any(|id| {
                live.notes.get(id).is_some_and(|s| {
                    s.recovery_blocked
                        || s.error.is_some()
                        || s.revision != s.saved
                        || s.pending.is_some()
                        || s.has_unobserved_edit(cx)
                        || s.has_pending_work(cx)
                        || s.input_is_composing(cx)
                })
            })
        {
            self.library_recovery.error = Some("Restore or save a recovery copy of the held draft before permanent deletion, so its attachment records remain available.".into());
            return;
        }
        let mut tags = BTreeMap::new();
        for id in ids {
            if let Some(s) = live.notes.get_mut(&id) {
                if !restoring
                    && (s.recovery_blocked
                        || s.error.is_some()
                        || s.input_is_composing(cx)
                        || s.has_pending_work(cx))
                {
                    self.library_recovery.error = Some("Finish the current insertion or composition and resolve the note's save error first. Your draft is unchanged.".into());
                    return;
                }
                let model = s
                    .active_editor()
                    .map_or(s.model_revision, |e| e.read(cx).model.revision);
                tags.insert(
                    id,
                    Tag {
                        local: (!s.has_unobserved_edit(cx)).then_some(s.revision),
                        model,
                        database: s.note.revision,
                    },
                );
                if !restoring {
                    s.last_edit = Instant::now() - Duration::from_secs(4);
                }
            } else if !restoring {
                self.library_recovery.error = Some(
                    "The note is no longer available. Refresh the library before trying again."
                        .into(),
                );
                return;
            }
        }
        self.library_recovery.actions_open = false;
        self.library_recovery.error = None;
        self.library_recovery.message = Some(
            if restoring {
                "Waiting for the saved recovery operation…"
            } else {
                "Saving the requested draft before changing the library…"
            }
            .into(),
        );
        self.library_recovery.pending = Some(Pending {
            root: live.root.clone(),
            kind,
            tags,
            operation: None,
        });
        cx.notify();
    }

    pub(super) fn poll_library_recovery(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.submit_recovery_mutation(cx);
        self.submit_trash_reads(cx);
        let Some(live) = &mut self.live else { return };
        self.library_recovery.retained_drafts.retain(|(id, _)| {
            live.notes.get(id).is_some_and(|s| {
                s.recovery_blocked
                    || s.error.is_some()
                    || s.revision != s.saved
                    || s.pending.is_some()
                    || s.has_unobserved_edit(cx)
                    || s.has_pending_work(cx)
                    || s.input_is_composing(cx)
            })
        });
        if self.library_recovery.purge.is_none()
            && Instant::now() >= self.library_recovery.next_purge
            && !self.library_recovery.mutating()
            && !self.portability.busy()
            && !live.upgrade.busy()
            && !live.pdf.busy()
            && !self.organization.busy()
            && live.creating.is_none()
            && !live.closing
            && live.notes.values().all(|s| {
                !s.recovery_blocked
                    && s.error.is_none()
                    && s.revision == s.saved
                    && s.pending.is_none()
                    && !s.has_unobserved_edit(cx)
                    && !s.has_pending_work(cx)
                    && !s.input_is_composing(cx)
                    && s.last_edit.elapsed() >= Duration::from_secs(30)
            })
        {
            live.operation += 1;
            let operation = live.operation;
            if live
                .worker
                .requests
                .try_send(Request::Purge {
                    at: notes_storage::now(),
                    operation,
                })
                .is_ok()
            {
                self.library_recovery.purge = Some((operation, live.root.clone()));
                self.library_recovery.next_purge = Instant::now() + Duration::from_secs(3600);
            }
        }
        let _ = w;
    }

    fn submit_recovery_mutation(&mut self, cx: &mut Context<Self>) {
        let Some(mut pending) = self.library_recovery.pending.take() else {
            return;
        };
        if pending.operation.is_some() {
            self.library_recovery.pending = Some(pending);
            return;
        }
        let Some(live) = &mut self.live else { return };
        if live.root != pending.root {
            self.library_recovery.error =
                Some("The library changed. The recovery request was cancelled.".into());
            return;
        }
        let barrier = matches!(
            pending.kind,
            Mutation::Duplicate(_) | Mutation::Trash(_) | Mutation::Folder { .. }
        );
        let mut waiting = false;
        if barrier {
            for (id, tag) in &mut pending.tags {
                let Some(s) = live.notes.get_mut(id) else {
                    self.library_recovery.error = Some("The requested note changed before its save completed. Review the action again.".into());
                    return;
                };
                let model = s
                    .active_editor()
                    .map_or(s.model_revision, |e| e.read(cx).model.revision);
                if model != tag.model
                    || s.error.is_some()
                    || s.recovery_blocked
                    || s.input_is_composing(cx)
                    || s.has_pending_work(cx)
                    || tag.local.is_some_and(|local| local != s.revision)
                {
                    self.library_recovery.error = Some("The draft changed while waiting to save. Your newer draft stays here; review the recovery action again.".into());
                    return;
                }
                if s.has_unobserved_edit(cx) {
                    waiting = true;
                    continue;
                }
                tag.local.get_or_insert(s.revision);
                if s.pending.is_some() || s.revision != s.saved {
                    s.last_edit = Instant::now() - Duration::from_secs(4);
                    waiting = true;
                } else {
                    tag.database = s.note.revision;
                }
            }
            if waiting {
                self.library_recovery.pending = Some(pending);
                return;
            }
        }
        if let Mutation::Folder {
            id,
            folder_ids,
            ids,
        } = &pending.kind
        {
            let current = notes_core::organization::descendants(&live.folders, id);
            let actual: std::collections::BTreeSet<_> = live
                .notes
                .values()
                .filter(|s| {
                    !s.recovery_blocked
                        && s.note
                            .folder
                            .as_ref()
                            .is_some_and(|id| folder_ids.contains(id))
                })
                .map(|s| s.note.id.clone())
                .collect();
            if current.is_err()
                || current.as_ref().is_ok_and(|current| current != folder_ids)
                || actual != ids.iter().cloned().collect()
            {
                self.library_recovery.error = Some("The folder contents changed after confirmation. Review the folder deletion again.".into());
                return;
            }
        }
        live.operation += 1;
        let operation = live.operation;
        let revisions = || {
            pending
                .tags
                .iter()
                .filter(|(_, t)| t.database > 0)
                .map(|(id, tag)| (id.clone(), tag.database))
                .collect::<Vec<_>>()
        };
        let request = match &pending.kind {
            Mutation::Duplicate(id) => {
                let revision = pending.tags[id].database;
                if revision == 0 {
                    self.library_recovery.error =
                        Some("Save this note before duplicating it.".into());
                    return;
                }
                Request::Duplicate {
                    id: id.clone(),
                    expected_revision: revision,
                    operation,
                }
            }
            Mutation::Trash(_) => Request::Trash {
                notes: revisions(),
                operation,
            },
            Mutation::Folder { id, .. } => Request::DeleteFolder {
                id: id.clone(),
                expected_notes: revisions(),
                operation,
            },
            Mutation::Restore(tokens) => Request::Restore {
                notes: tokens.iter().map(|t| (t.id.clone(), t.revision)).collect(),
                operation,
            },
            Mutation::Undo(receipt) => Request::UndoTrash {
                receipt: receipt.clone(),
                operation,
            },
            Mutation::Permanent(tokens) => Request::DeletePermanently {
                notes: tokens.iter().map(|t| (t.id.clone(), t.revision)).collect(),
                operation,
            },
        };
        match live.worker.requests.try_send(request) {
            Ok(()) => {
                pending.operation = Some(operation);
                self.library_recovery.pending = Some(pending);
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                self.library_recovery.pending = Some(pending);
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                self.library_recovery.error =
                    Some("Recovery is unavailable. Your draft remains here.".into());
            }
        }
    }

    fn refresh_trash(&mut self) {
        let state = &mut self.library_recovery;
        state.page_generation += 1;
        state.loading = true;
        state.next_offset = 0;
        state.wanted_page = Some(Query {
            generation: state.page_generation,
            scope: QueryScope::Trash,
            limit: 100,
            ..Default::default()
        });
    }
    fn select_trash(&mut self, id: String, w: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.library_recovery;
        let Some(index) = state.rows.iter().position(|r| r.id == id) else {
            return;
        };
        state.selected = Some(id.clone());
        state.preview_generation += 1;
        state.preview = None;
        state.preview_text.clear();
        state.preview_title.clear();
        state.preview_task = None;
        state.wanted_preview = Some(id);
        state.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        w.focus(&state.focus[1], cx);
    }
    fn submit_trash_reads(&mut self, cx: &mut Context<Self>) {
        let Some(live) = &mut self.live else { return };
        let state = &mut self.library_recovery;
        if state.page_flight.is_none()
            && let Some(query) = state.wanted_page.clone()
        {
            live.operation += 1;
            let operation = live.operation;
            match live.worker.requests.try_send(Request::Query {
                operation,
                query: query.clone(),
            }) {
                Ok(()) => {
                    state.page_flight = Some((operation, query));
                    state.wanted_page = None;
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(_) => {
                    state.loading = false;
                    state.wanted_page = None;
                    state.error = Some(
                        "Recently Deleted could not be loaded. Your library is unchanged.".into(),
                    );
                }
            }
        }
        if state.preview_flight.is_none()
            && let Some(id) = state.wanted_preview.clone()
        {
            live.operation += 1;
            let operation = live.operation;
            match live.worker.requests.try_send(Request::LoadTrash {
                operation,
                id: id.clone(),
            }) {
                Ok(()) => {
                    state.preview_flight = Some((operation, id, state.preview_generation));
                    state.wanted_preview = None;
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(_) => {
                    state.wanted_preview = None;
                    state.error = Some(
                        "The deleted note could not be previewed. Try selecting it again.".into(),
                    );
                }
            }
        }
        let _ = cx;
    }

    pub(super) fn library_recovery_event(
        &mut self,
        event: Event,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Event> {
        if let Event::Queried {
            operation,
            query,
            result,
        } = event
        {
            if query.scope != QueryScope::Trash {
                return Some(Event::Queried {
                    operation,
                    query,
                    result,
                });
            }
            let state = &mut self.library_recovery;
            if state
                .page_flight
                .as_ref()
                .is_some_and(|(op, q)| *op == operation && q == &query)
            {
                state.page_flight = None;
                if state.open && state.page_generation == query.generation {
                    match result {
                        Ok(Page { rows, has_more }) => {
                            if query.offset == 0 {
                                state.rows.clear();
                            }
                            state.next_offset = query.offset + rows.len() as u32;
                            for row in rows {
                                if !state.rows.iter().any(|r| r.id == row.id) {
                                    state.rows.push(row);
                                }
                            }
                            state.has_more = has_more;
                            state.loading = false;
                            if state
                                .selected
                                .as_ref()
                                .is_none_or(|id| !state.rows.iter().any(|r| &r.id == id))
                            {
                                state.selected = None;
                                state.preview = None;
                                if let Some(row) = state.rows.first() {
                                    let id = row.id.clone();
                                    self.select_trash(id, w, cx);
                                }
                            }
                        }
                        Err(e) => {
                            state.loading = false;
                            state.error =
                                Some(format!("Recently Deleted could not be loaded. {e}"));
                        }
                    }
                }
            }
            self.submit_trash_reads(cx);
            cx.notify();
            return None;
        }
        if let Event::TrashLoaded {
            operation,
            id,
            result,
        } = event
        {
            let state = &mut self.library_recovery;
            if let Some((op, expected, generation)) = state.preview_flight.clone()
                && op == operation
                && expected == id
            {
                state.preview_flight = None;
                if state.open
                    && state.preview_generation == generation
                    && state.selected.as_ref() == Some(&id)
                {
                    match result {
                        Ok(entry) => {
                            let root = self.live.as_ref().unwrap().root.clone();
                            state.preview_task = Some(cx.spawn(async move |this, cx| {
                                let (entry, title, text, truncated) = cx
                                    .background_executor()
                                    .spawn(async move {
                                        let title = entry.note.body.title();
                                        let plain = entry.note.body.plain_text();
                                        let end = plain.char_indices().nth(8192).map(|(i, _)| i);
                                        let text = plain[..end.unwrap_or(plain.len())].to_owned();
                                        (
                                            entry,
                                            if title.is_empty() {
                                                "Untitled note".into()
                                            } else {
                                                title
                                            },
                                            text,
                                            end.is_some(),
                                        )
                                    })
                                    .await;
                                let _ = this.update(cx, |s, cx| {
                                    let state = &mut s.library_recovery;
                                    if s.live.as_ref().is_some_and(|l| l.root == root)
                                        && state.open
                                        && state.preview_generation == generation
                                        && state.selected.as_ref() == Some(&entry.note.id)
                                    {
                                        state.preview = Some(entry);
                                        state.preview_title = title;
                                        state.preview_text = text;
                                        state.preview_truncated = truncated;
                                        state.preview_task = None;
                                        cx.notify();
                                    }
                                });
                            }));
                        }
                        Err(e) => {
                            state.error =
                                Some(format!("The deleted note could not be previewed. {e}"))
                        }
                    }
                }
            }
            self.submit_trash_reads(cx);
            cx.notify();
            return None;
        }
        if let Event::Purged { operation, result } = event {
            if self
                .library_recovery
                .purge
                .as_ref()
                .is_some_and(|(op, root)| {
                    *op == operation && self.live.as_ref().is_some_and(|l| &l.root == root)
                })
            {
                self.library_recovery.purge = None;
                match result {
                    Ok(report) => {
                        for id in &report.purged_ids {
                            self.remove_recovered_note(id, None, "The saved note expired from Recently Deleted. Your newer draft remains here; save a recovery copy.", cx);
                        }
                        if report.has_more {
                            self.library_recovery.next_purge =
                                Instant::now() + Duration::from_millis(100);
                        }
                        if !report.purged_ids.is_empty() {
                            if self.library_recovery.open {
                                self.refresh_trash();
                            }
                            self.refresh_real_library(cx);
                        }
                    }
                    Err(e) => {
                        self.library_recovery.error =
                            Some(format!("Automatic recovery cleanup could not finish. {e}"))
                    }
                }
            }
            cx.notify();
            return None;
        }
        let operation = match &event {
            Event::Duplicated { operation, .. }
            | Event::Trashed { operation, .. }
            | Event::Restored { operation, .. }
            | Event::Deleted { operation, .. }
            | Event::FolderDeleted { operation, .. } => *operation,
            _ => return Some(event),
        };
        let matches = self.library_recovery.pending.as_ref().is_some_and(|p| {
            p.operation == Some(operation) && self.live.as_ref().is_some_and(|l| l.root == p.root)
        });
        if !matches {
            return None;
        }
        let expected = &self.library_recovery.pending.as_ref().unwrap().kind;
        let kind_matches = match (expected, &event) {
            (Mutation::Duplicate(id), Event::Duplicated { source, .. }) => id == source,
            (Mutation::Trash(_), Event::Trashed { .. })
            | (Mutation::Folder { .. }, Event::FolderDeleted { .. })
            | (Mutation::Restore(_) | Mutation::Undo(_), Event::Restored { .. })
            | (Mutation::Permanent(_), Event::Deleted { .. }) => true,
            _ => false,
        };
        if !kind_matches {
            return None;
        }
        let pending = self.library_recovery.pending.take().unwrap();
        let result: Result<(), String> = match event {
            Event::Duplicated { source, result, .. } => result.map(|note| {
                let id = note.id.clone();
                let unchanged = pending.tags.get(&source).is_some_and(|t| self.recovery_tag_matches(&source, t, cx))
                    && self.library.selected.as_deref() == Some(source.as_str());
                self.attach_note(note, w, cx);
                self.refresh_real_library(cx);
                if unchanged { self.command(Command::Select(id), w, cx); }
                self.library_recovery.message = Some(if unchanged { "Duplicate saved as an independent note." } else { "Duplicate saved from the requested revision. Your newer original draft stays open." }.into());
            }),
            Event::Trashed { result, .. } => result.map(|receipt| self.apply_trash_receipt(&pending, receipt, cx)),
            Event::FolderDeleted { result, .. } => result.map(|deleted| {
                self.live.as_mut().unwrap().folders.retain(|f| !deleted.folder_ids.contains(&f.id));
                for id in pending.tags.keys() {
                    if !deleted.trash.entries.iter().any(|t| &t.id == id) {
                        self.remove_recovered_note(id, pending.tags.get(id), "The folder was removed while this draft changed. The draft stays here; save a recovery copy in Notes.", cx);
                    }
                }
                self.apply_trash_receipt(&pending, deleted.trash, cx);
                for id in pending.tags.keys() {
                    if let Some(s) = self.live.as_mut().unwrap().notes.get_mut(id) {
                        s.note.folder = None;
                    }
                }
                if matches!(&self.library.scope, Scope::FolderId(Some(id)) if deleted.folder_ids.contains(id)) { self.library.scope = Scope::All; }
                self.library_recovery.message = Some("Folder tree removed. Undo restores the newly deleted notes to Notes without recreating folders.".into());
                self.refresh_real_library(cx);
            }),
            Event::Restored { result, .. } => result.map(|notes| {
                for note in notes { self.apply_restored_note(note, w, cx); }
                self.library_recovery.undo = None;
                self.library_recovery.message = Some("Saved notes restored. Deleted folders were not recreated.".into());
                self.refresh_real_library(cx);
                if self.library_recovery.open { self.refresh_trash(); }
            }),
            Event::Deleted { result, .. } => result.map(|ids| {
                for id in &ids { self.remove_recovered_note(id, pending.tags.get(id), "The saved note was deleted permanently. Your newer draft remains here; save a recovery copy.", cx); }
                self.library_recovery.undo = None;
                self.library_recovery.message = Some("Saved note deleted permanently. This cannot be undone.".into());
                if self.library_recovery.open { self.refresh_trash(); }
                self.refresh_real_library(cx);
            }),
            _ => unreachable!(),
        };
        if let Err(e) = result {
            self.library_recovery.error = Some(format!(
                "Recovery did not complete. Your draft is retained. {e}"
            ));
        }
        if self.library.selected.is_none() && !self.library_recovery.open {
            w.focus(&self.focus, cx);
        }
        cx.notify();
        None
    }

    fn recovery_tag_matches(&self, id: &str, tag: &Tag, cx: &App) -> bool {
        self.live
            .as_ref()
            .and_then(|l| l.notes.get(id))
            .is_some_and(|s| {
                tag.local == Some(s.revision)
                    && s.model_revision == tag.model
                    && s.note.revision == tag.database
                    && s.revision == s.saved
                    && !s.has_unobserved_edit(cx)
                    && !s.has_pending_work(cx)
                    && !s.input_is_composing(cx)
            })
    }
    fn remove_recovered_note(
        &mut self,
        id: &str,
        tag: Option<&Tag>,
        error: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.live.as_ref().and_then(|l| l.notes.get(id)) else {
            return;
        };
        let safe = if let Some(tag) = tag {
            self.recovery_tag_matches(id, tag, cx)
        } else {
            s.revision == s.saved
                && !s.has_unobserved_edit(cx)
                && !s.has_pending_work(cx)
                && !s.input_is_composing(cx)
        };
        if safe {
            self.close_recovery_quick(id, cx);
            self.live.as_mut().unwrap().notes.remove(id);
            if self.library.selected.as_deref() == Some(id) {
                self.library.selected = None;
            }
        } else {
            let s = self.live.as_mut().unwrap().notes.get_mut(id).unwrap();
            s.recovery_blocked = true;
            s.error = Some(error.into());
            if !self
                .library_recovery
                .retained_drafts
                .iter()
                .any(|(held, _)| held == id)
            {
                let title = s.note.body.title();
                self.library_recovery.retained_drafts.push((
                    id.to_owned(),
                    if title.is_empty() {
                        "Untitled note".into()
                    } else {
                        title
                    },
                ));
            }
            self.library_recovery.error = Some(error.into());
        }
    }
    fn apply_trash_receipt(
        &mut self,
        pending: &Pending,
        receipt: TrashReceipt,
        cx: &mut Context<Self>,
    ) {
        for token in &receipt.entries {
            self.remove_recovered_note(&token.id, pending.tags.get(&token.id), "The saved note was deleted while a newer draft arrived. Restore the saved note or save a recovery copy; your draft stays here.", cx);
            if let Some(s) = self.live.as_mut().unwrap().notes.get_mut(&token.id) {
                s.note.revision = token.revision;
                if matches!(pending.kind, Mutation::Folder { .. }) {
                    s.note.folder = None;
                }
            }
        }
        self.library_recovery.undo = (!receipt.entries.is_empty()).then_some(receipt);
        self.library_recovery.message = Some(
            "Moved saved notes to Recently Deleted. Undo is available; notes are kept for 30 days."
                .into(),
        );
        self.refresh_real_library(cx);
        if self.library_recovery.open {
            self.refresh_trash();
        }
    }
    fn apply_restored_note(&mut self, note: Note, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.live.as_mut().unwrap().notes.get_mut(&note.id) {
            // Never clone a restored body into an already held model. Update the
            // acknowledged SQL baseline only, and require explicit Save/recovery.
            s.note.revision = note.revision;
            s.note.folder = note.folder;
            s.note.created_at = note.created_at;
            s.note.updated_at = note.updated_at;
            s.recovery_blocked = false;
            s.error = Some(
                "The saved note was restored. Your held draft is unchanged; use Save when ready."
                    .into(),
            );
        } else {
            self.attach_note(note, w, cx);
        }
    }
    pub(super) fn recovery_focus(
        &mut self,
        backward: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if backward {
            w.focus_prev(cx);
        } else {
            w.focus_next(cx);
        }
    }
}
