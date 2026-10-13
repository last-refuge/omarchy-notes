# Rust candidate dependency notices

This is a source inventory for the local **0.1.0 candidate**, not approval to
publish v1.0. The locked Linux normal/build graph has **523 packages**, and the
collector currently copies **921 license/notice files**. **Four packages still
lack complete source license texts.** Legal review, final artifact/runtime
inspection, and clean installation remain open gates.

The machine-readable summary in `rust-v1-dependency-inventory.json` records the
exact Cargo.lock SHA-256, declaration counts, unresolved package evidence, and
bundled asset hashes. Counts are from the selected x86_64 Linux package graph on
2026-10-13. They must be regenerated when the lock, features or target changes.

## Reproduce the collection

After fetching the pinned dependencies, run from the workspace root with a new
output directory:

```sh
python3 packaging/rust/collect-licenses.py \
  --package omarchy-notes-preview --target x86_64-unknown-linux-gnu \
  --output target/dependency-licenses --candidate
```

`--candidate` explicitly permits testing a local candidate with an incomplete
inventory. Its JSON retains `collection_complete: false`, all gaps, publisher
evidence and a testing-only purpose; successful exit does not close licensing
or stable release acceptance. The recipe permits this mode only when the
committed archive's `RELEASE_KIND` is exactly `candidate`.
The collector independently refuses candidate mode for an existing archive
marker with any other value. An unmarked local checkout requires the explicit
`--candidate` option.

Default collection and `--require-complete` fail on any missing declaration or
text. The stable recipe uses that strict mode. Neither mode treats SPDX strings
as legal conclusions. Existing output directories are refused; no file is
overwritten. Collection makes no network requests.

Cargo metadata is read with `--locked --offline --filter-platform`; Cargo tree
selects the app's normal/build dependency graph. Dev-only dependencies are
excluded. Build dependencies remain in the inventory conservatively, so this
is not an exact list of compiled code inside the executable. Saved metadata
via `--metadata` can overinclude workspace-unified features and says so in its
inventory. Ambiguous package source identities fail rather than guess.

## Text provenance and remaining gaps

Packaged LICENSE/COPYING/NOTICE/copyright files are copied unchanged from each
exact crate. Local workspace crates may use the repository's MIT text. Pinned
Git workspace license links must resolve inside that source workspace; shared
generic licenses require a matching declaring expression. In particular,
GPUI's Apache crates use their Apache text, not Zed's GPL editor license.

Eighteen published crate archives omitted their upstream license texts. The
29 checked-in supplements in `packaging/rust/license-texts/` are unmodified
primary-source texts from each crate's published VCS revision. The companion
`license-supplements.json` binds every file to its SHA-256, upstream URL/revision,
exact package version, Cargo declaration and locked archive checksum. A
mismatched revision, archive identity or text hash fails collection. The two
XIM subcrates use the MIT license present in their exact pinned Git workspace.

| Package | Publisher declaration | Remaining source-text evidence |
| --- | --- | --- |
| hexf-parse 0.2.1 | CC0-1.0 | Exact archive and VCS revision are identified; no full license text was found in the published crate or its revision. Publisher author metadata is retained, not treated as copyright ownership. |
| pathfinder_geometry 0.5.1 | MIT/Apache-2.0 | `src/lib.rs` names the Pathfinder Project Developers and licensing URLs. The corresponding full texts are absent; the historical primary-source revision was not available from the attempted archive lookup. |
| seahash 4.1.0 | MIT | Exact archive, revision and author metadata are retained. The source archive omits the full text, and the primary upstream endpoint was unavailable during this audit. |
| zune-inflate 0.2.54 | MIT OR Apache-2.0 OR Zlib | Exact archive and revision are retained. Its source header gives a copyright year and alternatives; a full exact-revision license supplement still requires review and incorporation. |

The generated inventory includes each missing package's publisher declaration,
archive checksum, manifest hashes, repository/homepage, authors and VCS revision.
Literal copyright excerpts are recorded with source path, line and file hash
when present. Empty excerpt lists do not establish the absence of a rights
holder. Generic replacement texts are not fabricated to make the gate pass.

## Manual review and assets

Review alternative expressions and NOTICE/copyright requirements before
redistribution. The graph includes Unicode-3.0 declarations, `option-ext` under
MPL-2.0, LLVM exceptions, combined BSD/Unicode expressions, and `libbz2-rs-sys`
under bzip2-1.0.6. `self_cell` declares Apache-2.0 **or** GPL-2.0-only; recording
both does not choose the GPL alternative. Slash-separated historical publisher
declarations are preserved verbatim rather than silently normalized.

The app embeds five unmodified Noto Sans/Serif fonts and project SVG line icons.
Their existing provenance is in `crates/notes-ui/assets/NOTICE.md`, with the
Noto copyright/OFL text and GPUI Apache guidance text alongside it. CJK and emoji
fallback fonts are system dependencies, not embedded font files. The package
must retain the repository MIT notice, bundled asset notice, OFL, GPUI license,
and both vendored AccessKit texts/provenance. The vendored AccessKit patch keeps
its original source identity and documents the local marshaling correction.

The recipe installs dependency texts and their inventory under
`/usr/share/licenses/omarchy-notes-rust/dependencies`, readable by ordinary users.
Final review must compare that installed inventory with the exact packaged
binary, native vendored sources, dynamically loaded system libraries, asset
hashes and declared runtime dependencies. This source-only pass does not prove
that a clean machine can launch the app or that every redistribution obligation
has been met.

## Focused verification

Six collector regressions pass: normal/build graph selection and deterministic
output, missing-text reporting and overwrite refusal, bounded source symlinks,
exact supplement identity/hash enforcement, shared Git license declaration
matching, and explicit candidate versus default/strict failure behavior.
The actual locked selected graph produced the local candidate inventory in
0.89 seconds, retaining all four gaps. No app build, native input, settings
change or audio operation was performed for this collection.
