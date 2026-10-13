# Rust edition: release identity and compatibility contract

Discovery recorded 13 October 2026 for R0 of the
[Rust v1 release plan](../llm-docs/rust-v1-release-plan.md).
This is the release contract and implementation map for Rust **1.0.0**.
The owner directed release of the implemented build on 13 October 2026.
The evidence ledger records completed checks and remaining follow-up;
the study and five-day trial also follow after release.
R4 source now provides the separate Rust executable/package, compatibility
launcher and metadata, candidate preparation, edition-filtered installer and
CI routes. Focused identity/installer checks pass; clean package builds,
installed/native matrices and the complete license audit remain acceptance work.

## Separate editions

| Identity | Supported Qt edition | Planned stable Rust edition |
|---|---|---|
| Arch package / executable | `omarchy-notes` | `omarchy-notes-rust` |
| Desktop / AppStream ID | `org.omarchy.Notes` | `org.omarchy.Notes.Rust` |
| Release tag | `v1.0.0` | `rust-v1.0.0` |
| Default library | `$XDG_DATA_HOME/omarchy-notes` | `$XDG_DATA_HOME/omarchy-notes-gpui` |
| Preferences | `$XDG_CONFIG_HOME/omarchy-notes/settings.ini` | `$XDG_CONFIG_HOME/omarchy-notes-gpui/preferences.json` |
| Initial advertised platform | Existing Qt contract | x86_64 Arch/Omarchy, Wayland/Hyprland |

The XDG fallbacks are `~/.local/share` and `~/.config`. Stable Rust retains the
preview library and preference locations. The Rust instance protocol already
uses a private `$XDG_RUNTIME_DIR/omarchy-notes-rust` directory; retaining that
protocol and its library-specific identity prevents a preview/stable process
from independently writing the same library.

Keep the Qt installer as the default. Select Rust explicitly with an edition
option and the Rust tag; do not infer the edition from version `1.0.0`. Rust
must not provide, conflict with, overwrite or silently replace the Qt package.
Desktop names and icons must allow a person to distinguish both editions.

For the preview executable transition, install an `omarchy-notes-preview`
compatibility launcher that executes the stable Rust binary with the same
arguments and exit status. It should report the Rust executable and version,
preserve the preview data/preferences locations, and never invoke Qt. Test both
direct and compatibility launch paths, including capture without a display.
An older manually installed preview earlier on `PATH` requires a clear warning
and instructions to move that specific old executable aside.

Availability checks on 13 October 2026:

- AUR's public RPC for `omarchy-notes-rust` returned `resultcount: 0`. This is
  an absence check, not reservation of the name. [AUR lookup](https://aur.archlinux.org/rpc/v5/info?arg%5B%5D=omarchy-notes-rust).
- `git ls-remote --tags origin rust-v1.0.0 rust-v1.0.0-rc.1` returned no refs.
  Local Git contains the existing Qt `v1.0.0` tag. Recheck before publishing.
- `org.omarchy.Notes.Rust` is distinct from the Qt and preview IDs in this
  repository. Desktop IDs have no central reservation mechanism; this audit
  does not claim a global registry check.

## Feature boundary

The full port remains larger than the first stable Rust release. The following
comparison is a target scope statement; the release evidence ledger determines
whether each Rust requirement is complete.

| Workflow | Qt | Rust v1 requirement / limitation |
|---|---|---|
| Rich writing, lists, checks, links, images/files | Available | Required |
| Basic tables and existing merged tables | Available | Basic editing required; preserve and display existing merges; new merges deferred |
| Autosave, conflict recovery, undo/redo | Available | Required |
| Quick Note and installed launch actions | Available | Required |
| Folders, pins, duplication, sorting | Available | Required, including full keyboard tree navigation |
| Search | FTS prefix search | FTS prefix search/snippets, scope and stable keyboard selection required |
| Delete, undo, Recently Deleted, restore | Available | Required, including 30-day retention |
| Markdown/text import | Available | App file/folder routes required |
| Markdown/HTML/PDF export | Available | App per-note routes and library Markdown/assets export required |
| Exact backup, inspect, restore | App routes; restore sets the old library aside | App routes required; restore into a new verified library |
| Palettes, fonts, writing size, pane choices | Available | Existing Rust preferences required |
| Tags and Smart Folders | Available | UI deferred; preserve records and references |
| Gallery, library-wide attachment browser | Available | Deferred |
| Browsable revision history | Stored revisions; no browsable UI found | UI deferred; preserve revisions and referenced media |
| Drag filing, bulk organization, date groups | No corresponding shipped controls found | Deferred |

Qt libraries are imported as copies with verified backups. Never select or
upgrade the original Qt library automatically. Schema-3 quotation libraries
require Rust; rollback to Qt uses the pre-upgrade backup and excludes later
Rust edits. Offer backup and portable export before rollback. Portable exports
may be lossy and must not be described as lossless backups.

## Command routes

These are the advertised Qt application options in `README.md`, its man page
and `src/app/main.cpp`, compared with `crates/omarchy-notes/src/main.rs`.

| Advertised Qt option | Rust preview route | Stable Rust decision |
|---|---|---|
| `--help`, `--version` | Source builds provide both executable names; the installed alias reports Rust identity | Match Rust workspace/package version; no stable claim while version is 0.x |
| `--data-dir PATH` | Opens any compatible directory supplied by the caller | Retain; guide Qt imports to copies and verified backups |
| `--new` | Main-window launch/forwarding | Retain |
| `--note UUID` | Specific-note launch/forwarding | Retain |
| `--quick-note` | Quick-window launch/forwarding | Retain |
| `--capture TEXT`, stdin `-` | Durable capture/receipt | Retain; verify without display |
| `--setup-hyprland` | Absent | Implement edition-specific setup or an explicit supported installation route before advertising it |
| `--samples` | Absent; `--demo` opens disposable samples | Document distinct behavior; do not silently populate a real library |
| `--simulate-save-failure` | Implemented; developer fixture | Retain as testing option; no success claims on failed writes |

Rust additionally implements `--request-id UUID` for safe capture retries,
`--capture-status UUID` for read-only receipt checks, `--backup PATH`,
`--inspect-backup PATH`, `--restore PATH --data-dir NEW`, `--reference`,
`--light` and `--demo`. These need Rust-specific help/man documentation.
CLI backup requires closing the writer. Settings → Data provides in-app import,
export, backup, inspection, restore and library opening. In-app backup waits for
pending edits and declares its committed revision boundary. No import/export
CLI is advertised by either edition.

## Keyboard contracts

| Advertised Qt action | Qt shortcut | Current Rust route |
|---|---|---|
| New note | Ctrl+N | Same |
| New folder | Ctrl+Shift+N | **Ctrl+Alt+N**; Ctrl+Shift+N opens Rust Quick Note |
| Library search | Ctrl+Shift+F | Same; Ctrl+K also focuses search |
| Find in note, next | Ctrl+F, F3 / Enter | Same |
| Delete selected note | Delete in note list | Keyboard-accessible Note actions → Delete; editor Delete remains text editing |
| Duplicate | Ctrl+D | Keyboard-accessible Note actions → Duplicate; Qt's Ctrl+D shortcut is not assigned |
| Show/hide folders | Ctrl+Shift+S | Absent; do not advertise until implemented |
| Bold / italic / underline | Ctrl+B / I / U | Same |
| Strikethrough / highlight / monospace | Ctrl+Shift+X / Y, Ctrl+E | Same |
| Title / heading / subheading / body | Ctrl+Shift+T / H / J / B | Same; Ctrl+Alt+1 / 2 / 3 / 0 are Rust aliases |
| Checklist / tick | Ctrl+Shift+L / U | Same; Ctrl+Shift+C is a Rust checklist alias |
| Bullet / numbered list | Ctrl+Shift+8 / 7 | Same, including shifted-symbol aliases |
| List indent / outdent | Tab / Shift+Tab | Same; Ctrl+] / [ also available |
| Insert table | Ctrl+Alt+T | Rust opens basic table tools; document that behavior |
| Attach / link note | Ctrl+Shift+A / Ctrl+L | Same |
| Open attachment / object | Double click / Ctrl+Enter | Ctrl+Enter implemented; verify installed cross-app opening |
| Follow link / tag | Ctrl+click | Links require native evidence; tags UI deferred |
| Developer save-failure toggle | Ctrl+Alt+Shift+F | No Rust runtime toggle; CLI fixture only |

The New Folder / Quick Note collision is deliberate for the separate edition,
not a transparent Qt shortcut migration. Stable documentation and desktop
actions must expose the Rust keys. Additional Rust keys include Ctrl+Shift+O
for Quick-to-main handoff, Ctrl+, for Settings, Ctrl+Backslash for focus mode,
Ctrl+S for Save and Ctrl+Shift+P for PDF. Full folder and recovery keyboard
contracts are R2 work and need focused-context tests so editor keys are not
captured by the tree.

## Packaging implementation map

| Files / route | Required change and verification |
|---|---|
| Workspace and `crates/omarchy-notes/Cargo.toml` | Stable executable/package identity; preserve compatibility launcher; align version only at final freeze |
| `notes-ui/src/lib.rs`, `quick_note.rs` | Use stable desktop ID for all windows; keep IDs distinct from Qt; update native tests that locate the launched test window |
| New Rust desktop, AppStream, man and icon metadata | Explicit Rust names, new/quick actions, stable app ID, help/version and target feature scope; validate desktop/AppStream syntax |
| New `packaging/arch/PKGBUILD-rust.in` | x86_64 only; pinned locked Cargo build; install app, alias, metadata, icon and all notices; no dependency on cached checkout |
| `packaging/prepare-release.py` | Explicit edition selection; version/tag checks per edition; immutable archive names and recipes; preserve Qt route |
| `install.sh` | Explicit opt-in edition; tag/asset/package/executable allowlists; separate dependency sets and default data paths; keep strict package identity checks |
| Installer latest route | GitHub `/releases/latest` can select either edition; select latest published release of the requested edition explicitly and test mixed ordering |
| `tests/test-release-metadata.sh`, package/installer tests | Preserve Qt checks; add Rust identity, cross-edition rejection, compatibility launch, capture and coexistence checks |
| `.github/workflows/ci.yml`, `release.yml` | Preserve Qt CI; add clean Rust packaging and `rust-v*` tag route; checksums/source hash and draft release from exact committed source |
| Rust release/build/rollback docs | Separate source build and installer commands, feature limits and actual screenshots; no untested X11/aarch64 claims |

The installer now selects package, executable, tag and asset patterns from a
closed Qt/Rust allowlist; it rejects cross-edition names. Latest discovery filters
published release tags by edition. Rust Quick Note uses its desktop action or
the documented manual distinct-key route; automatic Qt setup is never invoked
for Rust. See [Rust release instructions](rust-release.md).

## Runtime and license discovery

The existing local release binary reports `omarchy-notes-preview 0.1.0`, is
285,223,488 bytes before stripping, and has SHA-256
`295984b9066a78faf1ff84657f89324aabddd3fc06a971f9198f77eb36258ed0`.
It is a discovery artifact, not a reproducible release candidate. `readelf -d`
found no RPATH/RUNPATH and direct dependencies owned by Arch packages `glibc`,
`libgcc`, `glib2`, `libxcb`, `libxkbcommon`, `libxkbcommon-x11`, `pango` and
`cairo`; `ldd` resolved them on this developer machine. Wayland is loaded via
`dlopen` in pinned GPUI, so `ldd` alone is insufficient. Include Wayland,
Vulkan loader/driver, portal and attachment-launch requirements after testing
the candidate in a clean environment. Building with X11 support does not mean
X11 is advertised as tested.

Toolchain discovery: rustc 1.99.0, x86_64; GPUI/platform pin
`a84689073d296dfd39987bc7dd478e43ef76d83a`. The current workspace declares both
Wayland and X11 features. Re-evaluate dependency declarations from the final
binary rather than this preliminary list.

`cargo metadata --locked --offline` found 769 packages across all targets and
build/dev dependencies, with no missing Cargo license field. This is an
inventory starting point, not a redistribution-license audit. Optional
copyleft alternatives (`r-efi`, `self_cell`) also offer permissive choices;
MPL-2.0 appears on `cbindgen`, `dwrote` and `option-ext`. Determine which are
actually shipped, record selected licenses, gather required texts/notices, and
audit generated/bundled native code. Existing `notes-ui/assets/NOTICE.md`,
`GPUI-LICENSE` and Noto `fonts/OFL.txt` cover the bundled fonts/icon provenance
and GPUI source guidance. A release package must install these, the repository
MIT license and dependency notices. Runtime system libraries have their own
system packages and licensing.

R4 remains blocked on a clean source-to-package build, disposable install /
preview transition / coexistence / upgrade / removal / rollback checks,
candidate runtime and license audit, native installed-launch/portal tests and
exact-source hosted CI evidence. R5 requires real participants and five actual
working days; no automated result substitutes for those gates.
