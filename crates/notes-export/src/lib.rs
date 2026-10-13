//! Qt-free PDF export. Call on a worker with an immutable document snapshot.
mod layout;
use anyhow::{Context, Result, ensure};
use cairo::{Context as Canvas, PdfMetadata, PdfSurface};
use notes_core::document::Document;
use std::{
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Paper {
    #[default]
    Letter,
    A4,
}
impl Paper {
    pub fn dimensions(self) -> (f64, f64) {
        match self {
            Self::Letter => (612., 792.),
            Self::A4 => (595.276, 841.89),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Letter => "US Letter",
            Self::A4 => "A4",
        }
    }
}
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    fn check(&self) -> Result<()> {
        ensure!(!self.0.load(Ordering::Acquire), "Export cancelled.");
        Ok(())
    }
}
#[derive(Debug)]
pub struct Report {
    pub pages: usize,
    pub attachment_labels: usize,
}

/// Create a private, complete PDF at a new path. Existing files are never replaced.
/// Attachments print as labels, not embedded files. The source library is read-only.
/// Cancellation is cooperative until the final atomic publish; after that, success wins.
pub fn export(
    root: &Path,
    document: &Document,
    destination: &Path,
    paper: Paper,
    cancel: &Cancellation,
) -> Result<Report> {
    cancel.check()?;
    document.validate()?;
    ensure!(
        document.plain_text().len() <= 4 * 1024 * 1024 && document.leaves().len() <= 10_000,
        "PDF export supports up to 4 MiB of text and 10,000 paragraphs. Export a smaller note."
    );
    ensure!(
        document
            .leaves()
            .iter()
            .map(|b| b.spans.len())
            .sum::<usize>()
            <= 100_000,
        "PDF export supports up to 100,000 styled spans. Export a smaller note."
    );
    ensure!(
        document
            .leaves()
            .iter()
            .all(|b| b.spans.iter().map(|s| s.content().len()).sum::<usize>() <= 512 * 1024),
        "A paragraph exceeds the 512 KiB PDF layout limit. Split it into smaller paragraphs."
    );
    ensure!(
        !destination.try_exists()?,
        "Choose a new filename. Existing files are never overwritten."
    );
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".notes-pdf-")
        .tempfile_in(parent)
        .context("Couldn’t create the PDF in this folder.")?;
    let (width, height) = paper.dimensions();
    let surface = PdfSurface::for_stream(
        width,
        height,
        LimitedWriter {
            file: temporary.reopen()?,
            written: 0,
        },
    )?;
    let title = document.title().replace('\0', "");
    surface.set_metadata(
        PdfMetadata::Title,
        if title.is_empty() {
            "Untitled note"
        } else {
            &title
        },
    )?;
    surface.set_metadata(PdfMetadata::Creator, "Omarchy Notes — Rust edition")?;
    let canvas = Canvas::new(&surface)?;
    let mut engine = layout::Engine::new(root, &canvas, paper, cancel);
    let plan = engine.paginate(document)?;
    for (index, page) in plan.iter().enumerate() {
        cancel.check()?;
        engine.paint_page(&canvas, page, index, plan.len(), &title)?;
        canvas.show_page()?;
    }
    drop(canvas);
    // Finishing a Cairo surface can fail late (including a full disk).
    surface
        .finish_output_stream()
        .map_err(|e| anyhow::anyhow!("Couldn’t finish PDF: {e}"))?;
    surface.status()?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    cancel.check()?;
    temporary
        .persist_noclobber(destination)
        .map_err(|e| anyhow::anyhow!("Couldn’t publish PDF. Choose a new filename. {}", e.error))?;
    // A directory-sync error means the complete file exists, but durability is uncertain.
    std::fs::File::open(parent)?.sync_all().context(
        "The PDF was created, but its folder could not be synced. Check the file before retrying.",
    )?;
    Ok(Report {
        pages: plan.len(),
        attachment_labels: engine.attachment_labels,
    })
}
struct LimitedWriter {
    file: std::fs::File,
    written: usize,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.written.saturating_add(bytes.len()) > 128 * 1024 * 1024 {
            return Err(std::io::Error::other(
                "PDF exceeds the 128 MiB export limit.",
            ));
        }
        let n = self.file.write(bytes)?;
        self.written += n;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
