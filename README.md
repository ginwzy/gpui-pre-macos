# gpui-pre-macos synthetic oblique patch

This repository contains the published `gpui-pre-macos` 0.3.5 snapshot of
Zed's Apache-2.0 `gpui_macos` crate at
`d89e9c2124b2786a390c7a451c7488601b4da2e1`, with synthetic oblique text.
The initial commit imports the crates.io package unchanged. Modified files
carry a notice; the original license is preserved in `LICENSE-APACHE`.

When italic text resolves to a face without italics, including a Core Text
fallback inside an italic run (CJK in PingFang, for example), the text system
gives that face a separate oblique `FontId` and shears its glyphs by 14° when
computing raster bounds and drawing. Shaping and advances are unchanged.
Emoji are never slanted. It keeps the package version and all other GPUI
snapshot dependencies unchanged.

Consumers use a Cargo `[patch.crates-io]` entry pinned to the fix commit.
Run its unit tests on macOS with
`cargo test --features font-kit,runtime_shaders,test-support --lib`.
