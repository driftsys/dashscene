# Changelog

All notable changes to this package are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the version tracks
the Cargo workspace rather than moving on its own.

## [Unreleased]

### Added

- The C# declaration of boundary B — the value types `crates/dashpaint-abi`
  holds to a C representation (story #1239).
- The C# host on the C ABI: P/Invoke declarations for all fourteen entry points,
  a thread-affine managed lifetime, the `ds_last_error_message` channel on every
  failure a `DsStatus` describes, and the committed frame under a lease that
  checks each array's stride before a row is read (story #1121).
- `CommitPacer`, for committing below the display rate without drifting off it.
- A `Frame Loop` sample — a `MonoBehaviour` that loads a `.dsb`, ticks it, and
  takes each committed frame. It draws nothing; the painter is story #1122.
- The `BatchRendererGroup` painter, in three material classes — unlit-overlay,
  lit-opaque and lit-cutout (story #1122). It draws fills, both solid and
  gradient, corner radii, strokes, clips, per-node opacity and rotation.
  Shadows, blurs, image fills, baked vector nodes, render-target groups and text
  are **not** drawn and each is reported by name through `PackDiagnostic` — P4
  forbids a silent drop.
- `Runtime/Shaders/Sdf.hlsl`, generated from
  `crates/dashscene-gpu/src/shaders/sdf.wgsl` by `naga` rather than ported by
  hand. The signed-distance, coverage and gradient math a Unity shader evaluates
  is the same compiled module the lean painter evaluates
  (`docs/specification/03-target-hardware-rules.md` R-T5). Do not edit it.
- A dependency on `com.unity.render-pipelines.universal`. The painter's shaders
  include URP's `Core.hlsl`, which is what reaches the DOTS instancing
  declarations a `BatchRendererGroup` needs.
- `.meta` files for every path Unity imports, without which a Git-URL package
  delivers nothing (R-E2), and a `unity` field declaring `6000.3` (R-E1).
