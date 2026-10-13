//! Stable folder identities and framework-free hierarchy rules.
use anyhow::{Result, bail, ensure};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
    pub sort_key: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mutation {
    CreateFolder {
        name: String,
        parent: Option<String>,
    },
    RenameFolder {
        id: String,
        name: String,
    },
    MoveFolder {
        id: String,
        parent: Option<String>,
    },
    MoveNote {
        id: String,
        folder: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Folder(Folder),
    NoteMoved { id: String, folder: Option<String> },
}

pub fn name(raw: &str) -> Result<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    ensure!(!name.is_empty(), "Give this folder a name.");
    // Match Qt's 200 UTF-16-code-unit boundary, including non-BMP names.
    ensure!(
        name.encode_utf16().count() <= 200,
        "Use a folder name of at most 200 characters."
    );
    ensure!(
        !name.chars().any(char::is_control),
        "Folder names cannot contain control characters."
    );
    Ok(name)
}

/// Root is None. A folder cannot become its own ancestor. Invalid source
/// hierarchies are bounded and rejected instead of recursing forever.
pub fn parent(folders: &[Folder], moving: Option<&str>, destination: Option<&str>) -> Result<()> {
    let by_id: HashMap<_, _> = folders.iter().map(|f| (f.id.as_str(), f)).collect();
    let mut seen = HashSet::new();
    let mut cursor = destination;
    while let Some(id) = cursor {
        ensure!(
            Some(id) != moving,
            "A folder cannot be placed inside itself or one of its subfolders."
        );
        ensure!(
            seen.insert(id),
            "This folder hierarchy needs repair before it can be used."
        );
        let Some(folder) = by_id.get(id) else {
            bail!("That folder no longer exists.")
        };
        cursor = folder.parent.as_deref();
    }
    Ok(())
}

/// All safe destinations in one traversal. Orphans, cycles, the moving folder
/// and its descendants are excluded. Storage still revalidates on commit.
pub fn destinations(folders: &[Folder], moving: Option<&str>) -> HashSet<String> {
    let mut children: HashMap<Option<&str>, Vec<&str>> = HashMap::new();
    for folder in folders {
        children
            .entry(folder.parent.as_deref())
            .or_default()
            .push(&folder.id);
    }
    let mut stack = children.get(&None).cloned().unwrap_or_default();
    let mut valid = HashSet::new();
    while let Some(id) = stack.pop() {
        if Some(id) != moving && valid.insert(id.to_owned()) {
            stack.extend(children.get(&Some(id)).into_iter().flatten().copied());
        }
    }
    valid
}

#[derive(Clone, Debug)]
pub struct Row {
    pub folder: Folder,
    pub depth: usize,
    pub path: String,
    pub has_children: bool,
}
/// Iterative traversal supports deeply nested libraries without stack growth.
/// Broken/orphaned entries remain reachable for inspection, once each.
pub fn rows(folders: &[Folder], collapsed: &HashSet<String>) -> Vec<Row> {
    let by_id: HashMap<_, _> = folders.iter().map(|f| (f.id.as_str(), f)).collect();
    let mut children: HashMap<Option<&str>, Vec<&Folder>> = HashMap::new();
    for f in folders {
        let parent = f.parent.as_deref().filter(|id| by_id.contains_key(id));
        children.entry(parent).or_default().push(f);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(|a, b| {
            a.sort_key
                .cmp(&b.sort_key)
                .then_with(|| {
                    a.name
                        .to_ascii_lowercase()
                        .cmp(&b.name.to_ascii_lowercase())
                })
                .then(a.id.cmp(&b.id))
        });
    }
    let mut result = vec![];
    let mut seen = HashSet::new();
    let mut stack: Vec<_> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|f| (*f, 0, String::new()))
        .collect();
    // Append unreachable cycles after the valid tree; mutations still reject
    // them. Do not silently hide notes just because their folder data is broken.
    let mut fallback = folders.iter();
    loop {
        let (folder, depth, prefix) = match stack.pop() {
            Some(row) => row,
            None => match fallback.find(|f| !seen.contains(f.id.as_str())) {
                Some(f) => (f, 0, String::new()),
                None => break,
            },
        };
        if !seen.insert(folder.id.as_str()) {
            continue;
        }
        let path = if prefix.is_empty() {
            folder.name.clone()
        } else {
            format!("{prefix} / {}", folder.name)
        };
        let descendants = children.get(&Some(folder.id.as_str()));
        result.push(Row {
            folder: folder.clone(),
            depth,
            path: path.clone(),
            has_children: descendants.is_some_and(|c| !c.is_empty()),
        });
        if collapsed.contains(&folder.id) {
            // Mark the subtree visited so fallback cannot reveal collapsed rows.
            let mut hidden = vec![folder.id.as_str()];
            while let Some(id) = hidden.pop() {
                for child in children.get(&Some(id)).into_iter().flatten() {
                    if seen.insert(child.id.as_str()) {
                        hidden.push(child.id.as_str());
                    }
                }
            }
        } else {
            stack.extend(
                descendants
                    .into_iter()
                    .flatten()
                    .rev()
                    .map(|f| (*f, depth + 1, path.clone())),
            );
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn folder(id: &str, parent: Option<&str>) -> Folder {
        Folder {
            id: id.into(),
            parent: parent.map(str::to_owned),
            name: id.into(),
            sort_key: 0,
        }
    }
    #[test]
    fn names_and_hierarchy_preserve_identity_and_reject_cycles() {
        assert_eq!(name("  Client\n  notes ").unwrap(), "Client notes");
        assert!(name("\t ").is_err());
        assert!(name(&"😀".repeat(101)).is_err());
        assert!(name("a\0b").is_err());
        let folders = vec![
            folder("a", None),
            folder("b", Some("a")),
            folder("c", Some("b")),
        ];
        assert!(parent(&folders, Some("a"), Some("c")).is_err());
        assert!(parent(&folders, Some("b"), Some("b")).is_err());
        assert!(parent(&folders, Some("c"), None).is_ok());
        assert!(parent(&folders, None, Some("missing")).is_err());
        let visible = rows(&folders, &HashSet::new());
        assert_eq!(visible[2].path, "a / b / c");
        assert_eq!(visible[2].depth, 2);
        assert_eq!(rows(&folders, &HashSet::from(["a".into()])).len(), 1);
        let broken = vec![folder("a", Some("b")), folder("b", Some("a"))];
        assert_eq!(rows(&broken, &HashSet::new()).len(), 2);
        assert!(parent(&broken, None, Some("a")).is_err());
        assert!(destinations(&broken, None).is_empty());
        assert_eq!(
            destinations(&folders, Some("b")),
            HashSet::from(["a".into()])
        );
    }
    #[test]
    fn large_tree_keeps_every_safe_destination_reachable_once() {
        let mut folders = vec![folder("parent", None), folder("orphan", Some("missing"))];
        folders.extend((0..10_000).map(|i| folder(&format!("child-{i:05}"), Some("parent"))));
        let visible = rows(&folders, &HashSet::new());
        assert_eq!(visible.len(), 10_002);
        assert_eq!(visible.last().unwrap().folder.id, "child-09999");
        assert_eq!(rows(&folders, &HashSet::from(["parent".into()])).len(), 2);
        let valid = destinations(&folders, None);
        assert_eq!(valid.len(), 10_001);
        assert!(!valid.contains("orphan"));
        assert!(destinations(&folders, Some("parent")).is_empty());
    }
}
