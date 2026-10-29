# Apple rendering backend

This is the `gpui-pre-apple` 0.3.5 source distribution from crates.io, based on
Zed commit `d89e9c2124b2786a390c7a451c7488601b4da2e1`, retained inside the macOS
fork so renderer changes and the platform adapter are pinned by one submodule.
The upstream Apache-2.0 license is in `LICENSE-APACHE`.

The local changes add composited rounded clipping. Shader bindings are generated
from the adjacent `gpui-pre` checkout rather than a second snapshot of its data
structures. The package name and version remain unchanged.
