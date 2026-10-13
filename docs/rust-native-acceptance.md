# Isolated real input and screen-reader acceptance

Status: scripted real pinyin and basic Orca speech generation observed on
13 October 2026; candidate visual review, complete Orca caret/selection/task
acceptance and human listening/usability remain open.

`tests/rust-isolated-desktop.py` exercises the release executable in an owned
nested Hyprland desktop. Its default command only inspects prerequisites:

```sh
python tests/rust-isolated-desktop.py --binary target/release/omarchy-notes-preview
```

The installed test environment on 13 October 2026 is Hyprland 0.56.2-2,
Aquamarine 0.15.0-2, libseat/seatd 0.9.3-1, fcitx5 5.1.22-1,
fcitx5-chinese-addons 5.1.14-1, Orca 50.2-1 and Speech Dispatcher 0.12.1-3.
The user authorized and performed dependency installation. The runner neither
installs packages nor edits the desktop configuration.

After an exclusive performance run has finished and the runner's ownership and
cleanup have been reviewed, start the actual test explicitly:

```sh
python tests/rust-isolated-desktop.py --run \
  --binary target/release/omarchy-notes-preview \
  --output target/isolated-native-qa --mode both
```

Keep the owned nested window active. The runner refuses input immediately when
either its host window or exact nested Notes window loses focus or changes
workspace. A failed guard is an interrupted check, never a passed check; rerun
with a fresh synthetic library after resolving the cause.

## Isolation and cleanup

A private D-Bus alone does not isolate an input method on the host Wayland seat.
Fcitx's [Wayland documentation](https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland/en)
describes the shared input context and candidate positioning protocol. The runner
therefore gives its fcitx engine, Notes and Orca a separate nested Wayland socket.
It never launches a second fcitx against the host display and never uses
`fcitx5 --replace`, `fcitx5-remote -e` or `orca --replace`.

The nested compositor uses one temporary configuration with no includes,
startup commands or host settings writes. It is launched directly with
`Hyprland --config`, with systemd activation/environment updates and realtime
requests disabled. XWayland is disabled. The host Wayland socket is passed as an
absolute path; the nested display socket and IPC identity are discovered from
the temporary runtime's compositor lockfile and matched to the exact owned PID.

The audited Hyprland/Aquamarine versions normally attempt a physical DRM backend
before Wayland fallback. The runner sets the deliberately nonexistent
`LIBSEAT_BACKEND=notes-qa-no-physical-seat`. Audited
[libseat backend selection](https://github.com/kennylevinsen/seatd/blob/master/libseat/libseat.c)
rejects an unknown forced backend before calling any backend's `open_seat`.
The runner requires that refusal and actual Wayland-backend startup in its own
log, and rejects any successful physical-seat-open message. It refuses unaudited
compositor/libseat versions during preflight. Reaudit these conditions on upgrades;
do not guess an `AQ_BACKEND` environment switch or fall back to a physical session.

Each service/app is a tracked child in its own process group. On success,
failure or interruption, the runner sends termination to only those groups,
waits for its children and escalates to killing only its own group if needed.
The private D-Bus runner has a separate owned group for activation helpers and
permits only `org.a11y.Bus` activation. Portal/GVfs autostart and document FUSE
mounts are excluded from this test bus.
Temporary data/config/state/cache/runtime directories are removed on exit;
logs, screenshots and `report.json` remain under the explicit output directory.
The current notebook, original fixture and production IME/preferences are retained.
Output directories must be new; the runner refuses to overwrite earlier evidence.

All synthetic input checks exact PID, window address, application identity and
current workspace inside the nested compositor, plus the exact owned outer
compositor window in the host. Only printable text, named keys and Ctrl/Shift
chords are allowed. Window sizing targets the owned nested Notes address.
Screenshots use the exact owned Notes window geometry, clipped to its nested
output. A candidate outside that capture needs a separate reviewed owned-window
capture or manual observation. No clipboard operation,
external viewer, browser or host window-management action is part of the script.

## Actual composing engine evidence

The script loads the installed fcitx5 **pinyin** engine with a fresh private
profile. Keys pass through its real Wayland input-method frontend, rather than
calling the GPUI marked-text adapter. Main and Quick Note checks cover:

- `nihao` preedit, cancel and commit to `你好`.
- No preedit in SQLite's committed snapshot; verified committed body after save.
- One-step composition undo/redo, selected-text replacement, Enter handling and
  cancellation after scrolling/resizing.
- Quick/main ownership handoff refusal while composing and content retention
  after settling composition.

The fcitx protocol log records actual preedit and a positive candidate-placement
rectangle. The composition, scrolled and resized screenshots must also be
visually inspected: a protocol rectangle alone does not prove a correctly placed
candidate popup. Record whether candidates are adjacent to the actual caret,
remain visible within the owned output, follow resize/scroll and return to the
correct window after handoff. The script leaves that visual result **unverified**.
Emoji/combining/CJK/mixed RTL text is in the fixture; complete manual acceptance
also includes replacing selections near those boundaries and composition near
wrapped rows and table cells.

## Actual Orca evidence and human acceptance

Orca 50.2 uses GSettings and no longer accepts the older `--enable` or
`--user-prefs-dir` switches. The runner sets `GSETTINGS_BACKEND=memory`, private
XDG directories and a private accessibility bus/registry. It removes inherited
`NOTIFY_SOCKET`, `WATCHDOG_*` and `MANAGERPID` so Orca cannot notify or connect to
the host's service manager during import. The user's instruction **“no dictating”**
persists: the runner starts no Speech Dispatcher, contacts no Pulse server and
plays no audio. An in-process transport initializer is suppressed to prevent
server autostart and sound output is disabled in private memory settings.
Speech generation remains enabled for debug logs because disabling that presenter
also unregisters Orca's caret/selection listeners. Orca's event processing remains
the installed implementation. No installed file,
microphone, recording or audio-setting changes are made.

The script starts the installed Orca engine silently and checks guarded
focus/caret/selection interactions. It separately records raw private-bus
`TextCaretMoved`/`TextSelectionChanged` signals from only the owned application's
bus identity and parsed `Atspi.EventListener` events. Readable text/offset
properties alone do not establish delivered notifications. Orca's actual debug
log must show focused-editor queries and event processing; audio remains disabled
and generated utterance text may be recorded in the log. The fixture contains a heading, checklist,
Unicode/mixed-direction text, merged table, image, attachment and link for
further acceptance.

Orca's [speech implementation](https://github.com/GNOME/orca/blob/50.2/src/orca/speech.py)
also logs speech generation when no speech server exists. Therefore utterance
logs establish actual screen-reader script behavior; they do not establish
audible output or usability. A connected Speech Dispatcher, listening and the
following core tasks must be checked by a person. Keep the owned desktop open
with `--manual-seconds 900` if useful; automatic input stops during that period.

1. Identify current note, heading and focused control by speech; read ordinary,
   emoji/combining/CJK and mixed RTL/LTR text, including content outside the viewport.
2. Move the caret by character/word/line, extend/reverse/replace a selection and
   undo. Confirm useful selected/unselected and caret feedback without sight.
3. Format a heading/list/checklist, navigate existing merged table cells, identify
   image/attachment/link semantics and return to writing with the keyboard.
4. Switch notes, use search, capture in Quick Note and return to main; confirm
   focus/name/save feedback and continuity.
5. Exercise organization/recovery and portability tasks once their v1 routes are
   implemented. Those future tasks cannot be counted from an editor-only run.

Record participant, date, exact binary/source hashes, platform, tasks completed,
utterance/log excerpts, audible result, failures and friction. Do not rename the
scripted engine check as an Orca human task pass or replace it with AT-SPI API
assertions. The release evidence ledger remains the final authority for gate status.

## Dated scripted evidence

On 13 October 2026, the combined runner completed its pinyin main/Quick checks
and composing handoff guard. The actual Orca 50.2 engine generated 27 logged
utterances, including `Note content`, its rich-editor description, Quick Note,
`Selection deleted`, `你好` and named list/button focus. The report was recorded
at `/tmp/notes-isolated-input-r1-d/report.json`, with six bounded Notes-window
captures and `orca-speech.txt` in that directory. The binary SHA-256 was
`bced532434d28dba63efa1ddcb95301f8a359529dabc3bd9779c5985d61183b3`;
the workspace HEAD was `e9c12fc6528d7f2960de106c59c0c8fd7e842934` with tracked
implementation changes. This identifies the tested binary without implying the
uncommitted worktree was an immutable release.

All owned app, engine, speech, accessibility-registry and nested-compositor
processes exited; host fcitx and other host services remained running. The
restricted test bus activated no portals/GVfs or FUSE mounts.

That run's default audio endpoint was inside its private runtime and Speech
Dispatcher reported audio/module errors. Speech generation remains observed;
audible output is unverified. Its basic Orca sequence reached some toolbar focus,
so it did not prove editor caret/selection announcements specifically. The runner
subsequently verified keyboard caret/selection properties and a working audio
module in an intermediate run. Following the user's “no dictating” instruction,
all playback was stopped and the runner now remains silent. Its stronger
raw-signal/libatspi-delivery assertions subsequently passed in silent run
`/tmp/notes-isolated-silent-events-r1-k`: caret offsets 1 → 2 → 1 and two selection
signals were both present on the owned private bus, parsed by libatspi and processed
by actual Orca. Human task acceptance is still unverified and no further audio is
authorized.

After the pinned cache-encoding fix, silent run
`/tmp/notes-isolated-patched-cache-r1-n` validated 16 AddAccessible/RemoveAccessible
signals with the correct single-struct argument boundary and zero invalid-cache
client warnings. It retained the five parsed caret/selection events. The binary
SHA-256 was `6c973d143121633a387a062f8ccec475ce07f22a6c71b8c8711c7234ff3a2273`.
All owned processes exited; no speech server or audio connection was used.
