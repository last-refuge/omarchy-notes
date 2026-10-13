# 0019 — Measure Linux event-loop pressure before changing its dependency

Date: 2026-10-13

Status: source audit complete; load comparison and final R1 decision unverified

## Current resolution

Keep GPUI and `gpui_platform` at
`a84689073d296dfd39987bc7dd478e43ef76d83a` while measuring the release workload.
The committed workspace resolves crates.io `calloop` 0.14.5. Zed's workspace
patches are not inherited by this application; at the same GPUI revision its
`Cargo.toml` selects the Zed calloop fork at
`3759371fee14c40066d64777e2a36b6ffcc22590`.

Retaining the current resolution is a provisional baseline decision, not a
claim that the event-loop gate passes. Do not upgrade GPUI to fix a performance
problem whose source has not been measured. If ready foreground work delays
input or frame callbacks under the prescribed loads, compare the pinned calloop
fork on the same source, fixture, hardware and build features. Record that result
and make the final dependency decision before beta.

## What the audited source actually does

Pinned GPUI's Linux `PriorityQueueCalloopReceiver::process_events` drains ready
foreground runnables into callbacks. In
`LinuxCommon::register_sources`, each callback queues `runnable.run()` through
`loop_handle.insert_idle`, followed by GPUI's after-runnable hook. That is the
event-loop path relevant to this application. The separate generic calloop future
executor is not the GPUI foreground scheduler.

Crates.io calloop 0.14.5's `dispatch_idles` takes the complete idle vector and
dispatches every callback before returning to event-source polling. The
[pinned Zed fork's loop implementation](https://github.com/zed-industries/calloop/blob/3759371fee14c40066d64777e2a36b6ffcc22590/src/loop_logic.rs)
uses a queue and a **10 ms idle-dispatch budget**. It retains callbacks left after
that budget and polls without blocking while any remain, allowing ready event
sources to run before more idle callbacks.

The fork checks elapsed time **between callbacks**. It cannot interrupt one
expensive model edit, full-document layout, accessibility snapshot or paint.
A 100 ms callback remains a 100 ms stall. Application work must still be bounded,
cached, incrementally invalidated or run in background tasks as appropriate.
The fork also does not by itself bound the number of foreground tasks or fix
eager note-body loading or repeated full-list resets.

Sources inspected locally:

- Workspace `Cargo.lock`, the `calloop` package entry (0.14.5, crates.io).
- Pinned Zed `Cargo.toml`, `[patch.crates-io]`.
- Pinned GPUI `crates/gpui_linux/src/linux/dispatcher.rs`,
  `PriorityQueueCalloopReceiver::process_events`.
- Pinned GPUI `crates/gpui_linux/src/linux/platform.rs`,
  `LinuxCommon::register_sources`.
- Installed calloop 0.14.5 `src/loop_logic.rs`, `dispatch_idles` and `dispatch`.
- Exact fork `src/loop_logic.rs`, fetched as source into a temporary file;
  SHA-256 `cf7435e5ac665f3a4a420b5b670befcf3bad576a816c911f4ea50a5c50afb6d2`.

## Required load comparison

Use a release build and a disposable library. Record source and lockfile hashes,
Cargo features, Rust toolchain, CPU/GPU, compositor, scale, viewport, fixture and
background load. The test must exercise the actual Wayland event loop; a
headless `Window::draw` workload only diagnoses model/layout/paint CPU.

Measure input-handler-to-frame-submission latency and longest input/event stall
with indexing, import and housekeeping active while typing. Include the large
3,000-paragraph/20-table fixture, the 10,000-note library, long unbroken tokens,
many images and the required attachment corpus. Separately exercise a queue of
ready foreground callbacks; record queue service and input/frame event progress.
The fork should improve queue fairness where that is the measured cause without
changing ordering-sensitive saving, composition, task cancellation or shutdown.

First attribute a missed budget to model edit, layout, accessibility, paint,
storage result application or queued callbacks. Fix single expensive callbacks
in the application. Apply the exact audited fork in an isolated comparison only
when queue pressure demonstrates a need, then rerun the affected native and
durability regressions. Keep or reject the patch based on those measurements.

## Current evidence

The pure history profiler in
[0017 — bounded editor history](0017-bounded-editor-history.md) provides measured
model CPU and exact retained heap payload. It does not measure event-loop queue
fairness, frame submission or display presentation. The new whole-app performance
harness supplies additional attributed CPU measurements; its headless samples
also do not establish Wayland event-loop fairness.

The source audit identifies a concrete patch and its limits. **The native load
comparison remains unverified and blocks the final R1 event-loop decision.** No
manifest/lockfile patch or GPUI pin change was made for this audit.
