# Building and installing Omarchy Notes

Version **1.1.0** is published as `rust-v1.1.0`, adding live block formatting
and fixing caret movement on empty lines. Version **1.0.2** fixed Omarchy's
universal clipboard shortcuts and Up/Down navigation across paragraphs, and
version **1.0.1** fixed the renamed-package installer mismatch. Its checksummed
`omarchy-notes` package passed clean Arch build, metadata and install/remove
checks. Hosted workspace, PDF, storage and installer checks also passed.

The Rust **1.0.0** release was directed by the owner on 13 October 2026. Its
evidence ledger remains historical: completed checks do not certify 1.0.1.
Remaining native/package checks, the five-person first-use study and the
five-working-day trial remain follow-up; no unverified check is presented as passed.

The Arch package is now `omarchy-notes`; the binary remains `omarchy-notes-rust`.
This is a package-only rename: desktop identity `org.omarchy.Notes.Rust`,
profile/runtime paths and `rust-vVERSION` tags are unchanged. The new recipe
replaces and conflicts with the old `omarchy-notes-rust` package. Omarchy Notes
is Rust-only; Qt is retired and relevant only for legacy migration.

## Run from source

Install the toolchain in `rust-toolchain.toml` through rustup and the native
[development dependencies](gpui-preview.md#run-it). Do not run Cargo as root.

```sh
cargo run --release --locked -p omarchy-notes-preview --bin omarchy-notes-rust
cargo test --workspace --locked
```

The Cargo package name is historical; it does not change the app identity.

## Candidate source and package

Commit the reviewed increment first. Preparation archives an exact committed
revision, excluding dirty and untracked files, and refuses an existing output
directory. Use a new directory per candidate:

```sh
python3 packaging/prepare-release.py --candidate \
  --ref COMMIT_SHA --output build-rust-candidate
cd build-rust-candidate/aur
makepkg --syncdeps --cleanbuild
cd ../..
tests/check-rust-arch-package.sh build-rust-candidate/aur
```

New source archives are named `omarchy-notes-VERSION.tar.gz`, with a matching
`omarchy-notes-VERSION/` root; recipe archives are `omarchy-notes-VERSION-aur.tar.gz`.
The source archive and package carry `SOURCE_REVISION` and `RELEASE_KIND`.
Candidate artifacts say `candidate`; preparation refuses to treat unreleased
0.x sources as stable. Version remains the workspace version until the release
decision. Repeated preparation of one revision yields the same source/recipe
archives and checksums. The recipe reads the archived `rust-toolchain.toml`,
fetches the pinned locked dependencies into its own Cargo home, and builds
offline after fetching. A first build needs network access for that toolchain
and dependencies. It must not use a developer's cached checkout.

The advertised platform is x86_64 Arch/Omarchy with Wayland/Hyprland. Vulkan
rendering needs a working driver for the machine's GPU. A portal backend must
provide its file chooser; Omarchy commonly supplies Hyprland integration with
a GTK chooser. X11 and other architectures remain unadvertised until tested.

The recipe installs the app, preview compatibility launcher, desktop
actions, distinct icon, AppStream metadata and man page. It also installs the
repository, bundled asset/font, GPUI and vendored AccessKit license texts.
It collects declaration/provenance and packaged license/notice texts from the
locked Linux normal/build dependency closure; missing declarations or texts
refuse stable packaging. A local archive explicitly marked `candidate` may
retain documented source-text gaps in its installed inventory; this is not
permission to publish a stable package or claim license acceptance. That
inventory includes manual-review limits and is installed
under `share/licenses/omarchy-notes/dependencies` in newly named packages.
The complete dependency redistribution/runtime audit remains a final gate;
this list does not claim that audit is complete.

## Published installer route

After a release is actually published, download and inspect its installer
before running it. The current installer installs Omarchy Notes and needs no
edition selector. Older edition-aware installers require `--edition rust`.

Older published `rust-v1.0.0` artifacts still have the `omarchy-notes-rust`
package name. The new installer requires a newly named `omarchy-notes` release
package; it cannot install those old artifacts. No published release has been
modified. The published `rust-v1.0.1` release supplies the new package.

```sh
bash install.sh --version rust-v1.1.0 --dry-run
bash install.sh --version rust-v1.1.0 --no-quick-note
```

The installer selects Rust releases by default. Its latest-release lookup
filters published, non-draft, non-prerelease Rust tags; if no
match occurs in the newest 100 releases, choose an explicit tag. Tags, assets,
package metadata and the package-owned executable must all match that edition.
The installer requires Python 3 for this JSON lookup, along with its existing
Arch command-line prerequisites. It never infers Rust from a shared version or
silently falls back to a source build.

Rust retains `$XDG_DATA_HOME/omarchy-notes-gpui` and
`$XDG_CONFIG_HOME/omarchy-notes-gpui/preferences.json`, with the usual home
directory XDG fallbacks. Removing the package keeps notes and preferences.
Its installed `omarchy-notes-preview` launcher executes the stable-named Rust
binary with the same arguments and exit status. An older manual copy earlier
on `PATH` may take precedence; move that specific executable aside after
verifying the package-owned one. The installer reports such shadowing and does
not delete manual copies.

## Quick Note

Choose **Quick Note** from the **Omarchy Notes** desktop launcher's actions, run
`omarchy-notes-rust --quick-note`, or use Ctrl+Shift+N inside the app. For a
compositor binding, choose an unused key such as Super+Alt+Shift+N and execute
`/usr/bin/omarchy-notes-rust --quick-note`; use `org.omarchy.Notes.Rust` when
matching its windows. Do not overwrite a legacy binding during migration.
Follow the configuration format used by your installed Omarchy
version; Lua and native Hyprland configuration are not interchangeable.

Omarchy Notes does not currently implement automatic `--setup-hyprland`. The installer's
`--quick-note` option explains the desktop/manual route and makes no
binding changes; it never invokes Qt's setup. Installed launcher, portal,
floating-rule and shortcut behavior still require the supported-platform matrix.

## Recovery and release freeze

Use copies and verified backups when migrating retired Qt libraries into Rust;
see [migration](migrating.md). Enabling
quotations creates schema-3 data requiring Rust; returning to Qt uses the
pre-upgrade backup and excludes later Rust edits. Preserve the newer Rust library
and export/backup before rollback. Restore into a new verified directory rather
than replacing the current notebook. Portable Markdown/HTML files with assets
are useful copies, not lossless replacements for library backups.

At the final freeze, align workspace, man page, AppStream and release versions,
record every required gate on that exact revision, and use `rust-vVERSION`.
Stable artifact preparation requires that exact Rust tag and refuses Qt tags.
The Rust release workflow creates a draft only; reviewing and publishing it are
separate decisions. The package-only rename does not rename binaries or profiles.
Never replace an existing published archive or reuse a retired Qt tag.
