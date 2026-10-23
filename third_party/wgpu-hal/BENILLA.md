# benilla's `wgpu-hal` fork — what differs from upstream, and how to check

Upstream: [`gfx-rs/wgpu`](https://github.com/gfx-rs/wgpu) `wgpu-hal`, MIT OR Apache-2.0, version
`27.0.4`, the version the workspace lock resolves to. Wired in through `[patch.crates-io]` in the
workspace root. The crate is copied from the cargo registry as published (`src/`, `examples/`,
`build.rs`, `Cargo.toml`, `README.md` and the two licence files); one line differs.

## Why a fork exists at all

The Metal backend reports `TEXTURE_COMPRESSION_BC` on macOS only (`format_bc: os_is_mac`), so on
an iPhone benilla's BLP loader (`benilla-assets/src/gpu_blp.rs`) finds no BC and decodes every
DXT texture to RGBA8: 4× (DXT3/5) to 8× (DXT1) the memory the reference spends, which uploads the
blocks as they are stored. On an iPhone 13 (A15, iOS 27) the iNilla memory census measured
~317 MB of uncompressed textures and 84 MB of texture arrays in the world, against a 2341 MB
budget. Apple GPUs from the A14 (`MTLGPUFamilyApple7`) sample BC formats, and Metal reports it
through `MTLDevice.supportsBCTextureCompression`, available from iOS 16.4.

## The patch — `src/metal/adapter.rs`, `format_bc`

```rust
format_bc: os_is_mac
    || (version.at_least((11, 0), (16, 4), os_is_mac)
        && device.supports_BC_texture_compression()),
```

The version guard keeps the selector off an OS that lacks it. Everything downstream (the BC
format capabilities, `TEXTURE_COMPRESSION_BC_SLICED_3D`) already keys off `format_bc`, as on macOS.

## How to check

On the device, the iNilla memory census line reads `DXT upload on` and the block-compressed
column carries the world's textures. `WOW_NO_BC=1` forces the decoded path back for an A/B.

## Updating

Drop this fork when an upstream `wgpu-hal` enables BC on iOS (check `format_bc` in
`src/metal/adapter.rs`), or re-copy the new version and re-apply the one line.
