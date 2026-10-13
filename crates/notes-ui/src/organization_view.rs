//! Per-library folder view state. Disk work and discovery stay outside render.
use super::*;
use notes_core::library::Sort;
use notes_platform::library_view::{Event, LibraryKey, Save, ViewState, Worker};
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Default)]
pub(super) struct State {
    enabled: bool,
    generation: u64,
    worker: Option<Worker>,
    task: Option<Task<()>>,
    ready: bool,
    current: ViewState,
    touched: HashMap<String, bool>,
    sort_touched: bool,
    operation: u64,
    pending: Option<u64>,
    pub dirty: bool,
    pub error: Option<String>,
}
impl State {
    pub(super) fn preserve_expanded(&mut self, id: &str) {
        if self.enabled && !self.ready {
            self.touched.insert(id.to_owned(), false);
            self.dirty = true;
        }
    }
    pub(super) fn pruned_collapsed(&mut self, current: &HashSet<String>) {
        let next: BTreeSet<_> = current.iter().cloned().collect();
        for id in self.current.collapsed_folders.difference(&next) {
            self.touched.insert(id.clone(), false);
        }
        self.current.collapsed_folders = next;
        self.dirty = true;
    }
    pub(super) fn can_retry(&self) -> bool {
        self.ready && self.dirty && self.error.is_some()
    }
}

impl Shell {
    /// Production setup hook, called after the new library's folder inventory.
    /// Headless regressions use an explicit private configuration root instead.
    pub(in crate::shell) fn configure_folder_view(
        &mut self,
        root: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if cx.background_executor().is_test() {
            return;
        }
        self.configure_folder_view_in(root, None, cx);
    }
    pub(in crate::shell) fn configure_folder_view_in(
        &mut self,
        root: PathBuf,
        config: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let generation = self.organization.view.generation.wrapping_add(1);
        let fallback = ViewState {
            sort: self.query.sort,
            ..Default::default()
        };
        self.organization.view = State {
            enabled: true,
            generation,
            current: ViewState {
                collapsed_folders: self.organization.collapsed.iter().cloned().collect(),
                sort: fallback.sort,
            },
            ..Default::default()
        };
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let key = LibraryKey::discover(&root)?;
                    match config {
                        Some(config) => Worker::start_in_with_default(key, &config, fallback),
                        None => Worker::start_with_default(key, fallback),
                    }
                })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.organization.view.generation != generation {
                    return;
                }
                match result {
                    Ok(worker) => {
                        s.organization.view.worker = Some(worker);
                    }
                    Err(error) => {
                        s.organization.view.error =
                            Some(format!("Folder view could not be loaded. {error}"))
                    }
                }
                s.organization.view.task = None;
                cx.notify();
            });
        });
        self.organization.view.task = Some(task);
    }
    /// Existing storage-tick hook. Returns a loaded sort only if the user has
    /// not changed sorting while discovery/load was pending.
    pub(in crate::shell) fn poll_folder_view(&mut self, cx: &mut Context<Self>) -> Option<Sort> {
        let events: Vec<_> = self
            .organization
            .view
            .worker
            .as_ref()
            .map(|w| w.events.try_iter().collect())
            .unwrap_or_default();
        let mut loaded_sort = None;
        for event in events {
            match event {
                Event::Loaded { library, result } => {
                    if self
                        .organization
                        .view
                        .worker
                        .as_ref()
                        .is_none_or(|w| w.library() != &library)
                    {
                        continue;
                    }
                    match result {
                        Ok(mut state) => {
                            let valid: HashSet<_> = self
                                .live
                                .as_ref()
                                .map(|l| l.folders.iter().map(|f| f.id.as_str()).collect())
                                .unwrap_or_default();
                            state
                                .collapsed_folders
                                .retain(|id| valid.contains(id.as_str()));
                            let view = &mut self.organization.view;
                            // Establish this successfully loaded snapshot once,
                            // including inherited legacy sort, before close.
                            // Errors never enter this branch or permit writes.
                            view.dirty = true;
                            for (id, collapsed) in &view.touched {
                                if *collapsed && valid.contains(id.as_str()) {
                                    state.collapsed_folders.insert(id.clone());
                                } else {
                                    state.collapsed_folders.remove(id);
                                }
                            }
                            if view.sort_touched {
                                state.sort = view.current.sort;
                            } else {
                                loaded_sort = Some(state.sort);
                            }
                            self.organization.collapsed =
                                state.collapsed_folders.iter().cloned().collect();
                            view.current = state;
                            view.ready = true;
                            view.error = None;
                            self.refresh_folder_rows();
                            cx.notify();
                        }
                        Err(error) => {
                            self.organization.view.error = Some(format!(
                                "Folder view could not be loaded. Changes stay in this window. {error}"
                            ))
                        }
                    }
                }
                Event::Saved {
                    library,
                    operation,
                    result,
                } => {
                    let view = &mut self.organization.view;
                    if view.worker.as_ref().is_none_or(|w| w.library() != &library)
                        || view.pending != Some(operation)
                    {
                        continue;
                    }
                    view.pending = None;
                    match result {
                        Ok(state) if state == view.current => {
                            view.dirty = false;
                            view.error = None;
                            view.touched.clear();
                            view.sort_touched = false;
                        }
                        Ok(_) => view.dirty = true,
                        Err(error) => {
                            view.error = Some(format!("Folder view could not be saved. {error}"))
                        }
                    }
                    cx.notify();
                }
            }
        }
        self.submit_folder_view(cx);
        loaded_sort
    }
    pub(in crate::shell) fn set_folder_view_sort(&mut self, sort: Sort, cx: &mut Context<Self>) {
        let view = &mut self.organization.view;
        if view.current.sort == sort && view.ready {
            return;
        }
        view.current.sort = sort;
        view.sort_touched = true;
        view.dirty = true;
        if view.ready {
            view.error = None;
        }
        self.submit_folder_view(cx);
    }
    pub(in crate::shell) fn folder_view_busy(&self) -> bool {
        let view = &self.organization.view;
        view.task.is_some()
            || view.pending.is_some()
            || (view.enabled && !view.ready && view.error.is_none())
            || (view.enabled && view.dirty && view.error.is_none())
    }
    pub(super) fn folder_collapsed_changed(&mut self, cx: &mut Context<Self>) {
        let current: BTreeSet<_> = self.organization.collapsed.iter().cloned().collect();
        let view = &mut self.organization.view;
        for id in view
            .current
            .collapsed_folders
            .symmetric_difference(&current)
        {
            view.touched.insert(id.clone(), current.contains(id));
        }
        view.current.collapsed_folders = current;
        view.dirty = true;
        if view.ready {
            view.error = None;
        }
        self.submit_folder_view(cx);
    }
    pub(super) fn retry_folder_view(&mut self, cx: &mut Context<Self>) {
        if self.organization.view.ready {
            self.organization.view.error = None;
            self.submit_folder_view(cx);
        }
    }
    fn submit_folder_view(&mut self, cx: &mut Context<Self>) {
        let view = &mut self.organization.view;
        if !view.enabled || !view.ready || !view.dirty || view.error.is_some() {
            return;
        }
        if view.pending.is_some() {
            return;
        }
        let Some(worker) = &view.worker else {
            return;
        };
        view.current.collapsed_folders = self.organization.collapsed.iter().cloned().collect();
        view.operation += 1;
        match worker.submit(Save {
            operation: view.operation,
            state: view.current.clone(),
        }) {
            Ok(()) => view.pending = Some(view.operation),
            Err(error) => view.error = Some(format!("Folder view could not be saved. {error}")),
        }
        cx.notify();
    }
}
