//! Portable copies with immutable assets. Publication is atomic and never
//! replaces an existing destination. The source library is never changed.
use crate::{Note, Store, backup, media};
use anyhow::{Context, Result, ensure};
#[cfg(test)]
use notes_core::document::Document;
use notes_core::document::{Span, new_id};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Html,
}
#[derive(Clone, Debug)]
pub enum Selection {
    Note(Note),
    Library,
}
#[derive(Clone, Debug)]
pub struct Report {
    pub path: PathBuf,
    pub notes: usize,
    pub assets: usize,
    pub bytes: u64,
    pub warnings: Vec<String>,
    pub revisions: Vec<(String, i64)>,
}
impl Store {
    pub fn export_portable(
        &self,
        selection: Selection,
        destination: &Path,
        format: Format,
        cancel: &backup::Cancellation,
        mut progress: impl FnMut(backup::Progress),
    ) -> Result<Report> {
        cancel.check()?;
        progress(backup::Progress::Snapshot);
        let notes = match selection {
            Selection::Note(n) => vec![n],
            Selection::Library => self.load()?.notes,
        };
        ensure!(
            notes.len() <= 100_000,
            "Export up to 100,000 notes at once."
        );
        let stage = backup::Stage::new(destination)?;
        fs::create_dir(stage.path.join("assets"))?;
        let mut names = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        for note in &notes {
            note.body.validate()?;
            let stem = file_stem(&note.body.title());
            let mut name = stem.clone();
            let mut suffix = 2;
            while !used.insert(name.to_lowercase()) {
                name = format!("{stem}-{suffix}");
                suffix += 1;
            }
            names.insert(
                note.id.clone(),
                format!(
                    "{name}.{}",
                    if format == Format::Html { "html" } else { "md" }
                ),
            );
        }
        let mut report = Report { path: destination.to_owned(), notes: notes.len(), assets: 0, bytes: 0,
            warnings: vec!["Portable formats preserve visible content and copied files. App-specific attributes and revision history require a lossless library backup.".into()],
            revisions: notes.iter().map(|n| (n.id.clone(), n.revision)).collect() };
        let mut files = BTreeMap::<String, String>::new();
        progress(backup::Progress::Copying);
        for note in &notes {
            cancel.check()?;
            let mut doc = note.body.clone();
            let token = format!("OMARCHY{}", new_id().replace('-', ""));
            let mut substitutions = vec![];
            let mut index = 0;
            visit_spans(&mut doc.blocks, &mut |span| -> Result<()> {
                if span.is_text() {
                    if let Some(href) = span.0.get("href").and_then(|v| v.as_str())
                        && let Ok(notes_core::links::LinkTarget::Note(id)) =
                            notes_core::links::LinkTarget::parse(href)
                    {
                        if let Some(name) = names.get(&id) {
                            let placeholder = format!("https://omarchy.invalid/{token}/{index}");
                            index += 1;
                            span.0["href"] = serde_json::json!(placeholder);
                            substitutions.push((placeholder, url_path(name)));
                        } else {
                            if let Some(attrs) = span.0.as_object_mut() {
                                attrs.remove("href");
                            }
                            report.warnings.push(
                                "A link to a note outside this export was kept as text.".into(),
                            );
                        }
                    }
                    return Ok(());
                }
                let mut alt = span
                    .0
                    .get("alt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Attached file")
                    .to_owned();
                let (hash, name, inline) = if let Some(hash) =
                    span.0.get("image").and_then(|v| v.as_str())
                {
                    let bytes = media::read_blob(&self.root, hash)?;
                    let ext = image::guess_format(&bytes)
                        .map(|f| f.extensions_str()[0])
                        .unwrap_or("bin");
                    (hash.to_owned(), format!("{hash}.{ext}"), true)
                } else if let Some(id) = span.0.get("attachment").and_then(|v| v.as_str()) {
                    let info = media::attachment(&self.db, id)?;
                    if span.0.get("alt").is_none() {
                        alt.clone_from(&info.name);
                    }
                    let owner: String = self.db.query_row(
                        "SELECT note_id FROM attachments WHERE id=?1",
                        [id],
                        |r| r.get(0),
                    )?;
                    ensure!(
                        owner == note.id,
                        "An attachment belongs to another note; export stopped without publishing."
                    );
                    (
                        info.hash.clone(),
                        format!("{}-{}", info.hash, file_stem(&info.name)),
                        false,
                    )
                } else {
                    report
                        .warnings
                        .push("An unsupported media object was kept as its visible label.".into());
                    *span = Span::text(alt);
                    return Ok(());
                };
                let asset = if let Some(existing) = files.get(&hash) {
                    existing.clone()
                } else {
                    cancel.check()?;
                    let bytes = media::read_blob(&self.root, &hash)?;
                    let relative = format!("assets/{name}");
                    write_new(&stage.path.join(&relative), &bytes, cancel)?;
                    report.bytes += bytes.len() as u64;
                    report.assets += 1;
                    files.insert(hash, relative.clone());
                    relative
                };
                let placeholder = format!("{token}MEDIA{index}");
                index += 1;
                let markup = if inline {
                    format!(
                        "<img src=\"{}\" alt=\"{}\">",
                        url_path(&asset),
                        escape(&alt)
                    )
                } else {
                    format!("<a href=\"{}\">{}</a>", url_path(&asset), escape(&alt))
                };
                *span = Span::text(placeholder.clone());
                substitutions.push((placeholder, markup));
                Ok(())
            })?;
            let mut text = match format {
                Format::Html => notes_exchange::html::export(&doc),
                Format::Markdown => {
                    let exported = notes_exchange::markdown::export(&doc)?;
                    report.warnings.extend(exported.warnings);
                    // Tables are represented by inline HTML in Markdown.
                    exported
                        .text
                        .replace(
                            "<!doctype html><html><head><meta charset=\"utf-8\"></head><body>",
                            "",
                        )
                        .replace("</body></html>", "")
                }
            };
            for (from, to) in substitutions {
                text = text.replace(&from, &to);
            }
            cancel.check()?;
            write_new(&stage.path.join(&names[&note.id]), text.as_bytes(), cancel)?;
            report.bytes += text.len() as u64;
        }
        report.warnings.sort();
        report.warnings.dedup();
        let manifest = serde_json::to_vec_pretty(
            &serde_json::json!({"format":"omarchy-notes-portable","version":1,
            "notes":names,"revisions":report.revisions,"assets":files,"warnings":report.warnings}),
        )?;
        write_new(&stage.path.join("export.json"), &manifest, cancel)?;
        fs::File::open(stage.path.join("assets"))?.sync_all()?;
        progress(backup::Progress::Publishing);
        report.path = stage.publish(&mut |phase| {
            if phase != backup::Phase::Published {
                cancel.check()?;
            }
            Ok(())
        })?;
        Ok(report)
    }
}
fn visit_spans(
    blocks: &mut [notes_core::document::Block],
    f: &mut impl FnMut(&mut Span) -> Result<()>,
) -> Result<()> {
    for block in blocks {
        for span in &mut block.spans {
            f(span)?;
        }
        for cell in &mut block.cells {
            visit_spans(&mut cell.blocks, f)?;
        }
    }
    Ok(())
}
fn file_stem(value: &str) -> String {
    let mut out = String::new();
    for ch in value
        .chars()
        .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
    {
        if out.len() + ch.len_utf8() > 160 {
            break;
        }
        out.push(ch);
    }
    let out = out.trim().trim_matches('.');
    if out.is_empty() {
        "Note".into()
    } else {
        out.into()
    }
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn url_path(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~/".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn write_new(path: &Path, bytes: &[u8], cancel: &backup::Cancellation) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("Could not create {}.", path.display()))?;
    for chunk in bytes.chunks(64 * 1024) {
        cancel.check()?;
        file.write_all(chunk)?;
    }
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_unicode_names_and_internal_links_are_portable_and_noclobber() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("library")).unwrap();
        let first = store
            .create(
                Document::from_blocks(vec![notes_core::document::Block::paragraph("café")]),
                None,
            )
            .unwrap();
        let mut second = store.create(first.body.clone(), None).unwrap();
        second.body.blocks[0].spans[0].0["href"] =
            serde_json::json!(format!("note://{}", first.id));
        store.save(&second).unwrap();
        let target = dir.path().join("export");
        let report = store
            .export_portable(
                Selection::Library,
                &target,
                Format::Markdown,
                &backup::Cancellation::default(),
                |_| {},
            )
            .unwrap();
        assert_eq!(report.notes, 2);
        let names: serde_json::Value =
            serde_json::from_slice(&fs::read(target.join("export.json")).unwrap()).unwrap();
        let files = names["notes"].as_object().unwrap();
        assert_ne!(files[&first.id], files[&second.id]);
        let text = fs::read_to_string(target.join(files[&second.id].as_str().unwrap())).unwrap();
        assert!(text.contains("caf%C3%A9"));
        assert!(!text.contains("note://"));
        assert!(
            store
                .export_portable(
                    Selection::Library,
                    &target,
                    Format::Html,
                    &backup::Cancellation::default(),
                    |_| {}
                )
                .is_err()
        );
        assert_eq!(store.load().unwrap().notes.len(), 2);
    }
    #[test]
    fn cancellation_leaves_no_published_or_partial_copy() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("library")).unwrap();
        store.create(Document::default(), None).unwrap();
        let cancel = backup::Cancellation::default();
        let target = dir.path().join("export");
        assert!(
            store
                .export_portable(
                    Selection::Library,
                    &target,
                    Format::Markdown,
                    &cancel,
                    |p| {
                        if p == backup::Progress::Copying {
                            cancel.cancel();
                        }
                    }
                )
                .is_err()
        );
        assert!(!target.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn referenced_attachment_bytes_are_copied_and_remain_accessible() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("library")).unwrap();
        let mut note = store.create(Document::default(), None).unwrap();
        let source = dir.path().join("café.txt");
        let bytes = b"portable attachment\0exact bytes";
        fs::write(&source, bytes).unwrap();
        note.body = store
            .import_media(
                &note.id,
                media::Import::Files {
                    paths: vec![source],
                    images: Some(false),
                },
            )
            .unwrap();
        store.save(&note).unwrap();
        let destination = dir.path().join("export");
        let report = store
            .export_portable(
                Selection::Library,
                &destination,
                Format::Html,
                &backup::Cancellation::default(),
                |_| {},
            )
            .unwrap();
        assert_eq!(report.assets, 1);
        let file = fs::read_dir(destination.join("assets"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(file.path()).unwrap(), bytes);
        let html = fs::read_dir(&destination)
            .unwrap()
            .filter_map(|f| f.ok())
            .find(|f| f.path().extension().is_some_and(|e| e == "html"))
            .unwrap();
        let html = fs::read_to_string(html.path()).unwrap();
        assert!(html.contains("caf%C3%A9.txt"));
        assert!(html.contains("café.txt"));
        assert!(!html.contains("omarchy.invalid"));
    }
}
