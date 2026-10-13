# Rust notes: import, export and backups

Open **Settings → Your notes and files → Import, export & backups**. These
routes are implemented in the release branch; final native/package acceptance
is still pending.

## Import a copy

Choose Markdown/text files or a folder. Notes scans at most 1,000 text files
and 64 MiB of text and unique asset bytes. It refuses symbolic links and only
copies local resources contained by the selected folder or file's parent.
Sources are never edited. Folder structure is copied with fresh identities;
links between imported notes are mapped to their new identities.

Review the file counts and warnings before **Accept & import**. Missing or
remote files and unsupported formatting are listed before copying. The reviewed
bytes are retained, so changing a source file during review cannot silently
change the accepted import. Quotation content requires the existing explicit
format upgrade with a verified backup. Cancellation or a failed database commit
does not leave partially imported notes or folders.

## Export portable notes

A note export captures the current draft when its chooser opens. Library
Markdown export includes committed live notes. Every export uses a new folder,
copies referenced assets into `assets/`, resolves included note links to relative
filenames, and writes `export.json` with filenames, assets, saved revisions and
limitations. Duplicate titles receive distinct filenames; Unicode names are
retained and link paths are percent encoded.

Markdown uses inline HTML for formatting, tables and media that need it. HTML
includes local image/file links. Some application attributes and revision history
cannot be represented; keep a verified backup when those matter. PDF remains a
separate snapshot export and lists attached files without including their bytes.

## Back up and restore

**Back Up Library** waits for main and Quick Note drafts to settle and save.
A draft error prevents an incomplete backup from being reported as successful.
After the snapshot starts, return to writing while the worker verifies the copy.
Newer drafts stay in the app; another backup is needed to include them.

A backup contains the complete database, Recently Deleted, revision history and
verified immutable asset bytes. Its manifest records the exact committed
note/revision boundary and database hash. Inspect verifies the actual copied
files, counts and references. Progress and cancellation run outside the input
path. Cancellation before publication removes the private staging directory;
existing destinations and completed copies are preserved.

**Inspect Backup** selects a verified source. **Restore inspected backup**
copies it into a new folder and verifies it again. It preserves the backup and
the current notebook, then offers **Open restored library**.

## Open another library

**Open Library** validates an existing library and opens a separate process/window
with its own library ownership and settings. A confirmed window receipt is
required before the app reports success. Current main and Quick Note drafts stay
available, including when verification or opening fails. Close the old window
when convenient; its ordinary save/recovery guards still apply. Opening an
already owned Rust library routes to its existing window. A Qt owner must close
before the separate Rust edition can acquire that library.

Use copies and verified backups when moving Qt data. A quotation-format upgrade
requires Rust; the pre-upgrade backup is the rollback boundary for Qt.
