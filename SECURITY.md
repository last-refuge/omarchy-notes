# Security Policy

## Supported versions

Security fixes are provided for the latest published Rust Omarchy Notes release
(`rust-v*`, package/executable `omarchy-notes-rust`). The Qt app is retired and no
longer supported; legacy format compatibility is for migration only.
Pre-release builds from `master` receive fixes as they are developed, but are not
supported releases.

## Reporting a vulnerability

Please report security issues privately through
[GitHub Security Advisories](https://github.com/last-refuge/omarchy-notes/security/advisories/new).
Do not include note content, attachments, backups, or other personal data in a
public issue.

Include the affected version, operating system, steps to reproduce, expected and
observed behavior, and any logs you can safely share. Redact note titles, note
content and file names. We will acknowledge a report as soon as practical,
investigate it, and coordinate disclosure and a fixed release with the reporter.

## Data and storage

Omarchy Notes works offline and has no accounts or sync. Notes are stored locally
in SQLite under `$XDG_DATA_HOME/omarchy-notes-gpui` (by default
`~/.local/share/omarchy-notes-gpui`), with attachments beside the database.
Preferences remain under `$XDG_CONFIG_HOME/omarchy-notes-gpui`. `--data-dir` can
select a different library. Neither notes nor attachments are encrypted by the
app; protection depends on your user account and any system disk encryption.
Backups made through Settings → Your notes and files are plain, unencrypted
copies; store them somewhere trusted. Encrypted locked notes are not available.

Imported files are untrusted input. Review import warnings; external links and
attachments can open other applications whose network and security behavior is
outside the notes app. Installation and dependency fetching require network
access. Preserve original legacy Qt data and verified backups when migrating;
see [migration guidance](docs/migrating.md).
