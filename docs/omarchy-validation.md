# Omarchy package validation

Validated on October 1, 2026, for x86_64. The upstream recipe uses the immutable
published 1.0.0 source archive and produces package version 1.0.0-2. Aarch64 is not
included in the initial repository recipe.

| Dependency | Omarchy stable mirror | Omarchy edge mirror |
| --- | --- | --- |
| glibc | 2.44+r24+g16be1518495f-1 | 2.44+r50+g1848099f063e-1 |
| qt6-base | 6.11.2-3 | 6.11.2-3 |
| qt6-declarative | 6.11.2-1 | 6.11.2-2 |
| qt6-wayland | 6.11.2-1 | 6.11.2-1 |
| sqlite | 3.53.4-1 | 3.53.4-1 |

Local package validation used an isolated filesystem bootstrapped from Omarchy's
stable mirror, with package signatures verified using the system's public Arch
keyring. User and mount namespaces and bubblewrap kept package transactions inside
that filesystem; the workstation's packages and notes were not modified. Builds
and app launches ran as an unprivileged `builder` account. The same isolated root
was then fully synchronized to edge before building the generated project recipe.

Both dependency environments passed:

- Source checksum verification and an unprivileged `makepkg --cleanbuild` build.
- Package-content and RPATH checks, desktop-file validation, and AppStream validation.
- Installed application rendering with Qt's offscreen software backend.
- Upgrade from the checksum-verified published 1.0.0-1 package to 1.0.0-2.
- Removal and reinstall with note bodies, attachment records, attachment hashes,
  and database integrity preserved.

Namcap reported no errors. Its warnings concern the statically embedded
`OmarchyNotes` QML module and dynamically used SVG, Wayland, and `xdg-utils`
dependencies. These dependencies are retained for image rendering, the Wayland
platform plugin, and opening attachments.

Other checks passed:

- All six application CTest suites, including launch and optional Hyprland setup tests.
- A native Wayland launch in the current Omarchy session with a temporary sample
  library (first frame in 270 ms).
- Production CMake installation contract and release metadata consistency.
- Identical source and packaging archives, checksums, and source revision files
  from two preparation runs against the same commit.
- Omarchy's upstream watch recognized published 1.0.0 as current.
- All 33 upstream release-watch tests and the upstream pinned-source check.

The package repository's own container build and CI remain upstream checks. Local
render checks do not establish fresh-session launcher behavior or every graphical
configuration. Repository signing, publication, stable-channel availability, and
delivery of a subsequent app release remain maintainer-controlled steps.
