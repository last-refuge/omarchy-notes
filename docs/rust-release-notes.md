# Omarchy Notes Rust 1.0.0

The first separate Rust edition provides an offline notes app with rich writing,
Quick Note and durable command-line capture, nested keyboard-accessible folders,
pinning, three sort modes, prefix search and snippets, duplication, Recently
Deleted, Undo and recovery. Settings provides reviewed Markdown/text import,
Markdown/HTML/PDF note export, library Markdown plus assets, exact backups,
inspection, safe restore into a new library and library opening.

Install the x86_64 Arch/Omarchy edition explicitly:

```sh
bash install.sh --edition rust --version rust-v1.0.0 --no-quick-note
```

The executable/package is `omarchy-notes-rust`, desktop identity is
`org.omarchy.Notes.Rust`, and the `omarchy-notes-preview` compatibility launcher
forwards to Rust. Existing Rust notes/preferences remain at `omarchy-notes-gpui`.
Qt remains available and remains the default installer edition.

Use copies and verified backups for Qt libraries. Quotation/schema-3 libraries
require Rust; return to Qt using the pre-upgrade backup, which excludes later
Rust edits. Keep the newer library and export/backup before rollback. Folder
deletion Undo restores notes unfiled; it does not rebuild deleted folders.

Tags/Smart Folder interfaces, advanced table operations, gallery, drag filing,
bulk organization and browsable revision history remain deferred; existing
metadata and merged tables are preserved. X11 and other architectures are not
advertised. Use the desktop Quick Note action or a distinct manual compositor
binding; the Rust installer does not alter the Qt shortcut.

The implemented build was released at the owner's direction on 13 October 2026.
Recorded checks include 171 Rust tests, six dependency tests, full Qt regression
and interchange checks, 18 GPU workflows, actual CJK composition and silent Orca
processing, passing CPU budgets, verified dependency notices and a validated
candidate package. The evidence ledger retains exact revisions and limitations.

Further native performance/fairness, complete portal/scaling/suspend and
installed upgrade/rollback/coexistence checks remain follow-up. A native
accessibility test reached Unicode editing/undo but stopped on stale accessible
node traversal; complete human usability is unverified. The five-person study
and five-day trial also follow after release. These unverified items are not
reported as passing.

See [installation and recovery](rust-release.md) and the
[release evidence](../llm-docs/rust-v1-release-evidence.md).
