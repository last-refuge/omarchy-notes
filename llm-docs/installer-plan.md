# Omarchy Notes — command-line installer plan

Planning draft · 11 October 2026

Add a standalone Bash installer with restrained branding, animated progress,
concise status messages, and clear next steps. Follow the presentation and flow of
[Facet's installer](https://github.com/last-refuge/omarchy-facet/blob/master/install.sh),
using Notes' existing Arch packages and release checksums underneath.

The originally supplied reference URL used `lastrefuge/omarchy-facet` and returned
404. The reference inspected for this plan is `last-refuge/omarchy-facet`.

Implemented in `install.sh`, with isolated installer tests and CI/release wiring.
The primary public installation command stays on the existing package until a
release containing the installer asset is published and checked.

## 1. Proposed appearance

Use the website's blue accent, green success marks, muted secondary text, and
generous spacing. Avoid large ASCII art or additional UI dependencies.

Illustrative output:

```text
  omarchy notes  ·  A place for your thoughts.

  ✔  Omarchy detected · x86_64
  ✔  Release v1.0.0 selected
  ✔  Package downloaded
  ✔  Checksum verified

  Installing the package
     Administrator access may be requested.

     [normal pacman transaction and confirmation]

  ✔  Omarchy Notes installed
  ✔  Application verified

  ?  Set up Quick Note with Super + Alt + N? [y/N]

  Ready to write
     Super + Space → Omarchy Notes
     Or run omarchy-notes

  Your notes
     ~/.local/share/omarchy-notes/
```

During downloads and other background work, the active line gets a small spinner.
Errors show the failed step, a short diagnostic excerpt, and a persistent log
location. Display the resolved data location when reporting where notes live,
respecting the application's XDG conventions.

## 2. Installer entry point

Add a self-contained root-level `install.sh`.

- Support Omarchy and Arch Linux initially.
- Default to the latest published binary package; support `--version TAG`.
- Provide `--help`, `--yes`, `--dry-run`, `--source`, and
  `--quick-note` / `--no-quick-note`.
- Keep presentation helpers separate from detection, downloading, installation,
  and verification.
- Follow Facet's `main "$@"` structure and route interactive input through
  `/dev/tty`, so piping the script into Bash works correctly. Background commands
  must not consume the script's standard input.
- Require explicit `--yes` without an interactive terminal; fail clearly if
  privilege elevation cannot proceed.
- Validate arguments before starting work and explain invalid combinations.

## 3. Terminal presentation

- Use consistent banner, progress, success, warning, and error styles.
- Support truecolor with standard ANSI fallbacks.
- Disable animation and escape sequences for redirected output, `TERM=dumb`,
  and `NO_COLOR`.
- Provide ASCII symbols outside UTF-8 locales and readable output in narrow
  terminals.
- Keep `sudo` and pacman prompts visible.
- Restore the cursor and stop background work on interruption.
- Preserve failure logs outside the temporary download directory. Facet's
  inspected implementation prints a log path that its cleanup subsequently
  removes; avoid carrying that behavior over.

## 4. Release selection and package installation

Reuse the existing release contract:

1. Resolve the release once, then download its package and `SHA256SUMS` from that
   exact tag. This prevents a new release appearing between requests from mixing
   assets from different versions.
2. Select exactly one matching application package for the architecture,
   excluding debug packages.
3. Verify its checksum and package identity before invoking `sudo pacman -U`.
4. Treat network failures, missing assets, and checksum failures as distinct
   errors with actionable messages.
5. Never silently switch to a lengthy source build after a failed download.

At the time of inspection, the
[v1.0.0 release](https://github.com/last-refuge/omarchy-notes/releases/tag/v1.0.0)
already supplies the package and checksums. It publishes an x86_64 binary; the
installer must not imply that ARM binaries are available merely because the
PKGBUILD lists `aarch64`.

## 5. Explicit source-build route

- Use the selected release's generated AUR recipe and source archive.
- Build as the normal user with `makepkg`, then install through pacman.
- Show required build dependencies before installing them.
- Keep builds in a temporary workspace.
- Retain the documented CMake installation route for other Linux systems.

Broader `--prefix` support can follow once reliable ownership tracking and
uninstall behavior are defined. The first installer should focus on the existing
Arch package workflow.

## 6. Notes-specific finishing steps

- Verify the installed executable using off-screen `--version`, following the
  existing package tests.
- Offer Quick Note setup only on compatible Omarchy installations, delegating to
  the existing `omarchy-notes --setup-hyprland` command.
- Keep that integration opt-in; `--yes` alone must not enable it.
- Report shortcut conflicts as an optional setup issue without misreporting
  package installation as failed.
- Detect older user-local copies that could shadow the package and explain the
  conflict.
- Preserve the notes library and running application. Facet's forced application
  shutdown is unsuitable for a notes editor. Advise users to close and reopen
  Notes normally when an upgrade requires it.
- Finish with launch, update, and package-removal instructions.

## 7. Publishing and documentation

1. Attach `install.sh` to releases and include it in `SHA256SUMS`.
2. Publish through the existing draft-release workflow.
3. After a release containing the installer is available, replace the primary
   README and website command with the following:

   ```bash
   curl -fsSL https://github.com/last-refuge/omarchy-notes/releases/latest/download/install.sh | bash
   ```

4. Include download-and-inspect instructions and examples for dry runs and
   explicit versions.
5. Keep direct package installation documented as an alternative.

The command above is a planned distribution URL, not an installer available in
the inspected v1.0.0 release.

## 8. Files involved

| File | Planned change |
|---|---|
| `install.sh` | Installer and presentation helpers |
| `tests/test-installer.sh` | Isolated installer behavior tests |
| `tests/test_installer.py` | Command fixtures, real PTY prompts, and interrupt tests |
| `tests/test-installer-package.sh` | Real pacman installation in the disposable CI container |
| `.github/workflows/ci.yml` | Bash syntax, ShellCheck, and installer tests |
| `.github/workflows/release.yml` | Publish installer and checksum |
| `README.md` | Installation instructions |
| `docs/index.html` | Primary installation command and supporting instructions |
| `docs/arch-release.md` | Release checklist |

## 9. Validation and acceptance criteria

Use mocked commands for failure cases and a disposable Arch environment for a
real installation. Cover:

- Fresh installation, upgrades, and already-installed versions.
- Missing releases, unsupported architectures, and ambiguous or missing package
  assets.
- Network and checksum failures, with no package installation after failed
  verification.
- Denied elevation and unavailable interactive input.
- Interruption, background-process cleanup, and cursor restoration.
- Piped execution without commands consuming the installer script.
- Quick Note setup, skipped integration, and shortcut conflicts.
- Older user-local executables shadowing the installed package.
- Dry runs making no installation or configuration changes.
- Failure logs remaining readable after cleanup.
- Existing notes remaining untouched.

Visually check dark and light terminals, narrow widths, non-UTF-8 output, and
plain redirected output. Verify the published release contains the installer and
its checksum before changing the public installation instructions.
