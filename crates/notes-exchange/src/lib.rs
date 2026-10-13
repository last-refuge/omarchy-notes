//! Bounded, inert clipboard conversion. No network or filesystem access.
pub mod html;
pub mod markdown;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use notes_core::document::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FRAGMENT_MIME: &str = "application/x-omarchy-notes-fragment+json";
pub const MAX_CLIPBOARD: usize = 48 * 1024 * 1024;
pub const MAX_ASSET: usize = 32 * 1024 * 1024;
pub const MAX_HTML: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub hash: String,
    pub data: String,
}
impl Asset {
    pub fn new(hash: String, bytes: &[u8]) -> Self {
        Self {
            hash,
            data: STANDARD.encode(bytes),
        }
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.data.len() <= MAX_ASSET * 4 / 3 + 4,
            "The clipboard file is too large."
        );
        Ok(STANDARD.decode(&self.data)?)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub hash: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bundle {
    pub version: u32,
    pub assets: Vec<Asset>,
    pub attachments: Vec<Attachment>,
}
#[derive(Clone, Debug)]
pub struct Fragment {
    pub document: Document,
    pub bundle: Bundle,
}
impl Fragment {
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.document.validate()?;
        let mut doc = self.document.clone();
        doc.extra.insert(
            "omarchy_clipboard".into(),
            serde_json::to_value(&self.bundle)?,
        );
        let bytes = serde_json::to_vec(&doc)?;
        ensure!(
            bytes.len() <= MAX_CLIPBOARD,
            "This selection is too large for the clipboard. Copy fewer items."
        );
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CLIPBOARD,
            "The clipboard selection is too large."
        );
        let mut doc: Document = serde_json::from_slice(bytes)?;
        let bundle = doc
            .extra
            .remove("omarchy_clipboard")
            .map(serde_json::from_value::<Bundle>)
            .transpose()?
            .unwrap_or_default();
        ensure!(
            bundle.version <= 1 && bundle.assets.len() <= 64 && bundle.attachments.len() <= 64,
            "Unsupported clipboard asset bundle."
        );
        doc.validate()?;
        Ok(Self {
            document: doc,
            bundle,
        })
    }
    pub fn offers(&self) -> Result<BTreeMap<String, Vec<u8>>> {
        let mut offers = BTreeMap::from([
            (FRAGMENT_MIME.into(), self.encode()?),
            (
                "text/html".into(),
                html::export(&self.document).into_bytes(),
            ),
            (
                "text/plain;charset=utf-8".into(),
                self.document.plain_text().into_bytes(),
            ),
            ("text/plain".into(), self.document.plain_text().into_bytes()),
        ]);
        let spans: Vec<_> = self
            .document
            .leaves()
            .into_iter()
            .flat_map(|b| &b.spans)
            .filter(|s| !s.is_text() || !s.content().trim().is_empty())
            .collect();
        if spans.len() == 1
            && let Some(hash) = spans[0].0.get("image").and_then(|v| v.as_str())
            && let Some(asset) = self.bundle.assets.iter().find(|a| a.hash == hash)
        {
            let bytes = asset.bytes()?;
            let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                Some("image/png")
            } else if bytes.starts_with(b"\xff\xd8\xff") {
                Some("image/jpeg")
            } else if bytes.starts_with(b"GIF8") {
                Some("image/gif")
            } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
                Some("image/webp")
            } else {
                None
            };
            if let Some(mime) = mime {
                offers.insert(mime.into(), bytes);
            }
        }
        Ok(offers)
    }
}
pub fn safe_link(value: &str) -> bool {
    notes_core::links::LinkTarget::parse(value).is_ok()
}
pub fn file_urls(bytes: &[u8]) -> Result<Vec<std::path::PathBuf>> {
    ensure!(bytes.len() <= MAX_HTML, "The file selection is too large.");
    let mut paths = vec![];
    for line in std::str::from_utf8(bytes)?
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let u = url::Url::parse(line)?;
        ensure!(
            u.scheme() == "file"
                && u.host_str()
                    .is_none_or(|h| h.is_empty() || h == "localhost"),
            "Only local files can be attached. Remote files were not downloaded."
        );
        paths.push(
            u.to_file_path()
                .map_err(|_| anyhow::anyhow!("Invalid local file address."))?,
        );
    }
    ensure!(
        !paths.is_empty() && paths.len() <= 16,
        "Choose between 1 and 16 files."
    );
    Ok(paths)
}
