# Moving your notes into Omarchy Notes

The current app is Rust-only. Qt is retired; compatibility with its library and
backup formats exists for legacy migration, not as a supported installation route.

## Markdown and text imports

Open **Settings → Your notes and files → Import, export & backups** and choose
Markdown/text files or a folder. Sources are never edited. Review counts and
warnings before **Accept & import**. Folder structure and links between imported
notes receive new identities; supported local assets are copied.

Imports are bounded to 1,000 text files and 64 MiB of text and unique asset bytes.
Symbolic links are refused, and local resources must be within the selected folder
or file's parent. Missing/remote assets and unsupported formatting are reported
before copying. Quotation content requires an explicit format upgrade with a
verified backup. See [data ownership](rust-data-ownership.md) for exact boundaries.

### From Apple Notes

Use a Markdown exporter on your Mac, then copy its exported folders and assets to
your Omarchy machine and import the folder. Exporter output varies: review warnings
and check important notes after import. Unlock protected notes before export;
drawings or scans need an exporter that includes their image files.

### From Obsidian, Logseq, Bear, Joplin and others

Export to Markdown or select a copy of the Markdown folder. For Joplin, use
**File → Export all → MD – Markdown**. Application-specific metadata and wiki links
may not transfer as functional structures. Tags and Smart Folder interfaces are
not implemented in Rust; do not expect `#tags` to create browsable tag filters.
Review the supported formatting and asset warnings rather than assuming parity
with the source app.

## From the retired Qt app

The former default library is `~/.local/share/omarchy-notes/`; the Rust profile
remains `~/.local/share/omarchy-notes-gpui/`. Corresponding XDG overrides apply.
Rust does not automatically open, rename or upgrade the former library, and does
not import Qt's `settings.ini` as Rust preferences.

1. Close any legacy app using the library and preserve a consistent copy plus a
   verified backup. Never experiment on the only original.
2. Open the copied compatible library with **Open Library** in Settings, or
   `omarchy-notes-rust --data-dir /path/to/copied-library`. For a format-1 backup,
   use **Inspect Backup**, then restore into a new directory.
3. Check notes and assets. Existing tags/Smart Folder metadata, merged tables and
   revisions are preserved, but their deferred interfaces do not become available.
4. Enabling quotations explicitly upgrades to schema 3 and creates a verified
   pre-upgrade backup. Legacy Qt cannot read the upgraded library. A legacy rollback
   uses that backup and excludes later Rust edits; preserve the newer library and
   back it up or export before any rollback.

Only Rust owners provide the durable Rust capture/launch protocol. A legacy owner
must close before Rust can acquire the same library. Legacy media clipboard data
requires referenced assets already in the receiving library; cross-library rich
media exchange with Qt is not supported.

## Leaving again

Use Settings to export a note as Markdown/HTML, or a library as Markdown with
assets. PDF is a separate snapshot export and does not include attached file bytes.
Portable formats cannot preserve every application attribute or revision history.
For an exact move to another Rust installation, **Back Up Library**, inspect the
backup, and restore into a new library. The source backup and current notebook
remain intact. Backups are unencrypted; store them somewhere trusted.
