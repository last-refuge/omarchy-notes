# ADR 0009: editable links preserve words and navigate by identity

Status: implemented increment; full-port gates remain open.
Date: 13 October 2026.

Links use the existing schema-1 text span `href` property. Applying a destination
to selected text keeps its words, block identities, marks and unknown properties.
It can span paragraphs or table cells without replacing their structure. Objects
are excluded; an object-only selection produces an explanation. At a caret in
an existing link, editing or removal covers adjacent text spans with the same
href, including differently styled runs. A new link at an unlinked caret inserts
the address or target note title, then ends link typing. Each change is one undo
step. Composition rejects link edits; invalid edits leave the document intact.

`notes-core::links::LinkTarget` centralizes activation and clipboard-import
policy: HTTP(S) with a host, nonempty mailto destinations, and `note://` followed
by a nonempty ASCII letter/digit/hyphen/underscore identifier. Identifiers retain
case. Input is bounded to 8 KiB and control characters are rejected. Existing
stored values are preserved on load, even if activation is refused; users can
remove or replace those links. No document load, layout or hover launches a URL.
Explicit Open, Ctrl+Enter or Ctrl+click dispatches a validated destination.
Pointer activation uses actual glyph hit-testing, not nearest-line selection.
PDF export retains its existing external-link-only policy.

The Link panel captures editor revision and selection before focus changes.
Apply fails visibly if either has changed. Changing destination modes does not
renew that anchor. Its named input shares the existing accessible text provider.
Ctrl+Shift+K opens addresses; Ctrl+L opens the note chooser. Ctrl+K remains library
search. Arrows choose a note, Enter applies, Tab reaches the controls (including
Remove for an existing link), and Escape returns to writing. The picker replaces
the formatting strip while open and sizes its virtualized results to content and
window height. Light, dark, narrow and short-window states use the same tokens.

The shell supplies note identities, titles and folders outside render, refreshing
the selected editor's catalog after changes and on activation. The chooser omits
the current note, searches title/folder and does not impose an arbitrary result
cutoff. Activation emits a typed event; the shell accepts it only from the active
source editor. Missing targets retain the current note and offer an explanation,
Edit and Remove. Successful navigation retains per-note editor state and provides
a return action using a session-local history capped at 100 entries. Search and
folder filters are left intact. This is explicit navigation, not inferred
relationships; the separate related-notes summary remains future work.

Pure tests cover URI policy, Unicode, span/mark preservation, partial unlinking,
caret retargeting, undo/redo, table/object preservation, invalid input and
composition. The real-font GPU shell test covers routed insertion/edit/removal,
stale picker rejection, note navigation/return, missing targets, inactive-source
rejection and SQLite save/reopen. The private-bus native AT-SPI test verifies the
named input, guarded keyboard link edits, note navigation/return and removal,
including saved heading/marks in a table cell. External browser/mail applications
and real pointer activation were not exercised. Real IME/Orca, RTL link geometry,
large-library performance and the overall port release gates remain open.
