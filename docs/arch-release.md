# Releasing Omarchy Notes

Releases follow the same flow as Omarchy Calendar. `packaging/arch/PKGBUILD.in` is a template,
not an AUR submission: release preparation generates `PKGBUILD` and `.SRCINFO` with the checksum
of the exact source archive users will download.

## 1. Prepare the version

Update the version in `CMakeLists.txt` (`project(omarchy-notes VERSION …)`),
`packaging/arch/PKGBUILD.in`, `packaging/metainfo/org.omarchy.Notes.metainfo.xml`,
`packaging/man/omarchy-notes.1` and `CHANGELOG.md`, then check them together:

```bash
shellcheck install.sh tests/test-installer.sh tests/test-installer-package.sh
./tests/test-installer.sh
./tests/test-release-metadata.sh
./tests/test-package-install.sh
```

## 2. Build and check the package locally (optional)

Commit first: the generator archives a committed revision, not the working tree. It never
creates tags, pushes, or publishes anything.

```bash
python3 packaging/prepare-release.py --ref HEAD --output build-aur-release
cd build-aur-release/aur && makepkg --cleanbuild && cd ../..
./tests/check-arch-package.sh build-aur-release/aur
```

For a clean-chroot build, use `extra-x86_64-build` instead of `makepkg` (needs `devtools`).
CI builds with `makepkg --syncdeps` in a fresh container, since a chroot can't run inside it.
CI also runs `tests/test-installer-package.sh` as the build user in that disposable
container: it checks a dry run, installs the real package through the installer,
and verifies a repeat run. Only release transport is replaced with local artifacts.

## 3. Tag and publish

```bash
git tag v1.0.0 && git push origin v1.0.0
```

The Release workflow checks the metadata, builds and validates the package, and creates a
**draft** GitHub release with:

- `omarchy-notes-VERSION.tar.gz`: the source archive the AUR package downloads
- `omarchy-notes-VERSION-aur.tar.gz`: `aur/PKGBUILD` and `aur/.SRCINFO`
- the built package, `SHA256SUMS` and `SOURCE_REVISION`
- `install.sh`, also covered by `SHA256SUMS`

Review the draft: confirm `install.sh` is attached and its checksum is present.
Publish it, then check the AUR files fetch the published archive:

```bash
tar xf omarchy-notes-1.0.0-aur.tar.gz && cd aur && makepkg --verifysource
```

Verify the installer in a disposable Arch system as an unprivileged user, using
the version just published:

```bash
curl -fsSL https://github.com/last-refuge/omarchy-notes/releases/latest/download/install.sh -o install.sh
less install.sh
bash install.sh --version vVERSION --dry-run
bash install.sh --version vVERSION --no-quick-note
```

Before the first release containing an installer, keep the README and website's
primary command pointing at the existing package. After publishing and checking
the installer asset, update both primary commands to:

```bash
curl -fsSL https://github.com/last-refuge/omarchy-notes/releases/latest/download/install.sh | bash
```

Keep the download-and-inspect instructions, direct package command, and CMake
build instructions available. The old v1.0.0 release does not contain an installer;
do not replace its published archives to add one.

## 4. Submit to the AUR

The first time, create the package on the AUR by pushing to a new repo:

```bash
git clone ssh://aur@aur.archlinux.org/omarchy-notes.git aur-omarchy-notes
cp aur/PKGBUILD aur/.SRCINFO aur-omarchy-notes/
cd aur-omarchy-notes
git add PKGBUILD .SRCINFO && git commit -m "omarchy-notes 1.0.0" && git push
```

Only commit `PKGBUILD` and `.SRCINFO`. Never upload the source archive, binaries or
`PKGBUILD.in`. Don't replace a published source archive in place: any change needs a new
version (or `pkgrel` bump) and freshly generated checksums.

## Screenshots

`tools/capture-screenshots.sh BUILD_DIR` regenerates the website and README images from the
real app with the sample library, in stock Omarchy themes.
