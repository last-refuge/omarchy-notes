# Arch releases

Omarchy Notes now ships only the Rust app. The Qt release route is retired;
old Qt release plans and ADRs are historical records, not current build or
publication instructions.

Use [Building and installing Omarchy Notes](rust-release.md) for candidate
source preparation, Arch packaging, installer verification and release freeze.
The Arch package is now `omarchy-notes`; the executable remains
`omarchy-notes-rust`. This package-only rename leaves desktop ID
`org.omarchy.Notes.Rust`, profile/runtime paths and `rust-vVERSION` tags unchanged.
New source archives are `omarchy-notes-VERSION.tar.gz`.

Older published `rust-v1.0.0` artifacts still have the `omarchy-notes-rust`
package name. The new installer requires a newly named `omarchy-notes` release
package. The approved release target is `rust-v1.0.1`, which fixes this mismatch
with newly named artifacts once published; no published release has been modified.
Do not reuse the retired Qt `vVERSION` tags or replace published archives.

See [release identity](rust-edition.md), [dependency notices](rust-v1-dependency-inventory.md)
and [release notes](rust-release-notes.md) for scope and remaining acceptance work.
