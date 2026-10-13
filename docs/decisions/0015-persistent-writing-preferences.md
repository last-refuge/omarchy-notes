# 0015 — Remember the way you write

Date: 13 October 2026  
Status: implemented in the Rust preview; broader Settings/library work remains

## Experience

**Settings** or **Ctrl+,** opens a quiet, full-width page with three palette
previews, serif/sans/mono writing fonts, a live type sample and keyboard-adjustable
writing size and pane widths. Escape returns to the previous focus. Appearance
and typography changes preserve the note, selection and undo history. New Quick
Notes inherit the same writing preferences; open editors update together.

Beauty and delight include continuity, legible feedback and predictable controls.
Settings adapt down to 420 × 420 logical pixels, keep their save feedback visible,
and scroll the focused control into view during Tab/Shift+Tab navigation. The
numeric groups announce their current values. A successful retry returns focus
to the Settings header before removing the now-unneeded Retry control. The
editor retains a caret or reading anchor while typography reflows. Panel motion
and a blinking caret are absent in the current preview; there is no decorative
motion switch pretending to control an unimplemented behavior.

Pane dividers update locally while dragging and persist once on release. Escape
cancels an unfinished drag. Preferred widths return when the window has room;
narrow layouts still adapt. Focus mode and the current title/date-edited sort
choice are remembered too. This does not implement the remaining sort modes,
library actions, import/export or backup/restore screens.

## Persistence

Preferences live in `$XDG_CONFIG_HOME/omarchy-notes-gpui/preferences.json`, or
`~/.config/omarchy-notes-gpui/preferences.json`. They are separate from every
library and from Qt's settings. Merely reading defaults creates nothing. Demo
choices remain in memory. `--reference` and `--light` override this launch's
palette; unrelated changes never persist the override. Choosing a palette in
Settings explicitly replaces it. Forwarded launches continue using the running
owner's preferences.

The framework-free session tags one in-flight save by operation and retains
newer edits when an older acknowledgement arrives. A bounded background worker
merges only locally changed fields into a fresh file read under a separate
advisory lock. Different library owners can save independent choices without
overwriting unrelated fields. There is no live cross-process preferences watcher;
the next save/retry or launch reads the current file. Concurrent changes to the
same field use the last successful save.

Version 1 validates ranges and rejects unknown fields, future versions, malformed
JSON, oversized files and symlinks. Reads are limited to 64 KiB and reject
non-regular files. Writes use a private temporary file in the same directory,
file sync, atomic rename and parent-directory sync. Lock acquisition on the
worker has a one-second bound. File work never runs in render; the initial read
happens before opening GPUI windows.

The UI reports saving until acknowledgement. Failures retain the desired choices
for the current session and show a separate preferences error with **Try again**;
they never claim a note save failed or overwrite an unsupported preferences file.
Retry rereads externally repaired settings and merges the user's pending changes.
Graceful close waits for an outstanding preferences write. A reported preferences
failure does not trap the user in the app; closing can lose those unsaved choices,
while existing note-save protections remain in force.

## Keyboard activation correction

Native testing exposed duplicate Enter activation in shared button builders.
The pinned GPUI synthesizes a keyboard click on key release, while the old
builders also acted on key down. Buttons now use one `on_click` path for mouse,
keyboard and accessibility activation. This correction also applies to formatting,
selection, table, media and Quick Note controls. The GPU key helper now emits
key release as well as key down; explicit Enter/Space stepper checks assert one
change per activation.

## Verification boundaries

`settings-check` uses real fonts/GPU layout, SQLite and temporary preference files.
It exercises save/reopen, rapid edits, palette overrides, typography, undo,
Quick Note inheritance, pane commit timing, focus/sort persistence, close flushing,
future-version preservation, repair/retry and dark/light/compact/error layouts.
The native check uses an owned Wayland window, a private accessibility session,
and temporary library/configuration directories. The latest outcomes are recorded
in `llm-docs/gpui-rebuild-baseline.md`.

Neither check proves real IME or Orca usability, mixed-DPI behavior, large-document
performance or complete visual-state signoff. Accessibility frame bounds are
unavailable on the installed adapter; native control checks use the actual
Settings group's bounds. The existing AT-SPI cache ABI warnings remain. Settings
does not complete M1/M4 or authorize replacing the supported Qt installer.
