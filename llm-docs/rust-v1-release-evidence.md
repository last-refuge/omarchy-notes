# Rust v1.0 release evidence

Started 13 October 2026. The [release plan](rust-v1-release-plan.md) is the
acceptance contract. Stable publication remains blocked until every required
gate has evidence. A unit, GPU or AT-SPI API test does not certify real IME or
Orca operation, human first use, or elapsed daily use.

## Frozen distribution contract

- First stable target: x86_64 Arch/Omarchy, Wayland/Hyprland.
- Rust package/executable: `omarchy-notes-rust`; desktop ID:
  `org.omarchy.Notes.Rust`; stable tag: `rust-v1.0.0`.
- Retain preview data/preferences at `omarchy-notes-gpui`; provide an explicit
  preview executable compatibility route. Never select the Qt library by default.
- Keep Qt `omarchy-notes`, its release/tag and the default installer route.
  Rust is an explicit opt-in edition; its v1 does not complete full Qt parity.
- Preserve the exact frozen/deferred scope in the plan, including Qt metadata,
  merged tables and revisions during unrelated edits and lossless backups.

## Gate ledger

| Gate | Status | Revision / procedure / platform | Result and remaining work |
|---|---|---|---|
| V1-01 checkpoint | measured / observed | `10f392bb5d6075419b0e3e1549fe892d96b3deca`; archived Git tree and commands below | Checkpoint committed on `codex/rust-v1-release` and pushed to origin with owner approval. Exact archived-source headless CI checks pass locally. All four hosted CI jobs passed, including clean Qt package/install verification. |
| V1-02 identity/scope | observed contract; distribution unverified | Contract above; [edition audit](../docs/rust-edition.md) | Scope/platform/naming and preview alias route are explicit. AUR name and Rust tags absent at discovery. Executable/package/installer implementation remains R4 work. |
| V1-03 writing/capture/IME/Orca | unverified; partial native observations | R1 worktree; isolated fcitx5/Orca and native AT-SPI procedures below | Actual CJK composition and silent Orca event processing observed. Frozen-binary rerun, candidate visual review and human screen-reader core tasks remain required. Owner's no-dictation constraint persists. |
| V1-04 organization/search/recovery | unverified | R2 | Keyboard tree, sort, SQLite FTS queries, duplicate and trash UI pending. |
| V1-05 portability | unverified | R3 | Existing verified storage backup primitives; in-app import/export/backup/restore/open pending. |
| V1-06 performance/fault/rollback | unverified; measured bounds and CPU diagnostics | R1/R4; [performance record](../docs/rust-performance.md) | Undo and editor/cache bounds implemented; the latest measured CPU edit/scroll run meets its budgets, with previous misses retained. Final frozen-source rerun, native submission/under-load decision and full memory/fault/rollback matrix remain pending. |
| V1-07 visual/native integrations | unverified | R4 state matrix | Existing GPU artifacts are baseline evidence; new paths, scaling and native integrations require review. |
| V1-08 licenses/package/installer | unverified | R4 | Rust package not yet built; runtime/licenses and disposable installation matrix pending. |
| V1-09 human acceptance | unverified | R5, identifiable installed candidate | Five actual participants, owner visual scores and five working days of actual use required. No participants or trial results recorded. |
| V1-10 immutable release | unverified | R6 | No Rust stable tag/artifact published; final exact revision/hosted CI/checksums pending. |

## Reproduction environment

Local machine: Intel Core Ultra X7 358H, x86_64, Linux
`7.2.5-3-omarchy`. Rust/Cargo `1.99.0`, pinned in `rust-toolchain.toml`.
GPUI and GPUI platform share Zed revision
`a84689073d296dfd39987bc7dd478e43ef76d83a`. Do not treat historical preview
measurements as a fresh result on a release candidate.

Baseline commands:

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME= QT_QUICK_BACKEND=software \
  ctest --test-dir build-baseline --output-on-failure --timeout 300
./tests/test-release-metadata.sh
./tests/test-installer.sh
```

Local socket/desktop access may require a sandbox exception. Record environment
failures separately from product failures. Hosted CI must be observed on the
committed revision; configured jobs alone do not pass that check.

## Blocker register

| ID / stage | Reproduction | Expected behavior / verification |
|---|---|---|
| B01 / R1 | Original eager creation opened an editor for every loaded note. | Lazy creation and four clean idle editors implemented. GPU safety regression and 100-cycle CPU diagnostics pass; opaque native task/entity growth remains unverified. |
| B02 / R1 | Original 100 full undo snapshots had no byte cap. | `e9c12fc` implements strict combined 32 MiB/100-step history with exact owned-heap accounting and contiguous recent undo. Exact checkpoint tests and hosted CI pass. |
| B03 / R1 | Real CJK composition and Orca editing in main/Quick windows. | Dependencies installed; actual Pinyin and silent Orca event diagnostics observed. Final binary/candidate review and human core-task usability remain unverified. No audio playback is authorized. |
| B04 / R1 | Run defined 3,000-paragraph/20-table and 10,000-note workloads. | Record latency/frame/RSS/cache/undo budgets and pinned calloop decision under load. Eager layout remains a risk until measured. |
| B05 / R2–R4 | Execute each frozen workflow and visual/fault/install matrix. | Finish implementations and verify with exact fixtures and revision; no inferred passes. |
| B06 / R5 | Start acceptance on the installed release candidate. | Real first-use observations, owner review and five elapsed working days; cannot be replaced with a synthetic script. |

## Checkpoint inventory

The pre-existing Rust implementation comprises nine crates, the lockfile and
toolchain pin, required font/icon licenses, Qt interoperability fixtures/probes,
GPU/native checks, ADRs 0002–0016 and preview/rebuild documentation. Existing Qt
sources/distribution remain intact. Ignore `target/`, `build*/`, `.cache/` and
Python caches. Exclude the unrelated
`llm-docs/mortgage-automator-lendr-course-plan.md` from release commits.

## Dated verification

- 13 October 2026, local original preview tree: **measured** formatting and
  all-target/all-feature Clippy with warnings denied passed. Logs are local
  scratch artifacts (`/tmp/omarchy-r0-clippy.log`), not hosted CI evidence.
- 13 October 2026: **failed environment check**: `ctest --test-dir build` selected
  a nonexistent build directory; the existing Qt build is `build-baseline`.
  Corrected command above; outcome pending.
- 13 October 2026: **unverified** initial workspace run could not bind local
  Unix sockets in the sandbox. Rerun with isolated temporary paths and socket
  access required; do not count these EPERM failures as product test failures.
- 13 October 2026, original preview tree: **measured** socket-enabled rerun
  passed 121 workspace tests, zero failed, three ignored subprocess helpers;
  existing Qt baseline passed all six CTest targets. Release metadata and all
  23 installer tests passed. This used the existing Qt build, so clean-package
  reproducibility remains unverified. ShellCheck is not installed locally.
- 13 October 2026, exact archived Git source `10f392b`: **measured** configured
  Rust headless CI checks passed: PDF structure/text/fonts/bounds (five files),
  Qt → Rust → Qt library compatibility and mutual locks, backup/restore with
  schema-1/history/trash/assets, quotation upgrade/Qt refusal/rollback, nested
  folder interoperability, table editing (39 documents), display-free capture
  retries/concurrency/failure/stale endpoint/restart. Source archive and binaries
  were isolated under `/tmp/omarchy-r0-checkpoint` and `/tmp/omarchy-r0-binaries`.
- 13 October 2026: **observed** approved push of `10f392b` to the configured
  `last-refuge/omarchy-notes` remote. [Hosted CI](https://github.com/last-refuge/omarchy-notes/actions/runs/38440814282)
  started on this revision; its outcome is not yet verified.
- 13 October 2026, R1 worktree based on `10f392b`: **measured** 127 workspace
  tests passed, zero failed, three ignored subprocess helpers. Exact integrated
  checkpoint and final strict checks are pending remaining R1 changes.
- 13 October 2026, R1 worktree: **observed** expanded release-build native
  AT-SPI regression passed named headings, lists, links, tables/cells together
  with complete Unicode/offscreen/empty text, geometry, selection, edit/undo,
  checks/media, actual durable saves, find, formatting, links/related navigation,
  quotation upgrade, forwarding and search. The client emits known cache-signal
  signature warnings; the test uses live API reads. Orca usability remains
  unverified and must independently exercise the actual screen reader.
- 13 October 2026, R1 worktree: **measured** release GPU workflows passed bounded
  editor retention (1,000 notes), writing, Quick Note, Settings, organization
  (10,005 folders), window launching, PDF UI, media, quotes and links. These use
  real fonts and owned synthetic windows; they do not certify IME or scaling.
- 13 October 2026: test-dependency installation approved by the owner; the
  attempted noninteractive install stopped at `sudo: a password is required`.
  No packages were changed by that attempt. Local authenticated installation
  of Orca and fcitx5 Chinese addons is pending; no password is requested in chat.
- 13 October 2026: **observed** owner completed authenticated installation;
  package inventory confirms Orca `50.2-1` and fcitx5 Chinese addons `5.1.14-1`.
  Native composing-engine/screen-reader procedures use isolated profiles and
  do not change the owner's usual IME or settings.
- 13 October 2026: **observed** [R0 hosted CI](https://github.com/last-refuge/omarchy-notes/actions/runs/38440814282)
  completed successfully: Rust workspace/headless interoperability, Qt build/test,
  installer syntax/ShellCheck/23 tests, and clean Arch Qt packaging/install.
  This is actual hosted evidence for `10f392b`, not a Rust package acceptance pass.
- 13 October 2026: **measured** exact archived history checkpoint
  `e9c12fc6528d7f2960de106c59c0c8fd7e842934` passed formatting, 126 workspace
  tests (zero failed, three subprocess helpers ignored), and all-target/all-feature
  Clippy with warnings denied. It preserves the original baseline UI. With the
  owner's authorization for subsequent tested checkpoints, it was pushed to the
  same release-work branch; hosted CI on this revision is running.
- 13 October 2026, R1 diagnostic worktree: **measured** seed `20260923`, 10,000
  notes / 33,909,371 body JSON bytes, Intel Core Ultra X7 358H / Intel Arc B390
  Vulkan, 1440 × 960 logical, TestPlatform 2×. Ordinary complete foreground
  cycles p95 4.424 ms / p99 4.732 ms over 1,000 edits; 100 ordinary-note switches
  p95 7.514 ms. Large note (3,000 paragraphs / 20 tables), 100 edits: **failed**
  p95 101.722 ms / max 109.503 ms; scrolling CPU draw p95 22.183 ms over 100
  positions. These are CPU diagnostics, excluding native platform submission.
  The failed budgets prompted block-shape caching, viewport paint culling and
  a linear editor-normalization pass; remeasurement is pending.
- The same diagnostic run observed five retained session editors and exactly
  503,930,880 RSS bytes at every sample after 100 main/Quick open/close cycles.
  Simulated idle produced zero unforced draws. GPUI's headless scheduled-frame
  callbacks logged removed-window errors during teardown; native lifecycle and
  total opaque entity/task/cache growth remain separate unverified checks.
  Raw synthetic reports: `/tmp/omarchy-performance-r1-2/headless.json` and
  `fixture.png`; original slow attempt was stopped without loop metrics and is
  not counted as a performance pass.
- 13 October 2026, follow-up R1 worktree: **measured** typical complete cycle
  p95 4.524 ms / p99 4.814 ms over 1,000 edits; large edits p95 67.612 ms /
  max 68.338 ms and scrolling CPU draw p95 14.367 ms. Large editing and scrolling
  still **fail** their 33 ms / 8 ms budgets. The pure model profile is now
  p95 0.757 ms (1,000 coalesced) / 1.848 ms (125 separate edits), pointing to
  remaining UI work. Four assembled-geometry passes per unchanged scroll frame
  prompted a further cache correction; document range helpers also avoid
  allocating preceding paragraphs' text. No budget was relaxed.
  Raw reports: `/tmp/omarchy-performance-r1-final/headless.json` and
  `/tmp/omarchy-history-r1-final.log`; the source remains explicitly a worktree.
- 13 October 2026, isolated nested Hyprland / actual fcitx5 Pinyin: **observed**
  main/Quick preedit, cancel, commit to `你好`, undo/redo, selected replacement,
  composing Enter, no preedit autosave, scrolling/resizing and guarded handoff.
  The app binary hash and dirty-source state are in
  `/tmp/notes-isolated-input-r1-d/report.json`. Candidate screenshots show real
  popups next to composition in ordinary/scrolled main/Quick states; resize
  evidence is being corrected for the owned test-window geometry and settled
  viewport. These intermediate checks do not certify a frozen release candidate.
- Actual Orca 50.2 generated 27 utterances including the rich editor, Quick Note,
  selected-text deletion, CJK commit, list and button names. Complete caret and
  selection event delivery and human task usability remain **unverified**.
  Focus-interrupted reruns are recorded as interruptions, not product failures.
  On 13 October the owner requested **no dictation**. Audible checks stopped;
  subsequent event diagnostics have audio transport disabled. No audible
  acceptance or human participant result is inferred from debug utterance logs.
- 13 October 2026: **observed** [hosted CI on the undo checkpoint](https://github.com/last-refuge/omarchy-notes/actions/runs/38442593769)
  passed all four jobs for `e9c12fc`.
- 13 October 2026: **observed** silent native validation of the narrow vendored
  AccessKit cache fix: 16 actual Add/Remove cache signals have the required
  single-struct signatures and the installed client emitted zero invalid-cache
  warnings. Three caret events (offsets 1 → 2 → 1) and two selection events were
  parsed by libatspi and processed by actual Orca with audio transport disabled.
  Report: `/tmp/notes-isolated-patched-cache-r1-n/report.json`; binary SHA-256
  `6c973d143121633a387a062f8ccec475ce07f22a6c71b8c8711c7234ff3a2273`.
  All four owned processes exited; no Speech Dispatcher/audio was used. Six
  dependency serialization tests passed; CI now explicitly runs them. Complete
  audible/task usability remains unverified and no playback is permitted here.
- 13 October 2026, R1 draft-projection attribution: **measured** 100 large edits
  still failed the 33 ms CPU-cycle gate at p95 34.606 ms. Changed callbacks
  consumed 10.663 ms/edit, including 6.894 ms copying the entire owned draft;
  copy batches increased from 4.074 to 8.445 ms/edit. Row updates were 3.768
  ms/edit, including 1.955 ms in the link catalog. These timers overlap and
  must not be summed. Report: `/tmp/omarchy-performance-r1-derived/headless.json`.
- The measured draft-copy cost prompted exact `Document::clone_from` mirroring:
  compare complete root blocks, retain equal roots, copy differing roots and
  copy schema/unknown properties. Insertions, removals, reorder, nested merged
  cells, unknown properties and independent ownership pass the fixture-based
  regression. Editor input now flattens the document only for explicit UTF-16
  range conversion; closed link panels defer catalog filtering until opening.
  CPU remeasurement is pending; these changes alone supply no latency pass.
- R1 release GPU regression **passed** actual FIFO batched-save acknowledgments:
  an unchanged pinned row can reorder other rows without losing the anchored
  note ID or its 17-pixel offset, and changed group headings are remeasured.
  Routed Ctrl+Home/End and selection variants reach offscreen document edges.
  Existing lazy-cache/dirty-work/same-update edit-close-copy-export barriers
  and durable reopen checks passed in the same run. Artifact directory:
  `/tmp/omarchy-r1-mirror-bounded-check`; source is a worktree based on `e9c12fc`.
  Core/editor checks passed 57 tests; UI library checks passed 12 tests,
  including complete accessible Unicode units and oversized combining sequences.
  Final integrated workspace/strict checks and native rebuild remain pending.
- 13 October 2026, frozen R1 worktree CPU remeasurement: **measured** 100 large
  complete foreground edit cycles at p95 27.554 ms / max 28.923 ms, within the
  unchanged 33 ms / 100 ms CPU budgets. Draft mirroring dropped from 6.894 to
  0.567 ms/edit; the whole Changed callback dropped from 10.663 to 4.014 ms/edit.
  Typical 1,000-edit cycles measured p95 3.846 ms / p99 4.147 ms; large scrolling
  p95 2.593 ms. This remains CPU draw/effect diagnostics, excluding native
  submission and accessibility-active work. Report:
  `/tmp/omarchy-performance-r1-mirror/headless.json`; optimized diagnostic binary
  SHA-256 `886c31be6393eb13c879a8f1d36ea785337f4890508fb32a64701b764499c6a1`.
  Native submission, load/fairness, startup and full memory/fault gates remain
  unverified; no stable release gate is closed by this measurement alone.
- R1 cache review found and corrected two further restoration defects: an
  evicted note now retains its logical reading anchor through font/size/width
  changes, including a hidden editor invalidated before another draw; its exact
  typing context also survives, including explicit formatting at an empty or
  boundary caret and unknown span properties. Pure core and release GPU
  regressions pass. Reading-anchor and typing artifacts:
  `target/bounded-editor-check-r1-typing`. Final performance remeasurement with
  this state remains pending; the diagnostic asserts 100 distinct scroll offsets.
- Frozen R1 product tree: **measured** formatting, 134 workspace tests (zero
  failures, three ignored subprocess helpers), six dependency cache tests and
  all-target/all-feature Clippy with warnings denied passed. Standard app and
  all diagnostic examples built in release mode. App SHA-256:
  `b13182321db8318a453c767abd0e7f8f93a96a6aa02fd842e27fff6d58b102dc`;
  diagnostic SHA-256:
  `c3c44c9d7d3a3604fcd0a900b84d8d01d3ee481d660bf3f58ffaa6cb3ac96683`.
  Logs: `/tmp/omarchy-r1-final-{workspace,accesskit}-tests.log`,
  `/tmp/omarchy-r1-final-clippy.log` and corresponding app/examples build logs.
- Final native verification is in progress. One owned-window run stopped at its
  focus guard without further input. Removing the obsolete forced cache-clearing
  workaround exposed an oracle that read the old cell caret before native Tab
  navigation was acknowledged. The test now needs an explicit caret boundary
  wait. An isolated startup also stopped before input because its legacy
  Hyprland dispatcher syntax was rejected by the installed Lua API. These runs
  are recorded as interrupted/harness failures, with no inferred product pass.
  Corrected cached-client and composing-engine reruns remain pending. All
  observed owned processes from the stopped runs were cleaned up.
- 13 October 2026: **observed** private input routing proof on the frozen app
  `b13182321db8318a453c767abd0e7f8f93a96a6aa02fd842e27fff6d58b102dc`.
  Private and host Wayland socket inodes differed; kernel `SO_PEERCRED` matched
  owned nested compositor PID 519091 and UID 1000. With a separate owned Gtk
  Entry focused on the host, 21 ASCII bytes changed only the nested synthetic
  note. The host Entry retained its sentinel with zero change events. The
  capture used private Notes bounds; all five owned children exited. Report:
  `/tmp/notes-v1-private-routing-a/report.json`. This permits private operations
  to retain exact inner focus/workspace, peer/socket and parent-liveness guards
  without depending on host focus. It does not prove composition, frame timing
  or a human accessibility task; their dedicated checks remain required.
- Frozen-app silent combined run `/tmp/notes-v1-final-input-g/report.json`:
  main Pinyin commit/cancel, selection replacement, Enter, undo/redo, durable
  commit, scrolling and resize were **observed**. Root inspected the main
  composition/scrolled captures: the real candidate popup is visible beside
  preedit. The resized app-only crop excludes part of the popup, so its complete
  visibility remains **unverified**. The run stopped at Quick Note's viewport
  oracle, with compositor geometry already exactly 680 × 660 at (20, 20).
  Source inspection identifies the oracle's unbounded framework window root:
  Quick Note's rendered root is a bounded named group, unlike the main rendered
  window root. That diagnosis needs live ancestor/bounds confirmation. No full
  combined pass or new Orca result is inferred; all five owned children exited.
- Read-only R1 source review found no concrete product data-preservation or
  lazy-cache defect. A separate diagnostic review found missing unobserved-edit
  and composition barriers in the performance final-save predicate; they now
  share the complete pending-work predicate with its usage report. Driver
  checks retain failures, refuse optimized Python, verify exact fixture/edit/
  save counts and distinguish mixed navigation from a separate edit-only run.
  The source corrections are reviewed; diagnostic rebuild and remeasurement
  remain pending. The standard app binary is unchanged.
- Focused Quick viewport check
  `/tmp/notes-v1-quick-viewport-a/report.json` **observed** the exact rendered
  `Quick Note` panel at 680 × 660 through a live Component call. Its parent
  framework frame has no bounds, as diagnosed from source; the oracle now uses
  the bounded rendered ancestor and retains exact compositor geometry checks.
  All four owned children exited. Follow-up combined run `input-h` observed
  both main and Quick Pinyin scenarios, then stopped because it read Quick's
  durable identity before waiting for the final undo to save. All five owned
  children exited. Handoff and full silent Orca rerun remain pending; resized
  visual settlement remains unverified rather than waived.
- Captured Unicode row: **observed** accent, Japanese ideographs and joined
  Arabic. The family emoji's boxed appearance matches the installed
  `NotoColorEmoji.ttf` artwork: HarfBuzz maps the family sequence to one nonzero
  glyph (2314), whose embedded image contains four silhouettes in a rounded
  rectangle. Matching the capture to that font is an inference; native fallback
  font IDs were not logged. Existing Serif 19 / Sans 24 GPU probes show no
  glyph zero, but neither those probes nor this row prove complete Unicode,
  bidi, composition-boundary or font/scaling acceptance.
- Frozen standard app `b131823…`, combined run
  `/tmp/notes-v1-final-input-i/report.json`: **observed** both main and Quick
  actual Pinyin commit/cancel, replacement, Enter, undo/redo and final durable
  body acknowledgment, plus composing handoff refusal and settled content
  retention. Actual Orca 50.2 processed focused-editor/caret/selection actions
  silently and generated 19 logged utterances. Real Add/Remove cache signals
  had the required signatures and zero installed-client cache warnings. All
  five owned children exited. This is an engine/event pass, not human or audible
  acceptance. Quick's two-character fixture covers end-caret composition;
  actual long Quick scrolling and settled resized raster acceptance remain open.
- Reviewed diagnostic binary `28b701d7625d473bab5a79adb0271609d96e20b1d15bfb5ec867789c55c08869`
  rebuilt successfully in 11.76 seconds after the snapshot-barrier correction.
  All-target/all-feature Clippy with warnings denied passed in 1.59 seconds.
  Source digest `f71f4a94736611c0616beb59a8357fc358288ece0c7e65082f70ac59ed91100c`
  covers 190 workspace/manifests/vendor/assets/compile-fixture files; manifest
  `/tmp/omarchy-r1-reviewed-source-fingerprint.json`. Final focused CPU/GPU
  checks are running on that frozen binary; earlier results remain dated.
- Owner execution update: use focused quick checks per feature increment and
  comprehensive acceptance on the final candidate. Remaining cached-client,
  native timing/load, resized visual, full integration/fault/install and human
  gates stay explicitly unverified. This changes verification scheduling,
  not the frozen v1 scope or publication requirements.
- Reviewed frozen diagnostic `/tmp/omarchy-performance-r1-reviewed/headless.json`:
  **measured** typical 1,000 CPU edit cycles p95 4.740 ms / p99 5.112 ms;
  100 distinct large-document scroll offsets p95 4.913 ms, within the 8 ms
  CPU target. Large 100 edit cycles **failed** the unchanged 33 ms target:
  p95 60.217 ms, median 26.903 ms, mean 30.245 ms, max 95.500 ms. This latest
  failure supersedes any implication that the earlier 27.554 ms CPU result
  proves the frozen tree passes. Five session editors remained retained; RSS
  rose from 662.266 to 663.809 MiB then stayed stable through the window cycles.
  Final pending work was zero; accessibility was inactive. Focused bounded-
  editor and layout-cache GPU regressions passed (logs
  `/tmp/omarchy-r1-reviewed-{bounded,layout}.log`). Native submission and the
  performance release gate remain open. No budget changed or retry discarded
  this result; attribution and final-candidate correction remain required.
