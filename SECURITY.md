# Security Policy

## Supported versions

Security fixes are provided for the latest published Omarchy Notes release.
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

Omarchy Notes works offline and has no accounts, sync or network access. Notes
are stored locally in SQLite under `$XDG_DATA_HOME/omarchy-notes`, with
attachments kept as files beside the database. Neither is encrypted by the app,
so they're protected only by your user account and any disk encryption on your
system. Backups made with **Back Up Library…** are plain, unencrypted copies of
that data; store them somewhere you trust. Encrypted locked notes are planned but
not yet available.
