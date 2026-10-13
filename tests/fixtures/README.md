# Rebuild fixtures

`preview/library.json` contains deterministic, disposable sample content for the
GPUI visual foundation. It is **not** a persisted rich-document schema. Quote
blocks here do not authorize changing an existing library's format.

`qt-v1/` contains a real schema-2 SQLite library produced by the current Qt
application's `--samples` path, its two content-addressed blobs, and a JSON export
of the three schema-1 note bodies. The JSON export includes headings, marks,
nested lists, checklists, a table, image, attachment, note link and multilingual
text. `manifest.json` records checksums and is a test manifest, not an app backup.

Regenerate to a **new** directory after building the Qt app:

```sh
python tools/capture-qt-fixture.py build-baseline/src/app/omarchy-notes /tmp/new-qt-fixture
```

The tool launches only the supplied Qt binary with a newly allocated temporary
library and offscreen rendering. It takes a consistent SQLite backup, validates
blob hashes and terminates its own child. It refuses an existing destination.
Generated IDs, timestamps and PDF metadata will differ between captures. Keep the
checked-in capture fixed for compatibility tests; use scratch copies for writes.

Additional fixtures still needed before M2 exits: schema-1 database upgrade,
merged-cell/edge-case tables, a legacy backup, missing/corrupt blobs and a newer
unsupported schema. See the rebuild baseline checklist.

## Rust writing checks

`notes-core` parses and round-trips every captured document. `notes-editor` edits
a table cell without changing adjacent structure or asset references.
`notes-storage` saves fixture copies, checks conflicts/version guards and tests
a transactional schema-1 upgrade. The durability integration test kills its own
writer after 20 commit acknowledgments, then verifies every note on reopen.

The cross-version probe additionally compiles the current Qt document reader and
uses `QLockFile`, with scratch libraries only:

```sh
cargo build --release -p omarchy-notes-preview --locked
cargo build -p notes-storage --example compatibility-edit --locked
python tests/test-rust-compatibility.py target/release/omarchy-notes-preview target/debug/examples/compatibility-edit
```

Requires Qt6Core/sqlite3 development headers, pkg-config, g++ and Python. The
Rust app itself does not link Qt. GPUI screenshots and writing workflows are
covered by `notes-ui`'s `writing-check` example; use copies of fixtures for edits.
