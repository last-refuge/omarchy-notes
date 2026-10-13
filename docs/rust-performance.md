# Rust performance diagnostics

The Rust v1 [release gates](../llm-docs/rust-v1-release-plan.md) require measured
release builds, large fixtures, bounded memory, and native input-to-frame
submission. Qt's [performance report](performance.md) is useful workload
history; its results do not certify the Rust edition.

## Reproduce CPU and memory diagnostics

The `performance-diagnostics` feature is an opt-in developer tool. It enables
the profiler already present in the pinned GPUI revision; it changes neither
the framework pin nor the ordinary application build.

```sh
cargo build --release -p notes-ui --example performance-check --features performance-diagnostics --locked
OMARCHY_PERFORMANCE_REVISION="$(git rev-parse HEAD)" \
  target/release/examples/performance-check --output /tmp/notes-performance-new
```

Choose a new output directory. The tool refuses to overwrite an existing one.
It creates and removes its own temporary notebook, uses in-memory preferences,
and never opens the default library. It sends no desktop input and captures
only its own synthetic headless window. Run on an otherwise quiet machine;
record any concurrent build, thermal or power conditions. If the source is
dirty, record the base revision and the changes explicitly rather than calling
the measurement an exact-commit result.

The content generator uses seed `20260923`: 9,998 ordinary research notes with
3–15 paragraphs, checklists, tags, Unicode and mixed RTL/LTR text; one
3,000-paragraph note with 20 tables of 3 × 3 cells; and one paragraph containing
a 250,000-character unbroken token. The large note follows the existing Qt
editor workload. Generated body JSON bytes are recorded in the report.
The 1 GB attachment corpus remains a separate required workload.

`headless.json` records library open/load, shell creation to its first CPU draw,
1,000 typical edits, 100 ordinary-note switches, 100 distinct large-note edits,
100 large-document scroll positions, long-token layout, and 100 main/Quick
window open/close cycles. The report includes RSS samples, session-owned editor
counts, and exact history-owned heap usage. `fixture.png` uses real fonts and
the GPUI GPU renderer without capturing unrelated desktop content.

The boundaries are explicit:

- CPU draw means `Window::draw` produced the scene. It excludes platform
  submission, display presentation, and process startup.
- Complete foreground-cycle timings additionally drain queued effects and
  subscriptions after the draw. This exposes expensive library refreshes that
  an immediate input-handler/draw sample could miss. It still excludes native
  submission.
- GPU rendering/readback is a separate screenshot operation, and is not an
  input-latency measurement.
- History bytes count retained snapshot-owned heap allocations and their
  capacities; current documents, allocator resident pages and GPU allocations
  are separate. The report also records RSS and per-editor history limits.
- Session-owned editor counts describe retained application sessions. They
  do not count every opaque GPUI entity, task, subscription or driver object.
- Layout counters summarize the currently retained editors' root-block caches.
  Cache bytes are estimates for block/glyph snapshots, and exclude GPUI's
  global text cache and GPU resources. Shape hits/misses count root cache
  operations; a table root includes its child shapes. Selected-editor reports
  separately show painted and total line counts to expose viewport work.
- Geometry counters separately record assembled-layout width variants. Their
  estimates count geometry storage, excluding shared root glyph data. The
  instrumentation records phase deltas for edit, editor render construction,
  layout lookup/build, caret/viewport prepaint and masked paint. Edit excludes
  later subscriptions; prepaint excludes layout. Cumulative region maxima
  are labelled as lifetime values, and the accessibility-active flag records
  whether accessible tree work was enabled during paint. These regions do not
  account for every part of app drawing or platform submission.
- Simulated idle advances the headless executor without forcing a draw.
  Native idle repainting still needs an actual event-loop observation.

No failed target is turned into a pass by renaming CPU timings as native
submission. Keep the raw report and record the fixture, GPU, logical viewport,
scale, source revision, toolchain and method beside every result.

## Native profiler mode

```sh
OMARCHY_PERFORMANCE_REVISION="$(git rev-parse HEAD)" \
  target/release/examples/performance-check --native 90 --output /tmp/notes-performance-native-new
```

This mode opens one disposable window with app ID
`org.omarchy.Notes.Performance`. It sends no input. After startup it writes
`native-start.json` and prints `PERFORMANCE_WINDOW_READY` with its process ID.
Perform the documented workload manually, or through an exact-window guard
that checks process, app ID, window and active workspace before every input.
Stop if focus changes. Never reuse a guard aimed at a different app ID.

`tests/test-rust-performance-native.py` supplies the exact-window guarded
workload. It refuses optimized Python so ownership assertions remain active,
and retains `app.log` and `run-outcome.json` on failure as well as success.
The driver requires the exact model/UTF-8 edit count and acknowledged final
saves; the diagnostic's boundary also waits for unobserved edits, composition,
imports and error/upgrade states to settle. A pending Changed callback cannot
be mistaken for a committed snapshot.

After the requested interval, `native.json` contains histogram differences
from the initial snapshot: input dispatch to platform submission, frame CPU
draw, dirty-to-submission, events coalesced per frame, and input events excluded
because they arrived during draw. It also samples RSS, retained editors and
frame counts once per second. The profiler measures the native platform's
submission call; it does not measure when the compositor displays pixels.
Baseline/final model revisions and UTF-8 byte lengths independently record
whether the intended edits reached the synthetic note. The report waits for
pending synthetic save acknowledgments and declares that boundary explicitly.
Early window closure without a final report fails the diagnostic command.

Add `--large` to select the synthetic 3,000-paragraph/20-table note before
the native window opens. The harness still sends no input. Baseline/final
leading/trailing `x` counts and collapsed/end-caret flags let a guarded driver
prove alternating document-edge edits rather than merely count key requests.
Typical acceptance requires 1,000 observed edits; the large workload requires
100. Model edit counts and represented input events are recorded separately
from frame-latency samples because input can be coalesced and navigation can
invalidate a frame without editing. The final settled idle span counts
unforced draws while revisions and input-sample counts are stable and saves /
media work have settled. Inspect the actual accessibility-active flag: an
inactive accessibility run does not cover complete active screen-reader tree
cost.

The guarded `--large` procedure alternates document-edge navigation and typing.
Its mixed input distribution covers that interaction, but does not establish an
edit-only p95. Collect the separate `--large --large-typing-only` procedure for
100 edits without navigation; retain the edge run for selection/viewport proof.
Nonzero mid-draw dropped input means incomplete timing coverage and fails the
driver. GPUI still reports coalesced inputs per frame rather than a per-key
presentation measurement.

Record actual edit and selection counts. A report with zero samples supplies
no latency evidence. Collect at least 1,000 edits for the typical p95/p99 gate;
observe large-document edits and scrolling separately. Periods with no input
should show no unnecessary increase in draw counts after pending work settles.
The same procedure must later cover import/indexing/housekeeping under load.

Twenty warm process launches and an explicitly described first-launch sample
need a separate owned-window startup harness. Cached note-switch sampling,
200 end-to-end FTS queries, 1 GB attachments, comprehensive GPU cache accounting
and long-session native growth remain required. A single diagnostics run does
not complete R1, the performance gate, or release acceptance.

The [calloop decision](decisions/0019-rust-v1-event-loop.md) records the pinned
event-loop source audit. A framework patch requires an A/B measurement of the
same workload and no silent change to the release budgets.

## Optional native queue-pressure diagnostic

Use `--queue-probe` for a finite synthetic ready-task load or `--passive` for
the matched heartbeat/timer control, together with `--native` of at least
15 seconds. Activation follows the baseline/readiness marker. The default
probe has one second of passive warmup, forty bursts at 250 ms intervals with
64 foreground callbacks doing 0.5 ms work each, and one second of cooldown.
It generates no typing, drawing or audio. The external driver supplies only
guarded input to the owned synthetic window.

Each periodic record has cheap queue status; the final `queue_probe` contains
tagged raw callback and heartbeat timings plus completion and editor progress.
Its `edit_progress` journal records Changed-subscriber delivery timestamps and
revisions on the same fixed synthetic editor. It uses the probe's exact
monotonic origin, stores at most 4,096 records, stops at the probe deadline and
reports dropped records. A retained subscription and weak recorder cancel
with the diagnostic; no note text is recorded. Final burst summaries count
revision advances while callbacks remain pending, including raw timestamps
for inspecting progress between callbacks. These are subscription delivery
times, which can follow other edit effects; they are not key-send or native
event-arrival timestamps.
Dedicated background-timer-woken captures record `queue_load_window` input
histogram differences and baseline/final model revisions around the planned
load period. Periodic and boundary progress reads use the selected ID and
revision; full text fingerprints are collected only at baseline and final.
Planned boundaries, timer wake times and actual foreground capture times are
reported separately, since queue pressure can delay the capture itself.
Inspect the load-period results rather than only the mixed whole-run histogram.
Phase edit counts are diagnostic and do not replace the 1,000-edit native gate.

The pinned GPUI histogram starts at `Window::dispatch_event`'s `begin_input`
and ends after `platform_window.draw` returns. Waiting before native event
delivery is excluded, as is compositor/display presentation. A dispatch-to-
submission histogram alone cannot prove native event-loop fairness; pair it
with raw service gaps and observed editor progress.

Match source, binary, viewport, scale, input cadence and probe settings between
active and passive runs. These callbacks model queue pressure, not indexing
or imports. FIFO heartbeat delay need not improve merely because a framework
patch polls native events between idle batches; assess native input progress,
active-period latency and raw timing together. The optional helper supplies no
automatic calloop decision and has not yet been verified by native execution.

The guarded driver verifies all forty bursts and requested/enqueued/completed
callback counts (2,560 active, zero passive), plus missing-target, backward
revision and edit-journal overflow checks. Its outcome records whether revision
advances interleave with pending callbacks. No interleaving is useful baseline
starvation evidence; collecting a valid report does not turn it into a fairness
pass.

## Recorded evidence

The latest reviewed frozen sample is recorded at the end of this section. It
fails the 33 ms large-edit CPU p95 budget; earlier favorable measurements do
not replace that result.

On 13 October 2026, an earlier attribution diagnostic used the R1 worktree based on
`e9c12fc6528d7f2960de106c59c0c8fd7e842934`, including bounded editor/history
retention, changed-note-only preview updates, linear span normalization,
core offset/style shortcuts, multi-width shaping/geometry caches and viewport
paint culling, bounded title/snippet derivation, the no-links catalog shortcut,
media preparation only after reference changes, borrowed span-property
normalization, incremental capture rows, an exact incremental draft mirror,
and closed link-picker filtering deferred until opening. The exact AccessKit 0.22.1 transport dependency is vendored with
its cache-message tuple fix; the GPUI pin is unchanged. This is worktree evidence,
not the immutable final candidate. Hardware: Intel Core Ultra X7 358H,
Intel Arc B390 Vulkan, Omarchy 4.0.4/Linux 7.2.5-3-omarchy, Rust 1.99.0.
Viewport: 1440 × 960 logical, the pinned TestPlatform's 2× scale. The fixture
contained 33,909,371 body JSON bytes. No other heavy workloads ran during
sampling; filesystem cache was warm from generating the fixture.

| Diagnostic | Samples | Earlier attribution result |
|---|---:|---:|
| Storage open and eager body load | 1 | 281.98 ms |
| Shell creation to first CPU draw | 1 | 408.06 ms; process startup remains unmeasured |
| Typical complete foreground edit cycle | 1,000 | p95 3.85 ms; p99 4.15 ms; max 6.76 ms |
| Ordinary note switch to CPU draw end | 100 | p95 6.94 ms |
| Large note first CPU viewport | 1 | 119.29 ms |
| Large complete foreground edit cycle | 100 | p95 27.55 ms; max 28.92 ms; CPU diagnostic meets 33 ms / 100 ms |
| Large document scrolling CPU draw | 100 | p95 2.59 ms; CPU diagnostic meets 8 ms |
| Long-token note to first CPU draw | 1 | 65.87 ms |
| Main/Quick window cycles | 100 | 5 retained session-owned editors throughout; RSS constant at 532.69 MiB |
| Retained history after large edits | 1 | 30,142,584 allocated bytes; byte limit reached; 8 total undo steps |
| Simulated idle, no forced draw | 5 simulated seconds | 0 draw samples; native idle remains unverified |

The raw report is `/tmp/omarchy-performance-r1-mirror/headless.json`, with
`/tmp/omarchy-performance-r1-mirror.log` and its synthetic raster alongside.
The measured binary SHA-256 was
`886c31be6393eb13c879a8f1d36ea785337f4890508fb32a64701b764499c6a1`.
Its report declares the frozen but uncommitted R1 source condition. Exact
committed-source/native release evidence remains separate.
Raw sample arrays, total and mean cycle times are retained for further analysis.
The artifact is local diagnostic evidence,
not a shipped asset. Previous cycle logs contained late `window not found`
callbacks after removing headless windows. Source inspection shows that the pinned TestPlatform retains
its previously active TestWindow and asynchronously sends a deactivation
callback after another window activates; the test window's `on_close` hook is
empty. A stale callback can therefore reach GPUI's removed-window check. The
diagnostics logger now records caller line numbers to confirm the exact route.
The harness now explicitly activates its owned hidden Quick window before
removing main, matching the requested focus behavior that TestWindow ignores.
The new 100-cycle run produced no removed-window errors; its only warning was
the documented fake HTTP client in the headless context. Native close/reopen
remains a separate acceptance check.

After the 100 large edits, the selected editor retained 3,020 distinct block
IDs and 6,040 block/width entries with 20,524,638 estimated bytes. Its counters
recorded 609,840 root/width hits, 6,240 misses and no evictions. After the
100 scroll positions, root hits and misses stayed unchanged. The assembled
geometry cache held two width entries estimated at 337,472 bytes; geometry
hits increased from 202 to 602 while misses stayed at 204. The last viewport
painted 11 of 3,180 total shaped lines. All retained
editors together ended with 29,025,009 estimated cache bytes. These are cache
estimates, not total RSS or a GPU memory count.

The per-editor cache contract is 4,096 distinct root IDs, at most eight width
variants per root, and a shared 64 MiB estimated-byte cap. A root owns one
block snapshot. Estimates deduplicate shared unwrapped glyph data within its
width variants; global GPUI and native/GPU allocations remain outside the cap.
The separate assembled-geometry contract is eight widths and 8 MiB estimated
geometry-only bytes per editor, excluding shared root glyph data. The final
retained editors held ten geometry entries estimated at 343,008 bytes.
RSS after large edits was 461.20 MiB and during all window cycles 532.69 MiB;
no monotonic growth appeared in that measured cycle set. This is not yet a
frozen whole-application native RSS budget.

The separate pure-model `history-profile` uses 3,000 paragraphs and 20 tables
with 1,323,319 plain-text bytes. Its 1,000 coalesced inserts measured p95
0.724 ms; 125 distinct undo inserts measured p95 1.644 ms. History retained
32,037,062 allocated bytes and 12 steps, within the 32 MiB bound. After editor
drop, the forwarding allocator reported 17,956 live bytes while RSS remained
49,252 KiB; allocator resident pages and live payload differ. Raw results:
`/tmp/omarchy-history-r1-normalize.log`. This isolates the model from UI work and
does not measure input-to-frame submission. The GUI adapter chooses insert or
platform-range/composition input according to the request, and adds media,
related-note, subscriptions and rendering work; pure insert timings do not
account for that complete path.

Per-region deltas for the 100 large edits recorded 975.19 ms in the editor
edit path (9.75 ms per call), 186.11 ms in 200 editor render constructions,
415.63 ms across 1,200 layout lookups/builds, 7.73 ms in prepaint and 50.53 ms
in masked paint. Accessibility was inactive. The preceding geometry-only run
recorded 17.96 ms per edit call and 52.60 ms full-cycle p95. These changes
reduced the edit region by 7.84 ms per call and also improved work outside
those regions. That attribution run’s full-cycle total was 2,331.76 ms. Defined editor
regions accounted for 1,635.20 ms; Changed callbacks accounted for 401.39 ms,
including 56.65 ms copying the draft mirror, 344.62 ms for row updates and
204.93 ms catalog work **inside** the row total. After these non-overlapping
top-level timers, 2.95 ms per edit remained outside both timer sets. Mirror
cost fell from 6.89 to 0.57 ms per edit; its last ten-edit batch cost 0.58 ms
per edit. That attribution run met the unchanged large-note CPU budget; the reviewed frozen run below does not.
Region maxima are cumulative lifetime
values and include initial layout; they are not maxima of the measured delta.

The preceding measured worktree, before linear normalization and shape/paint
changes, recorded large-edit p95 101.72 ms/max 109.50 ms and scroll p95
22.18 ms in `/tmp/omarchy-performance-r1-2/headless.json`. The intermediate
root-cache worktree measured large edits at 67.61 ms and scrolling at 14.37 ms
in `/tmp/omarchy-performance-r1-final/headless.json`. The geometry/offset
worktree measured large edits at 52.60 ms and scrolling at 2.93 ms in
`/tmp/omarchy-performance-r1-geometry/headless.json`. Bounded derived-state
work measured 35.02 ms large-edit p95 in
`/tmp/omarchy-performance-r1-bounded/headless.json`. Those intermediate results
met the scrolling CPU diagnostic target, while large edits still missed 33 ms.
The difference pointed to UI or input-adapter cost for further attribution.
Pure insert timings alone could not explain or waive that cost.

The subsequent callback-attribution run at
`/tmp/omarchy-performance-r1-derived/headless.json` retained the same workload
and recorded large-edit p95 34.61 ms, mean 30.19 ms. The 100 Changed callbacks
totaled 1,066.34 ms, including 689.42 ms cloning/replacing the session draft
mirror and 376.78 ms updating the row. Catalog updates totaled 195.53 ms
**inside** the row time; these inclusive fields must not be added together.
Draft mirror cost rose from 4.07 ms per edit in the first ten to 8.45 ms in
the last ten. Combined with non-overlapping editor regions, these callbacks
accounted for 2,710.74 ms of the 3,018.90 ms complete-cycle total, leaving
3.08 ms per edit outside both timer sets. The root mirror and closed picker
work were identified as the next measured fixes; this run supplies no 33 ms pass.

Those fixes produced the earlier 27.55 ms large-edit CPU p95. The previous
34.60 ms normalization run and 34.61 ms attribution run remain failed
evidence; no timing budget was changed and no favorable rerun was selected.

An earlier run was stopped after 3 minutes 38 seconds of sustained CPU use
without completed loop timings. That run supplies no pass. Partial reports
now survive each sampling batch, and complete-cycle timing includes queued
subscriptions. The earlier large-note CPU edit path had a measured improvement. The reviewed frozen run below reopens its p95 failure. Native
submission, active-accessibility latency, process startup, native idle, complete
GPU/cache accounting and the full attachment corpus remain unverified.

### Reviewed frozen diagnostic, 13 October 2026

The final controlled run used the reviewed, uncommitted R1 source based on
`e9c12fc6528d7f2960de106c59c0c8fd7e842934`. All 190 source files matched
`/tmp/omarchy-r1-reviewed-source-fingerprint.json` immediately before execution.
Its source digest is
`f71f4a94736611c0616beb59a8357fc358288ece0c7e65082f70ac59ed91100c`.
The diagnostic executable SHA-256 is
`28b701d7625d473bab5a79adb0271609d96e20b1d15bfb5ec867789c55c08869`.
Both values are included in the raw report's provenance. GPUI and calloop pins
were unchanged; no other team build, GPU or native workload ran concurrently.
The workload and timing budgets were unchanged. No rerun was selected.

| Diagnostic | Samples | Reviewed frozen result |
|---|---:|---:|
| Storage open and eager body load | 1 | 287.86 ms |
| Shell creation to first CPU draw | 1 | 397.83 ms |
| Typical complete foreground edit cycle | 1,000 | p95 4.740 ms; p99 5.112 ms; max 6.772 ms |
| Ordinary note switch to CPU draw end | 100 | p95 7.433 ms |
| Large note first CPU viewport | 1 | 122.02 ms |
| Large complete foreground edit cycle | 100 | p95 **60.217 ms**; p99 71.675 ms; max 95.500 ms; **fails 33 ms p95** |
| Large document scrolling CPU draw | 100 | p95 4.913 ms; meets 8 ms CPU target |
| Achieved large-document scroll positions | 100 | 100 distinct offsets, 0 through −99,000 logical pixels |
| Main/Quick window cycles | 100 | 5 retained session-owned editors; RSS 662.27 → 663.81 MiB, then plateau |
| Final pending edit/composition/media/save sessions | 1 checkpoint | 0 |
| Simulated idle, no forced draw | 5 simulated seconds | 0 draw samples; native idle remains unverified |

Raw evidence is `/tmp/omarchy-performance-r1-reviewed/headless.json` and
`/tmp/omarchy-performance-r1-reviewed.log`; the report includes complete raw
sample arrays and `large_scroll_position_proof.achieved_y`. Large-cycle median
was 26.903 ms and mean 30.245 ms. Ten-edit means were 18.169, 20.441, 22.730,
25.047, 26.470, 26.416, 28.491, 49.965, 47.642 and 37.083 ms. The late increase
starts around edit 72; edit 79 reached 95.500 ms. This is an unresolved frozen
sample failure, not native input-to-submission evidence. Accessibility was
inactive.

For the 100 large edits, editor regions totaled 1,260.78 ms in edit,
281.93 ms in render, 517.82 ms in layout, 43.09 ms in prepaint and 60.68 ms
in paint. Changed callbacks totaled 488.52 ms, including 70.67 ms in the
incremental draft mirror and 417.69 ms in row updates; catalog work was
247.53 ms **inside** row updates. The full-cycle total was 3,024.55 ms, leaving
3.717 ms per edit outside the non-overlapping editor and Changed timers.
Mirror cost remained 0.707 ms per edit, so the earlier full-document draft
copying hotspot was not reproduced.

Root/width retention and shape counters matched the earlier attributed run:
3,020 roots, 6,040 entries, 20,524,638 estimated bytes, 609,840 hits and 6,240
misses. Scrolling left shape counters and geometry misses unchanged, used two
geometry entries estimated at 337,472 bytes and painted 11 of 3,180 lines.
All retained editors ended with 29,025,009 estimated shape-cache bytes and
343,008 estimated geometry bytes. History retained 30,142,584 bytes and eight
undo steps.

RSS already measured 605.67 MiB after initial open, compared with 377.10 MiB
in the earlier attribution run, before large edits. It reached 675.74 MiB
after large edits, compared with 461.20 MiB earlier. The higher initial
resident footprint makes runtime font/allocator/cache conditions a lead;
their contribution was not measured. Continuous Changed callbacks reset the
700 ms autosave delay, so current-note autosave overlap is not established.
The report contains no per-save timestamps. Late batches and broader region
costs require further attribution; the data does not justify assigning one
confirmed cause or relaxing the budget.

The two affected GPU regressions passed serially after the CPU run, with no
rebuild: bounded editor workflow/typing/reading-state eviction and layout
cache/geometry/scroll checks. Logs are
`/tmp/omarchy-r1-reviewed-bounded.log` and
`/tmp/omarchy-r1-reviewed-layout.log`. Their executable SHA-256 values were
`94290fe44b274ff9cc5abbb86263f1e34a4d7e51363119b658c74d93029a8af6`
and `1ade6c6537409daf0f92936502122b4607dc0bba8a94364162b1a387d4e42551`,
respectively. These passing regressions do not override the timing failure.
