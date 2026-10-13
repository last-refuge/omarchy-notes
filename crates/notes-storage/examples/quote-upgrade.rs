//! Test driver: explicitly upgrade a supplied scratch library, then write a quote.
use anyhow::{Context, Result, ensure};
use notes_core::document::{Block, Document};
use notes_storage::{Store, migration::Cancellation};
use std::path::PathBuf;
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().context("Expected scratch library path")?);
    let backup = PathBuf::from(args.next().context("Expected pre-upgrade backup path")?);
    ensure!(
        root.join("library.sqlite3").is_file(),
        "Use an existing scratch library."
    );
    let mut store = Store::open(&root)?;
    let receipt = store.upgrade_for_quotes(&backup, &Cancellation::default(), |_| {})?;
    let mut quote = Block::paragraph("A quotation worth keeping.");
    quote.set_kind("quote");
    store.create(Document::from_blocks(vec![quote]), None)?;
    println!(
        "Upgraded scratch library; verified backup: {}",
        receipt.backup.display()
    );
    Ok(())
}
