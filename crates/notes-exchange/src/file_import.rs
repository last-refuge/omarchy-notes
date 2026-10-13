//! Resource-aware inert conversion for reviewed file imports. No file access or
//! downloads occur here; the storage preparer owns local path validation.
use crate::{MAX_HTML, html};
use anyhow::{Result, ensure};
use notes_core::document::Document;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Resource {
    pub token: String,
    pub target: String,
    pub label: String,
    pub image: bool,
}

#[derive(Debug)]
pub struct Imported {
    pub document: Document,
    pub warnings: Vec<String>,
    pub resources: Vec<Resource>,
}

pub fn markdown(text: &str) -> Result<Imported> {
    ensure!(
        text.len() <= MAX_HTML,
        "Markdown exceeds the 4 MiB import limit."
    );
    let parser = pulldown_cmark::Parser::new_ext(
        text,
        pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_TASKLISTS,
    );
    let mut generated = String::new();
    pulldown_cmark::html::push_html(&mut generated, parser);
    html::import_file_resources(&generated)
}

/// Resolve an address relative to its source document without permitting a
/// remote fetch or a file outside the user's selected import root.
pub fn local_target(target: &str, source: &Path, root: &Path) -> Result<PathBuf> {
    let base = url::Url::from_file_path(source)
        .map_err(|_| anyhow::anyhow!("The source path cannot be represented as a file address."))?;
    let mut url = base.join(target)?;
    ensure!(
        url.scheme() == "file"
            && url
                .host_str()
                .is_none_or(|host| host.is_empty() || host == "localhost"),
        "Remote or active resources are not downloaded."
    );
    // Fragments identify a location within a note; import keeps the note link.
    url.set_fragment(None);
    ensure!(
        url.query().is_none(),
        "Local resource queries are unsupported."
    );
    let path = url
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Invalid local resource address."))?;
    let canonical = path.canonicalize()?;
    ensure!(
        canonical.starts_with(root),
        "A resource points outside the selected import folder."
    );
    let relative = path
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("A resource points outside the selected import folder."))?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component);
        ensure!(
            !std::fs::symlink_metadata(&cursor)?.file_type().is_symlink(),
            "Symbolic-link resources are not imported."
        );
    }
    Ok(canonical)
}
