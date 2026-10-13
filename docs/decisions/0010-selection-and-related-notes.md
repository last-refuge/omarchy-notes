# ADR 0010: selection tools follow layout; related notes follow explicit links

Status: implemented increment; full-port gates remain open.
Date: 13 October 2026.

A compact toolbar appears near a text selection without moving focus. It offers
bold, italic, underline, highlight, Link, More and dismissal. More opens the full
Format panel, using the same editor actions. Active and mixed marks have accessible
states. Ctrl+Shift+Space focuses the toolbar; Tab/Shift+Tab cycle its controls and
Escape returns to the selected words. No automatic animation or focus jump occurs.

A small custom element measures the toolbar and positions its deferred draw in
prepaint, after the editor's current shaped lines and scroll bounds are known.
It prefers above the selected rows, then below, stays inside the editor viewport,
and never covers the selection. If neither placement fits, it stays hidden;
the keyboard command falls back to the full Format panel. It is suppressed during
pointer dragging, IME composition, other editor panels and unrelated focus.
Escape/outside click dismiss it for the current selection. A new selection gesture
rearms it, including reselecting the same words. Actions capture document revision
and selection and refuse stale commands. There are no timers or global overlays.

Related notes are derived from explicit schema-1 text-span `note://` links. The
pure core extracts targets in document order, deduplicates by case-sensitive ID,
omits self references, and resolves current titles/folders from the live catalog.
It never infers relationships or follows text that merely looks like an address.
Missing targets retain a short source label and an explicit unavailable state.
The cached list refreshes after committed edits/catalog updates, outside render.

An optional footer inside document scrolling shows the related targets in a
fixed-row virtualized list. Ctrl+Shift+R brings it into view and focuses its header;
arrows choose a target, Enter opens it and Escape returns to writing. If there
are no targets, that shortcut opens the note-link picker. Missing targets reveal
the first corresponding stored link and open Edit, where the user can retarget or
remove it without changing its words. Navigation keeps the existing source guard
and return history. There is no additional relationship store, schema migration,
save revision or undo entry for showing or navigating the summary.

Pure tests verify placement bounds/non-overlap and explicit target identity,
ordering, deduplication, current metadata and missing targets. The real-font GPU
shell harness verifies actual placement, routed keyboard marks/undo/Escape,
dragging/composition suppression, reselecting dismissed words, related navigation,
return/repair, save/reopen and dark/light/compact layouts. A 5,000-target fixture
mounted four related rows at 1000 × 720 logical pixels and navigated directly to
the last target. This measures list mounting, not document virtualization,
latency, memory or display-presentation performance.

The native AT-SPI test verifies the actual selection toolbar, mark action/undo/
dismissal and related-note keyboard navigation/return while preserving stored
text and formatting. Real IME, Orca reading/announcements, RTL/bidi geometry,
mixed-DPI and comprehensive usability still need their dedicated passes. The
persistent toolbar/header and complete visual state matrix also remain subject
to final design review. Full M2/M3/M4 completion is not claimed.
