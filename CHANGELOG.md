# Changelog

All notable changes to Omarchy Notes are documented here. The project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.1] - 2026-10-13

### Fixed

- Installer/package-name mismatch: prepare new `rust-v1.0.1` artifacts with the
  `omarchy-notes` package name expected by the renamed-package installer.
  Published `rust-v1.0.0` artifacts remain unchanged; the updated installer
  requires the new release artifacts to be published before installation.

### Changed

- Complete the Rust-only Omarchy Notes branding cutover: remove edition labels
  from app and distribution metadata. Rust/GPUI is the only app implementation
  and installer target; executable names, app IDs and data directories are unchanged.
- The Arch package and release archive names are now `omarchy-notes`, replacing
  the former `omarchy-notes-rust` package. Existing data directories are retained;
  newly named release artifacts must be published before the updated installer
  can use them.

### Removed

- Retired Qt/C++/QML app, CMake build, Qt packages and live Qt test probes.
  Frozen legacy library fixtures remain for Rust migration regression tests.

### Added

- Command-line installer with formatted progress, verified Arch release
  packages, explicit source builds, dry runs, and opt-in Quick Note setup.

## [1.0.0] - 2026-09-23

### Added

- Rich note editing: titles and headings, bold, italic, underline,
  strikethrough, highlight and monospace; bulleted, numbered and nested
  checklists; tables; images; file attachments; links between notes.
- Autosave that confirms "Saved" only after the database commit, with retry,
  export and quit protection when a save fails.
- Folders, pinning, sorting, Recently Deleted with a 30-day purge, and
  duplicate/move actions.
- Search as you type with highlighted matches, and find in note.
- #tags, Smart Folders, a gallery view and an attachment browser.
- Quick Note window, desktop actions, command-line capture, and
  single-instance hand-off between launches.
- Markdown and text import (including local images and folders); Markdown,
  HTML and PDF export; export of the whole library as Markdown.
- Verified full-library backup and restore that keeps the replaced library.
- Live Omarchy theme following with contrast correction.
- Arch packaging with desktop entry, AppStream metadata and man page.

[Unreleased]: https://github.com/last-refuge/omarchy-notes/compare/rust-v1.0.1...HEAD
[1.0.1]: https://github.com/last-refuge/omarchy-notes/releases/tag/rust-v1.0.1
[1.0.0]: https://github.com/last-refuge/omarchy-notes/releases/tag/v1.0.0
