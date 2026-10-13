//! Framework-free library query and presentation contract.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_QUERY_BYTES: usize = 4096;
pub const MAX_QUERY_TERMS: usize = 32;
pub const MAX_PAGE_ROWS: u16 = 200;
pub const MAX_PAGE_OFFSET: u32 = 100_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Edited,
    Created,
    Title,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    #[default]
    All,
    Favorites,
    Folder(Option<String>),
    Trash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub generation: u64,
    pub scope: Scope,
    pub text: String,
    pub sort: Sort,
    pub offset: u32,
    pub limit: u16,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            generation: 0,
            scope: Scope::All,
            text: String::new(),
            sort: Sort::Edited,
            offset: 0,
            limit: MAX_PAGE_ROWS,
        }
    }
}
impl Query {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.text.len() <= MAX_QUERY_BYTES,
            "Search is limited to 4 KiB. Narrow your query."
        );
        ensure!(
            (1..=MAX_PAGE_ROWS).contains(&self.limit),
            "Request 1–200 notes per page."
        );
        ensure!(
            self.offset <= MAX_PAGE_OFFSET,
            "Narrow the scope before paging past 100,000 notes."
        );
        if let Scope::Folder(Some(id)) = &self.scope {
            ensure!(id.len() <= MAX_QUERY_BYTES, "Invalid folder identity.");
        }
        ensure!(
            self.text.split_whitespace().count() <= MAX_QUERY_TERMS,
            "Search with at most 32 terms."
        );
        ensure!(
            self.scope != Scope::Trash || self.text.trim().is_empty(),
            "Search live notes; Recently Deleted is listed separately."
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub id: String,
    pub folder: Option<String>,
    pub title: String,
    pub snippet: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub pinned: bool,
    pub revision: i64,
    pub deleted_at: Option<i64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub rows: Vec<Summary>,
    pub has_more: bool,
}

/// Qt-compatible all-term prefix matching. Quoted terms treat FTS operators and
/// punctuation as literal user input, rather than allowing query-language injection.
pub fn prefix_query(text: &str) -> Result<String> {
    ensure!(
        text.len() <= MAX_QUERY_BYTES,
        "Search is limited to 4 KiB. Narrow your query."
    );
    ensure!(
        text.split_whitespace().count() <= MAX_QUERY_TERMS,
        "Search with at most 32 terms."
    );
    Ok(text
        .split_whitespace()
        .filter_map(|term| {
            let term = term.replace('"', "");
            term.chars()
                .any(char::is_alphanumeric)
                .then(|| format!("\"{term}\"*"))
        })
        .collect::<Vec<_>>()
        .join(" "))
}

/// A duplicate gets independent block identities; all other rich properties,
/// media references and document extension data remain exact owned copies.
pub fn fresh_document(source: &crate::document::Document) -> crate::document::Document {
    fn renew(blocks: &mut [crate::document::Block]) {
        for block in blocks {
            block.id = crate::document::new_id();
            for cell in &mut block.cells {
                renew(&mut cell.blocks);
            }
        }
    }
    let mut copy = source.clone();
    renew(&mut copy.blocks);
    copy
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_input_is_literal_unicode_and_bounded() {
        assert_eq!(
            prefix_query(" bench OR \"日本\" foo-bar * ").unwrap(),
            "\"bench\"* \"OR\"* \"日本\"* \"foo-bar\"*"
        );
        assert_eq!(prefix_query("* \" - +").unwrap(), "");
        assert!(prefix_query(&"word ".repeat(33)).is_err());
        assert!(prefix_query(&"x".repeat(MAX_QUERY_BYTES + 1)).is_err());
        assert!(
            Query {
                scope: Scope::Trash,
                text: "x".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
