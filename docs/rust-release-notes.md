# Omarchy Notes 1.1.0

Version **1.1.0** formats as you type and fixes caret movement on empty lines:

- Typing a marker and a space at the start of a paragraph applies the style
  immediately: `*` or `-` for a bullet, `1.` or `1)` for a numbered list, `[]`
  or `[ ]` for a checklist item (also from inside a list item) and `#` to `###`
  for headings. Enter continues a list and Enter on an empty item ends it. Undo,
  or Backspace straight away, takes the formatting back and keeps what you typed.
- `>` starts a quote once quotes are enabled for the library (Ctrl+Shift+Q opens
  the verified-backup upgrade). Until then it stays text, so typing it never
  leaves a note that cannot be saved.
- Backspace at the start of a list item or quote removes the formatting instead
  of joining it to the line above.
- Enter on an empty line, such as the empty title of a new note, moves the caret
  down immediately. Up and Down no longer skip blank lines, and clicking beside
  one no longer lands in the next paragraph.

## Omarchy Notes 1.0.2

Version **1.0.2** fixed two editing problems:

- Omarchy's universal copy, cut and paste shortcuts (Super+C, Super+X,
  Super+V) now work in notes and search. Omarchy sends Ctrl+Insert and
  Shift+Insert to Omarchy Notes windows, which are now accepted alongside
  Ctrl+C/X/V, as is Shift+Delete.
- Up and Down arrow keys move between paragraphs reliably instead of sticking
  at paragraph boundaries.

## Omarchy Notes 1.0.1

Version **1.0.1** fixed the packaging/install mismatch and completed the
Omarchy Notes branding cutover. Release `rust-v1.0.1` uses `omarchy-notes`
packages and source/recipe archives. The updated installer cannot use the older
named 1.0.0 packages.
Executable names, app IDs, profile/runtime paths and user data remain unchanged.

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
package; the older release has not been modified. Install 1.1.0 with:

```sh
bash install.sh --version rust-v1.1.0 --no-quick-note
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

## Historical Rust 1.0.0 evidence

The 1.0.0 build was released at the owner's direction on 13 October 2026.
These historical checks do not certify the 1.0.1, 1.0.2 or 1.1.0 artifacts.
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
