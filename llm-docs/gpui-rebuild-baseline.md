# GPUI rebuild — baseline and progress

This is the implementation checklist for the [rebuild plan](rust-gpui-rebuild-plan.md).
The preview includes real rich editing, compatible SQLite storage, recovery,
image/file insertion, rich clipboard exchange, Qt-free PDF export, Quick Note,
durable capture, preferences and nested folder organization. The
Qt app remains the supported release. Status is recorded per capability rather
than treating a matching screenshot as feature parity.

## Baseline and ownership

| Capability | Current evidence / reference | Planned owner | Rust status |
|---|---|---|---|
| JSON document v1, stable block IDs, marks and nested structure | `RichDocument.*`, `tst_document`, captured `tests/fixtures/qt-v1/documents.json` | `notes-core` | Compatible reader/writer with fixture round trips; unknown versions rejected |
| Headings, text marks, lists/checks, merged tables, image/file objects, note links | `EditorController.*`, `DocumentConverter.*`, `tst_editor` | `notes-editor`, `notes-ui` | Six text marks, heading/list styles, formatting/selection panels, editable links/related notes, quotations with explicit upgrade, table creation/structural editing, inline images and attachment actions; new table merges pending |
| Unicode, IME, selection, clipboard and undo | `tst_document`, `tst_editor` | `notes-editor`, `notes-ui`, `notes-platform` | Document selection, grapheme/UTF-16 and composition logic, per-note undo, portable rich MIME tested on Hyprland; real IME/Orca and other desktop transports pending |
| Library CRUD, folders, tags, Smart Folders, sort and trash | `NoteStore.*`, `tst_store` | `notes-core`, `notes-storage` | Real notes/pins; nested create/rename/move and ID-based filing; folder deletion, tags/Smart Folders, remaining sort modes and trash pending |
| Global search and in-note find | `NoteStore.cpp`, `EditorController.cpp` | `notes-storage`, `notes-editor` | Loaded-note search and literal Unicode in-note find work; compatible FTS maintained; FTS query backend pending |
| List, gallery, attachment browser, thumbnails | QML views, `BlobImageProvider.*` | `notes-ui`, `notes-storage` | Virtualized text list; decoded editor previews; gallery and attachment browser pending |
| Autosave, conflicts, history, blobs, GC, failures and crash survival | `LibraryService.*`, `BlobStore.*`, `tst_store`, `crash_helper` | `notes-storage` | Revision-guarded autosave, conflicts/copy recovery, transactional history, failure retention and acknowledged-write kill test; history UI and GC pending |
| Markdown/text import; Markdown/HTML/PDF/library export | `NoteExchange.*`, `tst_exchange` | `notes-exchange`, `notes-export` | Letter/A4 PDF implemented with explicit limits; other interchange pending |
| Consistent backup, validated restore and rollback | `LibraryBackup.*`, `tst_store` | `notes-storage` | Verified headless backup/restore, Qt round trips, schema-1 preservation and quote-upgrade rollback; general in-app backup/restore pending |
| Single instance, capture, Quick Note, launch actions, Hyprland setup | `main.cpp`, `InstanceChannel.*`, `tst_launch` | `notes-platform`, binary | Durable capture/replay/status, Quick Note with shared sessions, forwarded window actions and activation adapter; remaining desktop/launcher acceptance and installed actions pending |
| Theme watching, contrast, XDG paths | `ThemePalette.*`, `ThemeController.*`, `tst_theme` | `notes-platform`, `notes-ui` | Theme polling, contrast adjustment, reference/light presets, persistent palette/font/size/pane/focus/sort preferences |
| Beautiful native composition and adaptive layouts | Generated reference, token specification in the plan | `notes-ui` | First running shell; visual correction and accessibility work continue |
| Packaging and installer | Existing CI, `packaging/`, installer tests | binary, packaging | Qt distribution retained |

## Decisions made

- See [ADR 0002](../docs/decisions/0002-gpui-preview-foundation.md) for the exact
  dependency pin, toolchain, ownership boundary, license inventory and limits.
- Favorites uses the pin state. Related notes resolve explicit note links.
  Real quotations use the opt-in backed-up schema upgrade in ADR 0011.
- Search adds Ctrl+K; Ctrl+Shift+F remains an alias. Focus mode uses Ctrl+backslash.
- Default Rust data lives in a separate `omarchy-notes-gpui` library. Explicit
  `--data-dir` accepts a compatible copy; tests never select production data.
- [ADR 0003](../docs/decisions/0003-compatible-rich-writing.md) keeps database
  schema 2/document schema 1 and documents the lock, save and recovery contract.
- Personal writing and pane preferences persist independently of libraries;
  see ADR 0015. Demo choices remain session-only.

## Milestone progress

- **M0 started:** contract inventory, resolved design semantics, exact dependency
  decision, deterministic visual fixture and an authentic schema-2 Qt library
  with schema-1 documents and blobs, plus a Qt-generated merged-table fixture.
  Schema-1 and backup round trips now have fixtures/checks. Remaining: complete
  fault fixtures and legacy desktop/shortcut inventory.
- **M1 started:** native window, typography/assets, semantic colors, functional
  navigation/search, focus mode, adaptive layout and splitters. Remaining:
  Orca usability, full theme/platform coverage and final visual quality sign-off.
  Persistent writing and layout preferences are implemented.
- **M2 in progress:** real create/edit/save/reopen, rich selection/undo, table creation,
  cell navigation and structural edits, compatible storage, conflicts, safe normal close and draft export are
  implemented, plus media insertion/previews/actions and rich MIME exchange on
  Hyprland. Rich-editor AT-SPI text/selection and keyboard editing now have a
  native API check. Remaining gates: real IME, Orca usability, remaining accessible
  document structure, wide/large table
  layout and typing/layout measurements. Qt-free PDF now has structural, text
  and visual checks; actual portal/viewer interaction remains unverified. This increment does not claim the full feasibility gate passed.
- **M3/M4 started:** formatting commands/menus and find-in-note implemented.
  Editable links, note navigation, the selection popover and explicit related-note
  summaries, quotations, durable capture, shared Quick Note, window forwarding,
  personal Settings and nested folders/filing are implemented. Full library
  organization/search, portability and remaining acceptance gates are open.
  **M5/M6 pending:** hardening and cutover.

## How to verify this increment

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all --check
cargo run --release -p omarchy-notes-preview -- --reference
cargo run -p notes-ui --example visual-check --features visual-tests -- target/visual-check
cargo run -p notes-ui --example writing-check --features visual-tests -- target/writing-check
cargo run -p notes-ui --example media-check --features visual-tests -- target/media-check
```

The optional visual harness uses the platform's real text system and GPUI's GPU
headless renderer. Those images verify layout without capturing desktop content;
they do not establish real input-method, screen-reader or compositor correctness.
Ordinary unit tests do not require a GPU/display. The visual command does require
a working GPU driver (or a separately configured software rendering environment).

The current Qt baseline can be rebuilt with ordinary CMake/Ninja commands. Its
six CTest targets passed during this increment. The fixture generator additionally
sets `QT_QPA_PLATFORMTHEME` empty because the host's GTK theme plugin attempted
to open an unavailable X display during the first offscreen fixture launch.

### Evidence and outstanding native checks

- **Observed:** optimized preview launches on Wayland with the preview app ID;
  GPUI selected the Intel Arc B390 Vulkan adapter. Existing Qt build remains usable.
- **Observed:** initial compositor capture inherited desktop opacity and showed
  faint background content. That capture was deleted and excluded from repository artifacts;
  use isolated GPU renders for shareable visual evidence. No desktop configuration
  or opacity rule was changed.
- **Measured:** test/check outcomes and GPU visual artifacts are recorded in
  [the preview guide](../docs/gpui-preview.md). Eight Rust unit tests, all-feature
  Clippy, formatting, six Qt CTest targets and the GPU input/visual harness passed
  in the foundation increment. See the writing evidence below for newer checks.
- **Not verified:** the attempted AT-SPI query did not find the preview process
  in the desktop accessibility tree; investigate registration before M1 sign-off.
- **Unverified:** real IME composition/candidate geometry, Orca and full editable
  accessibility, X11 runtime, suspend/resume, mixed-DPI, cold start and long-session
  memory. The rich editor and compatible migration now exist; these platform
  checks still block release sign-off.


### Writing increment — 12 October 2026

- **Measured:** 25 Rust test results passed, including real transaction rollback
  after a failed derived-data write, Qt fixture round trips, populated schema-1
  migration, future-version refusal, conflicts, asset-verified recovery export
  and a separate writer killed after 20 acknowledged commits. Formatting and
  all-target/all-feature Clippy with warnings denied passed on this tree.
- **Observed:** the release app on Wayland/Intel Arc B390 Vulkan created a heading
  and a bold body phrase with Unicode using input guarded by exact PID/window/
  active workspace. After a committed save it was killed, relaunched on the same
  scratch library, searched, edited again, saved and closed with Ctrl+Q. All
  test windows/processes were closed; no production library or desktop settings
  were changed, and no compositor screenshots were taken.
- **Measured:** headless help/version and stdin capture passed with no display,
  HOME or desktop environment. The actual Qt reader validated four Rust-written
  notes; both directions of Qt/Rust lock exclusion and unchanged blob hashes
  passed. The CI compatibility step is configured; hosted execution is unverified.
- **Observed:** both GPU harnesses passed. The writing harness additionally covers
  composition/Enter, same-process rich clipboard, table-cell editing, late save
  acknowledgments, per-note undo, failed save/close and draft recovery. Reviewed
  dark, light, compact, empty and error states at 1440, 1024, 640 and 420 logical
  widths; exported images are direct 2× GPUI renders. No display-latency or
  large-document performance claim follows from these checks.
- **Resolved test limitation:** five existing Qt CTest targets passed in the
  sandbox. `tst_launch` could not create its single-instance Unix socket; a
  separate socket probe confirmed `EPERM`. That target passed outside the
  sandbox with its existing isolated temporary libraries/configuration. All six
  existing Qt targets therefore passed with the required runtime permissions.
- **Unverified:** a fresh AT-SPI inspection did not find the owned Rust editor
  process. Orca and accessible text/selection remain blockers; real IME candidate
  behavior, cross-app rich MIME, X11, mixed-DPI and long-session behavior remain
  unverified. Input-handler composition tests are not a real-IME certification.


### Media and clipboard increment — 12 October 2026

- **Measured:** 37 Rust tests passed, including byte-exact portable assets, blob
  deduplication, remapped attachment ownership (including recovery copies), failed
  imports, missing/corrupt blobs, export refusal to overwrite, oversized image/file
  refusal and nonblocking special-file rejection. All-target/all-feature Clippy
  with warnings denied and formatting passed.
- **Observed:** the optimized app on Hyprland accepted HTML with bold/italic/list
  content, a PNG offer and a local-file URI offer from an independent clipboard
  process. A second Rust process with a separate scratch library received its
  custom MIME selection, including both files and remapped attachment IDs.
  Both libraries contained identical hash-verified asset bytes. Plain-text paste
  omitted media/marks. SIGKILL/reopen retained the acknowledged mixed document.
- **Observed:** the WLR adapter offered custom JSON, HTML and plain text together;
  `wl-paste` read exact bytes for each. Self-paste and cross-process selection,
  HTML preference and explicit plain-text preference passed. The probe only read
  clipboard data installed by the test and all owned processes were closed.
- **Measured:** the Qt oracle validated six Rust-written notes, now including
  imported image/file objects and a copied note. Mutual Qt/Rust lock exclusion
  and unchanged original blob bytes passed.
- **Observed:** writing, shell and media GPU harnesses passed with real fonts.
  Media checks cover cached decoded images, cards, selection/remove/undo,
  stale-cursor confirmation, close guard, cancellation, failed import and reopen.
  Direct GPU artifacts cover graphite/champagne, warm paper, 640 and 420 logical
  widths and a failure state. File cards use rounded corners, readable type/size,
  and contextual actions; a selected image does not show a stray text caret.
- **Unverified:** actual portal picker interaction, desktop drag/drop delivery,
  external viewer launch, clipboard-manager persistence, X11/non-WLR rich
  transport, real browser/office-suite fidelity, real IME and Orca remain open.
  The unit/GPU/native clipboard checks do not certify those paths. Large-document
  virtualization, long-session memory and display latency remain unmeasured.
- **Scope:** no production library or desktop setting was changed. No compositor
  screenshot was taken. Qt remains the supported release and installer.

See [ADR 0004](../docs/decisions/0004-media-and-rich-clipboard.md) for limits and
failure semantics, and the [preview guide](../docs/gpui-preview.md) for use and
repeatable verification commands.

### PDF increment — 12 October 2026

- **Measured:** 46 workspace Rust tests passed on the final implementation,
  including seven exporter tests and two GPUI export workflow tests. All-target,
  all-feature Clippy with warnings denied, formatting and the optimized build
  passed. Test-platform picker responses cover cancellation, existing destinations,
  immutable snapshots, newer edits, note switching, stale results and close guards.
- **Measured:** qpdf and Poppler checks passed for the authentic mixed Qt fixture
  on Letter/A4 (two pages each) and a synthetic long-table document (five pages).
  Verified syntax, selectable expected text, embedded fonts/Unicode mappings,
  image inclusion, page dimensions, repeated headers, exact paragraph/long-row
  tokens, merged cells, link targets and text/link bounds, including RTL links.
  The source document and fixture database remain unchanged. CI now includes
  these checks and the required native libraries/fonts; hosted CI is unverified.
- **Observed:** all nine rendered PDF pages were visually reviewed. Text, marks,
  multilingual shaping, chart proportions, attachment labels, merged cells and
  table continuation remain legible without clipping or lost content. This is
  a representative feasibility export, not full Qt print parity or PDF/A.
- **Observed:** the real-font GPU PDF harness passed for graphite/champagne and
  warm paper at 1440 × 860, and compact 420 × 860 / 420 × 420 windows. Reviewed
  ready, success and existing-file failure states. The export panel is capped
  and scrollable at short heights, preserving its controls and the save footer.
  Images in `docs/images/gpui-pdf-*.png` are direct GPUI renders; no compositor
  screenshots or unrelated window content were captured.
- **Observed:** a release window on Hyprland passed exact-PID/address/app-ID/
  workspace-guarded Ctrl+Shift+P focus, Escape back to writing, and durable save.
  The scratch library was removed and the owned process closed. No clipboard
  access or desktop configuration changes occurred. This verifies the shortcut
  and focus path, not actual portal interaction or external viewer launch.
- **Measured:** `ldd` shows Cairo, Pango, GLib and GObject as shared dependencies
  and no Qt dependency in the Rust executable. Local versions: Cairo 1.18.4,
  Pango 1.58.2. Installed language fonts supplied the tested fallback glyphs.
- **Unverified:** real portal save/cancel/error behavior, external PDF viewer
  activation, other font configurations, PDF/A/tagged-PDF accessibility, real
  IME/Orca, display scaling and large-document latency/memory. Native input and
  performance gates therefore still hold M2 open.

See [ADR 0005](../docs/decisions/0005-qt-free-pdf-export.md) for resource bounds,
unsupported-table behavior, native licenses and atomic export semantics.

## Accessible rich text increment — 13 October 2026

- **Implemented:** a multiline AccessKit input with synthetic text runs from the
  real editor model and shaped layout. Text includes Unicode, paragraph breaks,
  inline objects, empty paragraphs and offscreen content. Caret/selection actions
  use normal editing and save paths, with stale-revision/selection and IME guards.
  Runs expose formatting and character bounds. Checklists expose actionable
  checked states; image alt text, attachment names and actual error messages are
  accessible. See [ADR 0006](../docs/decisions/0006-accessible-rich-text.md).
- **Observed:** the release build passed `test-rust-accessibility-native.py` on
  Hyprland with AT-SPI 2.60.6. A private bus and owned registry let the client find
  the exact app PID, resolving the earlier registration investigation for this
  isolated configuration. Direct API checks passed for complete text, formatting,
  character bounds/hit testing, Unicode selection, guarded keyboard editing/undo,
  checkbox actions/state, named media, new empty notes and SQLite persistence.
  The search provider exposes query text/selection without including placeholder
  text; accessible query replacement returns the expected filtered note.
  Test libraries, owned windows and registry processes were cleaned up. No
  desktop settings, clipboard content or unrelated windows were used.
- **Observed:** all 51 Rust tests passed, including new Unicode/empty/long-run
  mapping, reversed selection, stale replacement, undo and composition guards in
  writing and search.
  Workspace Clippy (all targets/features, warnings denied), formatting and release
  build passed. The real-font GPU writing harness passed. Dark, light and compact
  renders were reviewed; the wrapper preserves the established visual layout.
- **Observed limitation:** the installed AT-SPI client logs cache-signature
  warnings from AccessKit's cache messages. Direct queries and actions passed;
  event-driven screen-reader behavior is not inferred from those results.
- **Unverified:** actual Orca reading, caret/selection announcements, the ordinary
  desktop bus configuration, real IME, RTL geometry and mixed-DPI.
  Heading/table structure and large-document latency
  remain open. The pinned Unix adapter has AT-SPI `Text` but not `EditableText`;
  accessible selection followed by keyboard input is the tested editing path.
  M2 and the overall Rust port are still in progress.


## Compatible table editing increment — 13 October 2026

- **Implemented:** table creation, row/column insertion and deletion, splitting
  existing merged cells, removal, Tab/Shift+Tab cell selection and an explicit
  return to writing after the table. Surviving rich content, block identities,
  unknown metadata and merge coverage are preserved. Each structural operation
  has one undo step and uses existing guarded autosave. The contextual panel has
  keyboard focus, applicable commands and scrolling in short windows. See
  [ADR 0007](../docs/decisions/0007-compatible-table-editing.md).
- **Observed:** all 60 Rust tests passed. New editor cases include 54 combinations
  of merged-cell positions and axis edits, plus sparse/nested tables, invalid
  overlap, cross-cell rejection, composition guards and undo/redo. Workspace
  Clippy (all targets/features, warnings denied) and formatting passed.
- **Observed:** 39 documents passed actual Qt editable-document load/read checks:
  one authentic Qt 6.11.2 merged-table fixture and 38 Rust-edited documents. Rust
  results also survived SQLite reopen. The oracle checks exact known-schema
  equality; separate pure tests check unknown-property preservation. CI now runs
  this check. Fixture provenance is in `tests/fixtures/qt-merged-table.md`.
- **Observed:** real-font GPU checks passed table insertion, routed Tab/Shift+Tab,
  automatic row addition, replacement, undo/redo, keyboard options/Escape, body
  focus navigation, save/reopen and injected save failure. Dark, light, 420 × 820
  and 420 × 420 logical-pixel renders were reviewed. Focused options scroll into
  view; short windows retain Done, Escape and the saved-status footer.
- **Observed:** the release native AT-SPI/Hyprland check passed creation, typed
  cell contents, Tab row addition, row/column edits, removal/undo, after-table
  writing and durable save, alongside the prior accessibility/search checks.
  Every input was guarded to the owned window. The test closes its own app and
  registry and uses only a disposable copied library and private bus.
- **Corrected during verification:** the pinned Linux accessibility adapter's
  disabled-button mapping exposed inactive commands as enabled; the menu now
  displays only applicable commands. Rejected AT-SPI cache events also left the
  test client's table labels stale after undo. Explicit fresh property reads
  confirmed the editor state. This remains an API test, not evidence that Orca's
  event-driven behavior works.
- **Still open:** real IME/Orca, accessible heading/table navigation, RTL geometry,
  large/wide-table layout and document performance; new merges and fully leafless
  imported tables also need work. This increment does not complete M2 or the port.


## Formatting and find increment — 13 October 2026

- **Implemented:** a compact Format panel with all six legacy text marks, title/
  heading/subheading/body, three list types and list indentation. It retains
  selection, supports keyboard focus/scrolling/Escape and exposes active or mixed
  marks. Inside-word formatting preserves the caret; boundary formatting affects
  subsequent typing. Existing list indentation/check state survives idempotent
  style application. The toolbar is quieter, with extended choices in the panel.
- **Implemented:** a separate accessible find-in-note field, Unicode literal
  case-insensitive matches, current/total feedback, highlighted results, wrapped
  next/previous navigation and return to writing. Search never changes note
  revision, undo or stored content. It searches table cells and styled spans,
  updates after edits and guards composition. Query/results have explicit bounds.
  See [ADR 0008](../docs/decisions/0008-writing-tools-and-find.md).
- **Observed:** 67 Rust tests passed, including new mixed/word/boundary formatting,
  checklist preservation, rejected commands/composition, Unicode/literal matching,
  wrapping, table boundaries, refresh and resource-bound cases. Formatting and
  workspace Clippy (all targets/features, warnings denied) passed.
- **Observed:** the actual GPUI shell's real-font GPU harness passed find,
  formatting menus/shortcuts, both digit and shifted-symbol list bindings,
  toggling, undo/redo, composition focus guards and save/reopen. It verifies search
  leaves document revisions and SQLite content untouched. Dark/light, 420 × 820,
  420 × 420 and no-result states were inspected. Focused format options scroll
  into view. Existing writing/clipboard/close/recovery GPU checks also passed.
- **Observed:** the release build passed the full private-bus AT-SPI/Hyprland
  regression, including table-cell find/count/next/previous, all new mark
  shortcuts, heading/menu changes, numbered/bulleted lists, undo and durable
  formatting, followed by the library-search workflow. Fresh state reads and
  explicit action/focus waits make asynchronous native transitions testable.
  Owned windows, registry processes and scratch libraries were cleaned up.
  Existing AccessKit cache-signature warnings remain; actual Orca is unverified.
- **Corrected during verification:** Linux consumes Shift for punctuation, so
  Ctrl+& and Ctrl+* supplement digit bindings for numbered and bulleted lists.
  Native wtype tests request the corresponding symbol keysyms explicitly. The
  GPU harness opens each viewport at its actual logical size because a headless
  Window::resize call alone only changes the render surface.
- **Still open:** the selection popover, link editing/navigation, quotes and schema
  migration, Quick Note/IPC, full library parity, native IME/Orca, performance and
  release hardening. This advances M3/M4 without declaring M2 or the port complete.

## Editable links and note navigation — 13 October 2026

- **Implemented:** address and note destination panels, searchable virtualized
  note results, keyboard insertion/edit/removal, safe explicit activation,
  navigation/return and clear missing-target feedback. Selected words, marks,
  metadata, table structure and object references survive link changes; each edit
  has one undo step. Revision/selection, source-identity and composition guards
  protect the destination of commands. Existing schema-1 hrefs remain compatible.
  See [ADR 0009](../docs/decisions/0009-editable-links.md).
- **Observed:** 73 workspace tests, all-target/all-feature Clippy with warnings
  denied, formatting and release builds passed. New tests cover destination
  policy, Unicode, differently styled link runs, partial removal, insertion,
  undo/redo, table cells/objects, invalid inputs and composition.
- **Observed:** actual shell GPU tests passed routed link edits and keyboard
  removal, invalid/stale dialog rejection, navigation/return, missing targets,
  inactive-source rejection, composition focus and SQLite save/reopen. Dark,
  light, 420 × 820, 420 × 420 and missing-target states were inspected. The initial
  short-window clipping was corrected: the picker replaces the formatting strip
  and sizes the results to the content and available height.
- **Observed:** the native release passed the private-bus AT-SPI/Hyprland workflow
  for named destination input, saved web/note links, in-app navigation/return and
  removal without disturbing table headings or marks. Existing rich editing,
  tables, find, formatting, media naming and global-search checks also passed.
  Tests use a scratch library and guard their owned window before/after input.
- **Unverified:** external browser/mail launch, real pointer activation, RTL link
  geometry and very large picker performance. The longstanding real IME, Orca
  and platform gates remain open. This does not complete M2, M3 or the port.

### Full-port completion audit after the links increment

The objective still covers the entire Rust replacement. No unfinished requirement
is removed by this increment; the Qt release/installer remains the supported app.

| Requirement | Current evidence and remaining work |
|---|---|
| M0: contracts and fixtures | Inventory, core schema/media fixtures and Qt table oracle exist. Complete the remaining legacy shortcut/IPC ownership inventory and migration/backup/fault fixtures. |
| M1: native visual foundation | Fonts, themes, panes, focus and compact layouts have native/GPU evidence. Persist settings and pane widths, complete font/text preferences and theme coverage, and obtain final visual/accessibility signoff. |
| M2: rich editing and compatible storage | Real writing, saving, undo, tables, media, clipboard, accessible text and PDF exist. Real IME/Orca, heading/table semantics, RTL geometry, leafless/wide/large tables, document virtualization and measured large-document performance remain. Complete native portal/viewer and clipboard/platform matrices. |
| M3: daily writing/capture | Formatting, find and editable links now work. Build the selection popover, quotes with migration/backup fixtures, Quick Note and durable CLI/IPC. Finish recovery/fault evidence and responsive daily-writing acceptance. |
| M4: full library/portability | Search over loaded text, favorites and note links exist. Finish folders/nesting/moving, tags/Smart Folders, date groups, FTS-backed search, gallery/attachment browser, related-note summaries, trash/undo, non-PDF interchange, backup/restore and complete Settings/action routes. |
| M5: native hardening | Selected Wayland and API workflows pass. Complete actual IME/Orca, scaling/tiling/platform checks, suspend/resume, performance/memory, crash/fault/migration/downgrade rehearsals, first-use study and five-day trial. |
| M6: controlled replacement | Pending: full dependency/license/runtime inventory, Arch/Cargo packaging, clean installed launch/capture, public identity/CLI/installer cutover, release artifacts/docs and tested Qt rollback. |
| Beauty and delight | New link panels use the shared typography, surfaces, focus and compact-layout treatment, with reviewed GPU images. The full reference/state matrix, selection popover and user study remain release requirements. |

Next implementation work can continue on the selection formatting popover and
explicit related-note summary, while the feasibility and release gates stay open.

## Selection formatting and explicit related notes — 13 October 2026

- **Implemented:** a selection toolbar using the current frame's shaped geometry,
  constrained to the editor viewport and outside selected rows. Common marks,
  Link, More, keyboard cycling, Escape/outside dismissal and stale-action guards
  share the existing editing model. It suppresses during dragging/composition and
  other panels. A new selection gesture rearms dismissal. When placement cannot
  fit, the keyboard command uses the full Format panel.
- **Implemented:** a cached, optional related-note footer based only on explicit
  note links, deduplicated by target identity with current titles/folders. Its
  virtualized results support Ctrl+Shift+R, arrows/Enter, navigation/return and
  recovery of unavailable targets through the actual source link. Viewing or
  navigating the summary never changes the stored body or adds undo entries.
  See [ADR 0010](../docs/decisions/0010-selection-and-related-notes.md).
- **Observed:** 75 workspace tests, all-target/all-feature Clippy with warnings
  denied and formatting passed. Release GPU checks cover toolbar placement,
  keyboard formatting/undo/dismissal, routed drag/reselection, composition guards,
  related navigation/return/repair and unchanged SQLite content after reopen.
  Dark/light, 420 × 820 and 420 × 420 states were inspected.
- **Measured:** the related list mounted **4 rows for 5,000 explicit targets** at
  **1000 × 720 logical pixels** in the release headless GPU shell harness. Up from
  the first item reached the last target and opened the correct source link for
  repair. This is a mounting bound, not a memory, latency or document-virtualization
  benchmark; the editor still shapes full documents.
- **Observed:** native AT-SPI/Hyprland checks exposed the selection toolbar and
  exercised highlight/undo/dismissal and related-note keyboard navigation/return.
  Existing text/media/table/find/link/search checks stayed intact. Owned processes
  and scratch libraries are cleaned up. Actual Orca and IME remain unverified.

### Updated full-port completion audit

The audit above still applies with these concrete changes: the **selection
popover (M3)** and **explicit outgoing related-note summary (M4)** are implemented
and verified for the stated workflows. Neither milestone nor the overall port
is complete. All other requirements remain in scope:

- **M0:** finish legacy contract/shortcut/IPC inventory and migration, backup and
  fault fixtures.
- **M1:** persist settings/pane widths, complete theme/font/text preferences and
  finalize reference/state-matrix visual review.
- **M2:** real IME/Orca, heading/table semantics, RTL geometry, leafless/wide/large
  tables, document virtualization/performance and the remaining native clipboard,
  portal and platform evidence.
- **M3:** real quote blocks and migration/rollback fixtures, Quick Note, durable
  CLI/IPC and remaining save/recovery/fault acceptance.
- **M4:** folders/nesting/moves, tags/Smart Folders, date groups, FTS search,
  gallery/attachment browser, trash/undo, interchange, backup/restore and Settings.
- **M5:** comprehensive platform, input, accessibility, performance, memory,
  fault/migration/downgrade and user-study/five-day-trial gates.
- **M6:** dependency/license/runtime audit, clean packaging and installed capture,
  public identity/CLI/installer cutover, release documentation and tested rollback.

Next, address quote-block persistence and migration as a compatible, recoverable
change; do not silently reinterpret italic paragraphs or flatten quote structure.

## Verified library backup and restore foundation — 13 October 2026

- **Implemented:** a synchronous storage API and headless `--backup`,
  `--inspect-backup` and `--restore` commands. Backups use a consistent SQLite
  snapshot under compatible exclusive ownership, preserve all tables and
  registered blobs, validate current/history documents and references, check
  copied search indexes, and stream/check blob hashes. Qt backup format 1 is
  retained with an optional database digest. Schema-1 backup sources remain
  untouched. Restoring requires a new explicit destination; it preserves both
  the source backup and existing libraries. See [ADR 0011](../docs/decisions/0011-backups-and-quote-migration.md).
- **Implemented:** unique private staging, file/directory syncs, manifest-last
  completion and atomic no-overwrite publication. Existing paths and unrelated
  partial folders are never replaced/deleted. After publication, sync failures
  preserve and identify the completed destination. Restore also rereads the
  actual staged database and blob bytes before publishing.
- **Observed:** 85 workspace tests passed, including ten new storage tests. One
  ignored child helper is explicitly run by the parent SIGKILL test. Nine killed
  children cover backup/restore snapshot, blob, manifest and publication
  boundaries; retries succeed and originals survive. ENOSPC/write faults,
  same-size corruption, destination corruption, missing/symlink/FIFO assets,
  future formats/documents, broken references/FTS, ownership and destination
  races are covered. A 33 MiB registered asset survives streamed backup/restore.
- **Observed:** the release CLI passed actual Qt backup creation → Rust restore
  and Rust backup → Qt inspection/open. Logical contents of every table,
  history/trash/folders/Smart Folders and blob bytes survive. Qt format-1 manifests
  without a database digest and schema-1 snapshots restore correctly. Backups
  remain byte-for-byte unchanged. The existing Qt document/lock regression also
  passed. CLI checks run with graphical/session environment variables cleared.
- **Observed:** all-target/all-feature Clippy with warnings denied, formatting,
  release build and diff checks passed. CI includes the Qt backup oracle. Test
  children exited and no repository-built processes remained running.
- **Unverified:** actual disk exhaustion, power loss, network-filesystem behavior,
  production-scale throughput/cancellation and backup UI usability. No new GUI
  verification is claimed. In-app progress/cancellation, backup/restore Settings
  routes and the explicit quote-upgrade/rollback UI remain unfinished.

### Completion audit after the backup increment

This is **verified progress**, not completion of the Rust port. The full audit
above remains authoritative. The backup/restore backend and headless commands
advance M0/M3/M4/M5; no unfinished requirement has been removed or reduced.

Next implement actual editable quote blocks and the explicit schema-3 migration
using this verified backup path. ADR 0011 records document-schema-2 examples,
conversion/Enter/undo rules, compatibility limits and the transaction/rollback
contract; these new versions are **not enabled yet**. Quote editing, native
dark/light/compact styling, clipboard/HTML/Markdown/PDF round trips, upgrade
approval/progress, migration interruption and actual old-Qt rejection remain.

The rest of the objective stays open: Quick Note and durable CLI/IPC; folder/tag/
Smart Folder/trash/gallery/search/interchange/Settings parity; durable preferences;
real IME/Orca, semantics/RTL/table and performance/virtualization work; full native
platform/fault/downgrade/user-study/five-day-trial gates; and dependency audits,
packaging, public installer/identity cutover and release artifacts. Beauty and
delight remain mandatory for the recovery and upgrade workflows as well as the
editor. Qt is still the supported installed edition.

## Quote editing and explicit format upgrade — 13 October 2026

### Delivered behavior

Real quote blocks now match the design's restrained gold rule and inset. Format
→ Quote and Ctrl+Shift+Q preserve the passage's IDs, text, marks and links. Enter
continues a quote, an empty Enter exits it, Shift+Enter adds a soft break and undo
restores the exact earlier structure. Quotes work in table cells. Pasting one
into prose retains separate paragraphs on either side. JSON/rich clipboard and
HTML preserve the quote type; the Markdown conversion backend supports `>` and
reports nested-quote flattening and media-label-only export. Full file interchange
is still pending. PDF keeps the rule through pagination.

Database schema 2/document schema 1 remain the default. Saving the new quote type
requires explicit consent in a panel that explains Qt incompatibility and names
the backup location. The storage worker creates and verifies a complete backup
of saved content, then atomically records the migration receipt and database
version 3. Existing bodies/history are untouched; subsequent saves emit document
schema 2. Unsaved drafts remain in memory and save after success. Cancellation,
failed backups, new destinations, retry, late events and close guards have explicit
behavior. Action buttons remain visible at compact sizes; explanation/path details
scroll with keyboard or mouse, and final status scrolls into view.

A backup is a recovery point, not a live mirror. The old Qt edition refuses the
upgraded library. Restore the pre-upgrade backup into a new directory to return
to Qt, keeping later Rust edits separately. General in-app backup/restore and its
Settings routes are not implemented by this quote-specific panel.

### Evidence

- **Observed:** 95 workspace tests pass. Two ignored subprocess helpers are
  explicitly invoked by their parent kill tests. Quote tests cover conversion,
  Unicode/marks/links, undo/redo, Enter, composition guards, table fragments, paste
  boundaries, HTML/Markdown/JSON, long PDF quotations and version rejection.
  Migration tests cover refusal before opt-in, complete verified backup, receipt,
  idempotence, unchanged old bodies/history, cancellation and pre-commit failures.
  Three additional SIGKILL boundaries prove version/receipt atomicity and retry.
- **Observed:** the actual Qt reader rejects schema-3 libraries and backups,
  while a restored pre-upgrade schema-2 backup opens in Qt with its prior data.
  A separate post-upgrade Rust backup/restore retains later quote edits. The
  broader full-library backup oracle also passes. These are scratch fixtures;
  no personal library was migrated.
- **Observed:** the real GPUI shell with its storage worker passes routed quote
  editing, opt-in/cancel, failed destination/retry, stale completion rejection,
  close guards, newer edits during upgrade, rich clipboard, save/reopen and
  rollback. Direct GPU renders with actual fonts were reviewed in dark/light at
  1440 × 900 and compact at 420 × 820 and 420 × 420. The first compact panel hid
  actions; the corrected layout pins them and keyboard-scrolls the details.
  Formatting/find, selection/related-note and link workflow regressions pass.
- **Observed:** native Wayland/AT-SPI actions passed cancel-without-upgrade,
  explicit opt-in, verified backup, committed quote save and undo preserving
  heading/marks. The existing native text/selection/table/link/search regression
  passed in the same run. Every synthetic key/character was guarded before and
  after by owned PID, address, app ID and active workspace; no screenshots or
  desktop configuration changes were used.
- **Observed:** Letter and A4 quote fixtures each produce four valid PDF pages.
  All 140 numbered passages and the table quote occur exactly once, within page
  bounds, with selectable text and embedded fonts. Raster review of the first
  and last Letter pages confirmed continuous rules and the inset table quotation.
- **Observed:** all-target/all-feature Clippy with warnings denied, formatting,
  release build and diff checks pass. CI includes the quote PDF fixtures and
  actual Qt rejection/rollback oracle. Reviewed images and usage/rollback details
  are linked from the preview guide and ADR 0011. Test children exited; no
  repository-built processes remained running after verification.
- **Unverified:** actual IME and Orca, quote-specific spoken semantics, native
  backup destination picker, production-scale cancellation/backup usability,
  actual disk exhaustion/power loss, and the full display/platform state matrix.
  The AccessKit cache-signature warnings remain; this is live AT-SPI API evidence,
  not a screen-reader usability pass.

### Completion audit after the quote increment

This is **verified progress**, not completion of the Rust port. M3 quote editing
and the explicit version boundary are implemented; no remaining milestone has
been deleted, reduced or silently declared complete. The next implementation
focus is Quick Note and durable CLI/IPC capture/ownership.

Still required: the remaining legacy shortcut/IPC inventory and fault fixtures;
persisted pane/theme/font/text preferences and full visual-state signoff; actual
IME/Orca, heading/table/quote semantics, RTL geometry, wide/leafless/large tables,
large-document virtualization/performance and native clipboard/portal matrices;
Quick Note, durable capture and recovery acceptance; folders/nesting/moves,
tags/Smart Folders, date groups, FTS-backed search, gallery/attachment browser,
trash/undo, complete file interchange, in-app backup/restore and Settings; the
full fault/downgrade/platform/performance/memory/user-study/five-day-trial gates;
and license/runtime audits, packaging, installed launch/capture, public identity,
CLI/installer cutover, release artifacts and rollback instructions. Qt remains
the supported installed app. Beauty and delight remain requirements for every
remaining workflow, including errors, cancellation and recovery.

## Durable CLI capture and the Rust instance channel — 13 October 2026

### Delivered behavior

`--capture` now saves through the running Rust app's single writer, or directly
when no edition owns the library, without creating a window or GPU context. It
uses the normal note/derived-index transaction and adds a durable capture receipt
in that same commit. The CLI prints a request UUID before dispatch and reports
“Saved note” only after receiving a committed result. `--request-id UUID` permits
safe retries with the original text; another payload using that ID is rejected.
`--capture-status UUID` is read-only and neither creates nor migrates a library.
A missing receipt is an observation at that snapshot, not proof that an in-flight
request cannot still finish. Receipts survive note edits/deletion and full backup/
restore; repeating a request cannot overwrite or resurrect its original note.

The new framework-free `notes-platform` crate provides versioned, framed Unix IPC,
private runtime directories, same-user peer checks, canonical library identity,
bounded input/writer queues, deadlines, explicit unknown outcomes and inode-safe
cleanup. A kernel poll with a shutdown wakeup avoids an idle polling timer. The
compatible Qt lock remains authoritative. Rust never forwards to Qt's receipt-
only socket, deletes a responding endpoint or bypasses the other edition's lock.

Committed capture events update the note list without replacing the current
editor session. The optional receipt table is additive metadata; ordinary
libraries remain database schema 2/document schema 1. Quotes still use their
explicit backed-up upgrade. Backups preserve receipts with the rest of SQLite.
Usage and protocol contracts are recorded in ADR 0012 and the preview guide.

### Evidence

- **Observed:** 106 workspace tests pass, with three ignored subprocess helpers
  explicitly invoked by their parent kill tests. Five capture storage tests cover
  atomic receipts, payload mismatch, retries after edits/deletion, invalid receipts,
  input/write failures, read-only lookup and backup/restore. Two killed-child
  boundaries cover interruption before commit and after commit/before reply.
- **Observed:** six local protocol tests cover durable results, a client disappearing
  before its reply, malformed/future/oversized frames, saturated/disconnected writer
  queues, receipt-only and mismatched replies, private runtime directories,
  unrelated/live/stale endpoints and cleanup. Tests require local Unix-socket
  permission; sandbox socket denials were not counted as successful checks.
- **Observed:** the real release CLI passes the display-free capture oracle:
  direct and forwarded capture, stdin, canonical aliases, two concurrent direct
  launches and eight parallel forwarded requests, durable status, identical retry,
  different-text rejection, lost replies, restarted/killed owners, failure mode,
  stale socket recovery and receipt-preserving backup/restore.
- **Observed:** actual Qt readers still accept ordinary captured libraries,
  derived text and unchanged blobs. Cross-edition backup/restore preserves the
  receipt table. Quote-upgrade rejection and schema-2 rollback checks also pass.
  The compatibility helper was rebuilt after an initial missing-binary invocation;
  only the completed oracle is evidence.
- **Observed:** all-target/all-feature Clippy with warnings denied, formatting,
  release builds, Python syntax and diff checks pass. CI runs the display-free
  capture oracle and the existing Qt compatibility/backup checks. Native test
  cleanup now removes only its exited owner's exact socket inode.
- **Observed:** the final native Wayland/AT-SPI run starts capture with a newer,
  unsaved draft, observes the captured note in the list, verifies unchanged editor
  text, exact caret and focus, retries without duplication, then confirms the
  original draft is saved intact. The complete editor/search regression passes
  in that run. Earlier fast synthetic runs failed at table undo/link persistence;
  the harness now awaits returned focus and each character's input acknowledgement.
  A further draft assertion was corrected to compare actual structured text,
  including empty table cells, rather than the abbreviated search text. Those
  failed runs are not counted as passes. All synthetic inputs remained guarded
  before/after by the owned process, address, app ID and active workspace.
- **Unverified:** real IME/Orca and the remaining native platform/state/performance
  matrix. Native AT-SPI cache-signature warnings remain. These checks do not prove
  screen-reader usability, physically full disks, power loss or production-scale
  capture throughput.

### Completion audit after durable capture

This is **verified progress**, not completion of the Rust port. Durable capture
and its retry protocol advance M0/M3/M5, but the next work is still the actual
Quick Note window, shared main/Quick Note sessions, default/new/note launch
forwarding and activation-token behavior. Capture forwarding does not imply those
window behaviors are implemented.

The full objective is unchanged. Outstanding work includes the remaining legacy
shortcut/desktop inventory; durable pane/theme/font/text preferences and visual
state signoff; real IME/Orca, heading/table/quote semantics, RTL, wide/leafless/large
tables, document virtualization/performance and native clipboard/portal matrices;
Quick Note and the remaining launch/recovery acceptance; folders/nesting/moves,
tags/Smart Folders, date groups, FTS search, gallery/attachment browser, trash/undo,
complete file interchange, general in-app backup/restore and Settings; full fault,
downgrade, platform, performance, memory, user-study and five-day-trial gates;
and license/runtime audits, packaging, installed launch/capture, public identity,
CLI/installer cutover, release artifacts and rollback instructions. Qt remains
the supported installed edition. Beauty and delight remain mandatory throughout.

## 13 October 2026 — Shared Quick Note windows

### Implemented

- A compact native Quick Note window with the same rich editor, semantic themes,
  quiet empty state, durable save status and visible recovery actions. Initial
  `--quick-note` opens only this window; Ctrl Shift N opens it from the notebook.
- One library controller and writer serve both windows. Each note still has one
  editing entity. Ctrl Shift O / Open in Notes preserves its document, selection
  and undo/redo, rebinding find/link callbacks and replacing obsolete geometry.
  Pending insertion or composition prevents a move. Main selection never renders
  a Quick Note editor simultaneously in another window.
- Quick drafts keep stable identities without writing an empty database note.
  First save is an INSERT, subsequent saves remain revision guarded, and stale
  revision-zero drafts cannot overwrite existing notes. First-attachment imports
  queue the parent save before import on the same writer. An untouched close
  discards only the in-memory draft; deliberate Open in Notes keeps it.
- Application-scoped polling survives main-window closure. Quick Note can keep
  saving, then reopen the main window. Error text is bounded at compact sizes,
  actions remain reachable and save feedback reveals the writing caret.
- Per-session media/link events route to their actual owner window. Main search
  subscriptions rebind on reopen, and silent programmatic search resets prevent
  a queued search event from hiding the just-opened note in a narrow window.
- README, CLI help, the preview guide and
  [ADR 0013](../docs/decisions/0013-shared-quick-note-sessions.md) describe the
  implemented behavior and the remaining launch-forwarding boundary.

### Evidence

- **Observed:** 107 workspace tests pass, with the same three ignored subprocess
  helpers invoked explicitly by their parent kill tests. The added storage test
  covers failed first creation, stable identity, pinned metadata, stale draft
  rejection and reopen. All-target/all-feature Clippy with warnings denied and
  formatting pass.
- **Observed:** the GPU Quick Note workflow passes Quick-only launch, untouched
  discard, a newer edit during first-save acknowledgement, exact selection/entity
  handoff, undo/redo, composition/insertion guards, find rebinding, attachment-only
  creation, main close/reopen and retained drafts after simulated save failure.
  Caret geometry is checked above the footer after compact save-error feedback.
  Real-font screenshots were inspected at 680×660 and 420×420 in dark/light,
  empty, writing and failure states; artifacts are `docs/images/gpui-quick-*.png`.
- **Observed:** the existing GPU writing/recovery check and sample-shell search,
  focus and narrow-layout regression pass. The new native Wayland/AT-SPI oracle
  passes first save, exact caret/undo handoff, typing after handoff, rebound find,
  untouched close, continued writing after main close and main recreation.
- **Observed:** the existing complete native editor/search regression passes,
  including table actions, formatting, selection tools, links and related notes,
  quote upgrade with verified backup, and capture without changing the current
  draft, caret or focus. Native input stays guarded to owned test windows.
- **Failed, then corrected:** the first shortcut choice conflicted with existing
  attachment opening; Open in Notes now uses Ctrl Shift O. A headless reopen
  check needed an effect cycle after releasing its explicit controller handle.
  The native test's old compositor syntax was replaced with the documented Lua
  dispatcher. Initial handoff input assertions read a stale closed-window AT-SPI
  node; diagnostics confirmed the active editor received input, and the oracle
  now scopes reads to the active window frame. Those failed runs are not passes.
- **Unverified:** real IME, Orca and the broader input/platform/performance matrix.
  The installed AT-SPI client still reports AccessKit cache-signature warnings
  and retains closed-window nodes. Native API checks do not establish usable
  screen-reader behavior across a window handoff. No compositor focus override
  was added to the application; the test only focuses its verified owned windows.

### Completion audit after Quick Note

This is verified M3 progress, **not completion of the Rust port**. The next work
is default/new/note/quick-note launch forwarding through the single owner and
activation-token behavior. Initial Quick Note startup and in-app opening work;
a second CLI window launch still reports the owner lock. The public Qt app and
installer remain unchanged.

The full objective is unchanged: finish the legacy desktop/shortcut inventory;
persist pane/theme/font/text preferences and finish visual-state signoff; verify
real IME/Orca, heading/table/quote semantics, RTL, wide/leafless/large tables,
document virtualization/performance and native clipboard/portal matrices; finish
launch/recovery acceptance; implement folders/nesting/moves, tags/Smart Folders,
date groups, FTS search, gallery/attachments, trash/undo, complete file interchange,
general in-app backup/restore and Settings; complete the fault, downgrade,
platform, performance, memory, user-study and five-day-trial gates; and finish
license/runtime audits, packaging, installed launch/capture, public identity,
CLI/installer cutover, release artifacts and rollback instructions. Beauty and
delight remain requirements for each increment.

## 13 October 2026 — Window launch forwarding

### Implemented

- Default, `--new`, `--note UUID` and `--quick-note` invocations reach the running
  canonical Rust library owner before initializing a second GUI. If there is no
  owner, startup acquires the compatible library lock, starts the writer and
  binds the endpoint before opening the requested native window. Display-free
  clients work when the owner is running. Qt's endpoint remains separate.
- UI requests are bounded, serialized and expire before dispatch. Recent identical
  protocol IDs replay their outcome without duplicating notes; different payloads
  are rejected. New-note replies wait for the matching committed storage event.
  Opened is distinct from durable capture Committed and never promises desktop
  focus. An ambiguous send never falls back to another writer or an automatic
  second New request. Recent replay is owner-local, not a durable retry contract.
- Default activation preserves the current note and caret; unknown IDs preserve
  the draft. Quick requests reuse one window; opening its note ID in main keeps
  the existing editor and undo. Composition/pending handoff remains guarded.
  Main can reopen while Quick retains the library controller.
- Forwarded Wayland tokens use a nonblocking native adapter on GPUI's borrowed
  display, with separate event dispatch, a two-second bound and explicit teardown
  before native target removal/application shutdown. Tokens target the intended
  wl_surface; process environment and desktop configuration are unchanged. The
  adapter never reads or roundtrips on the UI thread, owns no host surface/display,
  and falls back to an activation-unavailable report rather than forcing focus.
  New forwarded windows defer activation until the requested action succeeds.
- README, CLI help, preview guide, milestone plan and
  [ADR 0014](../docs/decisions/0014-window-launch-forwarding.md) explain behavior,
  replay limits, native handle lifetime assumptions and pending acceptance work.

### Evidence

- **Observed:** 113 workspace tests pass; three ignored subprocess helpers are
  still invoked by their parent kill tests. New core/protocol tests cover bounded
  replay, mismatched request payloads/tokens, token validation/redaction, queue
  exhaustion, capture-only owners, UI confirmation timing and rejection of
  mismatched Opened/Committed responses. All-target/all-feature Clippy with
  warnings denied, formatting and Python syntax checks pass.
- **Observed:** the real-font/GPU window workflow passes committed creation,
  failure, expiration, dropped-reply replay, draft/selection preservation, Quick
  reuse, guarded composition/handoff, main recreation and durable reopen. The
  existing Quick Note GPU regression also passes on the final tree.
- **Observed:** native release checks pass display-free default/new/note/Quick
  forwarding, canonical aliases, native typing/caret retention, concurrent Quick
  reuse, unknown targets, lost-reply replay, main close/reopen and Quick handoff.
  The CLI token environment path and direct protocol tokens each submit exactly
  once to their intended main/Quick surface. Invalid tokens do not steal focus
  from the existing Quick window. These are token-submission/refusal checks,
  not proof of valid-token focus grants.
- **Observed:** simultaneous initial `--new` processes commit two notes with one
  remaining GUI owner. Explicit-note startup adds no note; an unknown startup
  target fails; failed creation never reports Opened and leaves the owner
  responsive. The existing display-free capture/backup/restart regression passes,
  including rejection of window requests by a capture-only owner.
- **Failed, then corrected:** the first native harness used Quick's Ctrl W
  shortcut for main, which uses Ctrl Q. A later bulk-input run timed out waiting
  for the expected saved text; input now waits for focused accessible text and
  acknowledges each character. Another run stopped on a focus change before sending further input.
  Final guarded native runs passed; the failed runs are not counted as passes.
- **Unverified:** valid-token focus grants across launchers/desktops, X11,
  installed desktop actions, real IME, Orca, scaling and suspend/resume. The
  installed AT-SPI cache ABI warnings persist; native text assertions do not
  constitute screen-reader usability signoff. No frame-presentation latency or
  long-session memory claims are made by these checks.

### Completion audit after launch forwarding

This is verified M3 progress, **not completion of the Rust port**. The next
product work is persistent appearance/pane preferences and the remaining library
organization/settings workflows. Launch acceptance still needs valid-token and
installed-desktop coverage; recovery/input acceptance remains open. The supported
Qt release and installer remain unchanged.

The full objective still includes the legacy desktop/shortcut inventory; persisted
pane/theme/font/text preferences and visual-state signoff; real IME/Orca,
heading/table/quote semantics, RTL, wide/leafless/large tables, document
virtualization/performance and native clipboard/portal matrices; remaining launch
and recovery acceptance; folders/nesting/moves, tags/Smart Folders, date groups,
FTS search, gallery/attachments, trash/undo, complete file interchange, general
in-app backup/restore and Settings; full fault, downgrade, platform, performance,
memory, user-study and five-day-trial gates; and license/runtime audits,
packaging, installed launch/capture, public identity, CLI/installer cutover,
release artifacts and rollback instructions. Beauty and delight remain core
requirements for every increment.

## 13 October 2026 — Persistent writing preferences

### Implemented

- Settings now offers Graphite, Warm paper and live Omarchy palettes; serif,
  sans and mono writing fonts; writing size; and keyboard-adjustable pane widths.
  A live type sample, quiet save feedback and a reachable error/retry footer
  retain the design at desktop and 420 × 420 sizes. Tab/Shift+Tab scroll controls
  into view; Escape returns to the previous focus. Numeric groups expose their
  current values to accessibility clients. Removed Retry controls return focus
  to the Settings header so app shortcuts remain usable.
- Versioned XDG preferences remember appearance, writing font/size, pane widths,
  focus mode and the existing sort choice. The core retains newer edits during
  asynchronous saves; the worker merges changed fields under a bounded file lock
  and atomically replaces the file with syncs. Malformed/future files remain
  untouched, with retry after external repair. Different library owners preserve
  unrelated choices. Qt settings and demo sessions remain separate.
- CLI palette overrides stay temporary unless explicitly selected in Settings.
  Pane dragging writes once on release. Typography updates real layout and
  character coordinates without editing documents or their undo histories;
  Quick Note inherits the same preferences. Graceful close waits for in-flight
  preferences alongside existing note-save protections.
- Native verification exposed duplicate Enter activation: shared buttons acted
  on key down as well as GPUI's key-release click. Shared shell, formatting,
  selection, table, media and Quick Note buttons now use one click path. The GPU
  key helper includes release events, with explicit single-step Enter/Space checks.
- README, CLI help, preview guide, screenshots, milestone plan and
  [ADR 0015](../docs/decisions/0015-persistent-writing-preferences.md) describe
  the shipped increment and its limits.

### Evidence

- **Observed:** 118 workspace tests pass, with three ignored subprocess helpers
  exercised by parent kill tests. All-target/all-feature Clippy with warnings
  denied, formatting and Python syntax checks pass. The first sandboxed workspace
  run could not create protocol sockets; the final socket-enabled run passed.
- **Observed:** real-font/GPU Settings checks pass save/reopen, launch override
  isolation, rapid edits, exact single activation, real font metrics, retained
  document/selection/undo, Quick inheritance, drag-end persistence, remembered
  focus/sort, graceful close, keyboard-visible controls, future-file preservation
  and repaired-file retry. Dark/light/compact/error renderings were inspected.
- **Observed:** the existing writing, media, table, formatting/find, link,
  selection/related-note, quote, PDF, Quick Note, window-launch and visual GPU
  workflows pass with the shared key helper now emitting key-down/key-up sequences.
  Two visual harnesses needed
  to wait for their previous storage worker to release its disposable library
  before opening the next test window.
- **Observed:** the final owned-window native Settings run passes keyboard
  navigation, live character metrics, exact caret/undo/redo retention, durable
  note save, preference relaunch, selected palette/font and numeric values,
  future-file preservation, repair/merge and focus restoration before close.
  Native editor/accessibility and Quick Note regressions also pass. Every native
  check used a disposable library and XDG configuration; input remained guarded
  by exact process, app ID, address and workspace.
- **Failed, then corrected:** early native checks exposed double activation,
  missing accessible numeric labels and focus left on a removed Retry button.
  Harness corrections also opened the seeded note explicitly, used the Settings
  group's bounds instead of an unavailable platform-frame rectangle, scrolled
  before checking offscreen values and paced input. One run correctly stopped
  when the owned window lost focus. Only the final successful runs count as passes.
- **Unverified:** real IME/Orca, mixed-DPI, X11, suspend/resume, large-document
  typography/scroll performance and complete visual-state coverage. The installed
  AT-SPI cache ABI warnings persist; native text/control checks are not an Orca
  usability signoff. No presentation-latency or long-session memory claims.

### Completion audit after preferences

This completes the personal-preferences increment, **not M1, M4 or the Rust
port**. Full Settings still needs library-action routes. The next product work
is library organization and search. The supported Qt release/installer is unchanged.

Remaining scope includes legacy desktop/shortcut inventory and fault fixtures;
visual-state signoff; real IME/Orca, heading/table/quote semantics, RTL, wide/large
tables and new merges, document virtualization/performance and clipboard/portal
coverage; remaining launch/recovery acceptance; folders/nesting/moves, tags/Smart
Folders, date groups, FTS search, gallery/attachments, trash/undo, complete file
interchange, general in-app backup/restore and remaining Settings routes; full
fault, downgrade, platform, performance, memory, user-study and five-day-trial
gates; and license/runtime audits, packaging, installed launch/capture, public
identity, CLI/installer cutover, release artifacts and rollback instructions.
Beauty and delight remain requirements throughout the full objective.

## 13 October 2026 — Nested folders and careful filing

### Implemented

- Live navigation uses stable folder IDs and full paths, with nested disclosure
  controls and note counts. Same-named folders under different parents remain
  separate. New notes inherit their selected folder. Create/rename/move screens
  offer focused input, searchable destinations, keyboard navigation, retained
  input on errors and commit-gated feedback. Sidebar ancestors expand and the
  selected destination is revealed after a successful operation.
- The shared SQLite writer validates sibling names and hierarchy invariants in
  an immediate transaction. Folder identity and existing schemas are preserved.
  Filing only updates `folder_id`; stale content-save metadata cannot move a note
  back. The editor entity, draft, revision, caret and undo/redo survive filing.
- Main-window form events carry an epoch and operations carry unique IDs.
  Toolbar-initiated filing returns to the editor. Other forms restore stable
  entity-owned focus instead of removed toolbar handles. Composing name/search
  fields consume navigation without submitting or triggering parent shortcuts.
  Incoming window actions wait while the form is open, and main-window close
  clears form state even when Quick Note keeps the shared controller alive.
- Both folder lists use `uniform_list` with measured fixed-height rows. The
  destination set is prepared once per form; typing filters that set rather
  than rebuilding and revalidating every parent chain. Renames/reparenting retain
  an existing visible navigation anchor where possible.
- README, preview guide, plan, CI compatibility command, four screenshots and
  [ADR 0016](../docs/decisions/0016-folders-and-filing.md) describe this increment.
  The overview at the top of this record now reflects the later completed
  increments; dated evidence below it remains historical.

### Evidence

- **Observed:** 121 workspace tests pass; three ignored subprocess entry points
  are exercised by their parent durability tests. All-target/all-feature Clippy
  with warnings denied, formatting and Python syntax checks pass. Final logs:
  `target/organization-workspace-tests.log` and `target/organization-clippy.log`.
- **Observed:** the real-font/GPU organization workflow passes nested creation,
  same-name scope isolation, new notes in their folder, name conflict/retry,
  rename and parent moves, dirty-note filing, unchanged selection/revision/undo,
  stale-result rejection, composition protection and durable reopen. A separate
  in-memory 10,005-folder fixture renders fewer than 64 distinct rows at both
  the start and end of the tree and finds its final named collection. This is a
  rendering/reachability assertion, **not** a whole-library latency or memory
  benchmark. Dark/light/compact creation, filing, library and error renderings
  were inspected; published images are direct GPUI renders.
- **Observed:** all 13 GPU workflows pass: organization, window launch, Settings,
  writing, media, tables, formatting/find, links, selection/related notes,
  quotations, PDF, Quick Note and visual composition. Launch coverage now checks
  that a form defers a queued request and that closing it with the main window
  cannot block main-window reopening while Quick Note remains alive.
- **Observed:** the final owned-window native organization check passes nested
  creation, same-name folders, search/filing, exact caret retention, undo/redo,
  rename, reparent, recoverable sibling-name failure/correction and durable close.
  Native editor/accessibility and Quick Note regressions also pass. All use
  scratch libraries/configs and private accessibility sessions. Native folder
  opening uses scoped accessibility actions; Alt shortcuts are tested in the
  GPU harness so desktop bindings are not invoked by native automation.
- **Observed:** Rust → actual Qt `NoteStore` → Rust round trips preserve folder
  identity/parent paths, note rich content and revisions. Qt rejects duplicate
  sibling names, reads Rust note filing, then writes nested folders, a rename
  and a note move that Rust reopens. No schema upgrade is needed. CI is configured
  to run the oracle; hosted CI execution has not been observed.
- **Corrected during verification:** native checks exposed invalid return-focus
  handles after toolbar actions. GPU checks exposed composing-field propagation,
  closed-main form state and general-list offscreen reveal. The pinned general
  list discards height hints on first layout/width changes; the final fixed-row
  implementation passes the large-tree check. An initial pure large-tree test
  expected an index without accounting for the deliberately visible orphan;
  its expected ordering was corrected. One native regression safely stopped
  on loss of owned-window focus; its subsequent complete run passed.
- **Unverified:** real IME/Orca, full keyboard tree navigation, mixed-DPI/X11,
  suspend/resume, pathological hierarchy performance and whole-library latency/
  memory. AT-SPI cache-signature warnings remain. These checks certify neither
  a screen-reader usability pass nor the complete visual/platform matrix.

### Completion audit after folder filing

This completes the create/rename/reparent/filing increment, **not M4 or the full
Rust port**. Folder deletion/trash, drag-and-drop organization, persistent
expansion and complete keyboard tree navigation remain. Next library work also
includes tags/Smart Folders, date groups and remaining sort modes, SQLite FTS
queries, gallery/attachment browsing, duplicate/recovery actions, full file
interchange and in-app backup/restore/library Settings routes.

The earlier M0–M6 gates remain: complete contract/fault inventory, full visual
signoff and first-use study, real IME/Orca/RTL and accessible document structure,
wide/large tables and new merges, document virtualization/performance and undo
memory, clipboard/portal/launch matrices, recovery and fault/downgrade checks,
long-session and five-day daily-use trials, licensing/runtime audit, packaging,
installed actions/CLI identity, installer cutover, release artifacts and rollback.
The supported installer still installs Qt; no full-port completion is claimed.
