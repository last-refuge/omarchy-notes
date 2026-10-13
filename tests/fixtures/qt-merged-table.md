# Qt merged-table fixture

`qt-merged-table.json` was generated on 13 October 2026 with Qt 6.11.2 by
`tests/qt-table-probe.cpp` using a real `QTextDocument`, a 3 × 3 `QTextTable`,
`mergeCells(0, 0, 2, 2)` and the application's `DocumentConverter::read`.
The merged cell retains the original rich paragraphs, combining characters,
family emoji and Chinese text. The frozen generated block IDs are intentional.

SHA-256: `9fe2d9e8c614c217e78fdb24e8d4e47b7a6db5e0ee534080839a569958699812`.

Reproduction in a scratch directory (the generator refuses existing output):

```sh
g++ -std=c++20 -fPIC -Isrc/core tests/qt-table-probe.cpp \
  src/core/RichDocument.cpp src/core/DocumentConverter.cpp \
  $(pkg-config --cflags --libs Qt6Core Qt6Gui) -o /tmp/notes-qt-table-probe
QT_QPA_PLATFORM=offscreen /tmp/notes-qt-table-probe create /tmp/notes-merged-table.json
```

New captures have fresh IDs; do not overwrite the frozen fixture on every test.
The compatibility check builds the oracle in its own temporary directory:

```sh
cargo build -p notes-storage --example table-roundtrip --locked
python tests/test-rust-tables.py target/debug/examples/table-roundtrip
```

It verifies the fixture and 38 edited documents through actual Qt editable
load/read conversion, as well as Rust SQLite save/reopen and undo/redo. Exact
known-schema equality is required; unknown-property retention is tested in Rust.
