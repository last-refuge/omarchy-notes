# ADR 0011: verified backups before the quote format upgrade

Status: backup/restore, editable quotes and the explicit format upgrade implemented
and verified in the Rust preview. General backup/restore UI remains pending.
Date: 13 October 2026.

## A recovery point is part of the feature

The gold-rule quotation in the visual design is an editable block, not an
italic paragraph painted differently. It requires document schema 2 and a database
schema-3 compatibility marker. Before enabling it, the app must explain the change
and the complete backup location. Failure to create and verify that backup leaves
the original library usable. The Qt edition cannot read the new document format;
rollback restores the pre-upgrade backup into a separate directory. Later Rust
edits must be retained separately, never overwritten by rollback.

Full-library backups and restoration into a new directory are accessible without
a window and support database schemas 1/2/3 with their corresponding document
versions. A backup operation on a schema-1 library must not first migrate the
source. The quote-upgrade workflow runs on the storage worker, with progress,
cooperative cancellation before commit, actionable errors and no UI-thread I/O.

## Backup format and publication

Keep Qt backup format 1: `manifest.json`, a standalone `library.sqlite3`, and
`blobs/<first-two-hex>/<sha256>`. Preserve every database table and registered blob,
including trash, revision history, Smart Folders, timestamps, identities and
unreferenced registered assets. The existing manifest fields retain their meaning;
new backups add optional `databaseSha256`. Older manifests without this field
remain readable. Hashes detect corruption, not authenticity.

Take a consistent SQLite online snapshot while holding the compatible exclusive
library lock. Validate supported schemas, SQLite/foreign-key integrity, all current
and historical document bodies and their media references. Copy blobs as bounded
streams, checking both recorded sizes and SHA-256 hashes; the editor's asset-import
size limit does not apply to backups. Validate the snapshot's search index before
publication. No source body is decoded and reserialized into the snapshot.

Build in a unique private sibling directory; never reuse or delete someone else's
partial folder. Sync files and directories, write the manifest last, then publish
with an atomic no-overwrite rename and sync the parent. Refuse an existing target,
including an empty directory or dangling symlink. A failed operation cleans up only
its own unpublished stage. A process killed before publication may leave its own
hidden stage, but never a partial backup at the requested destination. After the
rename, an error must preserve the completed destination for inspection.

Inspect backups read-only. Restore into another unique stage, verify the actual
copied database and blob bytes, and publish only to a new directory. Do not copy
lock files, WAL sidecars, staging files or unrelated files. Do not modify either
the backup or an existing library. A restored schema-1 library remains schema 1
until deliberately opened by a supported edition.

## Quote contract

Readers accept document schemas 1 and 2, refusing later versions. Schema 2 adds
only the paragraph-like `quote` block, including within table cells:

```json
{"schema":2,"blocks":[{"id":"quote-1","type":"quote","spans":[{"text":"Make room for a good thought.","marks":["italic"]}]}]}
```

Schema-1 quotes are invalid. Existing paragraph/heading/list/table semantics,
unknown properties, stable block IDs, inline marks, links and media stay intact.
Converting a selected paragraph to a quote preserves its ID and spans, removes
inapplicable heading/list attributes, promotes the document version and creates
one undo step. Enter continues a quote; Enter on an empty quote exits to a
paragraph; Shift+Enter inserts a soft break. Converting back is explicit. Never
infer quotation from italics. JSON and rich clipboard preserve the block; HTML
uses `blockquote`, Markdown uses `>`, and PDF uses an inset gold rule without
turning a long quote into an unbreakable page item.

The explicit database-3 upgrade validates a completed pre-upgrade backup,
then commits its migration marker and `user_version=3` in one transaction. Opening
a library never implicitly opts into schema 3. Existing bodies/history remain
byte-for-byte unchanged; subsequent writes use document schema 2. Database-2
writers refuse schema-2 documents with a recoverable upgrade-required result.
Retries must distinguish an already committed migration from an interrupted one.

## Required evidence

Test Qt-created backups restored by Rust, Rust-created backups inspected/opened
by Qt, schema-1 restore, all tables/history/trash and media preservation, future
schemas, damaged/missing/same-size-corrupt assets, malformed references, immutable
backup inspection, existing destinations, write failures and process termination
at publication boundaries. Quote work additionally needs editing/clipboard/
HTML/Markdown/PDF fixtures, actual old-Qt rejection of schema 3, migration kill and
rollback drills, and reviewed dark/light/compact quote and upgrade states.
An injected ENOSPC error is fault injection, not evidence of a physically full disk
or power-loss durability. Native IME/Orca and all remaining port gates stay open.

The backup increment passed 85 workspace tests (one ignored subprocess helper is
invoked explicitly by the SIGKILL test), all-target/all-feature Clippy, formatting
and a release build. Ten new storage tests include nine killed-child scenarios,
write faults, same-size corruption, copied-destination corruption, no-overwrite
races, ownership, future/document/reference rejection, read-only inspection and a
33 MiB legacy asset. The release CLI passed the real Qt backup oracle in both
directions and the existing Qt document/lock compatibility regression. CI runs
the new cross-edition backup script. No GUI behavior was changed or claimed as
newly verified in that backup-only increment.

## Implemented quote increment

The Format panel and Ctrl+Shift+Q expose quotes. The editor uses the same shaped
text, selection and inline marks as prose, with a 24 px inset and a 2 px accent
rule. PDF uses a page-safe inset rule. Pasting a quote into ordinary prose keeps
the surrounding paragraphs separate. HTML blockquote and Markdown adapters
preserve text and marks; nested quotations flatten with a warning. Markdown
conversion uses MIT-licensed `pulldown-cmark` 0.13.4 and
`pulldown-cmark-escape` 0.11.0. Full file import/export and media packaging remain
M4 work; the text adapter reports media-label-only exports.

The upgrade panel states Qt compatibility and rollback consequences before
consent. It offers a fresh sibling backup path, a location picker, retained drafts,
progress and cancellation. Compact layouts keep actions pinned while details
scroll with mouse or keyboard. Late progress/completion results are matched to
the operation ID. Close waits while an upgrade is active. Success retries waiting
draft saves; failure retains the drafts and supports a new destination/retry.

The backup contains acknowledged saved content, not in-memory drafts. Cancellation
cannot undo a committed migration. Database 3 records the absolute backup path,
database digest and timestamp in the same transaction as its version marker.
Retries detect this receipt. Opening validates all current and historical document
versions before writes; it does not require a still-present backup file, since a
user can move or delete that file after upgrading.

**Verified:** 95 workspace tests pass, with two ignored child-entry helpers invoked
explicitly by the parent kill tests. New quote coverage includes stable IDs,
Unicode/marks, Enter and undo, table cells, paste boundaries, JSON/HTML/Markdown,
long PDF quotations, explicit upgrade refusal, cancellation and write faults.
Three additional killed-child boundaries verify atomic migration and retry.
The actual Qt reader refuses schema-3 libraries and backups, opens the restored
schema-2 pre-upgrade backup, and a separate Rust backup retains post-upgrade edits.

The real GPUI shell/storage harness passes quote conversion, opt-in cancellation,
failed backup/retry, late results, close guards, edits during upgrade, rich
clipboard, save/reopen and rollback. Direct GPU renders with real fonts were
reviewed in dark/light at 1440 × 900 and compact at 420 × 820 / 420 × 420.
Native Wayland/AT-SPI actions passed consent, cancellation, backup, saved quote
and undo without losing marks, followed by the existing editor/search regression.
Letter and A4 quotations span four valid PDF pages with selectable text and
embedded fonts; first/last Letter pages were raster-reviewed for rule continuity.
Clippy with warnings denied, formatting and the release build pass.

**Still unverified:** real IME/Orca, quotation-specific spoken semantics, native
portal destination choice, production-scale cancellation/backup usability, actual
power loss/disk exhaustion and the full platform/display/state matrix. The
AccessKit cache-signature warnings remain; passing the live AT-SPI API checks is
not an Orca usability claim. Qt remains the supported installed edition.
