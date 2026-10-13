# 0017 — Bound editor undo and redo by allocated bytes

Date: 2026-10-13

## Decision

Each live editor retains at most **32 MiB of heap payload and 100 steps across
undo and redo together**. The limit includes the history container capacities,
every retained document's block/cell/span/map vector capacity, string capacity,
selection identities and typing attributes. History uses an owned compact tree:
maps become ordered vectors, preserving their values without whole-document JSON
serialization. Its allocation sizes can therefore be counted directly without
depending on the private allocation layout of Rust's `BTreeMap`.

`Editor::history_usage()` reports this allocation count and the undo/redo step
counts in constant time. `can_undo()` and `can_redo()` describe availability.
`limit_reached` becomes true when either limit truncates the history.

Keep the most recent contiguous undo/redo chain. Evict the oldest undo first,
then the most distant redo when only redo remains. A new edit discards redo.
Adjacent ordinary typing still coalesces within 650 ms. Undo/redo reserve no
unbounded alternate history: their combined budget applies when moving a state
between the two stacks, including when the document has grown during coalescing.

An individual state larger than the available 32 MiB cannot be retained. End both
history chains at that boundary; do not retain the oversized entry or leave
earlier steps that would make Undo silently skip the unrecorded edit. The edit
itself remains in the document and follows normal saving. Undo may successfully
restore a smaller prior state while the oversized current state makes Redo
unavailable. The writing UI must expose the limit when it is reached.

The current document, the temporary before-edit copy and **one active composition
baseline** are editing state outside this retained-history limit. The composition
baseline remains needed to cancel preedit and to keep uncommitted text out of
autosave. Committing composition creates one history step; cancelling creates
none. Allocator bookkeeping/resident pages, GPU memory and live UI entities are
also outside this count; RSS and entity/cache bounds require separate measurements.

The limit applies per live editor. Bounding the number of retained editor entities
is a separate requirement; this decision does not establish a whole-library RSS
budget or make full-document shaping sufficiently fast.

## Evidence

The deterministic fixture has 3,000 paragraphs, 20 tables of 4 × 4 cells and
1,323,319 bytes of plain UTF-8 text, including emoji and combining marks. The
probe uses a forwarding global allocator to count actual live allocation payload
and reads `/proc/self/status` for RSS. It verifies compact document accounting
against those allocation sizes before timing. Run:

```sh
cargo run --release -p notes-editor --example history-profile --locked
cargo test -p notes-core -p notes-editor --locked
```

Baseline at R0 source `10f392bb5d6075419b0e3e1549fe892d96b3deca`, using the existing
release libraries, on Intel Core Ultra X7 358H / x86_64 Linux / rustc 1.99.0:

| Model operation | Samples | p50 | p95 | p99 | Maximum |
| --- | ---: | ---: | ---: | ---: | ---: |
| Adjacent coalesced insert | 1,000 | 14.811 ms | 15.833 ms | 16.858 ms | 23.453 ms |
| Insert with distinct undo group | 125 | 15.150 ms | 15.633 ms | 15.864 ms | 16.011 ms |

The distinct history run increased live payload from 7,476,394 to 326,881,787 bytes
and RSS from 14,696 to 356,652 KiB. Undo moved that payload to redo. Dropping the
editor reduced live payload to 17,956 bytes while allocator residency left RSS at
103,564 KiB. Thus snapshot count alone did not bound retained bytes or RSS.

The baseline editor rlib SHA-256 was
`f7dc734d2b6b8205130b914f8a0cc7124e994673c9196354daf57f04113a8a92`;
the core rlib was
`d8bb544a73fb0b709e804a2bf6f1eff0e09c5044231d68d98087c7c9d44d8b2e`.

The post-change release probe on the implementation worktree measured the
following. Other agents were compiling/testing concurrently; timing must be
repeated under a stated controlled load before setting performance budgets.

| Model operation | Samples | p50 | p95 | p99 | Maximum |
| --- | ---: | ---: | ---: | ---: | ---: |
| Adjacent coalesced insert | 1,000 | 16.319 ms | 22.503 ms | 50.431 ms | 64.721 ms |
| Insert with distinct undo group | 125 | 18.049 ms | 22.387 ms | 25.783 ms | 31.822 ms |

The probe's compact snapshot owned exactly 2,665,670 bytes, matching the measured
allocator payload. After the distinct-edit run, combined retained history was
32,037,062 bytes (12 recent undo steps), total live allocation was 36,603,291 bytes
and RSS was 49,428 KiB. After Undo, history was 32,037,074 bytes (12 redo steps).
Dropping the editor left 17,956 live bytes and 49,072 KiB RSS. This demonstrates the
retained history bound and lower memory use; it does not establish an RSS ceiling.

Pure model CPU is **not** input-handler-to-frame-submission latency and does not
pass the native performance gate. The profiler excludes GPUI shaping, paint and
accessibility. The baseline used existing full-application release libraries;
the post-change run rebuilt the pure editor example's dependency graph, so timing
comparison also needs matching Cargo feature unification.

## Verification coverage

Tests exercise combined count/byte bounds through undo/redo, oldest-history
truncation, redo invalidation, oversized-history boundaries, growing coalesced
documents, composition cancellation and one-step composition commit. Core tests
round-trip nested unknown JSON properties, Unicode, table-cell structure and
spare string capacity without changing document values.
