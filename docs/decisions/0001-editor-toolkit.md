# 0001 — Editor toolkit: Qt Quick TextEdit over an owned QTextDocument

Status: accepted for the local v1, 23 September 2026 (editor feasibility gate, milestone 1).

## Decision

Build the editor with Qt Quick's `TextEdit` (`TextArea`), driving its `QTextDocument` from C++ (`EditorController`). The Qt Widgets `QTextEdit` fallback is not needed.

- **Minimum Qt: 6.8 LTS.** The editor relies on `TextEdit.textDocument` and `QQuickTextDocument::textDocument()`. Development used 6.11.2.
- **Public Qt APIs only.** The code uses no private headers. `TextEdit.cursorSelection` was tried for typing formats and dropped (see findings).
- **Persistence is toolkit-independent.** It lives in `notes_core`: the JSON schema, `DocumentConverter` and SQLite. A Widgets front end could reuse it unchanged.

## Gate results

`tests/tst_editor.cpp` drives the real QML UI offscreen. It saves screenshots to `build/screenshots/`.

| Requirement | Result |
|---|---|
| Headings, marks, nested bullet/numbered/check lists, table, inline image, file attachment, note link in one document | Renders correctly (screenshots, dark and light) |
| Save/reload fidelity | Schema → editor → schema is identical for the fixture, merged-cell tables and edge layouts (`tst_document`) |
| Cross-element selection and clipboard | Copying headings, checklists and a table, then pasting, keeps structure. Pasted blocks get fresh ids. An internal JSON MIME type keeps copies lossless |
| Pasting from other apps | Web HTML is normalized to the schema. Unsupported content (web images, `javascript:` links) is reported to the user, not silently dropped |
| Keyboard flows | Return continues lists (new checklist items start unchecked). Return on an empty item steps out. Tab/Shift+Tab nest lists and move between table cells. Tab in the last cell adds a row. Return after a heading starts body text |
| Undo grouping | Formatting a selection is one undo step. Text typed after Ctrl+B at a bare cursor undoes together with its formatting |
| Unicode / IME | Preedit is never saved; the committed string is (`QInputMethodEvent`). Emoji, RTL and CJK render |
| Checklists | Qt Quick toggles checkbox markers natively on click, through the document (undoable, autosaved). Ctrl+Shift+U toggles from the keyboard |
| Autosave | "Saved" is shown only after the SQLite commit is acknowledged. The idle save fires after 0.7 s; the hard cap is 3 s during continuous typing (measured 2.9 s) |
| Crash recovery | Every acknowledged save survives SIGKILL (8 random kill points, `tst_store`). `integrity_check` and the blob checks stay clean |
| Save failure | The draft stays in the editor, the error shows, retries back off. Retry and Export (Markdown) are offered. Quitting is blocked until saved, exported or explicitly discarded |
| Theme | Follows an Omarchy theme switch live, with contrast-corrected semantic colours. The document content and saved state are untouched |
| Narrow tile | Single pane with a back button. Images scale to the column |
| Accessibility | The editor exposes `EditableText` with its full text. **Gap:** embedded images and attachments appear as unnamed U+FFFC placeholders |
| Performance (offscreen, software renderer) | Typical notes are instant. A 3,000-paragraph note with 20 tables opens in about 0.9 s; the worst keystroke is about 45 ms; the save snapshot takes 8 ms |

## Findings that shaped the code

1. **`TextEdit.cursorSelection.font` does not change the format of the next typed text.** Instead, the controller keeps a pending format and inserts the first typed character in it; Qt carries the format forward from there. Text committed through an input method is formatted after insertion, as a separate undo step.
2. **Qt Quick already toggles checkbox markers on click.** A custom hit test double-toggled them, so it was removed.
3. **Proportional line height stretches lines that contain images.** A fixed extra line distance (`LineDistanceHeight`) is used instead.
4. **A table cell's span lives in its first block's char format.** Replacing that format un-merges cells, so the converter preserves the cell properties.
5. **Qt keeps structural empty blocks around tables.** The converter treats an empty paragraph whose only role is that as implicit, both ways.

## Known gaps (tracked for later milestones)

- **Checklist markers draw as ☐/☒ glyphs**, not the round checkboxes Notes users expect. Next step: overlay themed QML checkboxes at marker positions.
- **Images and attachments need accessible names.** Next: expose image alt text and attachment file names to assistive tech, then do an Orca pass on real hardware.
- **Theme and font changes rebuild the document**, which clears undo history. Link and highlight colours are baked into formats.
- **Images are decoded synchronously on the UI thread during note load.** Move decoding to a worker with placeholders.
- **Opening very large notes is layout-bound** (about 0.9 s at 3k blocks). Re-measure on the reference machine with the GPU renderer.
- **Single-instance activation is a lock file for now.** A second launch exits. D-Bus activation comes with desktop integration.
- **Blob garbage collection is not implemented.** References are recorded (`note_blob_refs`) so it can be added safely.
- **Manual checks still needed on the real desktop:** fcitx5 composition, Orca, fractional and mixed-DPI scaling, file dialogs through the portal.
