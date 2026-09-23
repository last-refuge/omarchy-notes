# Omarchy Notes — build plan

Planning draft · 23 September 2026

Build a native Linux notes application that brings Apple Notes’ fast capture, rich documents, and simple organization to Omarchy. The first release should be useful as someone’s everyday notebook, with a clear roadmap toward broader functional parity.

This plan proposes local-first storage and Apple Notes’ familiar layout adapted to Omarchy themes. These are provisional choices, not confirmed preferences. The application name is a working name. This deliverable is a plan; implementation has not started.

## 1. Product scope and reference baseline

Primary workflow: launch → capture a thought immediately → enrich it with lists or attachments → find it later. The interface should remain easy to use with a mouse while supporting complete keyboard operation.

Use Apple’s macOS Notes guide as the feature inventory. Freeze the exact macOS reference version during discovery, since the live guide changes. Track each feature as complete, partial, deferred, or platform-specific, backed by an acceptance scenario. Do not describe the first release as full parity.

| Capability | First public release | Subsequent work |
|---|---|---|
| Capture and editing | Instant new note; autosave; headings; bold, italic, underline, strikethrough; highlight; lists; checklists; undo/redo | Collapsible sections and more editing refinements |
| Structured content | Editable tables; links between notes; pasted links; inline images; file and PDF attachments | Rich link previews, PDF annotation, drawing |
| Organization | Nested folders; move/duplicate; pin; sorting; tags; Smart Folders; recoverable trash | More filters and account-specific organization |
| Browsing and retrieval | Note list; gallery; attachment browser; full-text search; matching snippets; in-note find | OCR and attachment-content search |
| Quick capture | Separate Quick Note window; desktop launch action; command-line capture | Browser capture and optional shell widget |
| Portability | Markdown/text import; Markdown/HTML/PDF export; full-library backup and restore | ENEX and richer import adapters |
| Privacy | Offline operation; no account required | Encrypted locked notes with security review |
| Media and intelligence | Open attached media through desktop handlers | Audio recording, searchable transcription, math expressions, optional writing assistance |
| Devices and teams | Local library | Device sync, then shared notes/folders, permissions, activity and mentions |

Apple-specific services need explicit equivalents: importing exported notes is the initial migration path; iCloud Notes interoperability is a separate feasibility question. Continuity Camera, Apple Intelligence, and Apple account behavior are not assumed available on Linux. Phone capture would require its own companion workflow or integration.

Reference: [Apple Notes feature inventory](https://support.apple.com/guide/notes/toc), [current Notes overview](https://support.apple.com/guide/notes/welcome/mac).

## 2. Visual and interaction direction

Proposed direction: a quiet writing surface inside an Omarchy-native frame. Keep the recognizable folder → note → document hierarchy and give the content most of the space.

### Main window

- Wide layout: resizable folder sidebar, note list, and editor. Start around 200 / 280 / remaining logical pixels, then validate with real content.
- Medium tiled window: collapse folders into a drawer and retain the note list beside the editor.
- Narrow tile: show one pane at a time with explicit back navigation, preserving selection, scroll position, and draft state.
- Editor: prominent title, subdued timestamp, readable body type, generous line spacing, and a comfortable line length. Let tables and media use more width where needed.
- Toolbar: new note, formatting, checklist, table, attachment, search, and overflow. Expose uncommon actions through menus without hiding essential actions behind hover.
- List rows: title, short excerpt, date, and meaningful attachment/checklist indicators. Support long titles, untitled notes, and duplicate titles.

### Visual rules

- Follow the active Omarchy palette through semantic roles: canvas, sidebar, divider, text, secondary text, selection, accent, focus and error.
- Keep the document surface opaque and readable. Use compositor effects only where they do not reduce content contrast.
- Use a proportional system UI font and adjustable note font; reserve monospace for monospaced content.
- Use restrained line icons, consistent spacing, thin separators and obvious selected/focused states. Avoid forcing Apple’s yellow accent onto every theme.
- Keep transitions short and purposeful. Honor reduced motion and avoid animation while typing.
- Use native dialogs, accessible control semantics, desktop context menus and ordinary Linux shortcuts.

### Required flows and states

Design and test first launch, an empty library, a populated library, search with and without results, large attachments, unavailable attachments, import progress/errors, failed saves, read-only storage, full disk, trash restoration and recovery after a crash. Later phases add locked/unlocked, offline/syncing/conflict, and shared/read-only states.

First launch opens directly into a usable notebook. New note focuses the editor immediately. Quick Note uses the same storage and editing engine as the main window. A save failure remains visible and preserves the draft in memory while offering retry and export.

Design deliverables before full implementation: annotated main-window mockups in a light and dark Omarchy theme, narrow-tile behavior, a mixed-content note, Quick Note, and the failure states above. Use realistic meeting notes, shopping checklists, research links, images and a PDF rather than placeholder-only layouts.

## 3. Recommended implementation

**Qt 6 + Qt Quick/QML + C++20 + CMake**, subject to an editor feasibility gate. Build a standalone native Wayland application. The installed Omarchy 4.0.0.alpha shell uses Qt Quick; that is useful local alignment, not a requirement that every Omarchy application use Qt.

Qt provides native Wayland client support and a rich-text document model. Qt Quick TextEdit exposes editing primitives, but the application must supply substantial document behavior. Tables, custom attachments, selection, input methods and accessibility need a prototype rather than an assumption that a text control provides Notes parity. See [Qt on Wayland](https://doc.qt.io/qt-6/wayland-and-qt.html), [Qt rich-text structure](https://doc.qt.io/qt-6/richtext-structure.html), and [Qt Quick TextEdit](https://doc.qt.io/qt-6/qml-qtquick-textedit.html).

| Layer | Responsibility |
|---|---|
| QML views and controls | Window layouts, lists, menus, dialogs, theme tokens and accessibility |
| Document controller | Editing commands, selection, undo, tables, lists, links and attachment anchors |
| Library services | Notes, folders, tags, saved queries, trash and import/export |
| Persistence | SQLite transactions, migrations, revisions, recovery and blob storage |
| Background workers | Search indexing, thumbnails, exports; later OCR and transcription |
| Desktop adapter | Launch actions, file handling, activation, Quick Note and theme changes |

Keep the UI thread free of file decoding, indexing and large imports. Use a single library-owning process with explicit activation/IPC so multiple windows do not independently overwrite notes.

### Editor feasibility gate

Timebox a prototype before committing to the shell architecture. Demonstrate one document with headings, nested checklists, a table, inline image, file attachment and a note link. Verify cross-element selection, keyboard navigation, clipboard conversion, Unicode/IME composition, undo grouping, accessibility and save/reload fidelity.

Evaluate a QML editor backed by an owned QTextDocument first. If it fails the agreed interactions, evaluate a Qt Widgets application around QTextEdit before committing to the UI framework. This fallback should remain a coherent native application rather than an improvised mix of incompatible editing surfaces. Record the chosen minimum Qt version and supported APIs.

## 4. Data and reliability

- Use SQLite for notes, folders, tags, associations, saved queries, attachment metadata, revisions and schema versions. Use FTS5 for local text search with a rebuildable index. [SQLite FTS5 documentation](https://sqlite.org/fts5.html)
- Persist an explicit, versioned rich-document schema with stable identifiers for notes and structural elements. Prove its mapping to and from the editor in the prototype. Markdown is an interchange format; it should not silently discard richer document features during normal saving.
- Store attachment bytes in an app-managed blob directory. Stage and durably install files before committing references; reconcile abandoned staged files and unreferenced blobs after interruption.
- Use transactions for note updates and bounded autosave coalescing. Show “Saved” only after the persistence layer acknowledges a successful commit. Specify and test the maximum unsaved interval rather than promising zero loss from an uncommitted keystroke.
- Keep revision history and recoverable deletion. Back up through SQLite’s consistent snapshot facilities together with the referenced attachment set; test restore into a clean profile.
- Keep user data under XDG data directories, settings under XDG config directories, and disposable thumbnails/index caches in the appropriate cache location. Uninstallation must preserve user notes.
- Persist stable identities and revision metadata from the start. If live collaboration is mandatory, choose the collaboration-compatible document model before freezing the storage schema; a local change log alone does not make an editor collaborative.

For encrypted notes, protect content, attachments, revisions and previews together. Exclude plaintext from persistent search indexes and caches, define whether titles remain visible, and specify key storage, locking and recovery behavior. Use maintained cryptographic libraries and a dedicated review. A hidden editor or password dialog is not encryption.

## 5. Omarchy integration

Ship an Arch package with desktop entry, icon, application ID, MIME declarations and AppStream metadata. Provide new-note and Quick Note desktop actions, plus a small CLI for capture and opening a note by stable ID.

On this installation, Omarchy reads active theme colors from `~/.local/state/omarchy/current/theme/colors.toml`. Implement a version-aware adapter that watches theme-directory replacement, reloads semantic colors, checks contrast and falls back safely when a palette is missing or invalid. Detect other supported Omarchy versions instead of assuming this alpha installation represents every release.

Provide an optional, conflict-checked Hyprland shortcut invoking Quick Note. Let Hyprland manage placement and floating rules. Keep the app independent of private shell components; optional shell integration can follow later. Do not require modifications to packaged Omarchy files.

Test native Wayland operation, tiled and floating windows, fractional scaling, mixed-DPI monitors, drag-and-drop, clipboard formats, file dialogs, focus restoration and compositor-mediated window activation.

## 6. Delivery milestones

The following are rough effort estimates for an experienced native-app engineer with design support. They are planning ranges, not calendar commitments. Re-estimate after the editor prototype.

| Milestone | Deliverable and exit condition | Estimated effort |
|---|---|---|
| 0. Scope and design | Versioned parity checklist, priority workflows, realistic sample library, annotated visual direction | 1–2 engineer-weeks |
| 1. Editor and storage proof | Mixed-content editing round-trips without loss; undo, IME, accessibility and crash-recovery tests pass; toolkit decision recorded | 2–3 |
| 2. Daily-use alpha | Capture, edit, save, reopen, folders, pinning, search, trash and backup/restore work end to end | 3–4 |
| 3. Complete local beta | Tables, attachments, tags, Smart Folders, gallery, note links, import/export and Quick Note integrated | 4–6 |
| 4. Release quality | Omarchy packaging, theme integration, accessibility, performance and recovery gates pass; migration documentation complete | 2–3 |

Allow approximately **12–18 engineer-weeks for the proposed local v1**, plus roughly 20–30% contingency for editor and import fidelity problems. Advanced parity features are outside that estimate. Two engineers can parallelize independent storage and UI work after the document contract is stable, but the editor gate still sets the critical path.

### Expansion milestones

1. **Private and searchable media:** encrypted notes, PDF text extraction/OCR, annotation and drawing; each receives its own acceptance and resource-budget tests.
2. **Audio and math:** recording, offline transcription with explicit model download, searchable transcripts, and a constrained math parser. Keep expensive processing cancellable and off the UI thread.
3. **Device sync:** select supported device platforms and a transport/server; add authentication, attachment transfer, offline edits, deletion propagation, conflict recovery and encryption design. Never synchronize a live SQLite file as the protocol. An iPhone client or web companion is additional product scope.
4. **Collaboration:** prove a CRDT or equivalent merge model against rich documents, then add permissions, invitations, concurrent editing, activity and mentions. Test revocation and offline behavior before launch.
5. **Ecosystem conveniences:** browser capture, optional shell widget, richer import adapters and optional writing assistance through a replaceable provider interface.

## 7. Release gates

**Editing:** round-trip a fixture library covering nested lists, check states, tables, note links, images, attachment metadata, emoji, RTL text, IME input and long documents. Check import/export loss explicitly and report unsupported content instead of discarding it.

**Reliability:** inject failures during save, attachment import, schema migration and backup. Kill and reopen the app at persistence boundaries. Recover every acknowledged save and validate library/attachment consistency. Test full disk and read-only storage.

**Accessibility:** complete capture, edit, organize, search and export using the keyboard. Verify screen-reader names and structure on Linux, visible focus, enlarged text and reduced motion. Test semantic colors against readable contrast targets across representative themes.

**Performance:** establish a documented reference machine and a 10,000-note/1 GB attachment fixture. Initial targets: cold launch under 1.5 seconds, warm launch under 500 ms, search p95 under 150 ms excluding typing debounce, and no UI-thread stalls above 50 ms during ordinary editing. Benchmark these targets; they are not claims about unbuilt software.

**Desktop fit:** verify wide, narrow and floating layouts; 100%, 150% and 200% scaling; light and dark themes; live theme switches; and clean package install, upgrade and removal.

## 8. Decisions to settle at kickoff

- Whether v1 must include sync, encrypted locking, or collaboration. Each changes scope materially; collaboration also changes early document-model choices.
- Whether the visual target is a close Apple recreation or a familiar Notes layout interpreted through Omarchy. The latter is this plan’s recommendation.
- Which Apple/macOS version defines parity, which Omarchy releases must be supported, and whether migration of an existing library is required at launch.
- Whether the project prioritizes fastest personal delivery or a maintained public application; packaging, support and compatibility commitments depend on that answer.

The first implementation milestone should produce a native window that edits a realistic mixed-content note, saves it safely, restores it after a forced restart, and follows an Omarchy theme change. That validates the highest-risk foundations before broad feature work.
