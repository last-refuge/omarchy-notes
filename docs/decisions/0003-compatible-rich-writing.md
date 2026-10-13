# 0003 — Compatible rich writing before schema expansion

Status: accepted for the writing increment, 12 October 2026.
Media and clipboard limitations below are superseded by [ADR 0004](0004-media-and-rich-clipboard.md);
this document preserves the original writing increment’s scope.

## Decision

Keep Qt's SQLite schema 2 and rich-document JSON schema 1. The Rust edition uses
its own default data directory and preview application identity. Explicit
`--data-dir` can open a compatible copy. No quote block or document-v2 migration
is introduced, and the Qt installer/release remains unchanged.

`notes-core::document` represents the existing format, retaining stable IDs,
properties, marks, links, nested cell content and immutable asset references.
Unknown versions/kinds are rejected. A separate `notes-editor` owns document
selection, transactions, composition, rich fragments and per-note undo without
any GPUI dependency. The original visual sample remains opt-in with `--demo`.

`notes-ui::RichEditor` implements the pinned GPUI input-handler contract, paints
styled/wrapped runs and maps document positions to real caret geometry. Tables
are laid out from their existing cells; editing stays inside one cell/container.
Destructive cross-cell replacement is refused. Media references stay intact and
are visibly labeled until media rendering/insertion is implemented. Selection
and undo belong to the document, independent of painted content.

## Storage contract

`notes-storage` owns SQLite on one dedicated thread behind a bounded queue of 32
requests. Schema upgrades and note mutations are transactions. Writes use WAL,
`foreign_keys=ON` and `synchronous=FULL`. Title, snippet, plain text, tags, media
references and FTS update with the body. History uses the existing ten-minute
snapshot interval. There is no blob garbage collector in this increment.

The startup read-only probe checks database version, integrity and stored body
versions before migrations. Loading validates active documents and refuses an
unreadable note instead of substituting a blank document. Schema 1 upgrades to 2
atomically. A future schema is left unchanged.

Every save carries note identity, local revision and operation ID. A commit
returns the database revision; only a matching local revision becomes clean.
Switching notes never changes a response's destination. Optimistic revision
checks reject conflicting or deleted notes. Normal close waits for durable
writes; save failure or active composition keeps the window open. Forced process
termination only guarantees already acknowledged edits.

The Linux lock interoperates with Qt's exclusive `.lock` creation, advisory
file lock and owner metadata. See the [Qt 6.11 implementation](https://raw.githubusercontent.com/qt/qtbase/6.11/src/corelib/io/qlockfile_unix.cpp).
Rust only reclaims a recognizable dead Rust owner on this host after acquiring
the old file's lock and rechecking its inode. Live, Qt, remote or unknown locks
are retained. IPC forwarding is deferred; a second opener receives an error.
The protocol was implemented independently; Qt is not linked into the Rust app.

A failed draft can be retried, saved as a new note, or exported into a new recovery
directory with exact bodies, attachment records and hash-verified blobs. The
manifest is written after the assets. Closing through recovery requires an
acknowledged export covering every current dirty revision. Recovery export is
not the Qt backup format; an import UI and full backup/restore remain pending.

## Evidence and limits

Rust tests cover fixture preservation, cross-cell refusal, Unicode editing,
composition, undo, optimistic conflict rejection, schema guards, migration,
transaction failure and an actual killed writer. A Qt test-only probe reads
Rust-edited documents with the current Qt reader and checks `QLockFile` ownership
in both directions; blob hashes remain unchanged. The GPU writing harness covers
create/edit/save/reopen, stale acknowledgments, input-handler composition, rich
clipboard within the process, failure/close guards and recovery export.

Full M2 sign-off is deliberately open. The pinned Wayland clipboard transports
plain text externally; its GPUI JSON metadata is only recovered inside the same
process. Custom Qt/HTML MIME transport needs a platform adapter. Real IME and
Orca, accessible text/selection, media rendering, merged-table edge cases,
Qt-free PDF export and representative typing/layout measurements remain gates.
Rich document layout is cached by width/edit changes but currently shapes the
whole document. Undo keeps up to 100 full snapshots. Large-library/document
memory and responsiveness must be measured before release.

Storage runs on a dedicated OS thread, so it does not add blocking database work
to calloop. The foundation's calloop patch comparison under sustained UI load
remains outstanding; this increment makes no performance claim.
