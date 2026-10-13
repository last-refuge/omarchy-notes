//! Bounded, generation-tagged library queries over the existing writer.
use super::*;
use notes_core::library::{Page, Query, Scope as QueryScope, Sort};
use notes_storage::{Event, Request};
#[cfg(feature = "visual-tests")]
#[path = "library_query_check.rs"]
mod check;
#[cfg(feature = "visual-tests")]
pub use check::library_query_check;

pub(super) struct State {
    pub enabled: bool,
    pub sort: Sort,
    pub sort_loaded: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub has_more: bool,
    pub snippets: HashMap<String, String>,
    generation: u64,
    wanted: Option<Query>,
    ready: Option<u64>,
    in_flight: Option<(u64, Query)>,
    ids: Vec<String>,
    seen: std::collections::HashSet<String>,
    pending_ids: Vec<String>,
    pending_seen: std::collections::HashSet<String>,
    pending_snippets: HashMap<String, String>,
    displayed: Option<Query>,
    refresh_rows: usize,
    extending: bool,
    next_offset: u32,
    indices: HashMap<String, usize>,
    _debounce: Option<Task<()>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            sort: Sort::Edited,
            sort_loaded: false,
            loading: false,
            error: None,
            has_more: false,
            snippets: HashMap::new(),
            generation: 0,
            wanted: None,
            ready: None,
            in_flight: None,
            ids: Vec::new(),
            seen: Default::default(),
            pending_ids: Vec::new(),
            pending_seen: Default::default(),
            pending_snippets: HashMap::new(),
            displayed: None,
            refresh_rows: 100,
            extending: false,
            next_offset: 0,
            indices: HashMap::new(),
            _debounce: None,
        }
    }
}

impl Shell {
    fn query_scope(&self) -> QueryScope {
        match &self.library.scope {
            Scope::All => QueryScope::All,
            Scope::Favorites => QueryScope::Favorites,
            Scope::FolderId(id) => QueryScope::Folder(id.clone()),
            Scope::Folder(name) => QueryScope::Folder(self.live.as_ref().and_then(|live| {
                live.folders
                    .iter()
                    .find(|f| &f.name == name)
                    .map(|f| f.id.clone())
            })),
        }
    }

    pub(super) fn queue_library_query(&mut self, debounce: bool, cx: &mut Context<Self>) {
        if self.live.is_none() {
            return;
        }
        self.query.enabled = true;
        self.query.generation += 1;
        let generation = self.query.generation;
        let query = Query {
            generation,
            scope: self.query_scope(),
            text: self.library.query.clone(),
            sort: self.query.sort,
            offset: 0,
            limit: 100,
        };
        let same_view = self.query.displayed.as_ref().is_some_and(|displayed| {
            displayed.scope == query.scope
                && displayed.text == query.text
                && displayed.sort == query.sort
        });
        self.query.refresh_rows = if same_view {
            self.query.ids.len().max(100)
        } else {
            100
        };
        if !same_view {
            self.query.ids.clear();
            self.query.seen.clear();
            self.query.snippets.clear();
            self.refresh_query_visibility();
        }
        self.query.pending_ids.clear();
        self.query.pending_seen.clear();
        self.query.pending_snippets.clear();
        self.query.extending = false;
        self.query.wanted = Some(query);
        self.query.error = None;
        self.query.loading = true;
        self.query.ready = None;
        self.query._debounce = None;
        if debounce {
            self.query._debounce = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(45))
                    .await;
                let _ = this.update(cx, |s, cx| {
                    if s.query.generation == generation {
                        s.query.ready = Some(generation);
                        s.submit_library_query(cx);
                    }
                });
            }));
        } else {
            self.query.ready = Some(generation);
            self.submit_library_query(cx);
        }
        cx.notify();
    }

    pub(super) fn submit_library_query(&mut self, cx: &mut Context<Self>) {
        if self.query.in_flight.is_some() {
            return;
        }
        let Some(query) = self.query.wanted.as_ref() else {
            return;
        };
        if self.query.ready != Some(query.generation) {
            return;
        }
        let query = query.clone();
        let Some(live) = &mut self.live else { return };
        live.operation += 1;
        let operation = live.operation;
        match live.worker.requests.try_send(Request::Query {
            operation,
            query: query.clone(),
        }) {
            Ok(()) => {
                self.query.wanted = None;
                self.query.ready = None;
                self.query.in_flight = Some((operation, query));
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                // Retry through the ordinary writer poll; only one wanted query
                // is retained, so typing cannot fill the bounded save queue.
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                self.query.loading = false;
                self.query.wanted = None;
                self.query.error =
                    Some("Search is unavailable. Your open note is still here.".into());
            }
        }
        cx.notify();
    }

    pub(super) fn library_query_event(&mut self, event: Event, cx: &mut Context<Self>) {
        let Event::Queried {
            operation,
            query,
            result,
        } = event
        else {
            return;
        };
        if self
            .query
            .in_flight
            .as_ref()
            .is_none_or(|(op, requested)| *op != operation || requested != &query)
        {
            return;
        }
        self.query.in_flight = None;
        if query.generation == self.query.generation
            && query.scope == self.query_scope()
            && query.text == self.library.query
            && query.sort == self.query.sort
        {
            match result {
                Ok(page) => self.apply_library_page(query, page),
                Err(error) => {
                    self.query.loading = false;
                    self.query.error = Some(format!("Couldn’t search this library. {error}"));
                }
            }
        }
        self.submit_library_query(cx);
        cx.notify();
    }

    fn apply_library_page(&mut self, mut query: Query, page: Page) {
        let next_offset = query.offset.saturating_add(page.rows.len() as u32);
        self.query.has_more = page.has_more;
        let (ids, seen, snippets) = if self.query.extending {
            (
                &mut self.query.ids,
                &mut self.query.seen,
                &mut self.query.snippets,
            )
        } else {
            (
                &mut self.query.pending_ids,
                &mut self.query.pending_seen,
                &mut self.query.pending_snippets,
            )
        };
        for row in page.rows {
            if seen.insert(row.id.clone()) {
                ids.push(row.id.clone());
            }
            snippets.insert(row.id, row.snippet);
        }
        if !self.query.extending
            && self.query.has_more
            && self.query.pending_ids.len() < self.query.refresh_rows
        {
            query.offset = next_offset;
            self.query.ready = Some(query.generation);
            self.query.wanted = Some(query);
            return;
        }
        if !self.query.extending {
            self.query.ids = std::mem::take(&mut self.query.pending_ids);
            self.query.seen = std::mem::take(&mut self.query.pending_seen);
            self.query.snippets = std::mem::take(&mut self.query.pending_snippets);
        }
        self.query.extending = false;
        self.query.displayed = Some(query);
        self.query.next_offset = next_offset;
        self.query.loading = false;
        self.refresh_query_visibility();
        if !self.library.visible.is_empty() {
            self.list.remeasure_items(0..self.library.visible.len());
        }
    }

    pub(super) fn invalidate_query_indices(&mut self) {
        self.query.indices.clear();
        self.query.indices.extend(
            self.library
                .notes
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id.clone(), i)),
        );
    }

    pub(super) fn refresh_query_visibility(&mut self) {
        if !self.query.enabled {
            self.library.refresh();
            return;
        }
        let offset = self.list.logical_scroll_top();
        let anchor = self
            .library
            .visible
            .get(offset.item_ix)
            .map(|&i| self.library.notes[i].id.clone());
        let visible: Vec<_> = self
            .query
            .ids
            .iter()
            .filter_map(|id| self.query.indices.get(id).copied())
            .collect();
        if visible == self.library.visible {
            return;
        }
        self.library.visible = visible;
        self.list.reset(self.library.visible.len());
        if let Some(anchor) = anchor
            && let Some(item_ix) = self
                .library
                .visible
                .iter()
                .position(|&i| self.library.notes[i].id == anchor)
        {
            self.list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: offset.offset_in_item,
            });
        }
    }

    pub(super) fn more_library_results(&mut self, cx: &mut Context<Self>) {
        if !self.query.has_more || self.query.loading || self.query.in_flight.is_some() {
            return;
        }
        self.query.wanted = Some(Query {
            generation: self.query.generation,
            scope: self.query_scope(),
            text: self.library.query.clone(),
            sort: self.query.sort,
            offset: self.query.next_offset,
            limit: 100,
        });
        self.query.ready = Some(self.query.generation);
        self.query.extending = true;
        self.query.loading = true;
        self.submit_library_query(cx);
    }

    pub(super) fn library_sort_label(&self) -> &'static str {
        match self.query.sort {
            Sort::Edited => "Edited",
            Sort::Created => "Created",
            Sort::Title => "Title",
        }
    }
}
