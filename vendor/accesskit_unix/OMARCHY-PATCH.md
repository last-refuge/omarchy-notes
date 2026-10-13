# Local provenance and patch

This directory contains the source of crates.io `accesskit_unix` **0.22.1**,
copied from the exact version already resolved in the Omarchy Notes lockfile.
Original registry checksum:
`03e156ed3802e35eefe894ef2671bc6c889303d8a7e110b5e1b48f504b91362f`.

The published crate's `.cargo_vcs_info.json` identifies upstream commit
`c88605b96d04431f9c3c792464a0f2f253480e94`, path `platforms/unix`, in
<https://github.com/AccessKit/accesskit>. The original published manifests,
README, changelog, VCS metadata and source files are retained. License texts are
from that exact upstream commit because the published package did not include
the files referenced in its source headers. The license remains
**MIT OR Apache-2.0**; copyright and notices are preserved.

One production marshaling expression changes in `src/atspi/bus.rs`:
`body` becomes `&(body,)` in `emit_cache_signal`. The same private helper's
generic bound changes from `DynamicType` to `Type`, which the one-element tuple
requires. Both existing caller types already implement `Type`; public APIs,
crate version and dependency requirements remain unchanged.

The tuple preserves the single structured argument required for AT-SPI
`Cache.AddAccessible` and `Cache.RemoveAccessible`. Direct struct bodies lost
that boundary when zbus treated their fields as the message argument tuple.
Object/Window event bodies are unchanged. A test-only module verifies actual
zbus message wire signatures and typed payload round trips.

See `docs/decisions/0021-atspi-cache-signals.md` in the application repository.
Retire this local patch only after a future pinned dependency demonstrably
includes the equivalent correction and the native regression checks pass.

Excluded from this vendored copy: `.cargo-ok`, registry checksum state, the
dependency's standalone lockfile, and build output. The application's root
lockfile remains the single resolved dependency graph.
