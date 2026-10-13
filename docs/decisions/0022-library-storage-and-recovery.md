# 0022 — Bounded library queries and recoverable deletion

Date: 13 October 2026
Status: storage and core APIs implemented; Rust UI integration and release acceptance in progress

## Library queries

`notes_core::library` owns the framework-free `Sort`, `Scope`, `Query`, `Summary`
and `Page` contract. The existing serialized storage worker accepts a tagged
`Request::Query` and returns `Event::Queried` with the operation and exact request,
including generation, scope, text, sort and page coordinates. The UI can reject a
late reply without replacing an open editor or its draft. Results contain bounded
metadata and snippets, not rich documents.

Empty queries use deterministic Edited, Created or Title ordering. Pinned notes
come first, and note identity breaks final ties. Title ordering uses SQLite
`NOCASE`, puts empty titles last and uses edited time before identity to break
title ties. Nonempty queries use the existing FTS5 index: whitespace-separated
terms are quoted as literal prefix terms and combined with AND. FTS operators in
user input do not become operators. Relevance comes first, with the chosen sort
as a deterministic tie-breaker. This is token-prefix search, rather than interior
substring matching. Live queries exclude deleted notes and support All,
Favorites and a direct folder identity, including unfiled notes. Recently Deleted
uses a separate empty-query listing ordered by deletion time and identity.

Requests allow at most 4 KiB of query text, 32 terms, 200 rows and an offset of
100,000. The worker fetches one extra row to report `has_more`. SQL bounds title
and intermediate snippet values to 512 Unicode characters; final snippets fit
160 UTF-16 units, including an ellipsis when truncated. SQLite's progress hook
interrupts after about two million virtual-machine operations or 150 ms, checked
every 1,000 operations. This is cooperative interruption, not a hard wall-clock
deadline for an individual SQLite operation. The hook is removed after success
or failure so it cannot interrupt a subsequent save. No schema change or index
migration is required.

## Identity, revisions and deletion receipts

Duplication reads the acknowledged live source revision. A stale revision or
missing/inconsistent recognized media refuses the copy before creation. The
result gets a fresh note identity, fresh identities for every root and nested
table block, and fresh owned attachment records sharing validated immutable blob
bytes. Rich properties and unknown document, block, cell and span properties are
copied exactly. The source and its history remain intact. Creation uses the
existing compatible path, so the copy starts unpinned at revision 1. Conflict
recovery through `Request::Create` remains a separate operation that can preserve
an unsaved draft.

Trash, restore and permanent deletion accept explicit note identities and
expected storage revisions. Their immediate transactions validate the complete
set before committing; duplicate identities, stale revisions and invalid state
refuse the entire operation. A mutation accepts at most 10,000 notes. Trash and
restore advance storage revisions without rewriting the body, history or edited
time. A queued content save from before a deletion or restoration consequently
fails instead of resurrecting an old version.

`TrashEntry` wraps the unchanged `Note` API with its deletion date. A successful
trash operation returns a fresh `TrashReceipt` containing each post-deletion
revision and deletion date. Immediate Undo validates those exact values. An old
receipt cannot undo a later deletion of the same note. Receipts are ephemeral;
the durable deleted record remains available through Recently Deleted after
restart. Restoration preserves the original folder only while that folder still
exists, otherwise it restores the note unfiled. Permanent deletion accepts only
notes already in Recently Deleted.

Folder deletion resolves identities through the current hierarchy, rejects
corrupt cycles and duplicate folder identities, and deletes descendants before
their parents in one transaction. It verifies the exact current live note and
revision set against the reviewed request; a new capture or changed note refuses
the deletion. Live notes become trash and all affected notes become unfiled.
Existing trash keeps its original retention date but advances its revision,
invalidating stale restoration requests. The returned folder receipt undoes
**newly trashed notes only, restoring them unfiled**. It does not restore the
folder tree. A failed descendant or parent deletion rolls back both folders and
notes.

## Retention and assets

Housekeeping selects only notes whose deletion date is strictly older than
30 days. A purge removes at most 1,000 notes per batch and reports whether more
remain. Capture receipts stay independent of note deletion: replaying a completed
capture does not recreate its purged note.

Asset collection considers every surviving live note, deleted note and retained
revision. It checks recognized image and attachment references, plus registered
asset addresses found as literal strings or simple embedded address tokens in
unknown document, block, cell and span properties. Unknown properties are neither
rewritten nor reinterpreted; opaque encodings cannot be inferred. Unreadable
retained bodies refuse collection instead of being skipped. Permanent deletion
also refuses to remove an attachment owner while another surviving note or
revision references its attachment identity, including through an unknown
property.

Unreferenced registered asset metadata must be older than 24 hours before
collection. This grace period protects an import registered before its editor
change is saved. Current reference indexes and surviving attachment records
provide additional conservative retention. Collection checks blob addresses and
paths before deleting metadata and refuses symlinked media directories or files.
Metadata commits before physical file removal; failed unlinks are reported in
`unremoved_files`, and `bytes_freed` counts only successful removals. Collection
currently examines registered assets, not arbitrary unregistered files.

Reference checking scans the retained document corpus on the background writer.
The note-deletion batch is bounded, but this scan does not have a corpus-size or
time cap. Schedule housekeeping away from typing and routine mutations; full
corpus and foreground-fairness acceptance remain release gates.

## Focused verification

Focused core checks cover literal Unicode prefix input and corrupt/valid
descendant traversal. Eleven storage recovery tests cover real compatible
rich-document duplication, fresh identities and attachment ownership, stale
saves and Undo, atomic nested deletion and rollback, unfiled restoration,
restart, the 30-day boundary, retained capture receipts, live/trash/history and
unknown-property asset preservation, pending-import grace, unreadable history,
and symlink refusal. A real worker workflow checks FIFO save, trash, stale save,
query and Undo receipts. Focused query checks cover FTS prefix semantics and
snippets, scopes, deterministic sorting, paging and interruption followed by a
successful write. Strict Clippy passed for the core/storage library and tests.

These checks establish the backend contract. UI workflows, native usability,
the full compatibility/performance matrix and final release testing are separate
acceptance work; this decision does not claim those gates have passed.
