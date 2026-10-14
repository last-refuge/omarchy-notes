# Omarchy Notes 1.0.0

Omarchy Notes is now Rust-only; Qt is retired and retained only as a legacy
migration reference. Version 1.0 provides offline notes with rich writing,
Quick Note and durable command-line capture, nested keyboard-accessible folders,
pinning, three sort modes, prefix search and snippets, duplication, Recently
Deleted, Undo and recovery. Settings provides reviewed Markdown/text import,
Markdown/HTML/PDF note export, library Markdown plus assets, exact backups,
inspection, safe restore into a new library and library opening.

Install on x86_64 Arch/Omarchy. The current installer installs Omarchy Notes;
older edition-aware installers require `--edition rust`.
Older published `rust-v1.0.0` artifacts still use the old `omarchy-notes-rust`
package name. The new installer requires a newly named `omarchy-notes` release
package; no published release has been modified. For a release with the new package:

```sh
bash install.sh --version rust-vVERSION --no-quick-note
```

The Arch package is now `omarchy-notes`; the executable remains
`omarchy-notes-rust`, desktop identity is
`org.omarchy.Notes.Rust`, and the `omarchy-notes-preview` compatibility launcher
forwards to Rust. Existing Rust notes/preferences remain at `omarchy-notes-gpui`.
This is a package-only rename: app IDs, profile/runtime paths and `rust-vVERSION`
tags are unchanged. New source archives are `omarchy-notes-VERSION.tar.gz`.

Use copies and verified backups for retired Qt libraries; see [migration](migrating.md). Quotation/schema-3 libraries
require Rust; return to Qt using the pre-upgrade backup, which excludes later
Rust edits. Keep the newer library and export/backup before rollback. Folder
deletion Undo restores notes unfiled; it does not rebuild deleted folders.

Tags/Smart Folder interfaces, advanced table operations, gallery, drag filing,
bulk organization and browsable revision history remain deferred; existing
metadata and merged tables are preserved. X11 and other architectures are not
advertised. Use the desktop Quick Note action or a distinct manual compositor
binding; automatic compositor setup is not implemented and legacy bindings are
not altered.

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
