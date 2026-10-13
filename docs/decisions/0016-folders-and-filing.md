# 0016 — A place for every thought

Date: 13 October 2026  
Status: implemented in the Rust preview; library parity remains in progress

## Experience

The folder sidebar now reflects the real, nested library. Folder identities,
rather than names, determine scope: `Work / Clients` and `Personal / Clients`
remain separate. Disclosure buttons collapse subtrees; paths disambiguate
destinations and appear beside filed notes. New notes inherit the selected folder.
The top-level **Notes** row contains unfiled notes, while **All notes** spans the
library.

**New folder** / **Ctrl+Alt+N** creates inside the selected folder, or at the top
level from a library-wide scope. **F2** renames the selected folder. Its note-list
header also offers rename, move and new-subfolder actions. The editor's folder
button / **Ctrl+Alt+M** files the current note through a searchable destination
chooser. The same chooser moves folders, excluding self and descendants.

Beauty and delight apply to these secondary screens: a restrained serif heading,
clear source path, focused name/search field, compact action footer and warm
selection colors work in both palettes and at 420 × 420 logical pixels.
Tab/Shift+Tab navigate the form; arrow keys navigate destinations; Enter commits
and Escape returns. Composition does not submit or dismiss a form. Input errors
retain the name for correction and retry. Controls do not imply a successful move
until the writer acknowledges the committed transaction.

Moving a note retains its existing editor entity, draft, selection and undo/redo
history. Successful filing returns focus to that editor, including when opened
through the toolbar. Other forms restore an entity-owned editor/search focus or
the shell; transient toolbar handles disappear while the form is mounted and
are not valid return targets. Folder changes reveal the selected destination and
expand its ancestors. Fixed-height navigation and destination lists use GPUI's
`uniform_list`; search filters a prepared destination collection rather than
rebuilding and validating the hierarchy on every keystroke.

## Compatible persistence

The framework-free organization model owns name normalization, hierarchy rules
and destination eligibility. The existing bounded, single storage writer runs
folder and filing mutations in immediate SQLite transactions. The UI captures
the subject identity when opening the form, tags each submitted operation, and
ignores unrelated acknowledgements. Name-field events also carry a form epoch
and route through the main window, avoiding the shared main/Quick controller's
window-affinity hazard.

Folder changes use the existing `folders` table and UUID identities. Names are
whitespace-normalized and limited to Qt's 200 UTF-16-unit boundary; sibling
collisions use SQLite's `NOCASE` rule. Invalid parents, cycles and missing notes
are rejected. A malformed hierarchy remains visible without recursive traversal,
but invalid branches are excluded from filing destinations.

A note move updates only `folder_id`, matching Qt's behavior. It does not rewrite
rich content, revisions, history, edited time or search records. Ordinary content
saves do not write `folder_id`, so a queued save with old folder metadata cannot
undo a subsequent move. No schema upgrade is required. Qt can read and modify
these folders and notes, and Rust can reopen the resulting edits.

Window-launch requests wait while an organization form is open, subject to their
existing expiry. Capture and autosave remain independent. Graceful close waits
for an outstanding organization transaction; a failed write leaves the form and
window available for recovery. Closing the main window clears its form state so
Quick Note cannot keep an invisible form blocking later launches.

## Verification and remaining scope

Core/storage tests cover normalization, hierarchy corruption/cycles, a large
tree, sibling-name conflicts, unchanged content/revisions and stale metadata
saves. The GPU workflow covers real text/layout, dirty-note filing, duplicate-name
scope isolation, creating notes in a folder, retained selection/undo, form
composition, durable reopen, compact layouts and bounded folder rendering.
The native workflow uses a private accessibility bus and scratch library/config,
with input guarded to the exact owned window. A separate oracle runs the supported
Qt edition's actual `NoteStore` methods in both directions; CI runs that oracle.
See the verification record for outcomes and limitations.

This increment does not implement folder deletion, trash/recovery, drag-and-drop
organization, saved expansion state or a complete keyboard tree-navigation model.
Tags, Smart Folders, FTS search, remaining library views and portability workflows
remain part of M4. Real IME/Orca usability, full performance/platform matrices and
the release cutover gates remain open.
