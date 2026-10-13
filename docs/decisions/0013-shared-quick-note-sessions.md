# ADR 0013: Quick Note shares the library and editor session

Status: implemented for the Rust preview's local windows and initial `--quick-note`
launch. Forwarding a second window-launch request and activation tokens remain
separate work; capture forwarding is already implemented by ADR 0012.

## Experience

Quick Note is a small, ready-to-write window with the same typography, semantic
colors, rich editor, images, files and clipboard as the main notebook. Dark,
warm-paper and live Omarchy appearances use the existing theme. Empty space is
intentional. A quiet placeholder invites writing; the footer says “Ready when
you are” until there is something to save. Errors keep the draft visible, with
Retry and a route to the main window's recovery tools. At minimum window size,
the error text scrolls independently of the action buttons.

- `--quick-note` initially opens only Quick Note, without a hidden main window.
- Ctrl Shift N or the main toolbar opens Quick Note. Repeating this action raises
  the existing Quick Note rather than creating multiple competing scratchpads.
- Ctrl Shift O / Open in Notes moves the same editor into the main notebook.
  Ctrl Enter keeps its existing attachment-opening meaning.
- Ctrl W closes Quick Note after successful saves. An untouched Quick Note leaves
  no database note. A failed save keeps its window and draft open.
- The main window can close independently while Quick Note keeps writing and
  saving. Open in Notes can recreate the main window.

## Ownership and durability

The existing Shell entity owns one bounded storage worker and one editing entity
per note. Main and Quick Note views share that controller. A Quick Note view
holds the controller alive after the main window closes. Storage polling uses
the application executor and an available owned window, not the lifetime of the
first window. A window's close/recovery path clears its registered handle.

A session records its owning window. Only that window renders its rich editor.
Selecting a Quick Note in the main view performs a handoff; passive selection can
show a “Continue here” surface instead of rendering the editor in two windows.
Handoff preserves the model, current selection and complete undo/redo history.
Composition and asynchronous insertions must settle first. Find/link subscriptions
are rebound, obsolete image-preview work is cancelled and layout geometry is
invalidated before rendering in the destination.

At the pinned GPUI revision, `subscribe_in` associates the **subscriber entity**
with a window. The shared controller therefore uses ordinary per-session event
subscriptions, with explicit window routing for imports and links. Otherwise a
second window could silently redirect callbacks for the first. Responses remain
guarded by note identity, operation and local revision. The main search field is
rebound when the main window is recreated; programmatic search reset is silent
so a queued search event cannot navigate away from the just-opened note.

An untouched Quick Note has a stable UUID but lives only in memory. Revision zero
in storage denotes a draft's first INSERT. The first successful save commits the
note, its derived index and metadata together; it never upserts an existing ID.
Later saves use optimistic revision checks. Acknowledging revision N does not
mark N+1 saved. An attachment-only draft first queues its parent save, then import
on the same FIFO writer. Failed work is never presented as saved. Deliberately
moving an untouched Quick Note into Notes keeps it as a new note.

The preview retains its separate library and `org.omarchy.Notes.Preview` identity.
Public Qt executable/desktop identity and the final Quick Note identity are not
changed by this increment.

## Evidence and limits

The storage test covers first-save failure, stable identity, pinned metadata,
stale draft rejection and reopen. `quick-note-check` uses real fonts, GPU rendering
and SQLite to exercise Quick-only startup, untouched discard, revision-safe first
save, exact editor/selection handoff, undo/redo, composition/insertion guards,
find rebinding, a first attachment, main close/reopen, failure retention and
light/dark/minimum-size layouts. The existing writing/recovery regression also
continues to cover close-with-recovery behavior.

`tests/test-rust-quick-note-native.py` uses a private bus and disposable library,
with every synthetic input guarded by process, app ID, address and workspace.
It checks real Wayland windows, exact caret and undo handoff, rebound find,
untouched discard and continued saving after main-window close. Its compositor
focus command only selects an already-verified owned test window; the application
contains no compositor focus override.

The installed AT-SPI client reports AccessKit cache-signature warnings and can
retain closed-window nodes. The native oracle reads the current window's frame
rather than those stale nodes. This is a known accessibility limitation, not an
Orca signoff. Real IME, screen-reader usability and the full native clipboard,
portal and platform matrix remain unverified. A second invocation of
`--quick-note` still encounters the library-owner lock; use the in-app action
until launch forwarding and activation-token handling are implemented.
