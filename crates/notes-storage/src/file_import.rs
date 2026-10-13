//! Reviewed, bounded file snapshots followed by one atomic library transaction.
//! Source files are never modified and preparation does not touch the library.
use super::*;
use notes_core::document::Span;
use notes_exchange::file_import::{Imported, Resource};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Read,
    os::unix::fs::OpenOptionsExt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub const MAX_FILES: usize = 1_000;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_RESOURCES: usize = 4_096;
const MAX_WARNINGS: usize = 1_000;
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn check(&self) -> Result<()> {
        ensure!(
            !self.0.load(Ordering::Acquire),
            "Import cancelled. Your library is unchanged."
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Scan,
    Prepare,
    Assets,
    Commit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub phase: Phase,
    pub completed: usize,
    pub total: usize,
}
#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub files: usize,
    pub folders: usize,
    pub assets: usize,
    pub bytes: u64,
    pub warnings: Vec<String>,
    pub needs_quotes: bool,
}
#[derive(Debug)]
pub struct PreparedImport {
    summary: Summary,
    notes: Vec<PreparedNote>,
    folders: Vec<SourceFolder>,
    assets: BTreeMap<String, Vec<u8>>,
}
impl PreparedImport {
    pub fn summary(&self) -> &Summary {
        &self.summary
    }
}
#[derive(Debug)]
pub struct Report {
    pub notes: Vec<Note>,
    pub folders: Vec<Folder>,
    pub summary: Summary,
}
#[derive(Debug)]
struct SourceFolder {
    path: PathBuf,
    parent: Option<PathBuf>,
    name: String,
}
struct Source {
    path: PathBuf,
    root: PathBuf,
    folder: Option<PathBuf>,
}
#[derive(Debug)]
struct PreparedNote {
    id: String,
    folder: Option<PathBuf>,
    body: Document,
    attachments: Vec<notes_exchange::Attachment>,
}
struct Budget {
    bytes: usize,
    resources: usize,
}
impl Budget {
    fn add(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .context("Import size is out of range.")?;
        ensure!(
            self.bytes <= MAX_SOURCE_BYTES,
            "Import up to 64 MiB of text and unique asset bytes at a time. Choose a smaller folder."
        );
        Ok(())
    }
}
fn warning(summary: &mut Summary, path: &Path, message: impl std::fmt::Display) -> Result<()> {
    ensure!(
        summary.warnings.len() < MAX_WARNINGS,
        "This selection has too many import warnings. Choose fewer files and review them separately."
    );
    summary
        .warnings
        .push(format!("{}: {message}", path.display()));
    Ok(())
}
fn text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt"))
}
fn read(path: &Path, limit: usize, cancel: &Cancellation) -> Result<Vec<u8>> {
    cancel.check()?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= limit as u64,
        "Choose a regular file of {} MiB or less.",
        limit / (1024 * 1024)
    );
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    let mut input = file.take((limit + 1) as u64);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        cancel.check()?;
        let count = input.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        ensure!(
            bytes.len() <= limit,
            "A source file grew beyond its import limit."
        );
    }
    Ok(bytes)
}
fn scan(
    paths: Vec<PathBuf>,
    cancel: &Cancellation,
    progress: &mut impl FnMut(Progress),
    summary: &mut Summary,
) -> Result<(Vec<Source>, Vec<SourceFolder>)> {
    ensure!(
        !paths.is_empty() && paths.len() <= MAX_FILES,
        "Choose 1–1,000 Markdown/text files or folders."
    );
    let mut sources = vec![];
    let mut folders = vec![];
    let mut seen_files = HashSet::new();
    let mut seen_dirs = HashSet::new();
    let mut entries = 0usize;
    for chosen in paths {
        cancel.check()?;
        let metadata = std::fs::symlink_metadata(&chosen)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Symbolic-link selections are not imported."
        );
        let chosen = chosen.canonicalize()?;
        if metadata.is_file() {
            ensure!(text_file(&chosen), "Choose .md, .markdown or .txt files.");
            if seen_files.insert(chosen.clone()) {
                sources.push(Source {
                    root: chosen
                        .parent()
                        .context("A source file has no parent.")?
                        .to_path_buf(),
                    path: chosen,
                    folder: None,
                });
            }
            continue;
        }
        ensure!(metadata.is_dir(), "Choose regular files or folders.");
        let mut stack = vec![(chosen.clone(), None, 0usize)];
        while let Some((directory, parent, depth)) = stack.pop() {
            cancel.check()?;
            ensure!(
                depth < MAX_DEPTH,
                "Import folder nesting is limited to 64 levels."
            );
            if !seen_dirs.insert(directory.clone()) {
                continue;
            }
            ensure!(
                folders.len() < MAX_FILES,
                "Import up to 1,000 folders at a time."
            );
            let name = directory
                .file_name()
                .and_then(|s| s.to_str())
                .context("A folder name is not Unicode.")?;
            folders.push(SourceFolder {
                path: directory.clone(),
                parent,
                name: notes_core::organization::name(name)?,
            });
            let mut children = std::fs::read_dir(&directory)?
                .take(MAX_ENTRIES + 1)
                .collect::<std::io::Result<Vec<_>>>()?;
            entries = entries
                .checked_add(children.len())
                .context("Import selection is too large.")?;
            ensure!(
                entries <= MAX_ENTRIES,
                "Scan up to 20,000 directory entries at a time. Choose a smaller folder."
            );
            children.sort_by_key(|entry| entry.file_name());
            for entry in children.into_iter().rev() {
                cancel.check()?;
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    warning(
                        summary,
                        &entry.path(),
                        "Symbolic link skipped; its target was not read.",
                    )?;
                } else if kind.is_dir() {
                    stack.push((entry.path(), Some(directory.clone()), depth + 1));
                } else if kind.is_file() && text_file(&entry.path()) {
                    ensure!(
                        sources.len() < MAX_FILES,
                        "Import up to 1,000 notes at a time. Choose a smaller folder."
                    );
                    if seen_files.insert(entry.path()) {
                        sources.push(Source {
                            path: entry.path(),
                            root: chosen.clone(),
                            folder: Some(directory.clone()),
                        });
                    }
                }
            }
            progress(Progress {
                phase: Phase::Scan,
                completed: entries,
                total: 0,
            });
        }
    }
    ensure!(
        !sources.is_empty() && sources.len() <= MAX_FILES,
        "No importable Markdown or text files were found."
    );
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    // Only preserve directories that contain imported notes or are their ancestors.
    let by_path: HashMap<_, _> = folders
        .iter()
        .map(|folder| (&folder.path, folder.parent.as_ref()))
        .collect();
    let mut used = HashSet::new();
    for source in &sources {
        let mut current = source.folder.as_ref();
        while let Some(path) = current {
            if !used.insert(path.clone()) {
                break;
            }
            current = by_path.get(path).copied().flatten();
        }
    }
    folders.retain(|folder| used.contains(&folder.path));
    Ok((sources, folders))
}

enum Resolved {
    Image(Span),
    Link(String),
    Attachment(Span),
    Missing(String),
}

fn resolve(
    resource: &Resource,
    source: &Source,
    imported_ids: &HashMap<PathBuf, String>,
    assets: &mut BTreeMap<String, Vec<u8>>,
    attachments: &mut Vec<notes_exchange::Attachment>,
    budget: &mut Budget,
    cancel: &Cancellation,
) -> Result<Resolved> {
    cancel.check()?;
    budget.resources += 1;
    ensure!(
        budget.resources <= MAX_RESOURCES,
        "Import up to 4,096 resource references at a time."
    );
    let path =
        notes_exchange::file_import::local_target(&resource.target, &source.path, &source.root)?;
    if !resource.image
        && let Some(id) = imported_ids.get(&path)
    {
        return Ok(Resolved::Link(format!("note://{id}")));
    }
    let bytes = read(&path, notes_exchange::MAX_ASSET, cancel)?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let span = if resource.image {
        let (_, dimensions) = media::decode(&bytes)?;
        Span(
            serde_json::json!({"image":hash,"width":dimensions.0,"height":dimensions.1,"alt":resource.label}),
        )
    } else {
        let id = new_id();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("An attachment name is not Unicode.")?
            .to_owned();
        attachments.push(notes_exchange::Attachment {
            id: id.clone(),
            hash: hash.clone(),
            name,
            mime: mime_guess::from_path(&path)
                .first_or_octet_stream()
                .to_string(),
            size: bytes.len() as u64,
        });
        Span(serde_json::json!({"attachment":id}))
    };
    if let std::collections::btree_map::Entry::Vacant(entry) = assets.entry(hash) {
        budget.add(bytes.len())?;
        entry.insert(bytes);
    }
    Ok(if resource.image {
        Resolved::Image(span)
    } else {
        Resolved::Attachment(span)
    })
}
fn replace(blocks: &mut [notes_core::document::Block], resources: &[(Resource, Resolved)]) {
    fn inline_images(span: Span, resources: &[(Resource, Resolved)], spans: &mut Vec<Span>) {
        if !span.is_text() {
            spans.push(span);
            return;
        }
        let text = span.content().to_owned();
        let mut rest = text.as_str();
        while let Some((index, resource, resolved)) = resources
            .iter()
            .filter(|(r, _)| r.image)
            .filter_map(|(r, value)| rest.find(&r.token).map(|index| (index, r, value)))
            .min_by_key(|(index, _, _)| *index)
        {
            if index > 0 {
                spans.push(span.with_text(&rest[..index]));
            }
            match resolved {
                Resolved::Image(image) => spans.push(image.clone()),
                Resolved::Missing(label) => spans.push(span.with_text(&format!("[{label}]"))),
                _ => unreachable!(),
            }
            rest = &rest[index + resource.token.len()..];
        }
        if !rest.is_empty() {
            spans.push(span.with_text(rest));
        }
    }
    for block in blocks {
        let mut spans = vec![];
        let mut pending_attachment: Option<(String, Span)> = None;
        for mut span in std::mem::take(&mut block.spans) {
            let resolved = span
                .0
                .get("href")
                .and_then(|v| v.as_str())
                .and_then(|href| resources.iter().find(|(r, _)| !r.image && r.token == href));
            if let Some((resource, Resolved::Attachment(attachment))) = resolved {
                if pending_attachment
                    .as_ref()
                    .is_some_and(|(token, _)| token != &resource.token)
                    && let Some((_, attachment)) = pending_attachment.take()
                {
                    spans.push(attachment);
                }
                // The link label can contain several differently styled spans.
                // Keep all visible words/marks in order, then one adjacent file
                // object; never replace the label with the filename silently.
                span.0.as_object_mut().unwrap().remove("href");
                inline_images(span, resources, &mut spans);
                pending_attachment = Some((resource.token.clone(), attachment.clone()));
                continue;
            }
            if let Some((_, attachment)) = pending_attachment.take() {
                spans.push(attachment);
            }
            if let Some((_, resolved)) = resolved {
                match resolved {
                    Resolved::Link(href) => span.0["href"] = serde_json::json!(href),
                    _ => {
                        span.0.as_object_mut().unwrap().remove("href");
                    }
                }
            }
            inline_images(span, resources, &mut spans);
        }
        if let Some((_, attachment)) = pending_attachment {
            spans.push(attachment);
        }
        block.spans = spans;
        for cell in &mut block.cells {
            replace(&mut cell.blocks, resources);
        }
    }
}

pub fn prepare(
    paths: Vec<PathBuf>,
    cancel: &Cancellation,
    mut progress: impl FnMut(Progress),
) -> Result<PreparedImport> {
    let mut summary = Summary::default();
    let (sources, folders) = scan(paths, cancel, &mut progress, &mut summary)?;
    let imported_ids: HashMap<_, _> = sources
        .iter()
        .map(|source| (source.path.clone(), new_id()))
        .collect();
    let mut assets = BTreeMap::new();
    let mut notes = vec![];
    let mut budget = Budget {
        bytes: 0,
        resources: 0,
    };
    for (index, source) in sources.iter().enumerate() {
        cancel.check()?;
        let bytes = read(&source.path, notes_exchange::MAX_HTML, cancel)
            .with_context(|| format!("Could not prepare {}.", source.path.display()))?;
        budget.add(bytes.len())?;
        let text = std::str::from_utf8(&bytes).with_context(|| {
            format!(
                "{} is not UTF-8 text. Convert it before importing.",
                source.path.display()
            )
        })?;
        let Imported {
            mut document,
            warnings,
            resources,
        } = if source
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
        {
            Imported {
                document: Document::from_blocks(
                    text.replace("\r\n", "\n")
                        .split("\n\n")
                        .map(notes_core::document::Block::paragraph)
                        .collect(),
                ),
                warnings: vec![],
                resources: vec![],
            }
        } else {
            notes_exchange::file_import::markdown(text)?
        };
        for message in warnings {
            warning(&mut summary, &source.path, message)?;
        }
        let mut attachments = vec![];
        let mut resolved = vec![];
        for resource in resources {
            let value = match resolve(
                &resource,
                source,
                &imported_ids,
                &mut assets,
                &mut attachments,
                &mut budget,
                cancel,
            ) {
                Ok(value) => value,
                Err(error) => {
                    cancel.check()?;
                    // A batch budget is an error, not a lossy-import choice.
                    ensure!(
                        budget.bytes <= MAX_SOURCE_BYTES && budget.resources <= MAX_RESOURCES,
                        "Import limits exceeded. Choose a smaller selection."
                    );
                    warning(
                        &mut summary,
                        &source.path,
                        format!(
                            "{}: {error}; its visible label will be kept.",
                            resource.target
                        ),
                    )?;
                    Resolved::Missing(resource.label.clone())
                }
            };
            resolved.push((resource, value));
        }
        replace(&mut document.blocks, &resolved);
        document.validate()?;
        summary.needs_quotes |= document.schema > 1;
        notes.push(PreparedNote {
            id: imported_ids[&source.path].clone(),
            folder: source.folder.clone(),
            body: document,
            attachments,
        });
        progress(Progress {
            phase: Phase::Prepare,
            completed: index + 1,
            total: sources.len(),
        });
    }
    cancel.check()?;
    summary.files = notes.len();
    summary.folders = folders.len();
    summary.assets = assets.len();
    summary.bytes = budget.bytes as u64;
    Ok(PreparedImport {
        summary,
        notes,
        folders,
        assets,
    })
}

impl Store {
    pub fn import_prepared(
        &mut self,
        prepared: PreparedImport,
        folder: Option<String>,
        accept_warnings: bool,
        cancel: &Cancellation,
        mut progress: impl FnMut(Progress),
    ) -> Result<Report> {
        cancel.check()?;
        ensure!(
            !self.fail_saves,
            "Could not import. Your library is unchanged."
        );
        ensure!(
            accept_warnings || prepared.summary.warnings.is_empty(),
            "Review and accept the import warnings before committing."
        );
        let schema = self.schema()?;
        ensure!(
            schema == 3 || !prepared.summary.needs_quotes,
            "Enable quotes with a verified backup before importing these documents."
        );
        let current = organization::folders(&self.db)?;
        notes_core::organization::parent(&current, None, folder.as_deref())?;
        for (index, (hash, bytes)) in prepared.assets.iter().enumerate() {
            cancel.check()?;
            ensure!(
                media::install(&self.root, bytes)? == *hash,
                "Prepared asset integrity check failed."
            );
            progress(Progress {
                phase: Phase::Assets,
                completed: index + 1,
                total: prepared.assets.len(),
            });
        }
        cancel.check()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let time = now();
        for (hash, bytes) in &prepared.assets {
            tx.execute(
                "INSERT OR IGNORE INTO blobs(hash,size,created_at) VALUES(?1,?2,?3)",
                params![hash, bytes.len() as i64, time],
            )?;
            let stored: u64 =
                tx.query_row("SELECT size FROM blobs WHERE hash=?1", [hash], |r| r.get(0))?;
            ensure!(
                stored == bytes.len() as u64,
                "Existing asset metadata is inconsistent. Your library is unchanged."
            );
        }
        let mut folders = vec![];
        let mut by_source: HashMap<PathBuf, String> = HashMap::new();
        for source in prepared.folders {
            cancel.check()?;
            let parent = match source.parent {
                Some(path) => Some(
                    by_source
                        .get(&path)
                        .context("Import folder order is invalid.")?
                        .clone(),
                ),
                None => folder.clone(),
            };
            let name = available_name(&tx, &source.name, parent.as_deref())?;
            let id = new_id();
            tx.execute("INSERT INTO folders(id,parent_id,name,sort_key,created_at,updated_at) VALUES(?1,?2,?3,0,?4,?4)", params![id, parent, name, time])?;
            by_source.insert(source.path, id.clone());
            folders.push(Folder {
                id,
                parent,
                name,
                sort_key: 0,
            });
        }
        let total = prepared.notes.len();
        let mut notes = vec![];
        for (index, mut source) in prepared.notes.into_iter().enumerate() {
            cancel.check()?;
            let destination = match source.folder {
                Some(path) => Some(
                    by_source
                        .get(&path)
                        .context("Import folder is missing.")?
                        .clone(),
                ),
                None => folder.clone(),
            };
            if schema == 3 {
                source.body.schema = 2;
            }
            let json = serde_json::to_string(&source.body)?;
            tx.execute("INSERT INTO notes(id,folder_id,title,snippet,plain_text,body,body_schema,has_attachments,has_checklist,created_at,updated_at,revision) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,1)", params![source.id, destination, source.body.title(), source.body.snippet(), source.body.plain_text(), json, source.body.schema, media(&source.body), checklist(&source.body), time])?;
            for info in source.attachments {
                tx.execute("INSERT INTO attachments(id,note_id,blob_hash,file_name,mime_type,size,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![info.id, source.id, info.hash, info.name, info.mime, info.size, time])?;
            }
            derived(&tx, &source.id, &source.body)?;
            notes.push(Note {
                id: source.id,
                folder: destination,
                body: source.body,
                revision: 1,
                created_at: time,
                updated_at: time,
                pinned: false,
            });
            progress(Progress {
                phase: Phase::Commit,
                completed: index + 1,
                total,
            });
        }
        cancel.check()?;
        tx.commit()?;
        Ok(Report {
            notes,
            folders,
            summary: prepared.summary,
        })
    }
}
fn available_name(db: &Connection, requested: &str, parent: Option<&str>) -> Result<String> {
    for index in 0..MAX_FILES + 1 {
        let name = if index == 0 {
            requested.to_owned()
        } else {
            let suffix = format!(" ({})", index + 1);
            let max = 200 - suffix.encode_utf16().count();
            let mut units = 0;
            let prefix: String = requested
                .chars()
                .take_while(|ch| {
                    units += ch.len_utf16();
                    units <= max
                })
                .collect();
            format!("{prefix}{suffix}")
        };
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM folders WHERE name=?1 COLLATE NOCASE AND parent_id IS ?2)",
            params![name, parent],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(name);
        }
    }
    anyhow::bail!("Too many folders share this name. Choose another import destination.")
}

#[cfg(test)]
#[path = "file_import/tests.rs"]
mod tests;
