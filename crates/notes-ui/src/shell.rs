#[cfg(feature = "performance-diagnostics")]
#[path = "performance.rs"]
mod performance;
use crate::{
    search::{SearchEvent, SearchField},
    theme::{self, Appearance, Theme},
};
use gpui::{prelude::*, *};
use notes_core::preview::{Block, Library, Scope};
#[cfg(feature = "performance-diagnostics")]
pub use performance::performance_check;
use std::{collections::HashMap, time::Duration};
#[path = "library_query.rs"]
mod library_query;
#[path = "library_recovery.rs"]
mod library_recovery;
#[path = "portability.rs"]
mod portability;
#[cfg(feature = "visual-tests")]
pub use library_query::library_query_check;
#[cfg(feature = "visual-tests")]
pub use library_recovery::library_recovery_check;
#[cfg(feature = "visual-tests")]
pub use portability::portability_check;
#[path = "organization.rs"]
mod organization;
#[cfg(feature = "visual-tests")]
pub use organization::organization_check;
#[path = "quick_note.rs"]
mod quick;
#[path = "settings.rs"]
pub(crate) mod settings;
#[path = "window_launch.rs"]
mod window_launch;
#[cfg(feature = "visual-tests")]
pub use window_launch::window_launch_check;
#[path = "writing.rs"]
mod writing;
pub(crate) use quick::QuickView;
#[cfg(feature = "visual-tests")]
pub use quick::quick_note_check;
#[cfg(feature = "visual-tests")]
pub use writing::{
    bounded_editor_check, context_check, link_check, media_check, pdf_ui_check, quote_check,
    table_check, writing_check, writing_tools_check,
};

actions!(
    notes,
    [
        FocusSearch,
        ToggleFocus,
        NextFocus,
        PreviousFocus,
        Escape,
        NextNote,
        PreviousNote,
        NewNote,
        QuickNote,
        OpenQuickInNotes,
        SaveNotes,
        ExportPdf,
        OpenSettings,
        NewFolder,
        MoveNoteFolder,
        RenameFolder,
        CloseNotes
    ]
);
pub fn bind_keys(cx: &mut App) {
    organization::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("ctrl-n", NewNote, Some("Notes")),
        KeyBinding::new("ctrl-,", OpenSettings, Some("Notes")),
        KeyBinding::new("ctrl-alt-n", NewFolder, Some("Notes")),
        KeyBinding::new("ctrl-alt-m", MoveNoteFolder, Some("Notes")),
        KeyBinding::new("f2", RenameFolder, Some("Notes")),
        KeyBinding::new("ctrl-shift-n", QuickNote, Some("Notes")),
        KeyBinding::new("ctrl-shift-o", OpenQuickInNotes, Some("QuickNote")),
        KeyBinding::new("ctrl-w", CloseNotes, Some("QuickNote")),
        KeyBinding::new("ctrl-q", CloseNotes, Some("QuickNote")),
        KeyBinding::new("ctrl-s", SaveNotes, Some("QuickNote")),
        KeyBinding::new("ctrl-s", SaveNotes, Some("Notes")),
        KeyBinding::new("ctrl-shift-p", ExportPdf, Some("Notes")),
        KeyBinding::new("ctrl-q", CloseNotes, Some("Notes")),
        KeyBinding::new("ctrl-k", FocusSearch, Some("Notes")),
        KeyBinding::new("ctrl-shift-f", FocusSearch, Some("Notes")),
        KeyBinding::new("ctrl-\\", ToggleFocus, Some("Notes")),
        KeyBinding::new("tab", NextFocus, Some("Notes")),
        KeyBinding::new("shift-tab", PreviousFocus, Some("Notes")),
        KeyBinding::new("escape", Escape, Some("Notes")),
        KeyBinding::new("down", NextNote, Some("Notes")),
        KeyBinding::new("up", PreviousNote, Some("Notes")),
    ]);
}

#[derive(Clone)]
enum Command {
    LibraryRecovery(library_recovery::Action),
    Portability(portability::Action),
    Organize(organization::Action),
    New,
    Quick,
    Save,
    Recover,
    ExportRecovery,
    PdfPanel,
    PdfExport,
    PdfPaper(notes_export::Paper),
    PdfCancel,
    PdfOpen,
    UpgradePanel,
    UpgradeChoose,
    UpgradeStart,
    UpgradeCancel,
    CloseRecovered,
    Scope(Scope),
    Select(String),
    Pin,
    Check(String, usize),
    Focus,
    Folders,
    Back,
    LinkBack,
    Settings,
    Appearance(Appearance),
    Preference(notes_core::preferences::Patch),
    RetryPreferences,
    Sort,
    MoreResults,
    RetryQuery,
    Minimize,
    Maximize,
    Close,
}

#[derive(Clone, Copy)]
enum Divider {
    Sidebar,
    List,
}
impl Render for Divider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

pub struct Shell {
    live: Option<writing::Writing>,
    windows: quick::Windows,
    launches: window_launch::State,
    preferences: settings::State,
    organization: organization::State,
    query: library_query::State,
    library_recovery: library_recovery::State,
    portability: portability::State,
    _write_task: Option<Task<()>>,
    library: Library,
    theme: Theme,
    omarchy: Theme,
    appearance: Appearance,
    search: Entity<SearchField>,
    focus: FocusHandle,
    list: ListState,
    scrolls: HashMap<String, ScrollHandle>,
    focused: bool,
    folders_open: bool,
    reading: bool,
    settings_open: bool,
    sidebar_width: f32,
    list_width: f32,
    _subscriptions: Vec<Subscription>,
    _theme_task: Task<()>,
}

fn icon(name: &str, color: Hsla, size: f32) -> Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(size))
        .text_color(color)
        .flex_none()
}
fn label(text: impl Into<SharedString>, theme: Theme) -> Div {
    div()
        .text_size(px(11.))
        .text_color(theme.muted)
        .child(text.into())
}
fn keycap(text: &'static str, theme: Theme) -> Div {
    div()
        .px(px(6.))
        .py(px(2.))
        .rounded(px(4.))
        .bg(theme.overlay)
        .text_size(px(11.))
        .text_color(theme.muted)
        .child(text)
}

impl Shell {
    pub fn new(appearance: Appearance, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(SearchField::new);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let subscription = Self::search_subscription(&search, window, cx);
        let dirs = theme::theme_directories();
        let task = cx.spawn(async move |this, cx| {
            loop {
                let dirs = dirs.clone();
                let palette = cx
                    .background_executor()
                    .spawn(async move { theme::read_omarchy(&dirs) })
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if this.omarchy != palette {
                            this.omarchy = palette;
                            if this.appearance == Appearance::Omarchy {
                                this.apply_theme(cx);
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        });
        let library = Library::sample();
        let list = ListState::new(library.visible.len(), ListAlignment::Top, px(240.));
        let mut this = Self {
            live: None,
            windows: quick::Windows::default(),
            launches: window_launch::State::default(),
            preferences: settings::State::memory(appearance),
            organization: organization::State::new(cx),
            query: library_query::State::default(),
            library_recovery: library_recovery::State::new(cx),
            portability: portability::State::default(),
            _write_task: None,
            library,
            theme: Theme::reference(),
            omarchy: Theme::reference(),
            appearance,
            search,
            focus,
            list,
            scrolls: HashMap::new(),
            focused: false,
            folders_open: false,
            reading: false,
            settings_open: false,
            sidebar_width: 248.,
            list_width: 352.,
            _subscriptions: vec![subscription],
            _theme_task: task,
        };
        this.preferences.focus = (0..16).map(|_| cx.focus_handle()).collect();
        let organization_subscription = this.organization_subscription(cx);
        this._subscriptions.push(organization_subscription);
        this.apply_theme(cx);
        this
    }

    fn search_subscription(
        search: &Entity<SearchField>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(search, window, |this, _, event, window, cx| {
            match event {
                SearchEvent::Changed(query) => {
                    if this.live.is_some() {
                        this.library.query = query.clone();
                        this.queue_library_query(true, cx);
                    } else {
                        this.library.search(query.clone());
                        this.list.reset(this.library.visible.len());
                    }
                    this.reading = false;
                }
                SearchEvent::Next => {
                    this.library.move_selection(true);
                    this.activate_selected_media(window, cx);
                }
                SearchEvent::Previous => {
                    this.library.move_selection(false);
                    this.activate_selected_media(window, cx);
                }
                SearchEvent::Accept => {
                    if !this
                        .library
                        .visible
                        .iter()
                        .any(|&i| Some(&this.library.notes[i].id) == this.library.selected.as_ref())
                    {
                        this.library.move_selection(true);
                    }
                    if !this.library.visible.is_empty()
                        && let Some(id) = this.library.selected.clone()
                    {
                        this.command(Command::Select(id), window, cx);
                    } else {
                        window.focus(&this.focus, cx);
                    }
                }
                SearchEvent::Cancel => {
                    this.search.update(cx, |s, cx| s.clear(window, cx));
                    window.focus(&this.focus, cx);
                }
            }
            cx.notify();
        })
    }
    pub(super) fn bind_main(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.windows.main = Some(window.window_handle());
        self.install_folder_tree_focus(window, cx);
        self._subscriptions[0] = Self::search_subscription(&self.search, window, cx);
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |w, cx| {
            weak.update(cx, |s, cx| s.request_close(w, cx))
                .unwrap_or(true)
        });
        self.activate_selected_media(window, cx);
    }

    fn apply_theme(&mut self, cx: &mut Context<Self>) {
        self.theme = match self.appearance {
            Appearance::Reference => Theme::reference(),
            Appearance::Light => Theme::light(),
            Appearance::Omarchy => self.omarchy,
        };
        self.search.update(cx, |s, cx| {
            s.theme = self.theme;
            cx.notify();
        });
        self.organization.field.update(cx, |s, cx| {
            s.theme = self.theme;
            cx.notify();
        });
        if let Some(live) = &self.live {
            for session in live.notes.values() {
                if let Some(editor) = session.active_editor() {
                    editor.update(cx, |e, cx| e.set_theme(self.theme, cx));
                }
            }
        }
        cx.notify();
    }

    fn command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if (self.library_recovery.blocks_mutations() || self.portability.busy())
            && matches!(
                &command,
                Command::New
                    | Command::Recover
                    | Command::Organize(_)
                    | Command::UpgradeChoose
                    | Command::UpgradeStart
            )
        {
            self.library_recovery.error =
                Some("Finish the current library operation first. Your draft is retained.".into());
            cx.notify();
            return;
        }
        if self.organization.open()
            && !matches!(
                command,
                Command::Organize(_) | Command::Close | Command::Save
            )
        {
            return;
        }
        match command {
            Command::LibraryRecovery(action) => self.library_recovery_action(action, window, cx),
            Command::Portability(action) => {
                if self.library_recovery.blocks_mutations()
                    && !matches!(
                        action,
                        portability::Action::Close | portability::Action::Cancel
                    )
                {
                    self.library_recovery.error =
                        Some("Finish the current recovery operation first.".into());
                } else {
                    self.portability_command(action, window, cx);
                }
            }
            Command::Organize(action) => self.organize(action, window, cx),
            Command::UpgradePanel
            | Command::UpgradeChoose
            | Command::UpgradeStart
            | Command::UpgradeCancel => self.upgrade_command(command, window, cx),
            Command::Quick => self.open_quick(cx),
            Command::New => self.create_real_note(false, window, cx),
            Command::Save => self.save_real_notes(cx),
            Command::Recover => self.create_real_note(true, window, cx),
            Command::ExportRecovery => self.export_recovery(window, cx),
            Command::PdfPanel
            | Command::PdfExport
            | Command::PdfPaper(_)
            | Command::PdfCancel
            | Command::PdfOpen => self.pdf_command(command, window, cx),
            Command::CloseRecovered => {
                if self.can_close_recovered(cx) {
                    self.remove_main(window, cx);
                }
            }
            Command::Scope(scope) => {
                let query = std::mem::take(&mut self.library.query);
                self.library.set_scope(scope);
                self.library.query = query;
                self.queue_library_query(false, cx);
                self.activate_selected_media(window, cx);
                self.list.reset(self.library.visible.len());
                self.folders_open = false;
                self.reading = false;
                window.focus(&self.focus, cx);
            }
            Command::Select(id) => {
                if !self.take_quick_into_main(&id, window, cx) {
                    return;
                }
                self.library.select(&id);
                self.activate_selected_media(window, cx);
                self.reading = true;
                self.settings_open = false;
                if let Some(editor) = self
                    .live
                    .as_ref()
                    .and_then(|l| l.notes.get(&id))
                    .and_then(|s| s.active_editor())
                {
                    window.focus(&editor.read(cx).focus.clone(), cx);
                } else {
                    window.focus(&self.focus, cx);
                }
            }
            Command::Pin => {
                if self.live.is_some() {
                    if let Some(note) = self
                        .library
                        .notes
                        .iter_mut()
                        .find(|note| Some(&note.id) == self.library.selected.as_ref())
                    {
                        note.pinned = !note.pinned;
                    }
                    self.pin_real_note(cx);
                } else {
                    self.library.toggle_pin();
                    self.list.reset(self.library.visible.len());
                }
            }
            Command::Check(id, block) => self.library.toggle_check(&id, block),
            Command::Focus => {
                self.focused = !self.focused;
                self.change_preferences(
                    notes_core::preferences::Patch {
                        focus_mode: Some(self.focused),
                        ..Default::default()
                    },
                    cx,
                );
                self.reading = true;
                self.folders_open = false;
            }
            Command::Folders => {
                self.folders_open = !self.folders_open;
                self.focused = false;
                self.change_preferences(
                    notes_core::preferences::Patch {
                        focus_mode: Some(false),
                        ..Default::default()
                    },
                    cx,
                );
            }
            Command::LinkBack => self.back_from_link(window, cx),
            Command::Back => {
                self.reading = false;
                self.focused = false;
                self.change_preferences(
                    notes_core::preferences::Patch {
                        focus_mode: Some(false),
                        ..Default::default()
                    },
                    cx,
                );
                self.settings_open = false;
            }
            Command::Settings => {
                self.toggle_settings(window, cx);
            }
            Command::Appearance(appearance) => {
                self.preferences.appearance_override = None;
                self.change_preferences(
                    notes_core::preferences::Patch {
                        appearance: Some(appearance),
                        ..Default::default()
                    },
                    cx,
                );
                self.apply_preferences(cx);
            }
            Command::Preference(patch) => self.change_preferences(patch, cx),
            Command::RetryPreferences => {
                self.preferences.session.retry();
                self.poll_preferences(cx);
            }
            Command::MoreResults => self.more_library_results(cx),
            Command::RetryQuery => self.queue_library_query(false, cx),
            Command::Sort if self.live.is_some() => {
                self.query.sort = match self.query.sort {
                    notes_core::library::Sort::Edited => notes_core::library::Sort::Created,
                    notes_core::library::Sort::Created => notes_core::library::Sort::Title,
                    notes_core::library::Sort::Title => notes_core::library::Sort::Edited,
                };
                self.library.sort_by_title = self.query.sort == notes_core::library::Sort::Title;
                self.query.sort_loaded = true;
                self.set_folder_view_sort(self.query.sort, cx);
                self.queue_library_query(false, cx);
            }
            Command::Sort => {
                self.library.sort_by_title = !self.library.sort_by_title;
                self.change_preferences(
                    notes_core::preferences::Patch {
                        sort_by_title: Some(self.library.sort_by_title),
                        ..Default::default()
                    },
                    cx,
                );
                self.library.refresh();
                self.list.reset(self.library.visible.len());
            }
            Command::Minimize => window.minimize_window(),
            Command::Maximize => window.zoom_window(),
            Command::Close => {
                if self.live.is_some() {
                    self.request_close(window, cx);
                } else {
                    window.remove_window();
                }
            }
        }
        cx.notify();
    }

    fn control(
        &self,
        id: impl Into<ElementId>,
        name: impl Into<SharedString>,
        command: Command,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(name)
            .tab_index(0)
            .cursor_pointer()
            .border_1()
            .border_color(gpui::transparent_black())
            .rounded(px(6.))
            .hover(move |s| s.bg(theme.overlay))
            .active(move |s| s.bg(theme.selected))
            .focus_visible(move |s| s.border_color(theme.accent))
            .on_click(
                cx.listener(move |this, _, window, cx| this.command(command.clone(), window, cx)),
            )
    }

    fn icon_button(
        &self,
        id: &'static str,
        name: &'static str,
        glyph: &'static str,
        command: Command,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.control(id, name, command, cx)
            .flex()
            .items_center()
            .justify_center()
            .size(px(34.))
            .child(icon(glyph, self.theme.muted, 19.))
    }

    fn header(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(54.))
            .flex_none()
            .px(px(18.))
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.navigation)
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .flex_none()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                    .child(icon("note", theme.accent, 23.))
                    .when(width > 620., |d| {
                        d.child(
                            div()
                                .font_family("Noto Serif")
                                .text_size(px(18.))
                                .child("Notes"),
                        )
                    })
                    .child(
                        div()
                            .ml(px(6.))
                            .text_size(px(10.))
                            .text_color(theme.muted)
                            .child(if self.live.is_some() {
                                if env!("CARGO_PKG_VERSION").starts_with("0.") {
                                    "EARLY ACCESS"
                                } else {
                                    "RUST EDITION"
                                }
                            } else {
                                "PREVIEW"
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .w(px(if width < 700. { 230. } else { 420. }))
                    .min_w_0()
                    .px(px(10.))
                    .h(px(36.))
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(7.))
                    .bg(theme.editor)
                    .child(icon("search", theme.muted, 17.))
                    .child(self.search.clone())
                    .when(width > 700., |d| d.child(keycap("Ctrl K", theme))),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .flex_none()
                    .when(self.live.is_some(), |d| {
                        d.child(self.icon_button(
                            "new-note",
                            "New note (Ctrl N)",
                            "plus",
                            Command::New,
                            cx,
                        ))
                        .when(width > 700., |d| {
                            d.child(self.icon_button(
                                "quick-note",
                                "Quick Note (Ctrl Shift N)",
                                "note",
                                Command::Quick,
                                cx,
                            ))
                        })
                    })
                    .when(width > 600., |d| {
                        d.child(self.icon_button(
                            "minimize",
                            "Minimize window",
                            "minus",
                            Command::Minimize,
                            cx,
                        ))
                        .child(self.icon_button(
                            "maximize",
                            "Maximize or restore window",
                            "maximize",
                            Command::Maximize,
                            cx,
                        ))
                    })
                    .child(self.icon_button(
                        "close-window",
                        "Close preview",
                        "close",
                        Command::Close,
                        cx,
                    )),
            )
    }

    fn navigation_row(
        &self,
        id: &str,
        name: &str,
        glyph: &str,
        scope: Scope,
        count: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.library.scope == scope;
        let theme = self.theme;
        self.control(
            SharedString::from(id.to_owned()),
            SharedString::from(name.to_owned()),
            Command::Scope(scope),
            cx,
        )
        .aria_selected(selected)
        .flex()
        .items_center()
        .gap(px(12.))
        .h(px(42.))
        .px(px(12.))
        .text_size(px(14.))
        .when(selected, |d| d.bg(theme.selected))
        .child(icon(
            glyph,
            if selected { theme.accent } else { theme.text },
            20.,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .child(name.to_owned()),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme.muted)
                .child(count.to_string()),
        )
    }

    fn sidebar(&self, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        div()
            .id("library-navigation")
            .role(Role::Navigation)
            .aria_label("Library")
            .flex()
            .flex_col()
            .w(px(width))
            .h_full()
            .flex_none()
            .bg(theme.navigation)
            .px(px(16.))
            .pt(px(26.))
            .pb(px(16.))
            .child(label("LIBRARY", theme).px(px(10.)).mb(px(12.)))
            .child(self.navigation_row(
                "all",
                "All notes",
                "all-notes",
                Scope::All,
                self.library.notes.len(),
                cx,
            ))
            .child(self.navigation_row(
                "favorites",
                "Favorites",
                "star",
                Scope::Favorites,
                self.library.notes.iter().filter(|n| n.pinned).count(),
                cx,
            ))
            .when(self.live.is_some(), |d| {
                d.child(
                    self.control(
                        "recently-deleted",
                        "Recently Deleted",
                        Command::LibraryRecovery(library_recovery::Action::OpenTrash),
                        cx,
                    )
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .h(px(42.))
                    .px(px(12.))
                    .child(icon("all-notes", theme.muted, 20.))
                    .child("Recently Deleted"),
                )
            })
            .child(div().my(px(20.)).mx(px(10.)).h(px(1.)).bg(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(10.))
                    .px(px(10.))
                    .child(label("FOLDERS", theme))
                    .when(self.live.is_some(), |d| {
                        d.child(
                            self.control(
                                "new-folder",
                                "New folder (Ctrl Alt N)",
                                Command::Organize(organization::Action::New),
                                cx,
                            )
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("plus", theme.muted, 18.)),
                        )
                    }),
            )
            .child(self.folder_navigation(cx))
            .child(
                self.control(
                    "settings",
                    "Notebook settings (Ctrl ,)",
                    Command::Settings,
                    cx,
                )
                .flex()
                .items_center()
                .h(px(42.))
                .gap(px(12.))
                .px(px(12.))
                .child(icon("sun", theme.muted, 20.))
                .child("Settings"),
            )
            .child(
                label(
                    if self.live.is_some() {
                        "LOCAL LIBRARY · RUST EDITION"
                    } else {
                        "SAMPLE LIBRARY · LOCAL PREVIEW"
                    },
                    theme,
                )
                .text_size(px(9.))
                .px(px(10.))
                .mt(px(10.)),
            )
    }

    fn list_pane(&self, width: f32, show_folders: bool, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .w(px(width))
            .min_w_0()
            .h_full()
            .flex_none()
            .bg(theme.list)
            .child(
                div()
                    .px(px(24.))
                    .pt(px(20.))
                    .pb(px(14.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .gap(px(10.))
                            .items_center()
                            .when(show_folders, |d| {
                                d.child(self.icon_button(
                                    "show-folders",
                                    "Show folders",
                                    "sidebar",
                                    Command::Folders,
                                    cx,
                                ))
                            })
                            .child(
                                div()
                                    .child(
                                        div()
                                            .font_family("Noto Serif")
                                            .font_weight(FontWeight::BOLD)
                                            .text_size(px(23.))
                                            .child(self.scope_title()),
                                    )
                                    .child(
                                        div()
                                            .mt(px(4.))
                                            .text_size(px(12.))
                                            .text_color(theme.muted)
                                            .child(format!(
                                                "{} {}",
                                                self.library.visible.len(),
                                                if self.library.visible.len() == 1 {
                                                    "note"
                                                } else {
                                                    "notes"
                                                }
                                            )),
                                    ),
                            ),
                    )
                    .when(self.library.query.is_empty(), |d| {
                        d.child(
                            self.control(
                                "sort",
                                format!(
                                    "Sort: {}. Activate to change sorting.",
                                    self.library_sort_label()
                                ),
                                Command::Sort,
                                cx,
                            )
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(8.))
                            .py(px(6.))
                            .text_size(px(12.))
                            .child(icon("sort", theme.muted, 16.))
                            .child(self.library_sort_label()),
                        )
                    })
                    .when(!self.library.query.is_empty(), |d| {
                        d.child(label("By relevance", theme))
                    }),
            )
            .child(div().mx(px(24.)).h(px(1.)).bg(theme.border))
            .when(
                self.live.is_some() && matches!(self.library.scope, Scope::FolderId(Some(_))),
                |d| {
                    d.child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .px(px(20.))
                            .py(px(8.))
                            .child(
                                self.control(
                                    "rename-folder",
                                    "Rename this folder (F2)",
                                    Command::Organize(organization::Action::Rename),
                                    cx,
                                )
                                .px(px(8.))
                                .py(px(4.))
                                .text_size(px(12.))
                                .child("Rename"),
                            )
                            .child(
                                self.control(
                                    "move-folder",
                                    "Move this folder",
                                    Command::Organize(organization::Action::MoveFolder),
                                    cx,
                                )
                                .px(px(8.))
                                .py(px(4.))
                                .text_size(px(12.))
                                .child("Move folder…"),
                            )
                            .child(
                                self.control(
                                    "new-subfolder",
                                    "Create a subfolder",
                                    Command::Organize(organization::Action::New),
                                    cx,
                                )
                                .px(px(8.))
                                .py(px(4.))
                                .text_size(px(12.))
                                .child("New inside…"),
                            ),
                    )
                },
            )
            .child(
                div()
                    .id("note-list")
                    .role(Role::ListBox)
                    .aria_label("Notes")
                    .flex_1()
                    .min_h_0()
                    .when(self.library.visible.is_empty(), |d| {
                        d.child(
                            div()
                                .p(px(28.))
                                .text_color(theme.muted)
                                .text_size(px(14.))
                                .child(if self.query.loading {
                                    "Searching…"
                                } else if self.query.error.is_some() {
                                    "Search needs attention."
                                } else if self.library.query.is_empty() {
                                    "No notes here yet."
                                } else {
                                    "No notes found."
                                })
                                .child(div().mt(px(8.)).text_size(px(12.)).child(
                                    if self.query.loading {
                                        "Your open note stays available."
                                    } else if self.query.error.is_some() {
                                        "Retry below, or narrow your search."
                                    } else if self.library.query.is_empty() {
                                        "New notes will appear here."
                                    } else {
                                        "Try another word, or clear the search."
                                    },
                                )),
                        )
                    })
                    .when(!self.library.visible.is_empty(), |d| {
                        d.child(
                            list(
                                self.list.clone(),
                                cx.processor(|this, ix, _, cx| {
                                    this.note_row(ix, cx).into_any_element()
                                }),
                            )
                            .size_full(),
                        )
                    }),
            )
            .when(
                self.query.loading || self.query.error.is_some() || self.query.has_more,
                |d| {
                    d.child(
                        div()
                            .flex_none()
                            .px(px(20.))
                            .py(px(10.))
                            .border_t_1()
                            .border_color(theme.border)
                            .when(self.query.loading, |d| {
                                d.child(
                                    label("Updating results…", theme)
                                        .id("library-query-status")
                                        .role(Role::Status),
                                )
                            })
                            .when_some(self.query.error.clone(), |d, error| {
                                d.child(
                                    label(error, theme)
                                        .id("library-query-error")
                                        .role(Role::Status),
                                )
                                .child(
                                    self.control(
                                        "retry-search",
                                        "Retry search",
                                        Command::RetryQuery,
                                        cx,
                                    )
                                    .px(px(8.))
                                    .py(px(6.))
                                    .child("Retry"),
                                )
                            })
                            .when(self.query.has_more && !self.query.loading, |d| {
                                d.child(
                                    self.control(
                                        "load-more-notes",
                                        "Load more notes",
                                        Command::MoreResults,
                                        cx,
                                    )
                                    .w_full()
                                    .px(px(8.))
                                    .py(px(6.))
                                    .child("Load more"),
                                )
                            }),
                    )
                },
            )
    }

    fn note_row(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let theme = self.theme;
        let note = &self.library.notes[self.library.visible[index]];
        let summary = if self.query.enabled && !self.library.query.is_empty() {
            self.query
                .snippets
                .get(&note.id)
                .map(String::as_str)
                .unwrap_or(&note.summary)
        } else {
            &note.summary
        };
        let selected = self.library.selected.as_ref() == Some(&note.id);
        let group = if note.pinned {
            "PINNED"
        } else if !self.library.query.is_empty() {
            "RESULTS"
        } else if self.library.sort_by_title {
            "BY TITLE"
        } else {
            &note.day
        };
        let previous = index
            .checked_sub(1)
            .map(|i| &self.library.notes[self.library.visible[i]]);
        let show_group = previous.is_none_or(|p| {
            p.pinned != note.pinned
                || (!note.pinned
                    && !self.library.sort_by_title
                    && self.library.query.is_empty()
                    && p.day != note.day)
        });
        div()
            .px(px(8.))
            .when(show_group, |d| {
                d.child(
                    label(group.to_uppercase(), theme)
                        .mx(px(16.))
                        .mt(px(18.))
                        .mb(px(8.)),
                )
            })
            .child(
                self.control(
                    SharedString::from(format!("note-{}", note.id)),
                    SharedString::from(note.title.clone()),
                    Command::Select(note.id.clone()),
                    cx,
                )
                .role(Role::ListBoxOption)
                .aria_selected(selected)
                .w_full()
                .min_w_0()
                .rounded(px(7.))
                .border_0()
                .border_l(px(3.))
                .border_color(if selected {
                    theme.accent
                } else {
                    gpui::transparent_black()
                })
                .focus_visible(move |d| d.border_1().border_l(px(3.)).border_color(theme.accent))
                .when(selected, |d| d.bg(theme.selected))
                .px(px(16.))
                .py(px(15.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .when(note.pinned, |d| d.child(icon("pin", theme.accent, 16.)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family("Noto Serif")
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(16.))
                                .child(note.title.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme.muted)
                                .flex_none()
                                .child(note.time.clone()),
                        ),
                )
                .child(
                    div()
                        .mt(px(5.))
                        .truncate()
                        .text_size(px(12.))
                        .text_color(theme.muted)
                        .child(summary.to_owned()),
                )
                .child(
                    div()
                        .mt(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(11.))
                        .text_color(theme.muted)
                        .child(icon("folder", theme.muted, 14.))
                        .child(note.folder.clone()),
                ),
            )
            .when(!selected, |d| {
                d.child(div().mx(px(16.)).h(px(1.)).bg(theme.border))
            })
    }

    fn editor(&mut self, narrow: bool, height: f32, cx: &mut Context<Self>) -> Div {
        if self.live.is_some() {
            return self.real_editor(narrow, height, cx);
        }
        let theme = self.theme;
        let note = self.library.selected_note().cloned();
        let pane = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(theme.editor);
        let Some(note) = note else {
            return pane.child(
                div()
                    .m_auto()
                    .text_color(theme.muted)
                    .child("Choose a note to settle into."),
            );
        };
        let scroll = self.scrolls.entry(note.id.clone()).or_default().clone();
        pane.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(64.))
                .flex_none()
                .px(px(if narrow { 16. } else { 32. }))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .text_size(px(13.))
                        .text_color(theme.muted)
                        .when(narrow || self.focused, |d| {
                            d.child(self.icon_button(
                                "back-to-notes",
                                "Back to notes",
                                "back",
                                Command::Back,
                                cx,
                            ))
                        })
                        .child(icon("folder", theme.muted, 18.))
                        .child(note.folder.clone())
                        .child(div().px(px(3.)).child("/"))
                        .child("Notes"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .child(
                            self.control(
                                "pin-note",
                                if note.pinned {
                                    "Unpin note"
                                } else {
                                    "Pin note"
                                },
                                Command::Pin,
                                cx,
                            )
                            .aria_toggled(if note.pinned {
                                Toggled::True
                            } else {
                                Toggled::False
                            })
                            .size(px(34.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(
                                "pin",
                                if note.pinned {
                                    theme.accent
                                } else {
                                    theme.muted
                                },
                                20.,
                            )),
                        )
                        .child(self.icon_button(
                            "focus-note",
                            "Toggle focus mode (Ctrl backslash)",
                            "focus",
                            Command::Focus,
                            cx,
                        ))
                        .child(self.icon_button(
                            "editor-settings",
                            "Notebook settings (Ctrl ,)",
                            "sun",
                            Command::Settings,
                            cx,
                        )),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!("document-{}", note.id)))
                .role(Role::Document)
                .aria_label(note.title.clone())
                .track_scroll(&scroll)
                .overflow_y_scroll()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .max_w(px(910.))
                        .mx_auto()
                        .px(px(if narrow { 26. } else { 64. }))
                        .pt(px(40.))
                        .pb(px(56.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .text_size(px(12.))
                                .text_color(theme.muted)
                                .mb(px(22.))
                                .child("October 12, 2026")
                                .child("·")
                                .child(icon("folder", theme.muted, 16.))
                                .child(note.folder.clone()),
                        )
                        .child(
                            div()
                                .id("document-title")
                                .role(Role::Heading)
                                .aria_level(1)
                                .aria_label(note.title.clone())
                                .font_family("Noto Serif")
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(if narrow { 32. } else { 41. }))
                                .line_height(relative(1.25))
                                .mb(px(22.))
                                .child(note.title.clone()),
                        )
                        .children(
                            note.blocks
                                .iter()
                                .enumerate()
                                .map(|(index, block)| self.block(&note.id, index, block, cx)),
                        )
                        .when_some(
                            note.related
                                .as_ref()
                                .and_then(|id| self.library.notes.iter().find(|n| &n.id == id)),
                            |d, target| {
                                d.child(
                                    self.control(
                                        "related-note",
                                        SharedString::from(format!(
                                            "Related note: {}",
                                            target.title
                                        )),
                                        Command::Select(target.id.clone()),
                                        cx,
                                    )
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.))
                                    .mt(px(28.))
                                    .p(px(14.))
                                    .border_color(theme.border)
                                    .child(icon("link", theme.muted, 19.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_size(px(12.))
                                            .child(
                                                div().text_color(theme.muted).child("Related note"),
                                            )
                                            .child(
                                                div()
                                                    .mt(px(3.))
                                                    .text_color(theme.accent)
                                                    .child(target.title.clone()),
                                            ),
                                    )
                                    .child(icon(
                                        "arrow",
                                        theme.muted,
                                        16.,
                                    )),
                                )
                            },
                        ),
                ),
        )
        .child(
            div()
                .h(px(44.))
                .flex_none()
                .px(px(20.))
                .border_t_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .justify_between()
                .child(
                    label("Sample library · session changes only", theme)
                        .id("preview-status")
                        .role(Role::Status),
                )
                .child(
                    self.control("footer-focus", "Toggle focus mode", Command::Focus, cx)
                        .flex()
                        .gap(px(6.))
                        .items_center()
                        .px(px(6.))
                        .py(px(3.))
                        .child(icon("focus", theme.muted, 15.))
                        .child(label(
                            if self.focused { "Exit focus" } else { "Focus" },
                            theme,
                        )),
                ),
        )
    }

    fn block(
        &self,
        note_id: &str,
        index: usize,
        block: &Block,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.theme;
        let text = block.text().to_owned();
        let base = div()
            .font_family("Noto Serif")
            .text_size(px(19.))
            .line_height(relative(1.55));
        match block {
            Block::Lead { .. } => base
                .text_size(px(22.))
                .mb(px(24.))
                .child(text)
                .into_any_element(),
            Block::Paragraph { .. } => base.mb(px(25.)).child(text).into_any_element(),
            Block::Heading { .. } => base
                .id(SharedString::from(format!("heading-{index}")))
                .role(Role::Heading)
                .aria_level(2)
                .aria_label(text.clone())
                .font_weight(FontWeight::BOLD)
                .text_size(px(26.))
                .mt(px(12.))
                .mb(px(14.))
                .child(text)
                .into_any_element(),
            Block::Quote { .. } => base
                .mt(px(28.))
                .pl(px(20.))
                .py(px(2.))
                .border_l(px(2.))
                .border_color(theme.accent)
                .italic()
                .child(text)
                .into_any_element(),
            Block::Check { checked, .. } => self
                .control(
                    SharedString::from(format!("check-{note_id}-{index}")),
                    SharedString::from(text.clone()),
                    Command::Check(note_id.to_owned(), index),
                    cx,
                )
                .role(Role::CheckBox)
                .aria_toggled(if *checked {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .flex()
                .items_start()
                .gap(px(12.))
                .py(px(3.))
                .ml(px(-5.))
                .px(px(4.))
                .child(
                    div()
                        .size(px(21.))
                        .flex_none()
                        .mt(px(4.))
                        .rounded(px(3.))
                        .border_1()
                        .border_color(if *checked { theme.accent } else { theme.muted })
                        .when(*checked, |d| {
                            d.bg(theme.accent)
                                .child(icon("check", theme.on_accent, 19.))
                        }),
                )
                .child(base.child(text))
                .into_any_element(),
        }
    }

    fn splitter(&self, kind: Divider, _cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        div()
            .id(match kind {
                Divider::Sidebar => "sidebar-divider",
                Divider::List => "list-divider",
            })
            .w(px(3.))
            .h_full()
            .flex_none()
            .bg(theme.navigation)
            .border_l_1()
            .border_color(theme.border)
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(move |s| s.bg(theme.accent))
            .on_drag(kind, |kind, _, _, cx| cx.new(|_| *kind))
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width: f32 = window.viewport_size().width.into();
        let text_extra = (self.preferences.session.value.note_size as f32 - 19.).max(0.);
        let wide = width >= 1200. + text_extra * 12.;
        let narrow = width < 850. + text_extra * 10.;
        let theme = self.theme;
        let library_page = self.library_recovery.open
            || self.library_recovery.confirm.is_some()
            || self.portability.open;
        let show_sidebar = !library_page
            && !self.organization.open()
            && !self.focused
            && !self.settings_open
            && (wide || self.folders_open);
        let show_list = !library_page
            && !self.focused
            && !self.organization.open()
            && !self.settings_open
            && (!narrow || !self.reading)
            && !(self.folders_open && !wide);
        let show_editor = !library_page
            && !self.settings_open
            && !self.organization.open()
            && (self.focused || (!narrow || self.reading))
            && !(self.folders_open && !wide);
        let side = self.sidebar_width.min(width - 420.);
        let list_width = if narrow {
            width
        } else {
            self.list_width
                .min((width - if show_sidebar { side } else { 0. }) * 0.46)
        };
        div()
            .id("notes-shell")
            .role(Role::Window)
            .aria_label("Omarchy Notes")
            .key_context("Notes")
            .tab_group()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.editor)
            .text_color(theme.text)
            .font_family("Noto Sans")
            .text_size(px(14.))
            .on_action(
                cx.listener(|s, _: &OpenSettings, w, cx| s.command(Command::Settings, w, cx)),
            )
            .on_action(cx.listener(|s, _: &NewFolder, w, cx| {
                s.command(Command::Organize(organization::Action::New), w, cx)
            }))
            .on_action(cx.listener(|s, _: &MoveNoteFolder, w, cx| {
                s.command(Command::Organize(organization::Action::MoveNote), w, cx)
            }))
            .on_action(cx.listener(|s, _: &RenameFolder, w, cx| {
                s.command(Command::Organize(organization::Action::Rename), w, cx)
            }))
            .on_drop::<Divider>(cx.listener(|s, _, _, cx| s.finish_pane_resize(cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| s.finish_pane_resize(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|s, _, _, cx| s.finish_pane_resize(cx)),
            )
            .on_action(
                cx.listener(|s, _: &NewNote, window, cx| s.command(Command::New, window, cx)),
            )
            .on_action(
                cx.listener(|s, _: &ExportPdf, window, cx| {
                    s.command(Command::PdfPanel, window, cx)
                }),
            )
            .on_action(
                cx.listener(|s, _: &SaveNotes, window, cx| s.command(Command::Save, window, cx)),
            )
            .on_action(
                cx.listener(|s, _: &CloseNotes, window, cx| s.command(Command::Close, window, cx)),
            )
            .on_action(cx.listener(|s, _: &FocusSearch, window, cx| {
                if s.organization.open()
                    || s.library_recovery.open
                    || s.library_recovery.confirm.is_some()
                    || s.portability.open
                {
                    return;
                }
                s.focused = false;
                s.change_preferences(
                    notes_core::preferences::Patch {
                        focus_mode: Some(false),
                        ..Default::default()
                    },
                    cx,
                );
                s.settings_open = false;
                s.folders_open = false;
                s.reading = false;
                window.focus(&s.search.read(cx).focus.clone(), cx);
                cx.notify();
            }))
            .on_action(
                cx.listener(|s, _: &ToggleFocus, window, cx| s.command(Command::Focus, window, cx)),
            )
            .on_action(
                cx.listener(|s, _: &organization::FocusFolderTree, w, cx| {
                    s.focus_folder_tree(w, cx)
                }),
            )
            .on_action(cx.listener(|s, _: &NextFocus, window, cx| {
                if s.library_recovery.open
                    || s.library_recovery.confirm.is_some()
                    || s.library_recovery.actions_open
                {
                    s.recovery_focus(false, window, cx);
                } else if s.portability.open {
                    s.portability_focus(false, window, cx);
                } else if s.organization.open() {
                    s.organization_tab(false, window, cx);
                } else if s.settings_open {
                    s.move_settings_focus(false, window, cx);
                } else {
                    window.focus_next(cx);
                }
            }))
            .on_action(cx.listener(|s, _: &PreviousFocus, window, cx| {
                if s.library_recovery.open
                    || s.library_recovery.confirm.is_some()
                    || s.library_recovery.actions_open
                {
                    s.recovery_focus(true, window, cx);
                } else if s.portability.open {
                    s.portability_focus(true, window, cx);
                } else if s.organization.open() {
                    s.organization_tab(true, window, cx);
                } else if s.settings_open {
                    s.move_settings_focus(true, window, cx);
                } else {
                    window.focus_prev(cx);
                }
            }))
            .on_action(cx.listener(|s, _: &Escape, window, cx| {
                if s.library_recovery.confirm.is_some() {
                    s.library_recovery_action(library_recovery::Action::Cancel, window, cx);
                    return;
                }
                if s.library_recovery.open {
                    s.library_recovery_action(library_recovery::Action::CloseTrash, window, cx);
                    return;
                }
                if s.library_recovery.actions_open {
                    s.library_recovery_action(library_recovery::Action::CloseActions, window, cx);
                    return;
                }
                if s.organization.open() {
                    s.organize(organization::Action::Close, window, cx);
                    return;
                }
                if s.preferences.dragging {
                    s.preferences.dragging = false;
                    s.apply_preferences(cx);
                    return;
                }
                if s.live.as_ref().is_some_and(|l| l.upgrade.open) {
                    s.command(Command::UpgradeCancel, window, cx);
                    return;
                }
                if s.portability.open {
                    s.command(Command::Portability(portability::Action::Close), window, cx);
                    return;
                }
                if s.live.as_ref().is_some_and(|l| l.pdf.open) {
                    s.command(Command::PdfCancel, window, cx);
                    return;
                }
                if s.settings_open {
                    s.close_settings(window, cx);
                    return;
                } else if s.folders_open {
                    s.folders_open = false;
                } else {
                    s.focused = false;
                    s.change_preferences(
                        notes_core::preferences::Patch {
                            focus_mode: Some(false),
                            ..Default::default()
                        },
                        cx,
                    );
                    s.reading = false;
                }
                window.focus(&s.focus, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &QuickNote, _, cx| {
                if !s.organization.open() {
                    s.open_quick(cx)
                }
            }))
            .on_action(cx.listener(|s, _: &NextNote, w, cx| {
                if s.organization.open() {
                    s.organize(organization::Action::Step(true), w, cx);
                    return;
                }
                s.library.move_selection(true);
                s.activate_selected_media(w, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &PreviousNote, w, cx| {
                if s.organization.open() {
                    s.organize(organization::Action::Step(false), w, cx);
                    return;
                }
                s.library.move_selection(false);
                s.activate_selected_media(w, cx);
                cx.notify();
            }))
            .on_drag_move::<Divider>(cx.listener(
                move |s, event: &DragMoveEvent<Divider>, _, cx| {
                    s.preferences.dragging = true;
                    let x: f32 = event.event.position.x.into();
                    match event.drag(cx) {
                        Divider::Sidebar => s.sidebar_width = x.clamp(220., 300.),
                        Divider::List => {
                            s.list_width =
                                (x - if wide { s.sidebar_width } else { 0. }).clamp(300., 440.)
                        }
                    }
                    s.list.remeasure();
                    cx.notify();
                },
            ))
            .when(!library_page && !self.organization.open(), |d| {
                d.child(self.header(width, cx))
            })
            .when_some(
                self.live
                    .as_ref()
                    .and_then(|l| l.message.clone())
                    .filter(|_| self.library.selected.is_none()),
                |d, message| {
                    d.child(
                        div()
                            .px(px(24.))
                            .py(px(12.))
                            .text_color(theme.accent)
                            .child(message),
                    )
                },
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when(show_sidebar, |d| {
                        d.child(self.sidebar(if wide { side } else { width }, cx))
                            .when(wide, |d| d.child(self.splitter(Divider::Sidebar, cx)))
                    })
                    .when(show_list, |d| {
                        d.child(self.list_pane(list_width, !wide, cx))
                            .when(!narrow, |d| d.child(self.splitter(Divider::List, cx)))
                    })
                    .when(show_editor, |d| {
                        d.child(self.editor(narrow, window.viewport_size().height.into(), cx))
                    })
                    .when(
                        self.settings_open && !library_page && !self.organization.open(),
                        |d| d.child(self.settings(width, cx)),
                    )
                    .when(self.organization.open() && !library_page, |d| {
                        d.child(self.organization_page(cx))
                    })
                    .when(
                        self.library_recovery.open || self.library_recovery.confirm.is_some(),
                        |d| d.child(self.recovery_panel(cx)),
                    )
                    .when(
                        self.portability.open
                            && !self.library_recovery.open
                            && self.library_recovery.confirm.is_none(),
                        |d| d.child(self.portability_panel(cx)),
                    ),
            )
            .when(self.live.is_some() && !library_page, |d| {
                d.child(self.recovery_status())
                    .child(self.recovery_retained_controls(cx))
            })
    }
}

/// Real font shaping and GPU rasterization with a test window. No compositor
/// screenshots, global keyboard injection, or production library access.
#[cfg(feature = "visual-tests")]
pub fn visual_check(directory: &std::path::Path) -> anyhow::Result<()> {
    use std::sync::Arc;
    std::fs::create_dir_all(directory)?;
    let mut cx = HeadlessAppContext::with_platform(
        gpui_platform::current_platform(true).text_system(),
        Arc::new(crate::assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        crate::assets::load_fonts(cx).expect("bundled fonts load");
        bind_keys(cx);
        crate::search::bind_keys(cx);
    });
    let handle = cx.open_window(size(px(1440.), px(960.)), |window, cx| {
        cx.new(|cx| Shell::new(Appearance::Reference, window, cx))
    })?;
    let capture = |cx: &mut HeadlessAppContext,
                   handle: WindowHandle<Shell>,
                   name: &str|
     -> anyhow::Result<()> {
        cx.run_until_parked();
        let cleanup = cx.update_window(handle.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx)
        })?;
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
        cx.capture_screenshot(handle.into())?
            .save(directory.join(name))?;
        Ok(())
    };
    capture(&mut cx, handle, "dark.png")?;
    visual_keys(&mut cx, handle.into(), "ctrl-k")?;
    // Input registration takes place during paint.
    capture(&mut cx, handle, "search-focused.png")?;
    for character in "mountains".chars() {
        let key = character.to_string();
        let cleanup = cx.update_window(handle.into(), |_, window, cx| {
            window.dispatch_keystroke(
                Keystroke {
                    modifiers: Modifiers::default(),
                    key: key.clone(),
                    key_char: Some(key),
                },
                cx,
            );
            window.draw(cx)
        })?;
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
    }
    cx.run_until_parked();
    handle.update(&mut cx, |this, _, _| {
        assert_eq!(this.library.visible.len(), 1);
        assert_eq!(this.library.notes[this.library.visible[0]].id, "mountains");
    })?;
    capture(&mut cx, handle, "search.png")?;
    // Enter must open the first match even when the current document is absent
    // from the filtered results (no preliminary Down press required).
    visual_keys(&mut cx, handle.into(), "enter")?;
    handle.update(&mut cx, |this, _, _| {
        assert_eq!(this.library.selected.as_deref(), Some("mountains"))
    })?;
    visual_keys(&mut cx, handle.into(), "ctrl-\\")?;
    handle.update(&mut cx, |this, _, _| assert!(this.focused))?;
    capture(&mut cx, handle, "focus.png")?;
    visual_keys(&mut cx, handle.into(), "ctrl-k escape")?;
    handle.update(&mut cx, |this, window, cx| {
        this.command(Command::Back, window, cx);
        this.command(Command::Select("quiet".into()), window, cx);
        this.command(Command::Check("quiet".into(), 4), window, cx);
        assert!(matches!(
            this.library.notes[0].blocks[4],
            Block::Check { checked: false, .. }
        ));
        this.command(Command::Check("quiet".into(), 4), window, cx);
        this.command(Command::Appearance(Appearance::Light), window, cx);
    })?;
    capture(&mut cx, handle, "light.png")?;
    handle.update(&mut cx, |this, window, cx| {
        this.command(Command::Appearance(Appearance::Reference), window, cx);
        this.command(Command::Settings, window, cx);
    })?;
    capture(&mut cx, handle, "appearance.png")?;
    handle.update(&mut cx, |_, window, _| window.remove_window())?;
    // HeadlessAppContext has no platform resize-event helper. Open each size
    // directly; Window::resize alone does not update the test viewport.
    for (width, height, prefix) in [
        (1024., 820., "medium"),
        (640., 820., "narrow"),
        (420., 600., "small"),
    ] {
        let handle = cx.open_window(size(px(width), px(height)), |window, cx| {
            assert_eq!(window.viewport_size().width, px(width));
            cx.new(|cx| Shell::new(Appearance::Reference, window, cx))
        })?;
        capture(&mut cx, handle, &format!("{prefix}-list.png"))?;
        handle.update(&mut cx, |this, window, cx| {
            this.command(Command::Select("quiet".into()), window, cx);
        })?;
        capture(&mut cx, handle, &format!("{prefix}-note.png"))?;
        visual_keys(&mut cx, handle.into(), "escape")?;
        handle.update(&mut cx, |this, _, _| assert!(!this.reading))?;
        handle.update(&mut cx, |this, window, cx| {
            this.command(Command::Folders, window, cx);
        })?;
        capture(&mut cx, handle, &format!("{prefix}-folders.png"))?;
        handle.update(&mut cx, |this, window, cx| {
            this.command(Command::Settings, window, cx);
        })?;
        capture(&mut cx, handle, &format!("{prefix}-appearance.png"))?;
        handle.update(&mut cx, |_, window, _| window.remove_window())?;
    }
    cx.run_until_parked();
    println!(
        "Passed search → select → focus → escape, checklist routing, theme changes, and narrow layout. Images: {}",
        directory.display()
    );
    Ok(())
}

#[cfg(feature = "visual-tests")]
fn visual_keys(
    cx: &mut HeadlessAppContext,
    handle: AnyWindowHandle,
    keys: &str,
) -> anyhow::Result<()> {
    for key in keys.split_whitespace() {
        let keystroke = Keystroke::parse(key).expect("valid test key");
        let cleanup = cx.update_window(handle, |_, window, cx| {
            window.dispatch_keystroke(keystroke.clone(), cx);
            window.draw(cx)
        })?;
        cx.update(|cx| cleanup.clear(cx));
        cx.run_until_parked();
        // dispatch_keystroke sends only KeyDown. GPUI activates focused buttons
        // on KeyUp; include the real sequence so duplicate activation is caught.
        if cx.update(|cx| cx.windows().contains(&handle)) {
            let cleanup = cx.update_window(handle, |_, window, cx| {
                window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke }), cx);
                window.draw(cx)
            })?;
            cx.update(|cx| cleanup.clear(cx));
            cx.run_until_parked();
        }
    }
    Ok(())
}
