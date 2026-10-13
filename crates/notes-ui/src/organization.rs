//! Folder navigation and commit-gated organization over the shared writer.
use super::*;
use notes_core::organization::{Change, Mutation, Row};
use std::collections::HashSet;
#[path = "organization_tree.rs"]
mod tree;
#[path = "organization_view.rs"]
mod view;
pub(super) use tree::{FocusFolderTree, bind_keys};
use tree::{
    FolderTreeDown, FolderTreeEnd, FolderTreeHome, FolderTreeLeft, FolderTreeOpen, FolderTreeRight,
    FolderTreeUp, TreeKey,
};
#[cfg(feature = "visual-tests")]
#[path = "organization_check.rs"]
mod check;
#[cfg(feature = "visual-tests")]
pub use check::organization_check;

#[derive(Clone)]
pub(super) enum Action {
    New,
    Rename,
    MoveFolder,
    MoveNote,
    Close,
    Submit,
    Pick(usize),
    Step(bool),
    Collapse(String),
}
#[derive(Clone)]
enum Mode {
    Create { parent: Option<String> },
    Rename { id: String },
    MoveFolder { id: String },
    MoveNote { id: String },
}
impl Mode {
    fn naming(&self) -> bool {
        matches!(self, Self::Create { .. } | Self::Rename { .. })
    }
    fn title(&self) -> &'static str {
        match self {
            Self::Create { .. } => "A place for your ideas.",
            Self::Rename { .. } => "Give it a new name.",
            Self::MoveFolder { .. } => "Find a new home.",
            Self::MoveNote { .. } => "Put this thought in its place.",
        }
    }
}
#[derive(Clone)]
struct Destination {
    id: Option<String>,
    label: String,
}
pub(super) struct State {
    mode: Option<Mode>,
    source: String,
    input: String,
    pub field: Entity<SearchField>,
    focus: Vec<FocusHandle>,
    return_focus: Option<FocusHandle>,
    pub rows: Vec<Row>,
    pub paths: HashMap<String, String>,
    collapsed: HashSet<String>,
    tree_focus: FocusHandle,
    tree_active: bool,
    tree_subscriptions: Vec<Subscription>,
    focused_folder: Option<String>,
    view: view::State,
    nav: UniformListScrollHandle,
    #[cfg(feature = "visual-tests")]
    rendered_rows: std::cell::RefCell<HashSet<usize>>,
    candidates: Vec<Destination>,
    destinations: Vec<Destination>,
    selected: usize,
    destination_list: UniformListScrollHandle,
    pending: Option<(u64, Mutation)>,
    error: Option<String>,
    epoch: u64,
}
impl State {
    pub fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            mode: None,
            source: String::new(),
            input: String::new(),
            field: cx.new(|cx| SearchField::named("Folder name", Theme::reference(), cx)),
            focus: (0..3).map(|_| cx.focus_handle()).collect(),
            return_focus: None,
            rows: vec![],
            paths: HashMap::new(),
            collapsed: HashSet::new(),
            tree_focus: cx.focus_handle(),
            tree_active: false,
            tree_subscriptions: vec![],
            focused_folder: None,
            view: view::State::default(),
            nav: UniformListScrollHandle::new(),
            #[cfg(feature = "visual-tests")]
            rendered_rows: Default::default(),
            candidates: vec![],
            destinations: vec![],
            selected: 0,
            destination_list: UniformListScrollHandle::new(),
            pending: None,
            error: None,
            epoch: 0,
        }
    }
    pub fn open(&self) -> bool {
        self.mode.is_some()
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn close_with_window(&mut self) {
        debug_assert!(!self.busy());
        self.mode = None;
        self.return_focus = None;
        self.epoch += 1;
    }
}
impl Shell {
    pub(super) fn organization_subscription(&self, cx: &mut Context<Self>) -> Subscription {
        cx.subscribe(&self.organization.field, |s, _, event, cx| {
            if !s.organization.open() || s.organization.busy() {
                return;
            }
            let Some(window) = s.windows.main else {
                return;
            };
            let epoch = s.organization.epoch;
            let event = event.clone();
            let entity = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, w, cx| {
                    entity.update(cx, |s, cx| {
                        if s.organization.epoch != epoch
                            || !s.organization.open()
                            || s.organization.busy()
                        {
                            return;
                        }
                        match event {
                            SearchEvent::Changed(value) => {
                                s.organization.input = value;
                                s.organization.error = None;
                                s.organization_destinations();
                                cx.notify();
                            }
                            SearchEvent::Accept => s.organize(Action::Submit, w, cx),
                            SearchEvent::Cancel => s.organize(Action::Close, w, cx),
                            SearchEvent::Next => s.organize(Action::Step(true), w, cx),
                            SearchEvent::Previous => s.organize(Action::Step(false), w, cx),
                        }
                    })
                });
            });
        })
    }
    pub(super) fn refresh_folder_rows(&mut self) {
        let Some(live) = &self.live else {
            return;
        };
        let focused = self.organization.focused_folder.clone();
        let focus_index = focused.as_ref().and_then(|id| {
            self.organization
                .rows
                .iter()
                .position(|r| &r.folder.id == id)
        });
        let offset = self.organization.nav.0.borrow().base_handle.offset();
        let before = self.organization.collapsed.len();
        let valid: HashSet<_> = live.folders.iter().map(|f| f.id.as_str()).collect();
        self.organization
            .collapsed
            .retain(|id| valid.contains(id.as_str()));
        if before != self.organization.collapsed.len() {
            self.organization
                .view
                .pruned_collapsed(&self.organization.collapsed);
        }
        let first = (f32::from(-offset.y) / 42.).floor() as usize;
        let anchor = self
            .organization
            .rows
            .get(first)
            .map(|r| r.folder.id.clone());
        self.organization.paths = notes_core::organization::rows(&live.folders, &HashSet::new())
            .into_iter()
            .map(|r| (r.folder.id, r.path))
            .collect();
        self.organization.rows =
            notes_core::organization::rows(&live.folders, &self.organization.collapsed);
        if let Some(id) = focused
            && !self.organization.rows.iter().any(|r| r.folder.id == id)
        {
            let mut parent = live
                .folders
                .iter()
                .find(|f| f.id == id)
                .and_then(|f| f.parent.clone());
            let mut seen = HashSet::new();
            let mut visible_parent = None;
            while let Some(id) = parent {
                if !seen.insert(id.clone()) {
                    break;
                }
                if self.organization.rows.iter().any(|r| r.folder.id == id) {
                    visible_parent = Some(id);
                    break;
                }
                parent = live
                    .folders
                    .iter()
                    .find(|f| f.id == id)
                    .and_then(|f| f.parent.clone());
            }
            self.organization.focused_folder = visible_parent.or_else(|| {
                let index = focus_index
                    .unwrap_or(0)
                    .min(self.organization.rows.len().saturating_sub(1));
                self.organization
                    .rows
                    .get(index)
                    .map(|r| r.folder.id.clone())
            });
        }
        if let Some(index) = anchor.and_then(|id| {
            self.organization
                .rows
                .iter()
                .position(|r| r.folder.id == id)
        }) {
            self.organization
                .nav
                .0
                .borrow()
                .base_handle
                .set_offset(point(
                    px(0.),
                    offset.y + px((first as f32 - index as f32) * 42.),
                ));
        }
    }
    pub(super) fn scope_title(&self) -> String {
        match &self.library.scope {
            Scope::FolderId(Some(id)) => self
                .organization
                .paths
                .get(id)
                .cloned()
                .unwrap_or_else(|| "Folder unavailable".into()),
            _ => self.library.scope.title().into(),
        }
    }
    pub(super) fn organization_tab(&self, backward: bool, w: &mut Window, cx: &mut Context<Self>) {
        let mut handles = vec![self.organization.focus[0].clone()];
        if !self.organization.busy() {
            handles.push(self.organization.field.read(cx).focus.clone());
            if self.organization.mode.as_ref().is_some_and(|m| !m.naming()) {
                handles.push(self.organization.focus[1].clone());
            }
            handles.push(self.organization.focus[2].clone());
        }
        let current = handles.iter().position(|f| f.is_focused(w)).unwrap_or(0);
        let next = if backward {
            (current + handles.len() - 1) % handles.len()
        } else {
            (current + 1) % handles.len()
        };
        w.focus(&handles[next], cx);
    }
    fn prepare_organization_destinations(&mut self) {
        self.organization.candidates.clear();
        let Some(mode) = &self.organization.mode else {
            return;
        };
        if mode.naming() {
            return;
        }
        let Some(live) = &self.live else {
            return;
        };
        let moving = match mode {
            Mode::MoveFolder { id } => Some(id.as_str()),
            _ => None,
        };
        let root_label = if moving.is_some() {
            "Notebook — top level"
        } else {
            "Notes — unfiled notes"
        };
        let valid = notes_core::organization::destinations(&live.folders, moving);
        let mut destinations = vec![Destination {
            id: None,
            label: root_label.into(),
        }];
        for row in notes_core::organization::rows(&live.folders, &HashSet::new()) {
            if valid.contains(&row.folder.id) {
                destinations.push(Destination {
                    id: Some(row.folder.id),
                    label: row.path,
                });
            }
        }
        self.organization.candidates = destinations;
    }
    fn organization_destinations(&mut self) {
        let query = self.organization.input.to_lowercase();
        self.organization.destinations = self
            .organization
            .candidates
            .iter()
            .filter(|d| d.label.to_lowercase().contains(&query))
            .cloned()
            .collect();
        self.organization.selected = 0;
        self.organization
            .destination_list
            .scroll_to_item_strict(0, ScrollStrategy::Top);
    }
    pub(super) fn organize(&mut self, action: Action, w: &mut Window, cx: &mut Context<Self>) {
        if self.live.is_none() || self.organization.busy() {
            return;
        }
        match action {
            Action::Collapse(id) => {
                if !self.organization.collapsed.remove(&id) {
                    self.organization.collapsed.insert(id);
                }
                self.refresh_folder_rows();
                self.folder_collapsed_changed(cx);
            }
            Action::Close => {
                if self.organization.field.read(cx).composing() {
                    return;
                }
                self.organization.mode = None;
                self.organization.epoch += 1;
                if let Some(f) = self.organization.return_focus.take() {
                    w.focus(&f, cx);
                } else {
                    w.focus(&self.focus, cx);
                }
            }
            Action::Pick(index) => {
                if index < self.organization.destinations.len() {
                    self.organization.selected = index;
                    self.organization.error = None;
                    w.focus(&self.organization.focus[1], cx);
                }
            }
            Action::Step(next) => {
                if !self.organization.destinations.is_empty() {
                    let index = self.organization.selected;
                    self.organization.selected = if next {
                        (index + 1).min(self.organization.destinations.len() - 1)
                    } else {
                        index.saturating_sub(1)
                    };
                    self.organization
                        .destination_list
                        .scroll_to_item(self.organization.selected, ScrollStrategy::Nearest);
                }
            }
            Action::Submit => self.submit_organization(w, cx),
            Action::New | Action::Rename | Action::MoveFolder | Action::MoveNote => {
                if self.organization.open()
                    || self
                        .live
                        .as_ref()
                        .is_some_and(|l| l.upgrade.open || l.pdf.open || l.closing)
                {
                    return;
                }
                if self
                    .library
                    .selected
                    .as_ref()
                    .and_then(|id| self.live.as_ref()?.notes.get(id))
                    .is_some_and(|s| s.input_is_composing(cx))
                {
                    return;
                }
                let live = self.live.as_ref().unwrap();
                let folder = if self.organization.tree_focus.is_focused(w) {
                    self.organization.focused_folder.clone()
                } else {
                    match &self.library.scope {
                        Scope::FolderId(id) => id.clone(),
                        _ => None,
                    }
                };
                let (mode, source, input) = match action {
                    Action::New => (
                        Mode::Create {
                            parent: folder.clone(),
                        },
                        folder
                            .as_ref()
                            .and_then(|id| self.organization.paths.get(id))
                            .cloned()
                            .unwrap_or_else(|| "Notebook — top level".into()),
                        String::new(),
                    ),
                    Action::Rename | Action::MoveFolder => {
                        let Some(folder) =
                            folder.and_then(|id| live.folders.iter().find(|f| f.id == id))
                        else {
                            return;
                        };
                        let rename = matches!(action, Action::Rename);
                        (
                            if rename {
                                Mode::Rename {
                                    id: folder.id.clone(),
                                }
                            } else {
                                Mode::MoveFolder {
                                    id: folder.id.clone(),
                                }
                            },
                            self.organization.paths[&folder.id].clone(),
                            if rename {
                                folder.name.clone()
                            } else {
                                String::new()
                            },
                        )
                    }
                    Action::MoveNote => {
                        let Some(note) = self
                            .library
                            .selected
                            .as_ref()
                            .and_then(|id| live.notes.get(id))
                        else {
                            return;
                        };
                        (
                            Mode::MoveNote {
                                id: note.note.id.clone(),
                            },
                            note.note.body.title(),
                            String::new(),
                        )
                    }
                    _ => unreachable!(),
                };
                let naming = mode.naming();
                let return_focus = match &mode {
                    Mode::MoveNote { id } => live
                        .notes
                        .get(id)
                        .and_then(|note| note.active_editor().map(|e| e.read(cx).focus.clone())),
                    // Generic toolbar/nav controls are unmounted while this
                    // page is open, so their automatically created handles
                    // cannot be restored. Retain only entity-owned handles.
                    _ => self
                        .library
                        .selected
                        .as_ref()
                        .and_then(|id| live.notes.get(id))
                        .and_then(|note| note.active_editor().map(|e| e.read(cx).focus.clone()))
                        .filter(|focus| focus.is_focused(w))
                        .or_else(|| {
                            self.organization
                                .tree_focus
                                .is_focused(w)
                                .then(|| self.organization.tree_focus.clone())
                        })
                        .or_else(|| {
                            self.search
                                .read(cx)
                                .focus
                                .is_focused(w)
                                .then(|| self.search.read(cx).focus.clone())
                        })
                        .or_else(|| Some(self.focus.clone())),
                };
                self.organization.mode = Some(mode);
                self.organization.source = source;
                self.organization.input = input.clone();
                self.organization.epoch += 1;
                self.organization.error = None;
                self.organization.return_focus = return_focus;
                self.settings_open = false;
                self.organization.field.update(cx, |s, cx| {
                    s.set_label(
                        if naming {
                            "Folder name"
                        } else {
                            "Find destination"
                        },
                        cx,
                    );
                    s.theme = self.theme;
                    s.set_text(&input, w, cx);
                    w.focus(&s.focus, cx);
                });
                self.prepare_organization_destinations();
                self.organization_destinations();
            }
        }
        cx.notify();
    }
    fn submit_organization(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.organization.field.read(cx).composing() {
            return;
        }
        let Some(mode) = self.organization.mode.clone() else {
            return;
        };
        let input = self.organization.input.clone();
        let mutation = match mode {
            Mode::Create { parent } => Mutation::CreateFolder {
                name: input,
                parent,
            },
            Mode::Rename { id } => Mutation::RenameFolder { id, name: input },
            Mode::MoveFolder { id } => {
                let Some(destination) = self
                    .organization
                    .destinations
                    .get(self.organization.selected)
                else {
                    self.organization.error = Some("Choose a destination folder.".into());
                    return;
                };
                Mutation::MoveFolder {
                    id,
                    parent: destination.id.clone(),
                }
            }
            Mode::MoveNote { id } => {
                if self
                    .live
                    .as_ref()
                    .unwrap()
                    .notes
                    .get(&id)
                    .is_none_or(|s| s.note.revision == 0)
                {
                    self.organization.error =
                        Some("Save this new note before moving it. Your draft stays here.".into());
                    return;
                }
                let Some(destination) = self
                    .organization
                    .destinations
                    .get(self.organization.selected)
                else {
                    self.organization.error = Some("Choose a destination folder.".into());
                    return;
                };
                Mutation::MoveNote {
                    id,
                    folder: destination.id.clone(),
                }
            }
        };
        if let Mutation::CreateFolder { name, .. } | Mutation::RenameFolder { name, .. } = &mutation
            && let Err(e) = notes_core::organization::name(name)
        {
            self.organization.error = Some(e.to_string());
            return;
        }
        let live = self.live.as_mut().unwrap();
        live.operation += 1;
        let operation = live.operation;
        match live
            .worker
            .requests
            .try_send(notes_storage::Request::Organize {
                operation,
                mutation: mutation.clone(),
            }) {
            Ok(()) => {
                self.organization.pending = Some((operation, mutation));
                self.organization.error = None;
                w.focus(&self.organization.focus[0], cx);
            }
            Err(_) => {
                self.organization.error = Some("The library is busy. Try again in a moment.".into())
            }
        }
    }
    pub(super) fn organized(
        &mut self,
        operation: u64,
        result: Result<Change, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .organization
            .pending
            .as_ref()
            .is_none_or(|(op, _)| *op != operation)
        {
            return;
        }
        let (_, mutation) = self.organization.pending.take().unwrap();
        match result {
            Err(error) => {
                self.organization.error = Some(error);
                self.live.as_mut().unwrap().closing = false;
                w.focus(&self.organization.field.read(cx).focus.clone(), cx);
            }
            Ok(change) => {
                let live = self.live.as_mut().unwrap();
                match change {
                    Change::Folder(folder) => {
                        let id = folder.id.clone();
                        if let Some(existing) = live.folders.iter_mut().find(|f| f.id == id) {
                            *existing = folder;
                        } else {
                            live.folders.push(folder);
                        }
                        if matches!(mutation, Mutation::CreateFolder { .. }) {
                            self.organization.focused_folder = Some(id.clone());
                            self.library.scope = Scope::FolderId(Some(id));
                            self.library.selected = None;
                            self.reading = false;
                            self.organization.return_focus = None;
                        }
                    }
                    Change::NoteMoved { id, folder } => {
                        if let Some(note) = live.notes.get_mut(&id) {
                            note.note.folder = folder.clone();
                        }
                        self.library.scope = Scope::FolderId(folder);
                        self.library.selected = Some(id);
                    }
                }
                if let Scope::FolderId(Some(id)) = &self.library.scope {
                    let mut parent = live
                        .folders
                        .iter()
                        .find(|f| &f.id == id)
                        .and_then(|f| f.parent.clone());
                    let mut seen = HashSet::new();
                    while let Some(id) = parent {
                        if !seen.insert(id.clone()) {
                            break;
                        }
                        self.organization.collapsed.remove(&id);
                        parent = live
                            .folders
                            .iter()
                            .find(|f| f.id == id)
                            .and_then(|f| f.parent.clone());
                    }
                }
                self.refresh_folder_rows();
                self.folder_collapsed_changed(cx);
                if let Scope::FolderId(Some(id)) = &self.library.scope
                    && let Some(index) = self
                        .organization
                        .rows
                        .iter()
                        .position(|r| &r.folder.id == id)
                {
                    self.organization
                        .nav
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
                if matches!(
                    mutation,
                    Mutation::CreateFolder { .. } | Mutation::MoveNote { .. }
                ) {
                    self.library.query.clear();
                    self.search.update(cx, |s, cx| s.clear_silently(w, cx));
                }
                self.refresh_real_library(cx);
                self.organization.mode = None;
                self.organization.epoch += 1;
                // Keep the same editor/session after filing; naming a folder
                // never changes its note bodies, revisions or undo stacks.
                if let Some(f) = self.organization.return_focus.take() {
                    w.focus(&f, cx);
                    if f == self.organization.tree_focus {
                        self.reveal_folder_focus();
                    }
                } else {
                    w.focus(&self.focus, cx);
                }
            }
        }
        cx.notify();
    }
    pub(super) fn folder_navigation(&self, cx: &mut Context<Self>) -> AnyElement {
        let accent = self.theme.accent;
        if self.live.is_none() {
            return div()
                .children(["Personal", "Work", "Ideas"].map(|name| {
                    self.navigation_row(
                        &format!("folder-{name}"),
                        name,
                        "folder",
                        Scope::Folder(name.into()),
                        self.library
                            .notes
                            .iter()
                            .filter(|n| n.folder == name)
                            .count(),
                        cx,
                    )
                }))
                .into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                self.navigation_row(
                    "unfiled",
                    "Notes",
                    "folder",
                    Scope::FolderId(None),
                    self.library
                        .notes
                        .iter()
                        .filter(|n| n.folder_id.is_none())
                        .count(),
                    cx,
                ),
            )
            .child(
                div().id("folder-tree").role(Role::Tree).aria_label("Folders")
                .aria_description("Use arrows to navigate and expand folders; Enter opens the focused folder.")
                .key_context("FolderTree").track_focus(&self.organization.tree_focus).tab_index(0)
                .border_1().border_color(gpui::transparent_black())
                .focus_visible(move|s|s.border_color(accent))
                .flex_1().min_h_0()
                .on_action(cx.listener(|s,_:&FolderTreeUp,w,cx|s.folder_tree_key(TreeKey::Up,w,cx)))
                .on_action(cx.listener(|s,_:&FolderTreeDown,w,cx|s.folder_tree_key(TreeKey::Down,w,cx)))
                .on_action(cx.listener(|s,_:&FolderTreeHome,w,cx|s.folder_tree_key(TreeKey::Home,w,cx)))
                .on_action(cx.listener(|s,_:&FolderTreeEnd,w,cx|s.folder_tree_key(TreeKey::End,w,cx)))
                .on_action(cx.listener(|s,_:&FolderTreeLeft,w,cx|s.folder_tree_key(TreeKey::Left,w,cx)))
                .on_action(cx.listener(|s,_:&FolderTreeRight,w,cx|s.folder_tree_key(TreeKey::Right,w,cx)))
                .on_action(cx.listener(|s,_:&FolderTreeOpen,w,cx|s.folder_tree_key(TreeKey::Open,w,cx)))
                .child(uniform_list(
                    "folder-tree-rows",
                    self.organization.rows.len(),
                    cx.processor(|s, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| s.folder_row(index, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.organization.nav)
                .size_full()),
            )
            .when_some(self.organization.view.error.clone(), |d, error| {
                d.child(div().id("folder-view-error").role(Role::Status).aria_label(error.clone())
                    .px(px(12.)).py(px(8.)).text_size(px(11.)).text_color(self.theme.muted)
                    .child(error)
                    .when(self.organization.view.can_retry(), |d|d.child(div().id("folder-view-retry")
                        .role(Role::Button).aria_label("Retry saving folder view").tab_index(0)
                        .cursor_pointer().text_color(self.theme.accent).child("Retry saving view")
                        .on_click(cx.listener(|s,_,_,cx|s.retry_folder_view(cx)))))
                )
            })
            .into_any_element()
    }
    fn folder_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(feature = "visual-tests")]
        self.organization.rendered_rows.borrow_mut().insert(index);
        let row = &self.organization.rows[index];
        let id = row.folder.id.clone();
        let focused = self.organization.focused_folder.as_ref() == Some(&id);
        let opened = self.library.scope == Scope::FolderId(Some(id.clone()));
        let theme = self.theme;
        let clicked = id.clone();
        div()
            .id(SharedString::from(format!("folder-{id}")))
            .role(Role::TreeItem)
            .aria_label(row.folder.name.clone())
            .aria_description(row.path.clone())
            .aria_level(row.depth + 1)
            .aria_selected(opened)
            .when(focused, |d| d.aria_active_descendant())
            .when(row.has_children, |d| {
                d.aria_expanded(!self.organization.collapsed.contains(&id))
            })
            .w_full()
            .h(px(42.))
            .flex()
            .items_center()
            .cursor_pointer()
            .border_1()
            .border_color(if focused && self.organization.tree_active {
                theme.accent
            } else {
                gpui::transparent_black()
            })
            .bg(if opened {
                theme.selected
            } else {
                theme.navigation
            })
            .hover(move |s| s.bg(theme.overlay))
            .on_click(cx.listener(move |s, _, w, cx| s.click_folder(clicked.clone(), w, cx)))
            .pl(px((row.depth.min(5) * 10) as f32))
            .child(if row.has_children {
                let toggled = id.clone();
                div()
                    .id(SharedString::from(format!("folder-disclosure-{id}")))
                    .role(Role::Button)
                    .aria_label(format!("Toggle subfolders of {}", row.path))
                    .tab_index(-1)
                    .cursor_pointer()
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.organization.focused_folder = Some(toggled.clone());
                        s.organization.tree_focus.focus(w, cx);
                        s.organize(Action::Collapse(toggled.clone()), w, cx);
                        cx.stop_propagation();
                    }))
                    .aria_expanded(!self.organization.collapsed.contains(&id))
                    .w(px(18.))
                    .h(px(28.))
                    .text_center()
                    .text_color(self.theme.muted)
                    .child(if self.organization.collapsed.contains(&id) {
                        "›"
                    } else {
                        "⌄"
                    })
                    .into_any_element()
            } else {
                div().w(px(18.)).flex_none().into_any_element()
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .flex_1()
                    .min_w_0()
                    .child(icon(
                        "folder",
                        if opened { theme.accent } else { theme.text },
                        20.,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(row.folder.name.clone()),
                    )
                    .child(
                        div().text_size(px(12.)).text_color(theme.muted).child(
                            self.library
                                .notes
                                .iter()
                                .filter(|n| n.folder_id.as_ref() == Some(&id))
                                .count()
                                .to_string(),
                        ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn organization_page(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let state = &self.organization;
        let mode = state.mode.as_ref().unwrap();
        let t = self.theme;
        let naming = mode.naming();
        let submit = if matches!(mode, Mode::Create { .. }) {
            "Create folder"
        } else if naming {
            "Save name"
        } else {
            "Move here"
        };
        let input = div()
            .h(px(42.))
            .flex_none()
            .px(px(12.))
            .border_1()
            .border_color(t.border)
            .rounded(px(6.))
            .bg(t.navigation)
            .child(if state.busy() {
                div()
                    .py(px(10.))
                    .text_color(t.muted)
                    .child("Saving your changes…")
                    .into_any_element()
            } else {
                state.field.clone().into_any_element()
            });
        let body = div().flex().flex_col().w_full().px(px(24.)).py(px(22.)).flex_1().min_h_0()
            .child(div().font_family("Noto Serif").text_size(px(25.)).mb(px(8.)).child(mode.title()))
            .child(div().id("organization-source").role(Role::Group).aria_label(format!("Organizing {}",state.source)).text_size(px(12.)).text_color(t.muted).mb(px(18.)).overflow_hidden().text_ellipsis().child(if matches!(mode,Mode::Create{..}){format!("Inside {}",state.source)}else{state.source.clone()}))
            .child(input)
            .when(naming, |d| d.child(div().mt(px(14.)).text_size(px(12.)).line_height(relative(1.6)).text_color(t.muted).child("A clear name makes ideas easier to return to. Rename or move this folder whenever you need.")))
            .when(!naming, |d| d.child(div().id("folder-destinations").role(Role::ListBox).aria_label("Destination folders").track_focus(&state.focus[1]).flex_1().min_h_0().mt(px(14.)).border_1().border_color(t.border).rounded(px(8.)).overflow_hidden()
                .when(state.destinations.is_empty(), |d| d.child(div().p(px(20.)).text_color(t.muted).child("No matching folders. Try a shorter name.")))
                .when(!state.destinations.is_empty(), |d| d.child(uniform_list("destination-rows", state.destinations.len(),cx.processor(|s,range:std::ops::Range<usize>,_,cx| range.map(|index| {
                    let item=&s.organization.destinations[index];
                    let selected=s.organization.selected==index;
                    div().id(("destination",index)).role(Role::ListBoxOption).aria_label(item.label.clone()).aria_selected(selected).when(selected, |d|d.aria_active_descendant()).cursor_pointer().w_full().h(px(48.)).px(px(14.)).flex().items_center().gap(px(10.)).bg(if selected{s.theme.selected}else{s.theme.editor}).text_color(if selected{s.theme.accent}else{s.theme.text})
                        .on_click(cx.listener(move|s,_,w,cx|s.organize(Action::Pick(index),w,cx)))
                        .child(icon("folder",s.theme.muted,18.)).child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().child(item.label.clone())).child(if selected{"✓"}else{""}).into_any_element()
                }).collect::<Vec<_>>())).track_scroll(&state.destination_list).size_full()))));
        let footer =
            div()
                .flex_none()
                .px(px(24.))
                .py(px(14.))
                .border_t_1()
                .border_color(t.border)
                .bg(t.navigation)
                .when_some(state.error.clone(), |d, message| {
                    d.child(
                        div()
                            .id("organization-error")
                            .role(Role::Status)
                            .aria_label(message.clone())
                            .text_color(t.accent)
                            .text_size(px(12.))
                            .mb(px(10.))
                            .max_h(px(56.))
                            .overflow_y_scroll()
                            .child(message),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .gap(px(14.))
                        .child(div().text_color(t.muted).text_size(px(11.)).child(
                            if state.busy() {
                                "Waiting for your library…"
                            } else if naming {
                                "Enter to save · Esc to return"
                            } else {
                                "Choose a folder · Enter to move"
                            },
                        ))
                        .child(
                            self.control(
                                "organization-submit",
                                submit,
                                Command::Organize(Action::Submit),
                                cx,
                            )
                            .track_focus(&state.focus[2])
                            .when(state.busy(), |d| {
                                d.role(Role::Status)
                                    .aria_label("Saving organization changes")
                                    .opacity(0.5)
                            })
                            .flex_none()
                            .px(px(16.))
                            .py(px(9.))
                            .border_color(t.accent)
                            .bg(t.selected)
                            .text_color(t.accent)
                            .child(if state.busy() {
                                "Saving…"
                            } else {
                                submit
                            }),
                        ),
                );
        div()
            .id("organization-page")
            .role(Role::Group)
            .aria_label("Organize your notebook")
            .key_context("Organize")
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(t.navigation)
            .on_key_down(cx.listener(|s, event: &KeyDownEvent, w, cx| {
                if !s.organization.focus[1].is_focused(w) || event.keystroke.modifiers.modified() {
                    return;
                }
                let action = match event.keystroke.key.as_str() {
                    "down" => Action::Step(true),
                    "up" => Action::Step(false),
                    "enter" => Action::Submit,
                    _ => return,
                };
                s.organize(action, w, cx);
                w.prevent_default();
                cx.stop_propagation();
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .h(px(54.))
                    .flex_none()
                    .px(px(16.))
                    .border_b_1()
                    .border_color(t.border)
                    .child(
                        self.icon_button(
                            "close-organization",
                            "Return to notebook",
                            "back",
                            Command::Organize(Action::Close),
                            cx,
                        )
                        .track_focus(&state.focus[0])
                        .when(state.busy(), |d| {
                            d.role(Role::Status)
                                .aria_label("Saving organization changes")
                                .opacity(0.5)
                        }),
                    )
                    .child(label("Organize your notebook", t)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_1()
                    .min_h_0()
                    .p(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .max_w(px(660.))
                            .h(px(if naming { 320. } else { 540. }))
                            .max_h_full()
                            .min_h_0()
                            .border_1()
                            .border_color(t.border)
                            .rounded(px(12.))
                            .overflow_hidden()
                            .bg(t.editor)
                            .child(body)
                            .child(footer),
                    ),
            )
    }
}
