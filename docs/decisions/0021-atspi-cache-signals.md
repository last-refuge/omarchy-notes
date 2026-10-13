# 0021 — Preserve structured AT-SPI cache signal arguments

Date: 2026-10-13

Status: exact dependency vendored; serialization and real installed-client cache
signal regressions passed; human screen-reader usability remains unverified

## Measured compatibility defect

The real libatspi client on the supported Wayland test machine warned that
`AddAccessible` had flat signature `(so)(so)(so)iiassusau` and
`RemoveAccessible` had flat signature `so`. Live getters still returned text,
names and offsets when the tests explicitly cleared their cache. Those getters
did not prove that ordinary assistive clients received valid cache updates.

The exact installed AT-SPI 2.60.6
[Cache interface contract](https://github.com/GNOME/at-spi2-core/blob/2.60.6/xml/Cache.xml)
requires **one structured argument** for each signal: AddAccessible takes
`((so)(so)(so)iiassusau)` and RemoveAccessible takes `(so)`. Its
[client parser](https://github.com/GNOME/at-spi2-core/blob/2.60.6/atspi/atspi-misc.c)
rejects the flat signatures observed in the native run.

The locked `accesskit_unix` 0.22.1 passed `CacheItem` and `ObjectRef` directly to
zbus's `emit_signal`. zbus treats the body's outer structure as its argument
tuple, so the single struct's fields became separate arguments. Object/Window
event bodies intentionally contain multiple arguments and already have the
normal `siiva{sv}` shape; they need no change.

## Narrow pinned correction

Keep GPUI and `gpui_platform` at
`a84689073d296dfd39987bc7dd478e43ef76d83a`. Vendor the exact already-locked
`accesskit_unix` 0.22.1 and select it with a root `[patch.crates-io]` path.
Preserve its MIT/Apache license texts, copyright headers and published source
provenance in `vendor/accesskit_unix/OMARCHY-PATCH.md`.

The production marshaling expression changes by one line: the private
`emit_cache_signal` passes `&(body,)` instead of `body`. Its private generic
bound also changes from `DynamicType` to `Type` because a tuple requires the
static type implementation; both existing callers already provide it. This is
a two-line production delta with one behavioral correction, plus a test-only
module. No public API, framework revision, dependency version, text-selection
model, persistence or editing behavior changes.

The crate is excluded from workspace membership while remaining a patched
dependency. The root lockfile selects its path and preserves the existing
package-version set. Enabling the diagnostics also adds `hdrhistogram` 7.6.0
and links the existing `env_logger` dev dependency. Cargo's compatible resolution
selects the already-present `getrandom` 0.3.4 for `tempfile` 3.27.0 instead of
0.4.3; the published tempfile manifest permits `>=0.3.0, <0.5`. This is a
dependency edge change, not an identical dependency graph. The tested lockfile
retains it. Do not ship a dependency-local lockfile, `.cargo-ok` or build output.

## Verification

```sh
cargo test -p accesskit_unix --lib --locked
```

The targeted suite passed **6 tests**, including actual zbus message construction
for AddAccessible/RemoveAccessible, exact wire-signature assertions and typed
payload deserialization. Native regression must additionally exercise the real
adapter emission path, parse cache signals with the installed client and confirm
the previous signature warnings are absent.

`tests/rust-isolated-desktop.py` records only the owned app's private-bus cache,
caret and selection signals. It uses a temporary library, private accessibility
bus and owned nested Wayland window. Orca's installed event processing runs
silently with audio transport suppressed; no speech server or Pulse connection
is used. Require both cache signatures and absence of invalid-cache warnings
after dynamic selection-toolbar insertion/removal or another owned-node change.
Run the standard native accessibility regression against the same patched binary
as well, including its text/selection/table/link semantics.

Separate event delivery was already observed on the prepatch binary: native
Right/Shift-Right/Left emitted caret offsets 1 → 2 → 1 and two selection signals;
all five parsed through libatspi and were processed by actual Orca. Cache
incompatibility does **not** establish that those text events failed. Conversely,
valid cache/text events do not establish useful audible feedback or human
screen-reader usability. Those release requirements remain unverified; the user's
instruction to perform no dictation/audio playback remains in force.

The source audit used exact AT-SPI 2.60.6 XML SHA-256
`d89e329a4b9dd33fc6181371c853dce73250891dc15388b5666cd27cf433f350`
and client source SHA-256
`086477dfb319974c645f67f654f57675b28ddeb620f4a7349cd9261933db61db`.
On 13 October 2026 the silent isolated native run recorded 16 real owned-app
cache signals with the required argument boundary. The GVariant parameter tuple
signatures were `(((so)(so)(so)iiassusau))` and `((so))`, including the outer
argument-list tuple. The installed libatspi client reported **zero** invalid cache
signature warnings. Five caret/selection events parsed and the actual Orca engine
processed them with its audio transport disabled. All four owned processes
exited and no owned PID remained.

The report is `/tmp/notes-isolated-patched-cache-r1-n/report.json`. The tested
release binary SHA-256 is
`6c973d143121633a387a062f8ccec475ce07f22a6c71b8c8711c7234ff3a2273`;
workspace HEAD was `e9c12fc6528d7f2960de106c59c0c8fd7e842934` with tracked changes.
This verifies the measured binary; the exact committed candidate must retain
its own dated checks in the release ledger. The reference GPUI pin is unchanged.
