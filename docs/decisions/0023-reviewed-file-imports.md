# 0023 — Review file imports before an atomic commit

Date: 13 October 2026
Status: backend implemented; app integration and release acceptance in progress

Markdown/text import has two distinct worker operations. Preparation reads an
owned snapshot of the chosen files and safely resolved local resources, converts
it through the existing inert Markdown/HTML adapter, and returns an opaque
`PreparedImport` with a review summary. Preparation does not mutate the library.
The app moves that exact snapshot into the commit request; changing a source
file after review cannot silently change the imported result. Source files are
never modified.

Missing, remote, outside-root and symbolic-link resources retain their visible
labels and produce warnings. Unsupported formatting, HTML attributes/CSS,
heading/list limits and quotation flattening also produce review warnings.
Commit requires explicit warning acceptance. Local file references stay within
the selected folder, or the selected file's parent; relative links to another
selected note become links to its fresh imported identity. Images and attached
files are copied and checked using the existing media limits. No remote fetch
occurs. UTF-8 errors and hard batch limits refuse preparation rather than
offering a truncated import.

One import accepts at most 1,000 notes/folders, 20,000 scanned directory entries,
64 directory levels, 4,096 resource references and 1,000 review warnings. Text
files are limited to 4 MiB, each asset to 32 MiB, and aggregate source text plus
unique asset bytes to 64 MiB. These bound input bytes, not total allocator/RSS
overhead. Preparation holds converted documents and asset bytes in memory and
therefore remains a bounded batch workflow. Empty asset-only directories are
not added to the note tree.

Commit validates the current destination and schema, publishes immutable asset
bytes, and inserts all folder, note, attachment, reference and FTS metadata in
one immediate transaction. Fresh folder identities preserve the selected tree;
sibling-name conflicts use an explicit numeric suffix rather than overwriting
an existing folder. Note and attachment identities are fresh. Cancellation or a
SQL error rolls back every imported note/folder/metadata row. Previously
published immutable bytes may remain after a refused or cancelled transaction;
retry verifies and reuses them. This does not change existing notes or source
files, and is distinct from a claim that every failed attempt leaves no disk
footprint.

Quotation imports require the existing explicit verified-backup/schema-upgrade
workflow before commit. Import does not enable that format automatically.
Cancellation is a shared atomic token checked while scanning/reading/preparing,
before each asset/row and immediately before commit. Tagged progress is
throttled to 50 ms except phase changes and completion. A committed receipt is
the success boundary; late cancellation does not undo durable imported notes.

Focused tests cover review purity and source mutation after review, copied media
hashes, linked notes, fresh/collision-safe folder topology, restart and retry,
missing/remote/outside resources, transaction failure/cancellation, symbolic
links and invalid UTF-8, and quotation upgrade refusal. A separate formatting
fixture verifies warning acceptance before commit and retained visible text.
Focused portable-export and controlled-backup checks also passed on the source
increment. App/native workflows, killed-worker/full-disk faults, corpus load,
library switching and final compatibility testing remain separate release gates.
