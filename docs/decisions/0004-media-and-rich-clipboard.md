# 0004 — Durable media and portable rich clipboard

Status: accepted for the M2 media increment, 12 October 2026.

## Decision

Keep SQLite schema 2 and document schema 1. Add two small Qt-free crates:
`notes-exchange` handles bounded clipboard serialization and inert HTML
conversion; `notes-clipboard` implements explicit multi-MIME clipboard operations
on WLR data-control Wayland compositors. The pinned GPUI API remains the text
fallback where that protocol is unavailable. This is verified on Hyprland;
X11 and other Wayland protocols require further adapters and native verification.

Use the existing `application/x-omarchy-notes-fragment+json` representation,
with an optional `omarchy_clipboard` extension containing versioned, base64
encoded assets and attachment metadata. Strip that transport extension before
inserting into the document. Qt ignores the extension, preserving text/structure
interoperability, but does not import its assets. Cross-library media copies are
therefore supported between Rust editions only. Missing references from an old
Qt/plain fragment fail explicitly instead of creating broken new references.
Offer HTML and plain text simultaneously; a single image also offers native image
bytes. Paste prioritizes Notes JSON, local file URLs, image bytes, HTML, then text.
Explicit plain-text paste requests only text. Copy does not read the old clipboard.

The clipboard service owns a dedicated thread and bounded command queue. Reads
and writes have byte caps/timeouts; content is requested only by Paste. Selection
ownership events update MIME offers, not a clipboard history. The original
process must remain alive to serve a selection unless a desktop clipboard manager
retains it. Data-control is a compositor capability; absence is communicated in
the UI instead of claiming rich exchange works everywhere.

## Storage and safety

File reads, integrity checks, image decoding and SQLite work run off the GPUI
foreground thread. Imports enter the existing serialized storage worker. Accept
regular files only, open nonblocking to reject FIFO/device paths, and cap a batch
at 16 files/32 MiB. SHA-256 addresses select `blobs/<prefix>/<hash>`. Install through
private staging files, fsync, and a no-replace hard link; verify any existing blob.
Commit blob/attachment metadata before emitting an import result. The eventual
editor insertion is one undo group and its document body uses ordinary autosave.
A crash before the body commits may leave unreferenced assets; it must not leave
a committed body referring to an import that never finished writing.

Portable imports verify every supplied content address, require all referenced
assets, and reassign attachment IDs/ownership to the destination note. Images
reuse immutable hashes. Saving a recovery copy also clones attachment records in
the note creation transaction. Unknown document versions are still rejected.
Cancellation or an insertion error can leave unused immutable files/metadata;
there is no garbage collector yet. Existing file bytes are never rewritten.

PNG/JPEG/WebP/GIF decoding checks dimensions before allocation: 24 megapixels,
12,000 pixels per side, decoder allocation cap 128 MiB. Preview thumbnails are
at most 1,400 × 1,000, retaining at most 48 MiB per selected editor; switching notes
releases the previous previews. GIF uses its first frame. This bounds retained
preview pixels, not total process memory or simultaneous background work.
Layout reserves space using document dimensions so decoding does not move text.
Malformed legacy dimension metadata may produce letterboxing; originals remain
intact. No filesystem or image decoding work occurs during render/paint.

HTML is tokenized with html5ever into a bounded inert conversion tree, never
rendered/executed. A whitelist preserves supported marks, headings, lists, safe
links and table cells. Scripts/embedded active content are dropped. HTML image
URLs are never fetched, including local/data URLs; alt text and a notice explain
that the image must be pasted or attached separately. This is a note-format
converter, not browser layout parity. HTML is capped at 4 MiB, 20,000 nodes and
64 levels; clipboard representations at 48 MiB, portable file bytes at 32 MiB.

## Interaction and visual contract

Images fit the writing column with rounded frames and a quiet selected border.
Attachments show a rounded file card, filename and readable type/size. Selecting
one object reveals Open, Save as and Remove; selecting a larger text range does
not expose an action that could misleadingly remove the entire selection.
Removal and insertion are undoable. Toolbar buttons, keyboard shortcuts and
local-file drop/paste share the same import path. Missing files have an explicit
unavailable state with details and Reload file. Notices can be dismissed.

An import captures editor identity, operation, document revision and selection.
Only the matching session receives the result. If typing/selection changed, keep
the result ready and ask for Insert here; Cancel makes late responses inert.
Cut removes content only after copy succeeds and its selection is still current.
Close/recovery export waits for the user to finish/cancel a pending insertion.
Failed operations retain the current draft and never report a false save.

Open verifies the blob and creates a private, non-executable copy with a useful
extension, then asks the desktop's default application to open it. Executables
and active/unsupported file types require explicit export instead. Save as writes
a new file without overwriting. `opened/` copies are retained until future cache
cleanup. These operations never auto-open content merely because it was pasted.

New direct dependencies use permissive licenses (MIT/Apache-2.0; image supports
both). The pinned GPUI/Qt boundary is unchanged. A complete distribution notice
and dependency audit remains a release gate.

## Evidence and remaining gates

See the [verification record](../../llm-docs/gpui-rebuild-baseline.md) and
[preview guide](../gpui-preview.md). Automated tests cover portable media,
transaction/cancel guards, failure behavior, Qt reader compatibility, actual
Wayland multi-MIME transfer, two real Rust processes/libraries, and GPU layouts.
Portal picker/drag-drop delivery, external viewer launch, clipboard-manager
retention and other desktop transports still need manual/native verification.
Real IME, editable accessibility/Orca, deeper table behavior, Qt-free PDF and
representative layout/typing measurements keep M2 open. The Qt installer remains
the supported distribution.
