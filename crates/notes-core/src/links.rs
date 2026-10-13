//! Destinations that the app may activate. Legacy document values stay untouched.
use anyhow::{Result, ensure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Note(String),
    External(String),
}
impl LinkTarget {
    pub fn parse(value: &str) -> Result<Self> {
        ensure!(
            value.len() <= 8192 && !value.chars().any(char::is_control),
            "This link is too long or contains control characters."
        );
        let value = value.trim();
        if let Some(id) = value.strip_prefix("note://") {
            ensure!(
                !id.is_empty()
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
                "Choose a note in this library."
            );
            return Ok(Self::Note(id.into()));
        }
        let url = url::Url::parse(value).map_err(|_| {
            anyhow::anyhow!("Enter a complete https://, http:// or mailto: address.")
        })?;
        ensure!(
            match url.scheme() {
                "https" | "http" => url.host_str().is_some(),
                "mailto" => !url.path().is_empty(),
                _ => false,
            },
            "Use https://, http://, mailto: or a link to a note."
        );
        Ok(Self::External(value.into()))
    }
    pub fn href(&self) -> String {
        match self {
            Self::Note(id) => format!("note://{id}"),
            Self::External(href) => href.clone(),
        }
    }
}

/// Metadata shared by the link picker and explicit related-note summary.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkNote {
    pub id: String,
    pub title: String,
    pub folder: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RelatedNote {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub missing: bool,
}
/// Stable document order, one row per explicit target. No inferred relationships.
pub fn related_notes(
    document: &crate::document::Document,
    own_id: &str,
    catalog: &[LinkNote],
) -> Vec<RelatedNote> {
    use std::collections::{HashMap, HashSet};
    let mut seen = HashSet::new();
    let links: Vec<_> = document
        .leaves()
        .into_iter()
        .flat_map(|b| &b.spans)
        .filter_map(|span| {
            if !span.is_text() {
                return None;
            }
            let LinkTarget::Note(id) = LinkTarget::parse(span.0.get("href")?.as_str()?).ok()?
            else {
                return None;
            };
            if id == own_id || !seen.insert(id.clone()) {
                return None;
            }
            Some((id, span))
        })
        .collect();
    if links.is_empty() {
        return vec![];
    }
    let index: HashMap<_, _> = catalog.iter().map(|n| (n.id.as_str(), n)).collect();
    links
        .into_iter()
        .map(|(id, span)| {
            let note = index.get(id.as_str());
            RelatedNote {
                id,
                title: note.map_or_else(
                    || {
                        let label = span.content().trim();
                        if label.is_empty() {
                            "Unavailable note".into()
                        } else {
                            label.chars().take(160).collect()
                        }
                    },
                    |n| n.title.clone(),
                ),
                folder: note.map_or(String::new(), |n| n.folder.clone()),
                missing: note.is_none(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_policy_is_explicit_and_preserves_note_identity() {
        assert_eq!(
            LinkTarget::parse(" note://AbC-123_9 ").unwrap(),
            LinkTarget::Note("AbC-123_9".into())
        );
        for value in [
            "https://example.com/é?q=a#b",
            "http://localhost:8000",
            "mailto:team@example.com?subject=Hello",
        ] {
            assert_eq!(LinkTarget::parse(value).unwrap().href(), value);
        }
        for value in [
            "javascript:alert(1)",
            "file:///tmp/a",
            "data:text/html,a",
            "note://",
            "note://a/b",
            "note://a?x",
            "note://a%20b",
            "https://",
            "mailto:",
            "https://exa\nmple.com",
            "example.com",
        ] {
            assert!(LinkTarget::parse(value).is_err(), "{value}");
        }
    }
    #[test]
    fn related_targets_are_explicit_unique_live_and_recoverable() {
        use crate::document::{Block, Document, Span};
        use serde_json::json;
        let mut block = Block::paragraph("");
        block.spans = vec![
            Span(json!({"text":"old title", "href":"note://B"})),
            Span(json!({"text":"second reference", "href":"note://B", "marks":["bold"]})),
            Span(json!({"text":"a missing thought", "href":"note://Gone"})),
            Span(json!({"text":"self", "href":"note://A"})),
            Span(json!({"text":"web", "href":"https://example.com"})),
            Span(json!({"text":"note://unlinked"})),
            Span(json!({"image":"fake", "href":"note://object"})),
        ];
        let document = Document {
            blocks: vec![block],
            ..Default::default()
        };
        let catalog = vec![LinkNote {
            id: "B".into(),
            title: "Current title".into(),
            folder: "Ideas".into(),
        }];
        let found = related_notes(&document, "A", &catalog);
        assert_eq!(found.len(), 2);
        assert_eq!(
            (
                &*found[0].id,
                &*found[0].title,
                &*found[0].folder,
                found[0].missing
            ),
            ("B", "Current title", "Ideas", false)
        );
        assert_eq!(
            (&*found[1].id, &*found[1].title, found[1].missing),
            ("Gone", "a missing thought", true)
        );
        assert!(related_notes(&document, "A", &[])[0].missing);
        assert_eq!(related_notes(&Document::default(), "A", &catalog), vec![]);
    }
}
