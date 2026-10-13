# ADR 0012: durable capture receipts and a versioned Rust instance channel

Status: implemented for headless and forwarded text capture. Forwarding window
launches and activation tokens remains separate unfinished work.
Date: 13 October 2026.

## Preserve the meaning of “saved”

The Qt launch channel hashes the library path into a per-library Unix socket,
accepts newline-delimited JSON and replies `ok` before dispatching the action.
That confirms receipt, not a committed note. Rust must not use that response as
proof of durability. Both editions still use the compatible `.lock` ownership
protocol; a live Qt owner is never bypassed and its socket is never removed.

The Rust preview uses a separate version-1, length-prefixed JSON protocol with
`capture` and `status` actions. Replies explicitly identify the request and say
`committed`, `unknown` or `unconfirmed`. Only a committed receipt allows the CLI
to print `Saved note …`. The CLI performs no window/GPU initialization for capture
or status, including when forwarding to a graphical owner.

## One transaction, one request identity

Each capture has a canonical lowercase UUID. The CLI generates and prints one
before dispatch unless `--request-id` supplies it. Normalize CRLF/CR to LF,
preserve other text and reject whitespace-only input. Plain text is limited to
32 MiB and is subject to the normal document limits after conversion. Hash the
normalized text with SHA-256. A new capture uses the same document/create/derived-
index transaction as ordinary notes and records its request ID, payload digest,
note ID and commit timestamp in that transaction.

The optional `rust_capture_requests_v1` table is additive metadata. It does not
change `user_version` or enable a new document type. Existing Qt readers ignore
it, and complete SQLite backups preserve it. Quotes still require their separate
explicit schema-3 upgrade. The receipt has no cascading foreign key to its note:
a retry after editing, trashing or permanently deleting the note returns the
original receipt and never overwrites or resurrects it. Reusing an ID with other
text fails explicitly. Invalid stored receipts fail instead of reporting success.

`--capture-status UUID` opens SQLite read-only, without a writer lock, library
creation or migration. A missing receipt means no committed result at that read's
snapshot; an in-flight request may still finish. A receipt records the original
capture, not the note's current text or existence. Receipts are scoped to this
library and its snapshots. Restoring an earlier backup also restores its earlier
receipt set; this is not a global cross-library deduplication service.

On a lost/invalid/timed-out reply, preserve the ID and report an unknown outcome.
Do not silently retry with a new ID. Query the receipt, then retry the same ID and
original text if needed. Concurrent identical requests serialize at the single
writer and create one note. A request whose client disappears may still commit;
its receipt is the recovery mechanism.

## Ownership, bounds and lifetime

The storage worker is created while holding the compatible exclusive library
lock. Its canonical root is the identity used by `notes-platform`; aliases resolve
to the same endpoint. The server binds before GPU/window initialization and lives
until the owning app exits. New capture events add committed notes to the UI
without replacing the selected session, selection, caret or focus.

Endpoints live under a private, owned `XDG_RUNTIME_DIR/omarchy-notes-rust` directory
(or a private per-UID temporary runtime directory when XDG_RUNTIME_DIR is absent).
The filename is a 128-bit prefix of SHA-256 over the canonical path's native bytes.
Both ends check Linux `SO_PEERCRED` for the current user. Symlinks, public runtime
directories, unrelated files and responding endpoints are never reclaimed. A dead
socket can be removed only while the worker owns the corresponding library, after
connection refusal and an inode check. Normal shutdown removes only the socket
inode it created. A killed process leaves a stale socket/lock for safe reclamation.

Two request-reading threads and eight unparsed pending sockets bound input work.
Mutations enter the existing 32-item storage queue without blocking the UI. Text
is limited to 32 MiB; a frame permits its worst-case JSON escape expansion plus
metadata, is length-checked before allocation, and grows only as bytes arrive.
Input/write deadlines are five seconds, with 30 seconds for a commit reply.
Timeouts and queue saturation never produce `saved`. The accept loop sleeps in
kernel `poll`, with a dedicated shutdown wakeup; it has no idle polling timer.
Closing the UI receiver does not discard already queued writer work, although
forced process termination can still interrupt unacknowledged captures.

## Evidence and remaining work

Unit/integration tests cover transactional receipt creation, retry after edits and
permanent deletion, payload mismatches, read-only/non-creating lookup, invalid
receipts, rejected input, write failures, backup/restore and two SIGKILL boundaries
(before commit and after commit/before reply). Socket tests cover dropped replies,
future/malformed/oversized messages, queue saturation, disconnected writers,
receipt-only and mismatched replies, alias identity, private runtime checks,
unrelated/live/stale endpoints and cleanup.

The release CLI oracle runs without DISPLAY, WAYLAND_DISPLAY or session D-Bus. It
covers direct and forwarded capture, stdin, concurrent direct launches and eight
parallel requests, status, lost replies, identical retries, different-text reuse,
restarted/killed owners, failure mode, canonical aliases and backup/restore.
The native Wayland/AT-SPI check observes a captured note in the actual library
while retaining an unsaved draft, its exact caret and focus, then confirms one note
after retry and the original draft's intact save. The full native regression also
passes with explicit focus and per-character input acknowledgements. Earlier fast
synthetic runs failed at table undo/link persistence; those failed runs are not
counted as passes. Real IME/Orca remains unverified.

The public installed binary/identity still belongs to Qt. This increment does
not yet implement `--quick-note`, `--new`, `--note`, default-launch activation,
activation-token forwarding, shared main/Quick Note sessions, `--samples` or
`--setup-hyprland` in Rust. Quick Note must use this one writer and the same
per-note session rather than acquire a second lock or create competing drafts.
Those requirements, the native platform matrix and installed-package acceptance
remain in the full rebuild plan.

## Subsequent increment

[ADR 0013](0013-shared-quick-note-sessions.md) implements local Quick Note windows,
shared editing sessions and initial `--quick-note` startup. The remaining window
launch and activation-token forwarding scope above is unchanged.
