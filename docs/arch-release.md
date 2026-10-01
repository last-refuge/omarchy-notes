# Releasing Omarchy Notes

Distribution targets the [Omarchy package repository](https://github.com/omacom/omarchy-pkgs).
Omarchy maintains and builds a source recipe, signs the package, and publishes it to
its channels. AUR publication is outside this release process.

`packaging/arch/PKGBUILD.in` is a template. Release preparation generates a complete
`arch/PKGBUILD`, `.SRCINFO`, and `.omarchy/package.json` alongside an immutable source
archive. `.SRCINFO` describes the Arch recipe; it does not imply an AUR submission.

## Prepare and validate a release

Update the application version in `CMakeLists.txt`, `packaging/arch/PKGBUILD.in`,
`packaging/metainfo/org.omarchy.Notes.metainfo.xml`, `packaging/man/omarchy-notes.1`,
and `CHANGELOG.md`. Reset `pkgrel` to 1 for a new application version; increase it
when repackaging the same published source.

```bash
./tests/test-release-metadata.sh
./tests/test-package-install.sh
cmake --build build
ctest --test-dir build --output-on-failure
```

Commit first: preparation archives a committed revision, not the working tree.
It never creates tags, pushes, or publishes anything.

```bash
python3 packaging/prepare-release.py --ref HEAD --output build-arch-release
cd build-arch-release/arch
makepkg --cleanbuild
../../tests/check-arch-package.sh .
```

CI builds this recipe against both Omarchy's stable Arch snapshot and its edge
mirror. It synchronizes the entire container to the selected snapshot, including
downgrades, before installing tools or dependencies. Initial validation targets
x86_64. Add aarch64 to the recipe only after validating it against its target dependencies.

`tests/test-package-lifecycle.sh` runs **inside a disposable container as root**,
with an unprivileged `builder` account. It installs the checksum-verified 1.0.0-1
release, creates notes and attachments through the installed app, upgrades to the
new package, removes it, and reinstalls it. It verifies the note contents, attachment
hashes, and database integrity at each stage. It refuses a host environment or an
existing installation. CI runs it automatically; do not run it on your workstation.

## Publish immutable source artifacts

Tag a new source version and push the tag. The Release workflow creates a **draft**
GitHub release with:

- `omarchy-notes-VERSION.tar.gz`, the source archive downstream recipes download;
- `omarchy-notes-VERSION-arch.tar.gz`, containing the generated Arch recipe and
  Omarchy metadata;
- an x86_64 package, `SHA256SUMS`, and `SOURCE_REVISION`.

Review the checks and draft before publishing. Never replace an already published
source archive or move its tag. Any source change, including documentation and build
scripts embedded in the archive, needs a new source version. A downstream recipe-only
change may retain the existing source archive and bump `pkgrel` instead.

## Maintain the Omarchy recipe

The upstream files are:

```text
pkgbuilds/omarchy-notes/PKGBUILD
pkgbuilds/omarchy-notes/.omarchy/package.json
```

For the initial contribution, derive the recipe from this project's template but
hash the **already published** source archive. Do not use the checksum of a new
`HEAD` archive with the old release URL. Verify it against that release's
`SHA256SUMS`, then insert its BLAKE2 checksum in the recipe. Include the metadata
from `packaging/omarchy/package.json`. Commit only the recipe and metadata upstream,
not source archives, build output, or the template.

The metadata watches final `vMAJOR.MINOR.PATCH` releases on
`last-refuge/omarchy-notes`. Omarchy's watch excludes drafts and prereleases and
updates versions and checksums while preserving its maintained recipe. The
`rebuild_on` entries request rebuilds after Qt library changes.

From an upstream working checkout:

```bash
python helpers/upstream-watch.py check pkgbuilds/omarchy-notes
python tests/upstream-watch.py
./tests/pinned-sources.sh
bin/build --mirror edge --arch x86_64 --package omarchy-notes
```

The last command uses the upstream container builder and produces unsigned build
artifacts; it does not publish. Follow current upstream contribution and testing
instructions. Omarchy maintainers own signing, publishing, and channel policy.
Default local-package metadata enters edge; a merge does not guarantee stable
availability. Stable and RC publication follow the maintainers' channel process.

Once the package is published to stable, verify it appears under **Install → Package**,
installs with `omarchy pkg add omarchy-notes`, and opens from the app launcher. Then
change the README and website to lead with repository installation, retaining the
GitHub download as a fallback. A subsequent published app release should be detected
by the watch and reach installed users through `omarchy update`.

## Desktop integration

The package owns the desktop entry, icon, and New Note and Quick Note actions.
It does not change user configuration. `omarchy-notes --setup-hyprland` remains
an optional, explicit action for Omarchy's Lua-based Hyprland configuration.

`tools/capture-screenshots.sh BUILD_DIR` regenerates website and README images
from the real app with the sample library in stock Omarchy themes.
