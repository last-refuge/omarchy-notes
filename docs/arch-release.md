# Arch releases

Omarchy Notes now ships only the Rust app. The Qt release route is retired;
old Qt release plans and ADRs are historical records, not current build or
publication instructions.

Use [Building and installing Omarchy Notes](rust-release.md) for candidate
source preparation, Arch packaging, installer verification and release freeze.
The package/executable remains `omarchy-notes-rust`, desktop ID remains
`org.omarchy.Notes.Rust`, and release tags remain `rust-vVERSION`.
Do not reuse the retired Qt `vVERSION` tags or replace published archives.

See [release identity](rust-edition.md), [dependency notices](rust-v1-dependency-inventory.md)
and [release notes](rust-release-notes.md) for scope and remaining acceptance work.
