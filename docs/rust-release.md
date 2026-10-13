# Building and installing the separate Rust edition

The Rust workspace is still **0.1.0, early access**. These tools prepare a
candidate; they do not declare v1 ready, publish an artifact or install anything
on their own. The release evidence ledger retains the native, compatibility,
performance, license, installation, accessibility and owner visual acceptance
gates. The owner moved the five-person first-use study and five-working-day trial
to follow-up after release on 13 October 2026; neither has recorded results yet.

Rust uses `omarchy-notes-rust`, desktop identity `org.omarchy.Notes.Rust`, and
eventually the separate `rust-v1.0.0` tag. Qt remains the default installer route
and its `omarchy-notes` package remains available. Rust has no `provides` or
`conflicts` relationship with Qt.

## Candidate source and package

Commit the reviewed increment first. Preparation archives an exact committed
revision, excluding dirty and untracked files, and refuses an existing output
directory. Use a new directory per candidate:

```sh
python3 packaging/prepare-release.py --edition rust --candidate \
  --ref COMMIT_SHA --output build-rust-candidate
cd build-rust-candidate/aur
makepkg --syncdeps --cleanbuild
cd ../..
tests/check-rust-arch-package.sh build-rust-candidate/aur
```

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

The recipe installs the app, preview compatibility launcher, Rust desktop
actions, distinct icon, AppStream metadata and man page. It also installs the
repository, bundled asset/font, GPUI and vendored AccessKit license texts.
It collects declaration/provenance and packaged license/notice texts from the
locked Linux normal/build dependency closure; missing declarations or texts
refuse stable packaging. A local archive explicitly marked `candidate` may
retain documented source-text gaps in its installed inventory; this is not
permission to publish a stable package or claim license acceptance. That
inventory includes manual-review limits and is installed
under `share/licenses/omarchy-notes-rust/dependencies`.
The complete dependency redistribution/runtime audit remains a final gate;
this list does not claim that audit is complete.

## Published installer route

After a Rust release is actually published, download and inspect its installer
before running it. Select Rust explicitly:

```sh
bash install.sh --edition rust --version rust-vVERSION --dry-run
bash install.sh --edition rust --version rust-vVERSION --no-quick-note
```

Without `--edition rust`, the installer selects Qt. Its latest-release lookup
filters published, non-draft, non-prerelease tags by the chosen edition; if no
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

## Quick Note and coexistence

Choose **Rust Quick Note** from the Rust desktop launcher's actions, run
`omarchy-notes-rust --quick-note`, or use Ctrl+Shift+N inside Rust. Qt and Rust
must use distinct global shortcut keys. For a compositor binding, choose an
unused key such as Super+Alt+Shift+N and execute the absolute Rust command above;
use `org.omarchy.Notes.Rust` when matching its windows. Keep the existing Qt
binding intact. Follow the configuration format used by your installed Omarchy
version; Lua and native Hyprland configuration are not interchangeable.

Rust does not currently implement automatic `--setup-hyprland`. The installer's
Rust `--quick-note` option explains the desktop/manual route and makes no
binding changes; it never invokes Qt's setup. Installed launcher, portal,
floating-rule and shortcut behavior still require the supported-platform matrix.

## Recovery and release freeze

Use copies and verified backups when bringing Qt libraries into Rust. Enabling
quotations creates schema-3 data requiring Rust; returning to Qt uses the
pre-upgrade backup and excludes later Rust edits. Preserve the newer Rust library
and export/backup before rollback. Restore into a new verified directory rather
than replacing the current notebook. Portable Markdown/HTML files with assets
are useful copies, not lossless replacements for library backups.

At the final freeze, align workspace, man page, AppStream and release versions,
record every required gate on that exact revision, and use `rust-vVERSION`.
Stable artifact preparation requires that exact Rust tag and refuses Qt tags.
The Rust release workflow creates a draft only; reviewing or publishing it and
changing the default edition are separate decisions. Never replace an existing
published archive or reuse Qt's tag.
