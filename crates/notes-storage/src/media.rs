//! Durable content-addressed imports and bounded media reads. Called off the UI thread.
use crate::{Store, now};
use anyhow::{Context, Result, ensure};
use notes_core::document::{Block, Document, Span, new_id};
use notes_exchange::{Asset, Attachment, Bundle, Fragment, MAX_ASSET, MAX_CLIPBOARD};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub enum Import {
    Files {
        paths: Vec<PathBuf>,
        images: Option<bool>,
    },
    Image {
        bytes: Vec<u8>,
        name: String,
    },
    Fragment(Fragment),
}
#[derive(Clone, Debug)]
pub struct Media {
    pub key: String,
    pub hash: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub dimensions: Option<(u32, u32)>,
    pub preview: Option<Preview>,
    pub error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn blob_path(root: &Path, hash: &str) -> Result<PathBuf> {
    ensure!(valid_hash(hash), "Invalid file reference.");
    Ok(root.join("blobs").join(&hash[..2]).join(hash))
}
pub fn read_blob(root: &Path, hash: &str) -> Result<Vec<u8>> {
    let bytes = read_file(&blob_path(root, hash)?)?;
    ensure!(
        self::hash(&bytes) == hash,
        "This file failed its integrity check. Restore it from a backup."
    );
    Ok(bytes)
}
fn read_file(path: &Path) -> Result<Vec<u8>> {
    // Nonblocking open prevents a clipboard file URL naming a FIFO from stalling the writer.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .context("The file is missing or unreadable.")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= MAX_ASSET as u64,
        "Choose a regular file of 32 MiB or less."
    );
    let mut bytes = vec![];
    file.take((MAX_ASSET + 1) as u64).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= MAX_ASSET,
        "The file grew beyond the 32 MiB limit."
    );
    Ok(bytes)
}
fn directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    ensure!(
        !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "A media directory is unexpectedly a symbolic link."
    );
    Ok(())
}
pub(super) fn install(root: &Path, bytes: &[u8]) -> Result<String> {
    ensure!(bytes.len() <= MAX_ASSET, "The file exceeds 32 MiB.");
    let digest = hash(bytes);
    let target = blob_path(root, &digest)?;
    directory(&root.join("staging"))?;
    directory(&root.join("blobs"))?;
    directory(target.parent().unwrap())?;
    if target.exists() {
        ensure!(
            read_blob(root, &digest)? == bytes,
            "A stored file does not match its content address."
        );
        return Ok(digest);
    }
    let temp = root.join("staging").join(format!("{}.part", new_id()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        match std::fs::hard_link(&temp, &target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                ensure!(read_blob(root, &digest)? == bytes, "File collision.");
            }
            Err(e) => return Err(e.into()),
        }
        for path in [target.parent().unwrap(), &root.join("blobs"), root] {
            std::fs::File::open(path)?.sync_all()?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(temp);
    result?;
    Ok(digest)
}
/// Decode with the same format, dimension and allocation limits as an import.
pub fn decode(bytes: &[u8]) -> Result<(image::DynamicImage, (u32, u32))> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let dims = reader
        .into_dimensions()
        .context("This image is unsupported or damaged. Try a PNG, JPEG, WebP or GIF file.")?;
    ensure!(
        dims.0 > 0
            && dims.1 > 0
            && dims.0 <= 12000
            && dims.1 <= 12000
            && u64::from(dims.0) * u64::from(dims.1) <= 24_000_000,
        "Choose an image up to 24 megapixels and 12,000 pixels per side."
    );
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12000);
    limits.max_image_height = Some(12000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .context("This image could not be decoded within the memory limit.")?;
    Ok((decoded, dims))
}
fn clean_name(name: &str) -> String {
    let name = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Attachment");
    let result: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .take(180)
        .collect();
    if result.is_empty() || result == "." || result == ".." {
        "Attachment".into()
    } else {
        result
    }
}
fn mime(name: &str, bytes: &[u8]) -> String {
    if bytes.starts_with(b"%PDF-") {
        "application/pdf".into()
    } else if let Ok(format) = image::guess_format(bytes) {
        format.to_mime_type().into()
    } else {
        mime_guess::from_path(name)
            .first_or_octet_stream()
            .to_string()
    }
}
pub(super) fn attachment(db: &Connection, id: &str) -> Result<Attachment> {
    db.query_row("SELECT id,blob_hash,file_name,mime_type,size FROM attachments WHERE id=?1",[id],|r|Ok(Attachment{id:r.get(0)?,hash:r.get(1)?,name:r.get(2)?,mime:r.get(3)?,size:r.get(4)?})).optional()?.context("This selection references an attachment that is unavailable in this library. Copy it from a Rust edition with portable media, or attach the file.")
}
fn readonly(root: &Path) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        root.join("library.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

/// Read attachment metadata without acquiring a writer or changing the library.
pub fn attachment_info(root: &Path, id: &str) -> Result<Attachment> {
    attachment(&readonly(root)?, id)
}

impl Store {
    /// Install immutable bytes, then commit their metadata; the UI inserts the result as one undo group.
    pub fn import_media(&mut self, note_id: &str, input: Import) -> Result<Document> {
        ensure!(
            !self.fail_saves,
            "Couldn’t store the files. Your note is unchanged."
        );
        let exists: i64 = self.db.query_row(
            "SELECT count(*) FROM notes WHERE id=?1 AND deleted_at IS NULL",
            [note_id],
            |r| r.get(0),
        )?;
        ensure!(exists == 1, "The destination note is no longer available.");
        let mut fragment = match input {
            Import::Fragment(fragment) => fragment,
            input => {
                let mut sources = vec![];
                match input {
                    Import::Files { paths, images } => {
                        ensure!(
                            !paths.is_empty() && paths.len() <= 16,
                            "Choose 1–16 files at a time."
                        );
                        let mut total = 0;
                        for path in paths {
                            let name = clean_name(
                                path.file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("Attachment"),
                            );
                            let bytes = read_file(&path)?;
                            total += bytes.len();
                            ensure!(total <= MAX_ASSET, "Import up to 32 MiB at a time.");
                            let inline =
                                images.unwrap_or_else(|| image::guess_format(&bytes).is_ok());
                            sources.push((name, bytes, inline));
                        }
                    }
                    Import::Image { bytes, name } => sources.push((clean_name(&name), bytes, true)),
                    _ => unreachable!(),
                }
                ensure!(
                    sources.iter().map(|(_, b, _)| b.len()).sum::<usize>() <= MAX_ASSET,
                    "Import up to 32 MiB at a time."
                );
                let mut doc = Document {
                    blocks: vec![],
                    ..Default::default()
                };
                let mut bundle = Bundle {
                    version: 1,
                    ..Default::default()
                };
                for (name, bytes, images) in sources {
                    let digest = hash(&bytes);
                    let kind = mime(&name, &bytes);
                    let span = if images {
                        let (_, dims) = decode(&bytes)?;
                        Span(
                            serde_json::json!({"image":digest,"width":dims.0,"height":dims.1,"alt":name}),
                        )
                    } else {
                        let id = new_id();
                        bundle.attachments.push(Attachment {
                            id: id.clone(),
                            hash: digest.clone(),
                            name,
                            mime: kind,
                            size: bytes.len() as u64,
                        });
                        Span(serde_json::json!({"attachment":id}))
                    };
                    if !bundle.assets.iter().any(|a| a.hash == digest) {
                        bundle.assets.push(Asset::new(digest, &bytes));
                    }
                    let mut block = Block::paragraph("");
                    block.spans.push(span);
                    doc.blocks.push(block);
                }
                Fragment {
                    document: doc,
                    bundle,
                }
            }
        };
        fragment.document.validate()?;
        let mut assets = BTreeMap::new();
        let mut total = 0;
        for asset in &fragment.bundle.assets {
            ensure!(valid_hash(&asset.hash), "Invalid clipboard asset address.");
            let bytes = asset.bytes()?;
            total += bytes.len();
            ensure!(total <= MAX_ASSET, "Paste up to 32 MiB of files at a time.");
            ensure!(
                hash(&bytes) == asset.hash,
                "Clipboard file data failed its integrity check."
            );
            ensure!(
                assets.insert(asset.hash.clone(), bytes).is_none(),
                "Duplicate clipboard asset."
            );
        }
        let mut attachments = vec![];
        for id in fragment.document.references("attachment") {
            let info = fragment
                .bundle
                .attachments
                .iter()
                .find(|a| a.id == id)
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| attachment(&self.db, &id))?;
            ensure!(valid_hash(&info.hash), "Invalid attachment address.");
            if !assets.contains_key(&info.hash) {
                let bytes = read_blob(&self.root, &info.hash)?;
                total += bytes.len();
                ensure!(total <= MAX_ASSET, "Paste up to 32 MiB of files at a time.");
                assets.insert(info.hash.clone(), bytes);
            }
            ensure!(
                assets[&info.hash].len() as u64 == info.size,
                "Attachment size does not match its data."
            );
            attachments.push(info);
        }
        for digest in fragment.document.references("image") {
            if !assets.contains_key(&digest) {
                let bytes = read_blob(&self.root, &digest)?;
                total += bytes.len();
                ensure!(total <= MAX_ASSET, "Paste up to 32 MiB of files at a time.");
                assets.insert(digest.clone(), bytes);
            }
            decode(&assets[&digest])?;
        }
        ensure!(
            assets.values().map(Vec::len).sum::<usize>() <= MAX_ASSET,
            "Paste up to 32 MiB of files at a time."
        );
        // Ignore extra unreferenced asset data supplied by a clipboard owner.
        let wanted: BTreeSet<_> = fragment
            .document
            .references("image")
            .into_iter()
            .chain(attachments.iter().map(|a| a.hash.clone()))
            .collect();
        for digest in &wanted {
            install(&self.root, &assets[digest])?;
        }
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for digest in &wanted {
            tx.execute(
                "INSERT OR IGNORE INTO blobs(hash,size,created_at) VALUES(?1,?2,?3)",
                params![digest, assets[digest].len() as i64, now()],
            )?;
        }
        let mut remap = BTreeMap::new();
        for info in attachments {
            let id = new_id();
            let name = clean_name(&info.name);
            let kind = mime(&name, &assets[&info.hash]);
            tx.execute("INSERT INTO attachments(id,note_id,blob_hash,file_name,mime_type,size,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,note_id,info.hash,name,kind,info.size,now()])?;
            remap.insert(info.id, id);
        }
        let ids: Vec<_> = fragment
            .document
            .leaves()
            .iter()
            .map(|b| b.id.clone())
            .collect();
        for id in ids {
            for span in &mut fragment.document.block_mut(&id).unwrap().spans {
                if let Some(old) = span.0.get("attachment").and_then(|v| v.as_str())
                    && let Some(new) = remap.get(old)
                {
                    span.0["attachment"] = serde_json::json!(new);
                }
            }
        }
        tx.commit()?;
        Ok(fragment.document)
    }
}
pub fn portable(root: &Path, document: Document) -> Result<Fragment> {
    document.validate()?;
    let db = readonly(root)?;
    let attachments = document
        .references("attachment")
        .iter()
        .map(|id| attachment(&db, id))
        .collect::<Result<Vec<_>>>()?;
    let hashes: BTreeSet<_> = document
        .references("image")
        .into_iter()
        .chain(attachments.iter().map(|a| a.hash.clone()))
        .collect();
    ensure!(hashes.len() <= 64, "Copy fewer files at once.");
    let mut assets = vec![];
    let mut total = 0;
    for digest in hashes {
        let bytes = read_blob(root, &digest)?;
        total += bytes.len();
        ensure!(total <= MAX_ASSET, "Copy up to 32 MiB of files at a time.");
        assets.push(Asset::new(digest, &bytes));
    }
    let result = Fragment {
        document,
        bundle: Bundle {
            version: 1,
            assets,
            attachments,
        },
    };
    ensure!(
        result.encode()?.len() <= MAX_CLIPBOARD,
        "This selection is too large."
    );
    Ok(result)
}
pub fn load(root: &Path, document: &Document) -> Result<Vec<Media>> {
    let db = readonly(root)?;
    let mut media = vec![];
    let mut allocated = 0;
    for digest in document.references("image") {
        let span = document
            .leaves()
            .iter()
            .flat_map(|b| &b.spans)
            .find(|s| s.0.get("image").and_then(|v| v.as_str()) == Some(&digest))
            .cloned()
            .unwrap();
        let name = span
            .0
            .get("alt")
            .and_then(|v| v.as_str())
            .unwrap_or("Image")
            .to_owned();
        let mut item = Media {
            key: format!("image:{digest}"),
            hash: digest.clone(),
            name,
            mime: "image/*".into(),
            size: 0,
            dimensions: None,
            preview: None,
            error: None,
        };
        let result = (|| -> Result<()> {
            let bytes = read_blob(root, &digest)?;
            item.size = bytes.len() as u64;
            item.mime = mime(&item.name, &bytes);
            ensure!(
                allocated < 48 * 1024 * 1024,
                "Preview memory limit reached. Open the image to view it."
            );
            let (image, dims) = decode(&bytes)?;
            item.dimensions = Some(dims);
            let mut rgba = image.thumbnail(1400, 1000).to_rgba8();
            let (width, height) = rgba.dimensions();
            allocated += rgba.len();
            ensure!(
                allocated <= 48 * 1024 * 1024,
                "Preview memory limit reached. Open the image to view it."
            );
            for pixel in rgba.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            item.preview = Some(Preview {
                width,
                height,
                bgra: rgba.into_raw(),
            });
            Ok(())
        })();
        item.error = result.err().map(|e| e.to_string());
        media.push(item);
    }
    for id in document.references("attachment") {
        match attachment(&db, &id) {
            Ok(a) => {
                let error = read_blob(root, &a.hash).err().map(|e| e.to_string());
                media.push(Media {
                    key: format!("attachment:{id}"),
                    hash: a.hash,
                    name: a.name,
                    mime: a.mime,
                    size: a.size,
                    dimensions: None,
                    preview: None,
                    error,
                });
            }
            Err(e) => media.push(Media {
                key: format!("attachment:{id}"),
                hash: String::new(),
                name: "Missing attachment".into(),
                mime: String::new(),
                size: 0,
                dimensions: None,
                preview: None,
                error: Some(e.to_string()),
            }),
        }
    }
    Ok(media)
}
/// Prepare a private non-executable copy for a default application. Never execute a blob.
pub fn open_copy(root: &Path, item: &Media) -> Result<PathBuf> {
    let bytes = read_blob(root, &item.hash)?;
    ensure!(
        !bytes.starts_with(b"\x7fELF") && !bytes.starts_with(b"MZ") && !bytes.starts_with(b"#!"),
        "Executable files can only be saved to a location you choose."
    );
    let mut name = clean_name(&item.name);
    if item.key.starts_with("image:") {
        let format = image::guess_format(&bytes)?;
        let extension = format.extensions_str()[0];
        if !name
            .to_ascii_lowercase()
            .ends_with(&format!(".{extension}"))
        {
            name.push_str(&format!(".{extension}"));
        }
    }
    let extension = Path::new(&name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        matches!(
            extension.as_str(),
            "pdf"
                | "txt"
                | "md"
                | "csv"
                | "json"
                | "png"
                | "jpg"
                | "jpeg"
                | "webp"
                | "gif"
                | "odt"
                | "ods"
                | "docx"
                | "xlsx"
                | "pptx"
                | "mp3"
                | "mp4"
                | "wav"
                | "ogg"
        ),
        "Save this file first, then open it with an application you choose."
    );
    directory(&root.join("opened"))?;
    let dir = root.join("opened").join(new_id());
    directory(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join(name);
    write_new(&path, &bytes)?;
    Ok(path)
}
pub fn export_file(root: &Path, item: &Media, destination: &Path) -> Result<()> {
    write_new(destination, &read_blob(root, &item.hash)?)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .context("Choose a new filename; existing files are never overwritten.")?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Give a new note independent metadata ownership while reusing immutable blobs.
pub(super) fn copy_attachments(
    db: &Connection,
    document: &mut Document,
) -> Result<Vec<Attachment>> {
    let mut copies = vec![];
    let mut remap = BTreeMap::new();
    for old in document.references("attachment") {
        let mut info = attachment(db, &old)?;
        info.id = new_id();
        remap.insert(old, info.id.clone());
        copies.push(info);
    }
    let ids: Vec<_> = document.leaves().iter().map(|b| b.id.clone()).collect();
    for id in ids {
        for span in &mut document.block_mut(&id).unwrap().spans {
            if let Some(old) = span.0.get("attachment").and_then(|v| v.as_str())
                && let Some(new) = remap.get(old)
            {
                span.0["attachment"] = serde_json::json!(new);
            }
        }
    }
    Ok(copies)
}
