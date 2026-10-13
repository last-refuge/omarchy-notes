# ADR 0008: writing tools use document selection; find is presentation only

Status: implemented increment; full port and feasibility gates remain open.
Date: 13 October 2026.

The persistent toolbar keeps common marks and insertion actions compact. Format
opens a panel with all six existing marks, three heading levels, body and the
three list styles. It preserves the document selection when focus enters the
panel, exposes active/mixed mark states and returns focus after applying a
command. Tab scrolls each focused option into view. Opening a different writing
panel keeps the interface composed; IME preedit prevents focus-stealing commands.
The future selection popover will use these same actions.

The pure model matches the established Qt behavior: marks apply to selected text
or the word containing a caret strictly inside its boundaries. At a boundary
they set subsequent typing format. Formatting a word retains the caret and adds
one undo step. Paragraph style state considers the whole selected range. Applying
an already assigned style preserves list indentation and checklist state; the UI
toggles an already active list back to body. Invalid style/mark commands fail.

Legacy shortcuts are retained, including Ctrl+Shift+8 for bullets and
Ctrl+Shift+7 for numbers. Linux consumes Shift when reporting punctuation, so
Ctrl+* and Ctrl+& are bound too. Heading and checklist aliases from the early
Rust preview remain available. Indent/outdent commands outside lists are no-ops.

`notes-editor::Find` holds literal Unicode case-insensitive matches in global
UTF-8 byte coordinates, matching Editor's document selection. The existing
regex dependency provides Unicode simple case folding; user syntax is escaped.
Matches span styled runs inside a leaf, never distinct paragraphs or cells.
There is no normalization or full multi-character case expansion (for example,
searching SS does not treat a sharp S as the same text). Results are nonoverlapping.
Up to 100,000 ranges and 4 KiB queries are accepted; an explicit message reports
limits. Shared immutable ranges avoid copying the result set on every paint.

Find recomputes on committed editor revisions and query changes, outside render.
Navigation alone changes selection, never document revision, history or saving.
Editing refreshes matches without moving the caret. Painting visits only matches
intersecting each shaped line, draws highlights above span backgrounds, and gives
the current match a stronger tint and underline. Composition hides potentially
stale highlights and prevents find navigation from changing preedit selection.

The existing single-line field now accepts a name and placeholder, so global
search and find-in-note share tested text input and AccessKit behavior while
retaining independent models and focus. Each editor keeps its own query for the
session. Enter/F3 and Shift+Enter/Shift+F3 wrap through matches; Escape returns to
writing. Query state is not persisted with notes.

Tests cover mixed marks and metadata, inside-word/boundary formatting, checked
list preservation, invalid/composing edits, Unicode/literal matching, wrapping,
table boundaries, refresh and explicit resource limits. The real-font GPU shell
harness covers routed commands, composition focus guards, save/reopen and
unchanged stored bodies after search. Each compact viewport is opened explicitly:
headless Window::resize alone does not simulate the platform resize event.

Find/formatting are not claims of full M3/M4 completion. Links are now covered
by [ADR 0009](0009-editable-links.md). The contextual selection popover, quotes/schema migration, Quick Note, remaining library tools,
real IME/Orca, large-document performance and release hardening remain open.
