# 0002 — A separate, disposable GPUI preview

Status: accepted for the first rebuild increment, 12 October 2026.
Storage and writing restrictions are superseded by [ADR 0003](0003-compatible-rich-writing.md);
this document records the original foundation decision.

The Rust workspace lives alongside CMake and the Qt sources. Its only executable
is `omarchy-notes-preview`, with application ID `org.omarchy.Notes.Preview`.
It opens an embedded sample fixture and has no storage adapter, data-directory
argument, migration code or production-library access. Session changes are
explicitly labeled and disappear on exit. The installed Qt app and installer
continue to provide the supported note-taking experience.

## Dependency boundary

Both `gpui` and `gpui_platform` are pinned to Zed commit
`a84689073d296dfd39987bc7dd478e43ef76d83a`. That cached source was inspected and
compiled locally. Rust is pinned to the installed 1.99.0 toolchain; upstream's
source manifest asks for 1.98.1. This newer compiler is an explicit tested choice,
not an automatic tracking of stable. Commit the resolved `Cargo.lock`.

Linux Wayland and X11 features are explicit on both crates. Native verification
in this increment uses Wayland; X11 compilation is not evidence of runtime
verification. Vulkan initialization and the selected GPU are visible with
`RUST_LOG=gpui_linux=info,gpui_wgpu=info`.

Only Apache-2.0 Zed crates appear in the resolved dependency inventory. No Zed
`editor`, `ui` or `markdown` crate is linked. A manifest-level license inventory
found no exclusively strong-copyleft dependency; the full release license and
notice audit remains a distribution gate. Bundled fonts use OFL 1.1, verified
from their embedded metadata, and carry the upstream license/copyright. Icons
are repository assets and simple original additions. See the asset notices.

The upstream workspace patches are not inherited. This preview uses resolved
crates.io `calloop` 0.14.5 and `async-task`, not the Zed forks. Its background
palette polling is short and bounded. Before introducing sustained storage,
conversion or indexing work, compare event-loop behavior with the upstream
calloop patch under load and record the decision. Do not claim production
responsiveness from this small sample shell.

## Architecture and current limits

- `notes-core`: GPUI-free disposable library/navigation state and Unicode-aware
  single-line input model, with focused behavior tests.
- `notes-ui`: semantic colors, bundled assets, GPUI search adapter and shell.
  The optional `visual-tests` feature enables real-font GPU snapshots and routed
  input checks without screen capture or global input injection.
- `omarchy-notes-preview`: explicit preview CLI and application entry point.

The sample block enum is deliberately **not** RichDocument v2. The quote in the
reference preview makes no database change. The migration/schema ADR must be
settled before implementing a production storage writer.

The preview supports folder filters, Favorites backed by pins, sample-text
search, title/date ordering, checklist toggles, explicit related-note navigation,
focus mode, resizable panes and narrow layouts. It has no New Note/editing
button pretending to work. Creation, rich editing, persistence, attachments,
trash and import/export remain on the parity checklist.

Omarchy mode polls supported `colors.toml` paths on a background task once per
second and updates the UI only when colors change. This handles directory and
symlink replacement without writing desktop settings. The legacy
`alacritty.toml` fallback, robust general TOML parsing and preference persistence
remain to be ported. Reference and light appearances are selectable explicitly.

## Next gate

Implement the legacy document reader and pure editing transactions against the
captured Qt fixtures, then connect a real editor to a compatible SQLite adapter.
The rich-editor feasibility requirements in the rebuild plan remain mandatory.
Qt replacement, data migration and installer cutover are not part of this increment.
