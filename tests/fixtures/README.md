# Regression fixtures

`preview/library.json` contains deterministic, disposable GPUI sample content,
not a persisted rich-document schema.

`qt-v1/` is a frozen schema-2 SQLite library captured from the retired Qt app,
with two content-addressed blobs and JSON exports of its schema-1 note bodies.
The documents cover headings, marks, nested lists, checklists, tables, images,
attachments, note links and multilingual text. `manifest.json` records checksums;
it is a test manifest, not an app backup. `qt-merged-table.json` captures legacy
merged-cell behavior; its provenance is recorded in `qt-merged-table.md`.

Keep these captures fixed. Rust compatibility tests use scratch copies to verify
that existing libraries and rich documents remain readable and preserve data.
The Qt implementation and fixture-generation tools have been retired; running
the regression tests does not require Qt.

## Rust checks

`notes-core` parses and round-trips captured documents. `notes-editor` verifies
edits preserve neighboring structure and assets. `notes-storage` covers legacy
schema upgrades, conflicts, version guards, backup/restore and durable writes.

```sh
cargo test --workspace --locked
```

GPUI native workflows use the `notes-ui` examples and the Python native harnesses
in `tests/`. Always use copies of fixtures for edits.
