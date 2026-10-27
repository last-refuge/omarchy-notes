# Omarchy Notes: release identity and compatibility contract

Omarchy Notes is a Rust-only app. Qt is retired and relevant only to legacy data
migration. Historical rebuild plans and ADRs record earlier decisions; their Qt
support and installer-default statements are not current policy.

## Package rename and unchanged app identity

| Identity | Omarchy Notes |
|---|---|
| Arch package | `omarchy-notes` |
| Executable | `omarchy-notes-rust` |
| Source archive | `omarchy-notes-VERSION.tar.gz` |
| Cargo application package | `omarchy-notes-preview` |
| Desktop / AppStream ID | `org.omarchy.Notes.Rust` |
| Release tag | `rust-v1.0.2` (release pattern: `rust-vVERSION`) |
| Default library | `$XDG_DATA_HOME/omarchy-notes-gpui` |
| Preferences | `$XDG_CONFIG_HOME/omarchy-notes-gpui/preferences.json` |
| Runtime instance directory | `$XDG_RUNTIME_DIR/omarchy-notes-rust` |
| Advertised platform | x86_64 Arch/Omarchy, Wayland/Hyprland |

The XDG fallbacks are `~/.local/share` and `~/.config`. The rename is package-only:
binaries, app IDs, profile/runtime paths and release tags are unchanged. The installed `omarchy-notes-preview` compatibility
launcher executes `omarchy-notes-rust` with the same arguments and exit status,
never Qt. An older manual preview earlier on `PATH` can shadow it; inspect and move
that specific old executable aside rather than deleting profiles.

The current installer installs Omarchy Notes and needs no edition selector.
Older edition-aware installers require `--edition rust`. Use Rust tags, not the retired `v1.0.0` tag.
Checksums, assets, package metadata and the package-owned executable must match
Rust identity. The new Arch recipe replaces and conflicts with the old
`omarchy-notes-rust` package. Reusing the retired Qt package name does not select
or migrate Qt data.

Older published `rust-v1.0.0` artifacts still use the old `omarchy-notes-rust`
package name. The new installer requires a newly named `omarchy-notes` release
package. The published `rust-v1.0.1` release supplies the new package;
the older release has not been modified.
See [installation and release instructions](rust-release.md).

## Scope and evidence

Rich writing, basic tables, images/files, autosave, Quick Note, durable capture,
folders, pins, duplication, sorting, prefix search, Recently Deleted, reviewed
Markdown/text imports, portable exports and exact backup/restore are implemented.
Existing merged tables, tags/Smart Folder metadata and stored revisions are
preserved. New merges and other advanced table operations, tag/Smart Folder UI,
gallery, library-wide attachment browsing, browsable revisions, drag filing and
bulk organization remain deferred.

Automatic `--setup-hyprland` and the old `--samples` option are not current app
routes. Use the desktop Quick Note action or a manual unused compositor binding;
`--demo` opens disposable samples. X11 and other architectures are not advertised.
Retirement does not resolve native performance/fairness, portal/scaling/suspend,
installed upgrade/rollback verification or human accessibility/usability gaps.
See the [release notes](rust-release-notes.md) and historical
[release evidence](../llm-docs/rust-v1-release-evidence.md) for exact results and limits.

## Current command routes

| Option | Behavior |
|---|---|
| `--help`, `--version` | Show app help/version |
| `--data-dir PATH` | Open a selected compatible library |
| `--new`, `--note UUID` | Launch/forward a new or specific note |
| `--quick-note` | Launch/forward Quick Note |
| `--capture TEXT` (or `-` for stdin) | Durable text capture |
| `--request-id UUID`, `--capture-status UUID` | Safe capture retry and read-only receipt checks |
| `--backup PATH`, `--inspect-backup PATH` | Headless backup/inspection; close the writer for CLI backup |
| `--restore PATH --data-dir NEW` | Restore into a new library |
| `--reference`, `--light` | Launch palette overrides |
| `--demo` | Disposable design sample |
| `--simulate-save-failure` | Developer fixture, not a runtime keyboard toggle |

Settings → Your notes and files supplies reviewed import/export, backup,
inspection, safe restore and library opening. No import/export CLI is advertised.
See [data ownership](rust-data-ownership.md) and the [writing guide](gpui-preview.md).

## Keyboard routes

| Action | Shortcut / route |
|---|---|
| New note / folder | Ctrl+N / Ctrl+Alt+N |
| Quick Note / handoff to main | Ctrl+Shift+N / Ctrl+Shift+O |
| Library search / find in note | Ctrl+Shift+F or Ctrl+K / Ctrl+F |
| Next find match | F3 / Enter |
| Delete / duplicate note | Keyboard-accessible Note actions |
| Bold / italic / underline | Ctrl+B / I / U |
| Strikethrough / highlight / monospace | Ctrl+Shift+X / Y, Ctrl+E |
| Title / heading / subheading / body | Ctrl+Shift+T / H / J / B; Ctrl+Alt+1 / 2 / 3 / 0 |
| Checklist / tick | Ctrl+Shift+L / U; Ctrl+Shift+C checklist alias |
| Bullet / numbered list | Ctrl+Shift+8 / 7 |
| List indent / outdent | Tab / Shift+Tab; Ctrl+] / [ |
| Basic table tools | Ctrl+Alt+T |
| Attach file / link note | Ctrl+Shift+A / Ctrl+L |
| Open attachment / object | Ctrl+Enter |
| Settings / focus mode | Ctrl+, / Ctrl+Backslash |
| Save / PDF export | Ctrl+S / Ctrl+Shift+P |

Qt shortcuts are not a transparent migration contract: Ctrl+Shift+N is now Quick
Note, Ctrl+D is not assigned to duplication, and Ctrl+Shift+S is not advertised
for hiding folders. Native cross-app opening and link behavior still need the
verification described in the acceptance record.

## Legacy data boundary

The retired Qt library was `$XDG_DATA_HOME/omarchy-notes`; its preferences were
`$XDG_CONFIG_HOME/omarchy-notes/settings.ini`. Neither is automatically selected,
renamed or upgraded. Close any legacy writer, migrate consistent copies with
verified backups, and preserve the originals. Schema-3 quotation libraries require
Rust. A rollback to a legacy reader uses the pre-upgrade backup and excludes later
Rust edits. Portable exports can be lossy; keep exact backups.
See [migration](migrating.md).
