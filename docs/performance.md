# Performance

Release gate results for Omarchy Notes 1.0.0, from `tests/bench_library` (build it, then run
`./tests/bench_library 10000 1024` in the build directory).

The benchmark builds a library of 10,000 generated notes (3–15 paragraphs each, with
checklists and #tags mixed in) and 1 GB of image attachments. It then measures storage and
search in-process, and times six launches of the real app binary to its first rendered frame.

| Measure | Target | Result |
|---|---|---|
| Notes in library | 10,000 | 10,000 |
| Attachment data | 1 GB | 1 GB |
| First launch to first frame | < 1.5 s | 168 ms |
| Warm launch to first frame (median of 5) | < 500 ms | 160 ms |
| List all notes | | 15 ms |
| Sidebar data (folders, tags, Smart Folders, attachments) | | 4 ms |
| Search p50 (200 queries) | | 23 ms |
| Search p95 (200 queries) | < 150 ms | 28 ms |
| Worst keystroke in a 3,000-paragraph note with 20 tables | < 50 ms | 44 ms |

The last row comes from `tst_editor` (`largeNoteResponsiveness`).

## Reference machine

Intel Core i9-14900HX (32 threads), 32 GB RAM, Samsung NVMe SSD, Arch Linux with Qt 6.11.2.

## Caveats

- Launch timings use Qt's offscreen platform with the software renderer. On a real Wayland
  session the GPU renderer and compositor add or remove a little time; measure there before
  quoting numbers for other hardware.
- "First launch" runs with the operating system's file cache already warm from building the
  fixture. A truly cold start after a reboot will be slower, mostly waiting on the disk.
- Very long notes are the heaviest case. Opening a 3,000-paragraph note takes about 0.9 s
  (text layout). Typical notes open instantly.

## What changed to get here

Daily housekeeping (purging Recently Deleted and removing unused attachment data) reads every
note. It used to run at startup and delayed the first list query by about 150 ms on this
library. It now runs 30 seconds after launch, at most once a day, on the storage thread.
