//! Personal presentation choices, never part of a note or its undo history.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    Reference,
    Light,
    #[default]
    Omarchy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteFont {
    #[default]
    Serif,
    Sans,
    Mono,
}
impl NoteFont {
    pub fn family(self) -> &'static str {
        match self {
            Self::Serif => "Noto Serif",
            Self::Sans => "Noto Sans",
            Self::Mono => "monospace",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub appearance: Appearance,
    pub note_font: NoteFont,
    pub note_size: u16,
    pub sidebar_width: u16,
    pub list_width: u16,
    pub focus_mode: bool,
    pub sort_by_title: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            appearance: Appearance::Omarchy,
            note_font: NoteFont::Serif,
            note_size: 19,
            sidebar_width: 248,
            list_width: 352,
            focus_mode: false,
            sort_by_title: false,
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (14..=28).contains(&self.note_size),
            "Writing size must be between 14 and 28 pixels."
        );
        ensure!(
            (220..=300).contains(&self.sidebar_width),
            "Sidebar width must be between 220 and 300 pixels."
        );
        ensure!(
            (300..=440).contains(&self.list_width),
            "Note list width must be between 300 and 440 pixels."
        );
        Ok(())
    }
}

/// Only locally changed fields are merged under the file lock. Independent
/// library owners can save unrelated settings without overwriting one another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Patch {
    pub appearance: Option<Appearance>,
    pub note_font: Option<NoteFont>,
    pub note_size: Option<u16>,
    pub sidebar_width: Option<u16>,
    pub list_width: Option<u16>,
    pub focus_mode: Option<bool>,
    pub sort_by_title: Option<bool>,
}
impl Patch {
    pub fn between(before: &Preferences, after: &Preferences) -> Self {
        Self {
            appearance: (before.appearance != after.appearance).then_some(after.appearance),
            note_font: (before.note_font != after.note_font).then_some(after.note_font),
            note_size: (before.note_size != after.note_size).then_some(after.note_size),
            sidebar_width: (before.sidebar_width != after.sidebar_width)
                .then_some(after.sidebar_width),
            list_width: (before.list_width != after.list_width).then_some(after.list_width),
            focus_mode: (before.focus_mode != after.focus_mode).then_some(after.focus_mode),
            sort_by_title: (before.sort_by_title != after.sort_by_title)
                .then_some(after.sort_by_title),
        }
    }
    pub fn apply(&self, value: &mut Preferences) {
        if let Some(v) = self.appearance {
            value.appearance = v;
        }
        if let Some(v) = self.note_font {
            value.note_font = v;
        }
        if let Some(v) = self.note_size {
            value.note_size = v;
        }
        if let Some(v) = self.sidebar_width {
            value.sidebar_width = v;
        }
        if let Some(v) = self.list_width {
            value.list_width = v;
        }
        if let Some(v) = self.focus_mode {
            value.focus_mode = v;
        }
        if let Some(v) = self.sort_by_title {
            value.sort_by_title = v;
        }
    }
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
pub struct Save {
    pub operation: u64,
    pub patch: Patch,
}
pub struct Session {
    pub value: Preferences,
    saved: Preferences,
    generation: u64,
    pending: Option<(u64, Preferences)>,
    refresh: bool,
    pub error: Option<String>,
}
impl Session {
    pub fn new(value: Preferences, error: Option<String>) -> Self {
        Self {
            saved: value.clone(),
            value,
            generation: 0,
            pending: None,
            refresh: false,
            error,
        }
    }
    pub fn change(&mut self, patch: Patch) -> bool {
        let before = self.value.clone();
        patch.apply(&mut self.value);
        if self.value == before {
            return false;
        }
        self.generation += 1;
        self.error = None;
        true
    }
    pub fn retry(&mut self) {
        self.generation += 1;
        self.error = None;
        self.refresh = true;
    }
    pub fn begin_save(&mut self) -> Option<Save> {
        if self.pending.is_some()
            || self.error.is_some()
            || (!self.refresh && self.value == self.saved)
        {
            return None;
        }
        self.refresh = false;
        self.pending = Some((self.generation, self.value.clone()));
        Some(Save {
            operation: self.generation,
            patch: Patch::between(&self.saved, &self.value),
        })
    }
    pub fn complete(
        &mut self,
        operation: u64,
        result: std::result::Result<Preferences, String>,
    ) -> bool {
        if self.pending.as_ref().is_none_or(|p| p.0 != operation) {
            return false;
        }
        let (_, sent) = self.pending.take().unwrap();
        match result {
            Ok(saved) => {
                // Keep edits made after dispatch; adopt unrelated changes from
                // another owner without resending stale whole-file snapshots.
                let newer = Patch::between(&sent, &self.value);
                self.value = saved.clone();
                newer.apply(&mut self.value);
                self.saved = saved;
            }
            Err(error) => self.error = Some(error),
        }
        true
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
            || (self.error.is_none() && (self.refresh || self.value != self.saved))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_saves_keep_newer_edits_and_merge_unrelated_preferences() {
        let mut session = Session::new(Preferences::default(), None);
        session.change(Patch {
            note_size: Some(22),
            ..Default::default()
        });
        let first = session.begin_save().unwrap();
        session.change(Patch {
            note_size: Some(26),
            ..Default::default()
        });
        assert!(session.begin_save().is_none());
        let saved = Preferences {
            note_size: 22,
            appearance: Appearance::Light,
            ..Default::default()
        };
        assert!(!session.complete(first.operation + 50, Ok(saved.clone())));
        assert!(session.complete(first.operation, Ok(saved)));
        assert_eq!(session.value.note_size, 26);
        assert_eq!(session.value.appearance, Appearance::Light);
        let next = session.begin_save().unwrap();
        assert_eq!(
            next.patch,
            Patch {
                note_size: Some(26),
                ..Default::default()
            }
        );
        assert!(session.complete(next.operation, Err("Read-only configuration".into())));
        assert!(!session.busy());
        assert!(session.begin_save().is_none());
        session.retry();
        let retry = session.begin_save().unwrap();
        let mut disk = Preferences::default();
        retry.patch.apply(&mut disk);
        session.complete(retry.operation, Ok(disk));
        assert!(!session.busy());
        assert!(session.error.is_none());
    }
    #[test]
    fn malformed_values_are_not_clamped_or_silently_rewritten() {
        for size in [0, 13, 29, u16::MAX] {
            assert!(
                Preferences {
                    note_size: size,
                    ..Default::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Preferences {
                sidebar_width: 0,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(serde_json::from_str::<Preferences>(r#"{"unexpected":true}"#).is_err());
    }
}
