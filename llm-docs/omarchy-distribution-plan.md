# Omarchy Notes distribution and discovery plan

Prepare Omarchy Notes for inclusion in Omarchy's package repository so users can discover it in **Install → Package**, install it with `omarchy pkg add omarchy-notes`, and receive updates through Omarchy's normal update flow. Pursue a named menu entry and manual listing after the package is available.

This plan covers changes to this project and proposed upstream contributions. Upstream acceptance and publication are dependencies controlled by Omarchy maintainers. Research was checked on September 30, 2026.

Implementation scope confirmed October 1, 2026: focus on `omacom/omarchy-pkgs`; omit AUR publication and defer the separate named menu entry and manual proposal. Initial package support targets x86_64.

## Current position

The project already has most of the application packaging work:

- Version 1.0.0 is published with a source archive, an x86_64 Arch package, generated AUR files, checksums, and source revision metadata.
- `packaging/arch/PKGBUILD.in` builds from source and declares x86_64 and aarch64 support. Only x86_64 binary release assets are currently published.
- CMake installs a desktop entry, icon, AppStream metadata, license, documentation, and man page. The desktop entry includes search keywords and New Note and Quick Note actions.
- CI checks the install tree, package contents, dependencies, and application behavior. Its build environment is current Arch, rather than Omarchy's stable package snapshot.
- The README and website provide screenshots and an installation command tied to the 1.0.0 GitHub package. That command installs a package directly; it does not establish a repository that supplies future versions.
- The AUR RPC returned no package named `omarchy-notes`. The README's account-registration explanation needs fresh verification before being repeated.
- GitHub resolves the local `jasona/omarchy-notes` remote to `last-refuge/omarchy-notes`. Current topics include `calendar` and `omarchy-plugin`, which describe the wrong product or integration type.

The main work is distribution, compatibility verification, and discoverability. Preserve the existing binary name, application ID, and library location so current installations continue working.

## 1 Improve project discovery and installation documentation

**Files:** `README.md`, `docs/index.html`, `docs/arch-release.md`, and relevant release metadata. **External setting:** GitHub repository description, homepage, and topics.

- Standardize links on `last-refuge/omarchy-notes` and one primary homepage. Both the GitHub Pages URL and `lastrefuge.ai/projects/omarchy-notes` currently respond successfully; choose one consistently.
- Replace the `calendar` and `omarchy-plugin` topics with accurate topics such as `notes`, `note-taking`, `qt6`, and `local-first`, retaining relevant Omarchy, Hyprland, and Wayland topics.
- Lead with one sentence describing the app, one screenshot, and the installation route that actually works at the time of publication.
- Document installation, launching, upgrading, and removal. Explain that removal preserves the user's notes and attachments, and distinguish repository updates from manual GitHub-package replacement.
- Retain the current GitHub release installation as the interim route and eventual fallback. Explain its x86_64 scope and how users obtain a newer package. Include checksum verification instructions.
- Once the Omarchy package is published to stable, make `omarchy pkg add omarchy-notes` and **Install → Package → search for omarchy-notes** the primary instructions. Keep source-build instructions available for contributors.
- Remove the pending AUR installation messaging; focus installation documentation on the Omarchy repository and release-download fallback.

**Completion criterion:** the README, homepage, and release instructions agree on available installation methods, supported architectures, and the update process. Every installation command is verified before it is advertised.

## 2 Verify packaging against supported Omarchy channels

**Files:** `packaging/arch/PKGBUILD.in`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `tests/check-arch-package.sh`, and `docs/arch-release.md`.

- Define the initial compatibility target as Omarchy stable on x86_64, with edge as an additional validation target. Record the exact Omarchy and Qt versions tested.
- Build the source recipe against Omarchy's stable dependencies as well as current Arch. Check the Qt 6.8 minimum and dependency names, including the declared `libgcc` and `libstdc++` packages. Resolve any channel differences before submission.
- Review runtime dependencies using the installed package. Decide whether `qt6-wayland` and `xdg-utils` should be required to deliver the advertised Wayland and attachment-opening behavior on a clean installation. Keep optional image format support clearly documented.
- Reuse existing desktop-file, AppStream, namcap, package-content, and RPATH checks. Add a meaningful package lifecycle check that installs into an isolated environment, launches the installed app, upgrades an existing library, removes the package, and confirms the library remains intact.
- Validate aarch64 with a real package build and launch before claiming tested support. Keep its status explicit if validation or upstream publication is pending; do not delay the initial x86_64 submission unnecessarily.
- Continue publishing immutable source archives, generated checksums, and source revision metadata. Packaging-only changes need a `pkgrel` bump; application changes need an appropriate version bump.

**Completion criterion:** the package builds and runs with stable dependencies, contains all launcher assets, and survives install, upgrade, removal, and reinstall without losing notes or attachments.

## 3 Prepare the Omarchy package repository contribution

**Upstream destination:** `omacom/omarchy-pkgs`.

Create these upstream files from a published, validated release:

```text
pkgbuilds/omarchy-notes/PKGBUILD
pkgbuilds/omarchy-notes/.omarchy/package.json
```

- Use a complete source-building `PKGBUILD` derived from this project's generated recipe. Commit the real checksum, not the template placeholder. Download the immutable source release rather than repackaging the GitHub binary built against current Arch.
- Start with local source metadata and a watch for published versioned GitHub releases. Omarchy's tooling supports maintained local recipes with direct upstream watches, so AUR publication is not a prerequisite. [Upstream watch documentation](https://github.com/omacom/omarchy-pkgs/blob/master/docs/upstream-sources.md)
- Validate this proposed metadata against the upstream tooling:

```json
{
  "source": "local",
  "upstream": {
    "watch": {
      "github": "last-refuge/omarchy-notes",
      "pattern": "v(?P<version>[0-9]+\\.[0-9]+\\.[0-9]+)"
    }
  }
}
```

- Leave channel and release-ring policy to upstream review. The documented default builds into edge and reaches stable through promotion; do not assume merging a recipe immediately makes it available to stable users. [Package repository documentation](https://github.com/omacom/omarchy-pkgs/blob/master/README.md)
- Run `python helpers/upstream-watch.py check pkgbuilds/omarchy-notes`, the focused upstream checks required by the contribution, and a package build using the repository's documented build tooling.
- Prepare a focused PR description with the application's purpose, maintainer contact, runtime dependencies, tested versions and architectures, screenshots, and validation results. Explain how rich notes and Quick Note complement the Obsidian and Omawrite apps already described in the Omarchy manual.

**Completion criterion:** the contribution is reviewable, its watch identifies the correct published release and source checksum, and an upstream-built signed package is ultimately available in the intended channel.

## 4 Add discovery within Omarchy

Package publication provides the initial discovery route through **Install → Package** and installation through `omarchy pkg add omarchy-notes`. These are documented Omarchy interfaces. [Other Packages manual](https://github.com/omacom/omarchy/blob/quattro/manual/29-other-packages.md)

For additional visibility, prepare a separate proposal for `omacom/omarchy`:

- Suggest a named optional installation entry for **Omarchy Notes** in `default/omarchy/omarchy-menu.jsonc`. The current menu has no general GUI-app category; agree on placement with maintainers instead of assuming a category exists.
- Reuse the existing action `omarchy-install-app 'Omarchy Notes' omarchy-notes` and installed-state guard `omarchy-pkg-present omarchy-notes`. A dedicated installer script should only be needed if maintainers request behavior beyond package installation.
- Propose a short section in `manual/22-guis.md` describing offline rich notes, installation, launcher search, Quick Note, and the project link. Present this as an optional app proposal.
- Keep desktop launcher registration in this project's package. Omarchy should not need a duplicate desktop entry or icon.
- Use the Suggestions discussion route for the new listing proposal. Read the upstream `AGENTS.md` and relevant menu/testing guides before implementing it. Include focused menu checks and screenshots with the eventual PR. [Contribution guidance](https://github.com/omacom/omarchy/blob/quattro/default/agents/skills/omarchy/contributing.md)

**Completion criterion:** users can find and install the published package through the existing package menu. If the additional proposal is accepted, the named entry installs successfully and displays the installed state afterward.

## 5 Keep desktop setup optional and document maintenance

**Files:** `src/app/HyprlandSetup.cpp`, related launch tests, `README.md`, and `docs/arch-release.md`.

- Confirm that a normal package install and first launch require no manual configuration. Test launcher search, both desktop actions, single-instance behavior, and live theme switching in a supported Omarchy session.
- Keep `--setup-hyprland` an explicit user action. Package installation must not edit user keybindings or window rules.
- Verify setup detects an occupied shortcut, remains idempotent, and restores both files after a configuration error or partial write failure. Harden these paths where testing identifies a gap.
- Document any older Hyprland configuration limitations rather than promising untested version support.
- Maintain a release checklist covering version metadata, source archives, package validation, published assets, downstream watch detection, and channel availability. Verify a subsequent release can reach repository users through normal updates.
- Keep the release artifacts and documentation focused on Omarchy repository maintenance.

**Completion criterion:** desktop integration works on the supported system, optional setup preserves user customizations, and the maintainer can repeat the release process without guessing.

## Delivery order

| Stage | Deliverable | Dependency |
| --- | --- | --- |
| 1 | Discovery metadata and accurate interim installation docs | Available now |
| 2 | Stable-compatible packaging and lifecycle validation | Supported versions chosen |
| 3 | Published packaging update, if needed, and package repository contribution | Validation passes |
| 4 | Primary docs switch to Omarchy installation | Package published to stable |
| Deferred | Named menu entry and manual contribution | Separate listing proposal accepted and package available |
| 5 | Next-release update verification | Release/watch flow |

The minimum useful outcome is a stable-channel package that appears in the existing package search, installs by name, launches from the app launcher, updates normally, and preserves notes when removed. A dedicated menu entry improves visibility but is a separate upstream decision.
