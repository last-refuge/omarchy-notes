# 0005 — Qt-free PDF export with Cairo and Pango

Status: accepted for the bounded M2 PDF increment, 12 October 2026.

## Decision

Add `notes-export`, a GPUI-free Rust crate depending on the existing document and
read-only media APIs. Use cairo-rs, pango and pangocairo 0.21.5 (locked) over the
system Cairo/Pango libraries. Require Cairo 1.16+ and Pango 1.50+. The backend
runs off the foreground thread and does not invoke Qt, a browser, a print server
or a network service. The desktop's default viewer opens only on **Open PDF**.

Pango provides shaping, bidirectional layout, fallback and measured line geometry;
Cairo produces selectable text with embedded fonts and PDF link annotations.
The implementation uses local pinned wrapper sources and native Pango API
documentation. In particular, `LayoutLine::x_ranges` uses coordinates relative
to the complete layout, so link rectangles subtract the line alignment offset.
This matters for RTL text. Rust wrappers are MIT; native Cairo is dual
LGPL-2.1-only/MPL-1.1, Pango is LGPL-2.0-or-later and GLib is LGPL-2.1-or-later. They are dynamically linked. The complete
native dependency/license inventory and package audit remain release gates.

## Appearance and pagination

White paper, 54-point horizontal margins, ink-colored Noto Serif prose,
Noto Sans metadata, a small gold header rule and understated page numbering
make export feel like the writing surface. Paper is US Letter or A4. Typeface
fallback comes from installed system fonts, including language/emoji fonts;
the GPUI-bundled assets do not register fonts with Pango. Missing glyphs fail
explicitly. Font variations can change wrapping/page counts across machines.

The engine lays out a document before painting, retaining shaped lines and bounded
image surfaces. It groups overlapping items (such as a list marker and its text,
or a file label and its box), so page breaks never cut through them. Paragraphs
split on visual lines, keep final lines together, and keep headings with following
content where it fits on a page. Tables preserve cell spans, keep connected
vertical merges together, repeat independent first rows, and split long unmerged
body rows at whole lines/objects. Nested tables, overlapping cells, more than
eight columns, oversized headers or oversized vertically merged groups fail with
specific guidance. This is bounded support, not full Qt print parity.

PNG/JPEG/WebP/GIF images are hash-verified and decoded with storage's dimension
and allocation limits; export scales proportionally and downsamples to at most
2,400 × 2,400. GIF uses its first frame. Attachments are verified and printed as
filename/type/size labels, with an explicit contents-not-included caption.
They are never embedded or opened. HTTP(S)/mailto links have PDF annotations;
internal note links and unsupported schemes print their visible labels only.
No remote resource is fetched. Source document JSON, blobs and database are
unchanged. PDF/A and a complete tagged accessible PDF are not claimed.

## Completion and failure contract

The UI takes an immutable snapshot identified by note ID, local revision and
operation ID **before** the save picker. It blocks only uncommitted IME text or
pending file insertion. Autosave and editing continue independently. Results
match the operation, name the original note even after navigation, and mention
newer edits. Choosing/cancelling a path never changes the source note. Ordinary
close waits for export; dropping the UI requests cancellation of remaining work.

Write to a private sibling temporary file (0600), check Cairo's finish/stream
errors, sync the file, check cancellation, then publish without replacing any
existing path and sync the destination directory. A failure before publication
removes temporary output and leaves the destination untouched. A directory-sync
failure explicitly says the complete PDF exists but durability is uncertain.
Cancellation is cooperative at layout/paint boundaries and immediately before
publication; a file published before a late cancellation is correctly reported
as successful. The UI never marks a note saved merely because PDF export worked.

Resource bounds: 4 MiB text, 512 KiB per paragraph, 10,000 leaf paragraphs,
100,000 spans, 500 pages, 64 MiB retained decoded image data, 128 MiB output.
These bounds exclude transient decoder allocations and shaping internals; they
are not a measured process-memory or latency budget.

## Verification

Core tests cover paragraph bounds, merged-cell geometry, unsupported nested/
large merged tables, cancellation, missing assets/glyphs, private complete output,
no overwrite and source preservation. GPUI tests simulate the platform save
picker and exercise cancellation, composition guards, newer edits, navigation,
stale results, existing-file failure and the close guard. The real-font GPU
harness reviews the export sheet in both appearances and at narrow widths,
including success/error states. Its test platform does not prove a real portal.

`tests/test-rust-pdf.py` generates synthetic multi-page PDFs and uses qpdf and
Poppler to verify syntax, embedded fonts/Unicode mappings, selectable text,
image inclusion, exact long-row/paragraph tokens, repeated headers, page dimensions,
link targets (including punctuation and RTL geometry) and printable bounds.
Rendered pages must also be visually reviewed; extracted text alone cannot prove
layout quality. See the dated evidence in `llm-docs/gpui-rebuild-baseline.md`.
