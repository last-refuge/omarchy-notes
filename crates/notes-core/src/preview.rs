//! Disposable sample data, deliberately separate from the persisted RichDocument schema.
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub folder: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    pub day: String,
    pub time: String,
    pub pinned: bool,
    pub blocks: Vec<Block>,
    pub related: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Lead { text: String },
    Paragraph { text: String },
    Heading { text: String },
    Check { text: String, checked: bool },
    Quote { text: String },
}

impl Block {
    pub fn text(&self) -> &str {
        match self {
            Self::Lead { text }
            | Self::Paragraph { text }
            | Self::Heading { text }
            | Self::Check { text, .. }
            | Self::Quote { text } => text,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Favorites,
    Folder(String),
    FolderId(Option<String>),
}

impl Scope {
    pub fn title(&self) -> &str {
        match self {
            Self::All => "All notes",
            Self::Favorites => "Favorites",
            Self::Folder(name) => name,
            Self::FolderId(None) => "Notes",
            Self::FolderId(Some(_)) => "Folder",
        }
    }
}

pub struct Library {
    pub notes: Vec<Note>,
    pub scope: Scope,
    pub query: String,
    pub selected: Option<String>,
    pub visible: Vec<usize>,
    pub sort_by_title: bool,
}

impl Library {
    pub fn sample() -> Self {
        let notes: Vec<Note> =
            serde_json::from_str(include_str!("../../../tests/fixtures/preview/library.json"))
                .expect("embedded preview fixture is valid");
        let selected = notes.first().map(|note| note.id.clone());
        let mut library = Self {
            notes,
            selected,
            scope: Scope::All,
            query: String::new(),
            visible: vec![],
            sort_by_title: false,
        };
        library.refresh();
        library
    }

    pub fn selected_note(&self) -> Option<&Note> {
        self.notes
            .iter()
            .find(|note| Some(&note.id) == self.selected.as_ref())
    }

    pub fn select(&mut self, id: &str) {
        if self.notes.iter().any(|note| note.id == id) {
            self.selected = Some(id.to_owned());
        }
    }

    pub fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
        self.refresh();
        if !self
            .visible
            .iter()
            .any(|&i| Some(&self.notes[i].id) == self.selected.as_ref())
        {
            self.selected = self.visible.first().map(|&i| self.notes[i].id.clone());
        }
    }

    pub fn search(&mut self, query: String) {
        self.query = query;
        self.refresh();
    }

    pub fn move_selection(&mut self, forward: bool) {
        if self.visible.is_empty() {
            return;
        }
        let current = self
            .visible
            .iter()
            .position(|&i| Some(&self.notes[i].id) == self.selected.as_ref());
        let next = match current {
            None => 0,
            Some(i) if forward => (i + 1).min(self.visible.len() - 1),
            Some(i) => i.saturating_sub(1),
        };
        self.selected = Some(self.notes[self.visible[next]].id.clone());
    }

    pub fn toggle_pin(&mut self) {
        if let Some(note) = self
            .notes
            .iter_mut()
            .find(|note| Some(&note.id) == self.selected.as_ref())
        {
            note.pinned = !note.pinned;
        }
        self.refresh();
    }

    pub fn toggle_check(&mut self, note_id: &str, block: usize) {
        if let Some(note) = self.notes.iter_mut().find(|note| note.id == note_id)
            && let Some(Block::Check { checked, .. }) = note.blocks.get_mut(block)
        {
            *checked = !*checked;
        }
    }

    pub fn refresh(&mut self) {
        let words: Vec<_> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.visible = self
            .notes
            .iter()
            .enumerate()
            .filter(|(_, note)| {
                let in_scope = match &self.scope {
                    Scope::All => true,
                    Scope::Favorites => note.pinned,
                    Scope::Folder(name) => &note.folder == name,
                    Scope::FolderId(id) => &note.folder_id == id,
                };
                if !in_scope {
                    return false;
                }
                if words.is_empty() {
                    return true;
                }
                let text = format!(
                    "{} {} {}",
                    note.title,
                    note.summary,
                    note.blocks
                        .iter()
                        .map(Block::text)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .map(|(i, _)| i)
            .collect();
        self.visible.sort_by(|&a, &b| {
            self.notes[b]
                .pinned
                .cmp(&self.notes[a].pinned)
                .then_with(|| {
                    if self.sort_by_title {
                        self.notes[a].title.cmp(&self.notes[b].title)
                    } else {
                        a.cmp(&b)
                    }
                })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_matches_body_and_all_terms_without_changing_open_note() {
        let mut lib = Library::sample();
        let selected = lib.selected.clone();
        lib.search("LEAVE ideas".into());
        assert_eq!(lib.visible.len(), 1);
        assert_eq!(lib.selected, selected);
        lib.search("not in any note 123".into());
        assert!(lib.visible.is_empty());
        assert_eq!(lib.selected, selected);
    }
    #[test]
    fn favorites_use_pins_and_navigation_is_bounded() {
        let mut lib = Library::sample();
        lib.set_scope(Scope::Favorites);
        assert_eq!(lib.visible.len(), 1);
        lib.toggle_pin();
        assert!(lib.visible.is_empty());
        lib.move_selection(true);
        lib.set_scope(Scope::Folder("Work".into()));
        assert!(lib.selected_note().is_some_and(|n| n.folder == "Work"));
        for _ in 0..30 {
            lib.move_selection(true);
        }
        assert_eq!(
            lib.selected.as_ref(),
            lib.visible.last().map(|&i| &lib.notes[i].id)
        );
    }
    #[test]
    fn checkbox_events_are_routed_by_note_identity() {
        let mut lib = Library::sample();
        let first_id = lib.notes[0].id.clone();
        let second_id = lib.notes[1].id.clone();
        lib.select(&second_id);
        lib.toggle_check(&first_id, 4);
        assert!(matches!(
            lib.notes[0].blocks[4],
            Block::Check { checked: false, .. }
        ));
        assert_eq!(lib.selected.as_deref(), Some(second_id.as_str()));
    }
}
