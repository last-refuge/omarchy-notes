# Bounded editor retention

Status: implemented for the Rust edition; release verification remains in R1.

A library session retains its note document and storage state. It creates the
GPUI editor only when that note is opened in main or Quick Note. Startup creates
the visible editor; incoming background captures attach only their document and
metadata. Rendering never constructs an editor.

Keep the main and Quick Note editors, plus at most four clean idle editors using
least-recently-opened order. Dirty revisions, queued writes, save errors, editor
errors, quotation upgrades, composition and pending media work are protected.
The clean cache bound does not bound unsaved drafts: keeping recoverable work
takes precedence over eviction.

Evicting a saved idle editor releases its model, undo snapshots, geometry,
media task and subscriptions. The session keeps the caret/selection and scroll
offset, a logical reading anchor and the exact typing context. The reading anchor
uses a document position and relative vertical offset, with the last actual font
and width signature. It survives typography invalidation of a hidden editor
before another draw. Reopening with unchanged geometry restores pixels; changed
font/size/width resolves the logical anchor at the first actual prepaint. An
explicit pixel-scroll override remains authoritative.

Reopening creates an editor from the retained committed document, applies
current font/size/theme choices and refreshes the link catalog. Restoring the
opaque core typing state preserves both inherited span properties and explicit
formatting choices at empty or boundary carets, without changing document,
revision, composition or history. Its undo history
starts empty. Frequently visited notes retain their history while cached.

The same editor still moves from Quick Note into main. Composition or unfinished
insertions still prevent handoff, close, export or persistence as appropriate.
Optional-editor helpers return false for composition/media work on unopened
notes; save/error/recovery checks continue to inspect every note's storage state.
An editor model revision awaiting its Changed callback is protected immediately.
Close, copy, export and recovery checks wait for the observed draft, and autosave
never queues the stale session body during that interval.

The `bounded-editor-check` GPU harness creates 1,000 stored notes, checks lazy
startup and cache size during traversal, evicts/reopens a scrolled selection,
checks reading anchors after eviction/font/width changes, hidden typography
changes before reflow, exact styled/boundary/empty-caret typing, current
typography/theme and protects dirty, queued, failed, composing
and importing sessions. Its injected fault states and synthetic composition
adapter are regression coverage; native IME and storage crash evidence remain
separate gates. Existing writing/Quick/settings/organization/launch/PDF harnesses
exercise real persistence and shared-session workflows after this change.
