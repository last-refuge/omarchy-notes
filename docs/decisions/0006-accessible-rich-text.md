# ADR 0006: accessible rich text shares the editing model

Status: implemented increment; full accessibility acceptance remains open.
Date: 13 October 2026.

The custom-painted editor previously supplied an accessible value on its input
container, which did not expose caret, selection or character geometry. The pinned
GPUI revision supports synthetic AccessKit children and action handlers, so the
editor now supplies its own text runs through those public APIs. No GPUI fork or
new dependency is required.

`accessible_input.rs` wraps the existing focusable div and delegates its
layout, prepaint and paint directly. The focus handle and synthetic text runs
therefore belong to the same accessible multiline input. The painted editor and
accessible representation read the same document, selection and shaped layout.
Ordinary rendering performs no accessibility snapshot work unless accessibility
is active. Character bounds use the window's physical-pixel scale.

Both `editor_accessibility.rs` and `search_accessibility.rs` use that wrapper.
Search captures current text geometry during prepaint, exposes empty values
without its placeholder, handles horizontally scrolled text, and routes accessible
query changes through the existing search events. Both adapters guard selection
replacement and leave active IME composition alone.

Runs preserve logical document order, hard breaks, empty paragraphs, offscreen
text and inline object replacement characters. Stable block identity and run
offsets supply keys. Runs split at visual rows, style/direction changes and the
AccessKit word-index bound. Graphemes are selection units; a pathological grapheme
larger than AccessKit's 255-byte character limit falls back to scalar boundaries
without truncating bytes. Bold, italic, underline, strikethrough and font family
are text attributes. Custom-painted checkboxes expose names, checked state and
actions. Images expose alt text; attachments expose resolved filenames. Media
activation selects the object, after which the existing open/save shortcuts work.

Accessible selection and replacement actions reuse normal editing, undo, redraw
and autosave. Captured revision, selection and entity identity guard against late
actions editing different content; actions do not interrupt IME preedit. Errors
expose their actual message as the alert's accessible name.

The native regression runs in a private D-Bus session with an explicitly owned
AT-SPI registry, in-memory settings and a copied Qt fixture. It checks text through
the real AT-SPI `Text` API, changes selection, then sends keyboard input only to the
exact owned Hyprland window. It verifies undo, checked-state updates and SQLite
durability. No screenshots, clipboard reads or desktop settings are involved.

Limits: the pinned AccessKit Unix adapter exposes `Text`, not `EditableText`.
The application implements AccessKit `ReplaceSelectedText` for adapters that
dispatch it; Linux editing is verified through keyboard input after AT-SPI
selection. Actual Orca reading/announcements and navigation are still unverified.
Heading/table structure, RTL character geometry,
mixed-DPI and large-document performance remain gates. The installed AT-SPI
client reports cache-signature warnings with this adapter; direct text/action
queries passed, but the warning needs investigation in the Orca pass. This
increment does not authorize replacing the Qt release.
