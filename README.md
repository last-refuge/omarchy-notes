# Omarchy Notes (working name)

A native, local-first notes app for Omarchy: Apple Notes-style capture and rich documents, in the active Omarchy theme. See `llm-docs/omarchy-notes-plan.md` for the plan and `docs/decisions/` for recorded decisions.

Status: editor feasibility spike (milestone 1). It passes the gate; see [docs/decisions/0001-editor-toolkit.md](docs/decisions/0001-editor-toolkit.md).

## Build

Requires Qt ≥ 6.8 (base, declarative, wayland, svg), SQLite ≥ 3.35 with FTS5, CMake ≥ 3.24 and Ninja.

```bash
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Debug
cmake --build build
ctest --test-dir build --output-on-failure
```

## Install (per user)

This installs the binary to `~/.local/bin`, plus a desktop entry and icon so Notes appears in the app launcher:

```bash
cmake -S . -B build-release -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$HOME/.local"
cmake --build build-release
cmake --install build-release
```

Notes live in `~/.local/share/omarchy-notes`. Reinstalling or removing the app never touches them.

## Run

```bash
./build/src/app/omarchy-notes
```

- `--data-dir <dir>` uses a separate library, which is useful for trying things out. The default is `$XDG_DATA_HOME/omarchy-notes`.
- `--new` starts with a new note.
- `--samples` fills an empty library with sample notes (a meeting note with a table, image and PDF; research links; a checklist).
- `--simulate-save-failure` makes saves fail, to exercise the failure path. Ctrl+Alt+Shift+F toggles it at runtime.

## Layout

| Path | What |
|---|---|
| `src/core` | Toolkit-independent library: document schema (`RichDocument`), `QTextDocument` converter, SQLite store, blob store, persistence thread, Omarchy theme palette |
| `src/app` | Qt Quick app: `EditorController` (the `NoteEditor` QML type), `ThemeController` (`Theme`), `AppLibrary` (`Library`), QML views |
| `tests` | Round trip, storage (including a SIGKILL durability loop), theme contrast, and the end-to-end editor gate |

## Keyboard

| Action | Shortcut |
|---|---|
| New note | Ctrl+N |
| Bold / Italic / Underline | Ctrl+B / Ctrl+I / Ctrl+U |
| Strikethrough / Highlight / Monospace | Ctrl+Shift+X / Ctrl+Shift+Y / Ctrl+E |
| Title / Heading / Subheading / Body | Ctrl+Shift+T / H / J / B |
| Checklist / toggle item | Ctrl+Shift+L / Ctrl+Shift+U |
| Bulleted / numbered list | Ctrl+Shift+8 / Ctrl+Shift+7 |
| Indent / outdent list item | Tab / Shift+Tab (also Ctrl+] / Ctrl+[) |
| Insert table / next cell | Ctrl+Alt+T / Tab |
| Attach file / link to note | Ctrl+Shift+A / Ctrl+L |
| Follow link / open attachment | Ctrl+click / double-click |
