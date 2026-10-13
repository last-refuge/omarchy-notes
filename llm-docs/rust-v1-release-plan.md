# Omarchy Notes — focused Rust v1.0 release push

Date: 13 October 2026  
Status: implementation in progress; R0 checkpoint `10f392b` is backed up on the
remote, R1 checkpoint `12375b2` is pushed, R2 implementation is active, and the
stable release gates remain incomplete. See the
[release evidence](rust-v1-release-evidence.md).

Execution update, 13 October 2026: the owner requested focused, quick checks
after each implementation increment and comprehensive testing on the final
candidate. Preserve every frozen feature and stable acceptance gate below;
carry remaining native/performance/visual matrix checks forward explicitly
instead of delaying missing user workflows through repeated harness refinement.

## Outcome

Ship a **beautiful, dependable, offline notes app that is a delight to use every
day**. A new user must be able to capture a thought, shape it, organize it, find
it again, recover an accidental deletion, and take their data elsewhere without
needing a terminal or an explanation from the developer.

This is a focused release milestone within the
[full Rust/GPUI rebuild](rust-gpui-rebuild-plan.md). It prioritizes a complete
everyday experience over completing every Qt feature before the first stable
Rust release. Deferred features remain in the full-port backlog; shipping this
milestone does **not** mean the full port is finished. Safety, accessibility,
responsiveness and beauty are not deferred to make a release date.

## Starting point and release boundary

The current preview already has rich editing, autosave and conflict recovery,
images/attachments, rich clipboard exchange, tables, editable links and quotes,
PDF export, Quick Note, durable capture, window forwarding, writing preferences,
and nested folder creation/renaming/moves. Headless verified backup/restore also
exists. Extend these implementations rather than starting a second UI or editor.

The [latest verification record](gpui-rebuild-baseline.md) reports 121 passing
workspace tests, 13 GPU workflows, native folder/editor/Quick Note checks and Qt
storage interoperability. Those results are the starting baseline, not proof of
v1 readiness. Real IME/Orca, full performance and release acceptance remain open.

At the start of this push the Rust workspace was version `0.1.0` and its
implementation was uncommitted, based on `5063e7c` (11 October 2026,
installer/documentation). R0 now preserves that implementation as `10f392b`;
the version stays a preview until the stable gates pass. The Qt
package already uses `1.0.0`; do not overwrite its release/tag or imply that a
smaller Rust feature set is a transparent upgrade.

**Planned distribution:** first ship a separately installable stable Rust edition
for x86_64 Arch/Omarchy on Wayland/Hyprland. Use `omarchy-notes-rust` for the stable
package/executable, `org.omarchy.Notes.Rust` for its desktop identity, and
`rust-v1.0.0` for its release tag. Validate availability and the transition in R0.
Retain the current Rust data/preferences locations so preview users do not lose
their notebook. Handle the preview executable's compatibility route explicitly.
Keep Qt available and keep the existing default Qt installer route until the
separate replacement gate in the full plan is met. Add a documented, tested
Rust opt-in installer route; never infer the edition from a shared version number.
X11 and additional architectures remain unadvertised until tested.

## Frozen v1 scope

| Area | Required for v1 | Deliberately deferred |
|---|---|---|
| Writing | Existing rich editing, lists/checklists, quotes, basic tables, images/files, links, in-note find, undo/redo and reliable saving | Creating new table merges and advanced table selection/editing |
| Capture | Quick Note, main-window handoff, CLI capture/status/retry, installed launch actions | New capture integrations |
| Organization | Nested folders, rename/move, full keyboard tree navigation, remembered expansion, pins, duplicate note, edited/created/title sorting | Drag-and-drop filing, date-group headings, bulk organization tools |
| Recovery | Delete note, Recently Deleted, immediate Undo, restore, folder deletion with clear consequences, permanent-delete confirmation and the existing 30-day retention contract | A browsable revision-history UI |
| Finding | SQLite FTS5 prefix search, meaningful snippets, folder filtering, stable keyboard selection, no-match/error states | Tags UI, Smart Folders, gallery and library-wide attachment browser |
| Portability | Markdown/text file and folder import, per-note Markdown/HTML/PDF export, whole-library Markdown plus assets, exact backup and safe restore through the app | Additional formats, sync, collaboration, AI, OCR and encryption |
| Personalization | Existing palettes/fonts/type size/pane widths/focus preferences, accessible keyboard controls, library actions in Settings | Additional preference panels and decorative animation |

Deferring a UI must never discard its existing data. Preserve Qt tags, Smart
Folder records, merged-cell structure, attachment identities, revisions and
unknown supported properties during unrelated edits, backup and migration.
Keep existing merged tables readable and safely round-trippable; document any
unsupported editing operation instead of flattening it. Deferred controls should
not appear as inert buttons. Publish an explicit Rust/Qt feature comparison.

## Delivery sequence

| Stage | Result | Depends on |
|---|---|---|
| R0 | Reviewed checkpoint, frozen scope and repeatable baseline | Current worktree |
| R1 | Editing/input/accessibility risks resolved; performance budgets recorded | R0 |
| R2 | Complete organize → find → delete → recover workflow | R1 |
| R3 | Complete import → export → backup → restore workflow | R1; integrate with R2 |
| R4 | Polished, hardened, installable release candidate | R2 and R3 |
| R5 | Candidate passes first-use review and five-working-day trial | R4 |
| R6 | Immutable, verified stable Rust release and recovery instructions | R5 |

Keep one product increment active at a time. Packaging discovery can begin in R0,
and representative fault/visual tests belong in each stage. Do not leave all
native testing or visual correction until R4. Set a release-date estimate after
R1 exposes the actual input/accessibility/performance work; the five-working-day
trial is real elapsed use, not a test script or an implementation estimate.

### R0 — Preserve the work and establish the release baseline

1. Review the complete tracked/untracked change inventory. Exclude build output,
   temporary libraries and unrelated material, including the Lendr course plan.
   Check in the Rust workspace, lockfile, required fixtures/assets, tests and
   relevant documentation as a coherent checkpoint. Do not invent a historical
   sequence of already completed milestone commits.
2. Reproduce the documented tests on that commit; record the exact revision,
   toolchain/GPUI pin, machine and known gaps. Retain Qt checks while it remains
   a supported edition. Run configured CI and distinguish local passes from
   observed hosted results.
3. Confirm the frozen scope, supported platform, package/tag identity and preview
   transition. Audit existing keyboard/CLI contracts against the Qt code. List
   each advertised command and its Rust route; resolve conflicts deliberately.
4. Create a short release-evidence ledger, indexed by the gates below, plus a
   blocker list with reproduction, expected behavior and verification method.
   Make checkpoint commits after each verified increment and push reviewed
   checkpoints to the project remote as part of the implementation workflow.

**Exit:** the current application can be reproduced from Git, scope and naming
are explicit, and every remaining v1 requirement has a stage and evidence path.
Writing this plan does not itself commit, push, tag or publish the work.

### R1 — Resolve the risks that could invalidate the release

**Primary code:** `notes-editor`, `notes-ui`, `notes-platform`.

- Exercise a real fcitx5/ibus composing engine: commit/cancel, Enter/Escape,
  candidate placement, selection replacement, undo, scrolling, resize and
  main/Quick handoff. Include emoji, combining marks, CJK and mixed RTL/LTR text.
- Use Orca for the actual core tasks. Finish reachable heading/list/check/table/
  image/link semantics, focus announcements and selection/caret behavior needed
  to operate the shipped editor. AT-SPI API assertions alone do not pass this gate.
- Profile a release build with the large fixtures and budgets below. Address
  eager document loading/shaping, repeated full-list resets, retained editor
  entities, image caches and undo snapshots where measurements identify them.
  Virtualize/paginate the expensive paths while keeping selection in the document
  model, preserving scroll anchors and never evicting unsaved work.
- Set explicit memory/cache/undo budgets from the recorded baseline before beta.
  A 100-snapshot count alone is not a byte bound for large documents. Preserve
  predictable recent undo and document any history limit.
- Record the pinned GPUI Linux event-loop/`calloop` patch decision under load.
  Do not upgrade the framework just to obtain a newer API without a measured need.

**Exit:** no unresolved fundamental typing, selection, accessibility or
responsiveness blocker. Remaining implementation has a demonstrated technical
path. If a real native check is unavailable, record it as unverified and arrange
the required test environment; do not replace it with a synthetic pass.

### R2 — Finish everyday organization, search and recovery

**Primary code:** `notes-core`, `notes-storage`, `notes-ui` organization/search.

1. Finish keyboard tree navigation: Up/Down/Home/End, expand/collapse, parent/child
   movement, Enter to open, visible focus and offscreen reveal. Persist expansion
   by folder ID per library. Retain position after rename/move and recover cleanly
   when a stored folder no longer exists. Add the remaining sort modes and note
   duplication with fresh identities and preserved attachment bytes.
2. Query SQLite FTS5 through bounded background work. Support Qt-compatible prefix
   matching/snippets and explicit All notes/current-folder scope. Tag responses
   by query generation and scope so old results cannot replace newer ones.
   Keep the current draft and keyboard selection stable; refresh committed results
   after saves/moves/deletes. Page results or expose a clear load-more route.
3. Add Recently Deleted, restore and immediate deletion Undo. Serialize dirty-save
   and deletion operations so a stale autosave cannot resurrect or overwrite a
   deleted note. Coordinate the same note across main and Quick Note. Keep content
   recoverable on failures and across restart; ordinary deletion is not permanent.
4. Match and explain folder deletion: descendants are removed, their notes move
   to Recently Deleted, and later note restoration falls back to unfiled Notes
   when the original folder is gone. State whether an immediate Undo also restores
   the folder tree; implement and test that exact contract. Do not imply that
   restoring notes automatically reconstructs deleted folders.
5. Implement the 30-day purge and explicit permanent deletion with visible
   consequences. Housekeeping runs off the input path; attachment cleanup must
   retain anything referenced by live notes, trash or revision history.

**Exit:** a user can create, duplicate, file, find, delete, undo and restore a
note entirely with the keyboard. Tests cover stale operations, failures/restart,
same-name folders, nested deletion, missing-parent recovery, search consistency,
the retention boundary and retained attachments/revisions.

### R3 — Make data ownership usable from the app

**Primary code:** `notes-exchange`, `notes-export`, `notes-storage` backup/media,
and `notes-ui` Settings/portability screens.

- Add file/folder pickers, progress, cancellation and result summaries around
  Markdown/text import. Reuse existing conversion code. Resolve local images and
  files safely; report missing assets and unsupported formatting before committing
  a lossy import. Import copies data and leaves source files intact. Quote imports
  follow the existing explicit backup/upgrade contract.
- Export individual notes as Markdown/HTML/PDF and the library as Markdown with
  an asset directory. Handle duplicate filenames, relative links, linked notes,
  Unicode and media bytes. Show explicit format limitations; portable exports
  are not substitutes for lossless backups. Keep attachments accessible even when
  a format cannot display their contents inline.
- Expose **Back Up Library**, **Inspect Backup**, **Restore Backup** and **Open
  Library** through polished, keyboard-accessible app routes. Reuse verified
  storage primitives; extend progress/cancel support where necessary.
- A backup includes a declared committed revision boundary and verified assets.
  Flush the desired pending edits first, or explain that they are not yet included;
  never call a backup complete while silently omitting an unsaved draft. Keep
  typing responsive if work continues after the snapshot boundary.
- Restore into a new verified library, then offer to open it. Preserve the current
  library and backup. Coordinate all open windows and pending saves before a
  switch; failed verification or cancellation leaves the current notebook usable.
  In-place replacement is not required for this release.

**Exit:** a nontechnical user can import a folder, export a note/library, make a
backup, inspect it and restore/open it without CLI commands. Kill/fault tests
cover staging/publication boundaries, permissions/full disk, corrupt assets,
cancel/retry and library switching. Qt fixtures round-trip with content and
attachment hashes preserved; documented lossy exports surface their limitations.

### R4 — Polish and harden an installable candidate

**Primary code:** all shipped UI paths; `packaging/`, installer and CI.

- Review the actual visual state matrix below. Refine editor chrome, hierarchy,
  spacing, long paths/titles, empty states, selection, focus, disabled states and
  errors against the generated reference. Remove unnecessary controls from the
  writing surface while keeping every shipped action discoverable.
- Complete native portal open/save/cancel/error checks, attachment viewer launch,
  cross-app clipboard behavior, installed launcher/activation and capture tests,
  scaling, suspend/resume, minimize/restore and theme/font coverage on the supported
  platform. Never claim X11 or another compositor from Wayland-only evidence.
- Complete fault/recovery rehearsals: killed writer, interrupted media import,
  read-only/full disk, busy/corrupt database, unsupported future schema, damaged
  preferences and interrupted backup/restore/migration. Acknowledged content must
  survive; failures must not falsely report success or destroy recoverable work.
- Build the separate Rust package from an immutable Git revision. Audit runtime
  libraries and dependency/font/icon licenses/notices. Update edition-aware
  release preparation, package validation, desktop actions, AppStream, man page,
  version/help, source-build instructions, checksums and the Rust installer route.
  Do not weaken the current installer's package-identity validation to accept
  arbitrary package names.
- Test clean install, preview-to-stable transition, upgrade, reinstall, uninstall
  with data retained and rollback in disposable environments. Verify CLI capture
  without a display, desktop launches and coexistence with Qt. Builds must not
  depend on the developer's cached checkout or untracked files.

**Exit:** a tagged candidate installs reproducibly, all automated gates pass on
its exact source revision, and evidence covers its advertised platform/features.
Only documented minor defects can remain; unsupported core workflows are blockers.

### R5 — Prove the product is ready to use

1. Run the first-use study with at least five people. At least four must complete
   each task without hints: create/format, organize, search/reopen, open an
   attachment, recover a deleted note, and create/restore a backup. Record actual
   observations, time/friction and accessibility issues; do not invent participants.
2. Conduct five working days of actual use of the installed candidate. Include
   repeated main/Quick handoff, notes with media, imports/exports, backup and
   suspend/resume. Record crashes, confusing behavior and recurring annoyances.
3. Score composition, typography, spacing, color/contrast, feedback and state
   completeness from 1–5. Require at least 4 in each, with the product owner's
   assessment of the reference comparison. Correct material deviations.
4. Fix blockers and retest their affected paths. A material change to saving,
   migration, input or ownership requires a fresh relevant trial; unchanged areas
   may retain dated evidence. The final candidate must be identifiable throughout.

**Exit:** no open data-loss, false-save, crash-in-core-workflow, keyboard-trap,
unusable accessible editing, serious performance or severe visual issue. Repeated
everyday friction has been resolved. Human evidence is real and recorded.

### R6 — Release the tested Rust edition

- Freeze the source revision and align workspace/package/application versions.
  Run the required checks against that exact tree and verify every distributed
  artifact's source hash and checksums. Tag it `rust-v1.0.0` only after the gates
  pass; do not reuse or move the Qt release tag.
- Publish accurate Rust-specific installation, scope, migration and rollback
  instructions with current screenshots and known limitations. Keep deferred
  features visible in the roadmap without advertising them as implemented.
- For Qt imports, use copies and verified backups; never silently open/upgrade
  the original Qt library. Schema-3 quotation libraries require Rust. Returning
  to Qt uses the pre-upgrade backup and does not include later Rust edits; state
  that limit plainly and offer export/backup before rollback.
- Retain the Qt download and existing default installer route. Any later change
  to make Rust the default must satisfy the full replacement decision, including
  the fate of deferred features. Separate-edition v1 success is not full-port
  completion.

**Exit:** the published package matches the tested candidate, installation and
recovery instructions are verified, and users can identify which edition and
library they are running.

## Beauty, delight and performance gates

Every feature increment includes a visual/interaction correction pass. **A working
but visually unfinished flow is not done.** Use semantic colors, the established
serif/sans hierarchy, restrained accents and opaque surfaces. Preserve caret,
selection and reading position across ordinary changes. Routine saves stay quiet;
destructive actions and failures explain consequences and recovery. Reduced motion
must be respected; adding decorative animation is not part of this push.

Review native behavior and real-font GPU artifacts at 1440 × 960, 1024 × 768,
640 × 820 and 420 × 420 logical pixels, in graphite, warm paper and representative
Omarchy themes, with larger text and 125%/150% scaling. Include:

- Populated/empty library, blank/long note, long titles/paths, focus mode and Quick Note.
- Text/selection/formatting, lists/checks, existing merged tables, quotes, media
  loading/missing/error, links and keyboard focus.
- Search loading/results/no matches/error; collapsed/offscreen folders; trash,
  restore, destructive confirmation and Undo.
- Save pending/failure/conflict, import warnings, export/backup/restore progress,
  cancellation/failure and Settings. No unstyled fallback or unexplained blank pane.

Retain the [rebuild performance budgets](rust-gpui-rebuild-plan.md#9-performance-budgets):

| Measure | Gate and minimum sampling |
|---|---|
| Warm startup / first launch | p95 < 500 ms over 20 warm launches; first launch < 1.5 s with cache conditions stated |
| Typical typing/selection | Input handler → frame submission p95 < 16 ms, p99 < 50 ms across at least 1,000 edits |
| Note switching | Cached ordinary note p95 < 100 ms over 100 switches |
| Large note | 3,000 paragraphs / 20 tables: editable viewport < 500 ms; edit p95 < 33 ms; no input stall > 100 ms in the defined run |
| Library search | p95 < 150 ms including debounce/render over 200 queries against 10,000 notes |
| Scrolling | Frame CPU p95 < 8 ms in a scripted large-list/document run |
| Memory/idle | R1's recorded byte/cache/RSS budgets pass; no monotonic retained entity/task growth over 100 open/close cycles; no unnecessary idle repainting |

Record build revision, hardware/GPU, compositor, scale, fixture and method.
Exercise indexing/import/housekeeping while typing, long single paragraphs/URLs,
wide tables, many images and a 1 GB attachment corpus. Folder-row boundedness is
not a document-performance pass. Frame submission is not display-presentation
latency; warm-cache launch is not cold boot. Missed targets require a measured
fix or an explicit revision to the release contract, never a silent test change.

## Evidence and release checklist

Maintain `llm-docs/rust-v1-release-evidence.md` during implementation, with a row
per gate: status, source revision, command/manual procedure, fixture/platform,
result/artifact and unresolved defect. Use **measured**, **observed**, **failed**
or **unverified**; a required unverified item blocks release. Keep test logs and
shareable screenshots free of production notes and unrelated desktop content.

- [x] V1-01 — R0 checkpoint is committed, reproducible and backed up on the remote.
- [x] V1-02 — Scope, platform, edition identity and preview transition are fixed.
- [ ] V1-03 — Existing writing/capture/media workflows and real IME/Orca pass.
- [ ] V1-04 — Keyboard organization, FTS search and trash/recovery pass end to end.
- [ ] V1-05 — Import/export, in-app backup/restore and library switching pass.
- [ ] V1-06 — Performance, memory, failure/crash and migration/rollback gates pass.
- [ ] V1-07 — Included visual states, focus/scaling and native integrations pass.
- [ ] V1-08 — License/runtime audit and installed-package/installer matrix pass.
- [ ] V1-09 — First-use study, product-owner visual review and five-day trial pass.
- [ ] V1-10 — Exact release artifacts, documentation, tag and checksums agree.

For each final candidate run formatting, workspace tests and all-target/all-feature
Clippy with warnings denied; the applicable GPU/native, Qt interoperability,
fault, interchange, installer and package checks; and hosted CI on the committed
revision. Re-run affected checks after changes rather than repeatedly running
unrelated suites. Keep Qt validation while Qt is supported.

## Execution discipline and first steps

1. Start with R0: preserve the currently uncommitted application and verify the
   checkpoint. Add the evidence ledger and freeze the scoped backlog.
2. Run R1's real input/accessibility checks and first full-library/editor profile
   before expanding the feature set. Let those results set the implementation
   estimate and identify any critical framework work.
3. Deliver R2 in reviewable increments: keyboard folders/sorting, FTS, then
   deletion/recovery/duplication. Commit each increment with its tests and visual
   evidence, then complete R3 and converge on the installed candidate.

Each increment must contain working behavior, meaningful failure coverage,
polished UI, updated documentation and a checkpoint commit. Use pure core rules,
one bounded writer, tagged asynchronous results, model-owned selection and no
I/O/parsing in render, as specified by the
[GPUI skill](gpui-best-practices-skill.md). Capture native evidence only from owned
test windows and temporary libraries; stop synthetic input if focus changes.

Add new features only when required for a frozen core workflow or a demonstrated
release blocker. Otherwise place them in the full rebuild backlog. The push ends
when every V1 gate is supported by evidence, not when a calendar date arrives or
the screenshots look finished.
