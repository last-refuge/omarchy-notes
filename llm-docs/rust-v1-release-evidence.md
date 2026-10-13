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
| V1-01 checkpoint | unverified | R0 checkpoint; commands below | Original Rust workspace, fixtures and relevant docs inventoried; commit/remote reproduction pending. |
| V1-02 identity/scope | unverified | Contract above; edition audit | Naming frozen; executable transition, package and opt-in installer pending. |
| V1-03 writing/capture/IME/Orca | unverified | Workspace and existing GPU/native suites | Existing preview evidence is a baseline. Real composing-engine and Orca core-task acceptance still required. |
| V1-04 organization/search/recovery | unverified | R2 | Keyboard tree, sort, SQLite FTS queries, duplicate and trash UI pending. |
| V1-05 portability | unverified | R3 | Existing verified storage backup primitives; in-app import/export/backup/restore/open pending. |
| V1-06 performance/fault/rollback | unverified | R1/R4 | Full-library/large-note profile and byte budgets pending; existing fault tests are partial evidence. |
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
| B01 / R1 | Open a library with 10,000 notes; `Writing::new_live` currently creates an editor for each. | Bounded active/cached editor entities; 100 open/close cycles show no retained growth and dirty work is never evicted. |
| B02 / R1 | Edit a large note through 100 distinct undo snapshots. | Explicit byte bound as well as count bound; predictable recent undo and documented history limit. |
| B03 / R1 | Real CJK composition and Orca editing in main/Quick windows. | Real candidate geometry, commit/cancel/selection/undo and accessible core tasks pass. Host currently lacks a verified running composing engine and Orca environment. |
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
