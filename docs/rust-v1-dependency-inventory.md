# Rust candidate dependency notices

This is a source inventory for the Rust **1.0.0** source. Release was directed by the owner on
13 October 2026; the inventory does not turn unverified checks into passes. The locked Linux normal/build graph has **523 packages**, and the
collector currently copies **927 license/notice and publisher-evidence files**.
**Strict source-text collection now passes with zero missing entries.** Legal
review, final artifact/runtime inspection, and clean installation remain open
gates.

The machine-readable summary in `rust-v1-dependency-inventory.json` records the
exact Cargo.lock SHA-256, declaration counts, resolved notice provenance, and
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
inventory. Its JSON retains `collection_complete: false` when gaps exist, plus publisher
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

## Text provenance and resolved omissions

Packaged LICENSE/COPYING/NOTICE/copyright files are copied unchanged from each
exact crate. Local workspace crates may use the repository's MIT text. Pinned
Git workspace license links must resolve inside that source workspace; shared
generic licenses require a matching declaring expression. In particular,
GPUI's Apache crates use their Apache text, not Zed's GPL editor license.

Twenty-two published crate archives omitted full notices. The 35 checked-in
supplements in `packaging/rust/license-texts/` preserve exact source texts,
publisher declarations and canonical authority texts. The companion
`license-supplements.json` distinguishes those origins and binds each to its
SHA-256, primary URL, applicable version, Cargo declaration and locked archive
checksum. A mismatched revision, archive identity, publisher binding or text
hash fails collection. The two XIM subcrates use the MIT license present in
their exact pinned Git workspace.

| Package | Publisher declaration | Verified resolution |
| --- | --- | --- |
| hexf-parse 0.2.1 | CC0-1.0 | Its exact published manifest matches the primary revision's `parse/Cargo.toml` byte for byte. That declaration is preserved beside the unmodified official Creative Commons CC0 1.0 legal text. A later upstream change to 0BSD is not applied to this version. |
| pathfinder_geometry 0.5.1 | MIT/Apache-2.0 | The complete unmodified `src/lib.rs` copyright/licensing notice from the checksum-bound crate is preserved beside the official Apache 2.0 text at the URL that notice identifies. The unavailable historical Git object is not presented as a fetched license file. |
| seahash 4.1.0 | MIT | Upstream's immediate next commit adds only the missing MIT `LICENSE`. Primary commit/diff evidence verifies its sole parent is the exact published revision and that the copied text equals the added lines; no source change or copyright attribution is invented. |
| zune-inflate 0.2.54 | MIT OR Apache-2.0 OR Zlib | The unmodified `LICENSE-ZLIB` from the exact published VCS revision is included. This documents the Zlib alternative; it does not claim all three alternative texts were found. |

The generated inventory includes each package's publisher declaration,
archive checksum, manifest hashes, repository/homepage, authors and VCS revision.
Publisher declarations and copyright notices are copied unchanged. Canonical
license texts explicitly identify their authority/version and exact source
binding; they are not falsely labeled files found in the archive. Author
metadata is not converted into invented copyright holders or years.

The source bindings are [hexf's exact declaration](https://github.com/lifthrasiir/hexf/blob/4225763d744183d720f575ae96d04161b4d08ea0/parse/Cargo.toml),
the [official CC0 legal text](https://creativecommons.org/publicdomain/zero/1.0/legalcode.txt),
Pathfinder's published crate notice and the [official Apache text](https://www.apache.org/licenses/LICENSE-2.0.txt),
the [SeaHash notice-only correction](https://gitlab.redox-os.org/redox-os/seahash/-/commit/3088c5c912b70b586d27bf553fbe964e025a2c89),
and [Zune's exact Zlib text](https://github.com/etemesi254/zune-image/blob/69502ce83fdfecdd0beefd677e2abb3781b29d98/LICENSE-ZLIB).

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

Eight collector regressions pass: normal/build graph selection and deterministic
output, missing-text reporting and overwrite refusal, bounded source symlinks,
exact supplement identity/hash enforcement, shared Git license declaration
matching, explicit candidate versus default/strict failure behavior, exact
canonical publisher/authority bindings, and notice-only successor parent/path/
diff verification. Changed publisher declarations, wrong license standards,
unrelated parents, code-changing successors and mismatched notice additions
fail before output creation. The actual locked selected graph passes
`--require-complete` with zero gaps. No app build, native input, settings change
or audio operation was performed for this collection.
