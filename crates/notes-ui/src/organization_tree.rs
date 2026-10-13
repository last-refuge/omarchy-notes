//! One keyboard focus for the virtualized folder tree; opening is explicit.
use super::*;

actions!(
    folder_tree,
    [
        FocusFolderTree,
        FolderTreeUp,
        FolderTreeDown,
        FolderTreeHome,
        FolderTreeEnd,
        FolderTreeLeft,
        FolderTreeRight,
        FolderTreeOpen
    ]
);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-alt-f", FocusFolderTree, Some("Notes")),
        KeyBinding::new("up", FolderTreeUp, Some("FolderTree")),
        KeyBinding::new("down", FolderTreeDown, Some("FolderTree")),
        KeyBinding::new("home", FolderTreeHome, Some("FolderTree")),
        KeyBinding::new("end", FolderTreeEnd, Some("FolderTree")),
        KeyBinding::new("left", FolderTreeLeft, Some("FolderTree")),
        KeyBinding::new("right", FolderTreeRight, Some("FolderTree")),
        KeyBinding::new("enter", FolderTreeOpen, Some("FolderTree")),
    ]);
}

#[derive(Clone, Copy)]
pub(super) enum TreeKey {
    Up,
    Down,
    Home,
    End,
    Left,
    Right,
    Open,
}

impl Shell {
    pub(in crate::shell) fn install_folder_tree_focus(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.organization.tree_subscriptions.clear();
        self.organization.tree_active = false;
        let focus = self.organization.tree_focus.clone();
        let gained = cx.on_focus(&focus, w, |s, w, cx| {
            s.organization.tree_active = true;
            s.focus_folder_tree(w, cx);
        });
        let lost = cx.on_blur(&focus, w, |s, _, cx| {
            s.organization.tree_active = false;
            cx.notify();
        });
        self.organization.tree_subscriptions = vec![gained, lost];
    }
    fn preserve_focused_ancestors(&mut self) {
        let Some(live) = &self.live else {
            return;
        };
        let mut parent = self
            .organization
            .focused_folder
            .as_ref()
            .and_then(|id| live.folders.iter().find(|f| &f.id == id))
            .and_then(|f| f.parent.clone());
        let mut seen = HashSet::new();
        while let Some(id) = parent {
            if !seen.insert(id.clone()) {
                break;
            }
            self.organization.view.preserve_expanded(&id);
            parent = live
                .folders
                .iter()
                .find(|f| f.id == id)
                .and_then(|f| f.parent.clone());
        }
    }
    pub(in crate::shell) fn focus_folder_tree(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.live.is_none() || self.organization.open() {
            return;
        }
        if self.organization.focused_folder.is_none() {
            self.organization.focused_folder = match &self.library.scope {
                Scope::FolderId(Some(id))
                    if self.organization.rows.iter().any(|r| &r.folder.id == id) =>
                {
                    Some(id.clone())
                }
                _ => self.organization.rows.first().map(|r| r.folder.id.clone()),
            };
        }
        self.focused = false;
        self.folders_open = true;
        self.organization.tree_active = true;
        self.preserve_focused_ancestors();
        self.organization.tree_focus.focus(w, cx);
        self.reveal_folder_focus();
        cx.notify();
    }
    pub(super) fn reveal_folder_focus(&self) {
        if let Some(index) = self.organization.focused_folder.as_ref().and_then(|id| {
            self.organization
                .rows
                .iter()
                .position(|r| &r.folder.id == id)
        }) {
            self.organization
                .nav
                .scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }
    fn focus_folder_index(&mut self, index: usize, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.organization.rows.get(index) {
            self.organization.focused_folder = Some(row.folder.id.clone());
            self.preserve_focused_ancestors();
            self.organization.tree_focus.focus(w, cx);
            self.reveal_folder_focus();
            cx.notify();
        }
    }
    pub(super) fn folder_tree_key(&mut self, key: TreeKey, w: &mut Window, cx: &mut Context<Self>) {
        if self.live.is_none() || self.organization.open() || self.organization.busy() {
            return;
        }
        let Some(index) = self
            .organization
            .focused_folder
            .as_ref()
            .and_then(|id| {
                self.organization
                    .rows
                    .iter()
                    .position(|r| &r.folder.id == id)
            })
            .or_else(|| (!self.organization.rows.is_empty()).then_some(0))
        else {
            return;
        };
        let row = self.organization.rows[index].clone();
        match key {
            TreeKey::Up => self.focus_folder_index(index.saturating_sub(1), w, cx),
            TreeKey::Down => {
                self.focus_folder_index((index + 1).min(self.organization.rows.len() - 1), w, cx)
            }
            TreeKey::Home => self.focus_folder_index(0, w, cx),
            TreeKey::End => self.focus_folder_index(self.organization.rows.len() - 1, w, cx),
            TreeKey::Left
                if row.has_children && !self.organization.collapsed.contains(&row.folder.id) =>
            {
                self.organization.focused_folder = Some(row.folder.id.clone());
                self.organize(Action::Collapse(row.folder.id), w, cx);
                self.reveal_folder_focus();
            }
            TreeKey::Left => {
                if let Some(parent) = row.folder.parent.and_then(|id| {
                    self.organization
                        .rows
                        .iter()
                        .position(|r| r.folder.id == id)
                }) {
                    self.focus_folder_index(parent, w, cx);
                }
            }
            TreeKey::Right
                if row.has_children && self.organization.collapsed.contains(&row.folder.id) =>
            {
                self.organize(Action::Collapse(row.folder.id), w, cx);
                self.reveal_folder_focus();
            }
            TreeKey::Right if row.has_children => {
                if self
                    .organization
                    .rows
                    .get(index + 1)
                    .is_some_and(|child| child.depth > row.depth)
                {
                    self.focus_folder_index(index + 1, w, cx);
                }
            }
            TreeKey::Right => {}
            TreeKey::Open => {
                self.organization.focused_folder = Some(row.folder.id.clone());
                self.command(Command::Scope(Scope::FolderId(Some(row.folder.id))), w, cx);
                self.organization.tree_focus.focus(w, cx);
                self.reveal_folder_focus();
            }
        }
        cx.stop_propagation();
    }
    pub(super) fn click_folder(&mut self, id: String, w: &mut Window, cx: &mut Context<Self>) {
        self.organization.focused_folder = Some(id.clone());
        self.preserve_focused_ancestors();
        self.command(Command::Scope(Scope::FolderId(Some(id))), w, cx);
        self.organization.tree_focus.focus(w, cx);
        cx.notify();
    }
}
