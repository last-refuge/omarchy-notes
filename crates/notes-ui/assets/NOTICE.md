# Bundled assets and source guidance

The five unmodified Noto Sans / Noto Serif font files are Copyright 2022 The Noto
Project Authors. Their embedded name-table license records specify SIL Open Font
License 1.1. See `fonts/OFL.txt`, obtained from
https://github.com/notofonts/latin-greek-cyrillic/blob/main/OFL.txt.
They were copied from the installed Arch `noto-fonts` package; the embedded license
was checked independently of the package's aggregate license file.

Existing line icons were adapted from this project's `src/app/icons/`, under the
repository MIT license, changing black strokes to `currentColor`. The additional
simple geometric icons were authored for this preview under the same license.

The search adapter's input-handler and custom paint integration follows the GPUI
Apache-2.0 example at Zed revision
`a84689073d296dfd39987bc7dd478e43ef76d83a`, `crates/gpui/examples/input.rs`.
The pure text model and application behavior are implemented in this repository.
GPUI source guidance is Copyright Zed Industries and contributors; the applicable
Apache-2.0 text is included in `GPUI-LICENSE`. No Zed GPL editor/UI crate is used.
