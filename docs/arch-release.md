# Releasing Omarchy Notes

Releases follow the same flow as Omarchy Calendar. `packaging/arch/PKGBUILD.in` is a template,
not an AUR submission: release preparation generates `PKGBUILD` and `.SRCINFO` with the checksum
of the exact source archive users will download.

## 1. Prepare the version

Update the version in `CMakeLists.txt` (`project(omarchy-notes VERSION …)`),
`packaging/arch/PKGBUILD.in`, `packaging/metainfo/org.omarchy.Notes.metainfo.xml`,
`packaging/man/omarchy-notes.1` and `CHANGELOG.md`, then check them together:

```bash
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

Use `extra-x86_64-build` instead of `makepkg` for a clean-chroot build, like CI does.

## 3. Tag and publish

```bash
git tag v1.0.0 && git push origin v1.0.0
```

The Release workflow checks the metadata, builds and validates the package, and creates a
**draft** GitHub release with:

- `omarchy-notes-VERSION.tar.gz`: the source archive the AUR package downloads
- `omarchy-notes-VERSION-aur.tar.gz`: `aur/PKGBUILD` and `aur/.SRCINFO`
- the built package, `SHA256SUMS` and `SOURCE_REVISION`

Review the draft and publish it. Then check the AUR files fetch the published archive:

```bash
tar xf omarchy-notes-1.0.0-aur.tar.gz && cd aur && makepkg --verifysource
```

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
