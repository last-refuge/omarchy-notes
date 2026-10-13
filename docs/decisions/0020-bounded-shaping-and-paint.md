# Bounded root shaping and masked painting

Status: implemented for the Rust R1 increment; release performance and native
accessibility acceptance require dated measurements of the final candidate.

The editor keeps complete document geometry for caret movement, wrapped
Home/End, hit testing, find, selection, merged tables and accessibility. Painting
skips lines, media, checks, borders, quotes and labels outside the current GPUI
content mask. Offscreen model content and accessible text are retained.

Each root block has one reusable shape entry keyed by stable block ID and exact
block content. Each root retains up to eight recent width variants because GPUI
negotiates several widths while measuring a view. Theme and writing font/size
form the cache configuration; changing them invalidates the entries. An edit
reshapes its changed root at the required widths and
repositions the unchanged shapes with current global text offsets. Ordered-list
numbers are computed from current root order separately from text shaping.
Editing a table reshapes that table root, including its cells and merged spans.

The cache retains at most 4,096 distinct roots, eight width variants per root and
64 MiB **estimated** retained bytes shared across an editor's cache. Each root
shares one content clone and deduplicates unwrapped glyph allocation estimates
among its variants. Least-recently-used shapes leave the cache when a limit is reached;
roots removed from the document are discarded immediately at the next layout.
An entry larger than the estimated byte limit is shaped without being cached.
The active full layout still retains the geometry needed to operate the document.

The estimate combines serialized source size, exposed glyph/run/vector
capacities and allowance for block/map/decorations. It is not an exact allocator
measurement. GPUI shared text/font caches, native allocations and GPU raster
memory are excluded; process RSS and long-session measurements remain necessary.
The entry count is a hard bound on retained root identities, independently of
this byte estimate.

Repeated width probes also reuse the assembled full-document geometry until
the editor's existing `layout = None` invalidation starts a new generation.
Content, font/theme, window-rebinding and media layout invalidations clear those
variants before rebuilding. This cache has a separate eight-width and 8 MiB
estimated geometry budget; its accounting counts vectors, strings and reference
handles while excluding shared glyph data already held by root shapes. An
oversized active layout remains usable even when it cannot enter this cache.

`layout_diagnostics` reports root/width shape hits/misses, evictions, estimated
retained bytes, distinct root and variant counts and painted/total text lines.
A miss counts one root at one width; a table root may make several native shaping calls.
Separate complete-geometry hits/misses, variant count and estimated bytes show
whether scrolling avoids root walks. Optional performance diagnostics record
editor edit/render, geometry lookup/build, prepaint placement and masked paint
time, plus whether native accessibility is active. These are CPU regions, not
display-presentation timings, and omit asynchronous subscription effects from
the edit region.

The real-font `layout-cache-check` compares cached geometry with fresh shaping
after text, mark, check, list, merged-table, width, font and theme changes. It
checks shared unchanged glyph layouts, masked scrolling, complete offscreen
selection, count-limit eviction and removed-root cleanup. Composition tests keep
the caret visible after width/height and height-only viewport changes while
ordinary reading resizes keep their scroll offset. Glyph-ID-zero probes record
actual emoji, combining-mark, CJK and mixed RTL/LTR shaping evidence. The main editor,
table/media/quote/link harnesses and native AT-SPI checks remain separate
regressions. Headless rendering does not establish display-presentation latency,
real IME operation or Orca task completion.
