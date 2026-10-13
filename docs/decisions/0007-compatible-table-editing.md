# ADR 0007: table editing preserves compatible structure

Status: implemented increment; M2 and the overall port remain open.
Date: 13 October 2026.

Table commands live in the framework-free editor. A transaction snapshots the
document, validates the resulting structure, rolls back on error and records one
undo step. Surviving blocks and unknown table/cell properties retain their
identities. Database schema 2 and document schema 1 remain unchanged.

The current selection determines the deepest shared table cell. Cross-cell
structural commands fail explicitly; nested edits affect the intended inner
table. Composition must finish before structure changes. Insertion preserves the
current paragraph and its selection text, creates a writable paragraph after the
table when needed, and focuses the first cell. Removal and leaving a trailing
table also preserve a route back to ordinary writing.

Before structural edits, a cloned table materializes implicit empty cells, checks
coverage and refuses overlap. Row/column insertion extends a merge crossing the
insertion boundary. Deletion shrinks a covering merge, retaining its content until
the entire cell disappears. Splitting retains all rich content in the top-left
cell. Dimensional bounds are 1,000 rows and 64 columns, with at most five levels
of table nesting. These limits do not establish usable large-table performance.

Tab selects the next distinct cell in row/column order, skipping covered merge
coordinates. At the end it appends one row. Shift+Tab at the first cell stays
there; the explicit After table action exits. Selection-only navigation does not
create document undo steps. Body Tab follows native focus navigation; list Tab
keeps indent/outdent behavior.

The UI presents a contextual dimension/cell indicator and a small options panel.
Availability comes from the editor model. Only applicable commands are shown;
keyboard focus scrolls options into view and Escape returns to writing. This also
avoids misleading disabled-button states: the pinned AccessKit AT-SPI adapter
reports `Enabled` for disabled button roles. No dependency fork was introduced.
Actual Orca table reading and table/row/cell semantics remain future work.

Evidence uses three independent layers: pure merged/sparse/nested table tests,
real-font GPU input/layout/save/reopen tests, and a native AT-SPI/guarded keyboard
workflow. An authentic `QTextTable` fixture and 38 Rust-edited documents survive
SQLite reopen and the existing Qt application's editable load/read conversion.
The Qt oracle checks known schema fields; Rust tests separately check retention
of unknown metadata. Qt is a test dependency only.

Remaining limitations include creation of new merges, partial cross-cell text
replacement, completely leafless imported tables, wide-table horizontal overflow,
large-document virtualization and full accessible table navigation. The release
still uses Qt while the complete rebuild gates are satisfied.
