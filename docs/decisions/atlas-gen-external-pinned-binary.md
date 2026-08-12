# Decision: atlas generation shells out to an external, version-pinned msdf-atlas-gen binary

    status   accepted (story #27, 2026-07-12)
    scope    crates/dashscene-typeset atlas module — tool invocation
             (atlas/tool.rs)
    evidence docs/technotes/arabic-atlas-coverage.md (spike #25)
    related  docs/decisions/q1-msdf-below-14px.md

## Context

The Arabic spike (#25) validated `msdf-atlas-gen` 1.4.0 directly: it accepts
glyph-id input, loads GSUB-only glyphs with no cmap entry, and its output keys
atlas entries by glyph index — the property the pinned "keyed by glyph id, never
codepoint" contract depends on. Story #27 had to decide how the atlas pipeline
runs that generator in product code.

## Options

1. Shell out to an external, version-pinned `msdf-atlas-gen` binary. Discovery:
   `MSDF_ATLAS_GEN` env var override, else `PATH`. Refuse to run with anything
   but the pinned version, with a named error.
2. Replace it with a pure-Rust MSDF crate (`fdsm`, `msdf`).
3. Vendor the C++ source and build it via `build.rs`.

## Choice

Option 1. Pinned version: `1.4.0` (the spike-validated version).

## Why

- Option 1 keeps the exact component the spike measured. The spike's
  contextual-form-coverage and legibility findings are about `msdf-atlas-gen`
  1.4.0 specifically, not about "an MSDF generator" in the abstract.
- Option 2 (pure-Rust MSDF crates) is rejected for v0: it replaces the
  spike-validated component with an unvalidated one. Output quality and
  Arabic-contextual-form parity with `msdf-atlas-gen` are unknown, and adopting
  one now would invalidate the spike's evidence without a new spike to replace
  it. Revisit only if the external-tool dependency becomes a real operational
  problem.
- Option 3 (vendor + `build.rs`) is rejected: it makes every workspace build pay
  a C++ toolchain cost and complicates contributor setup, for a tool that only
  needs to run at asset-build time, not on every `cargo build`.
- The version gate is enforced in code, not by convention: `find_tool_checked`
  parses the tool's `-help` banner and returns
  `AtlasError::ToolVersion { found, required }` on any mismatch — P4's "no
  silent drift" posture applied to a build dependency, not just to document
  vocabulary.
- Availability gap: upstream publishes Windows-only release binaries. macOS
  contributors install via `brew install msdf-atlas-gen` (bottled 1.4.0); Linux
  CI builds the pinned `v1.4` tag from source once and caches the binary (the
  `atlas-repro` job — see `docs/design/atlas-pipeline.md`).

## Packaging alternatives raised after the decision (2026-07-12)

Two further packaging ideas were raised. Neither changes the decision.

- **Vendor the prebuilt binary in a dedicated repo (subtree or submodule) and
  package it inside the final binary.** The second half does not apply:
  `msdf-atlas-gen` runs at asset-build time, not in the product
  (`docs/design/atlas-pipeline.md` — the build-time half of
  `docs/archive/2026-07-14-design-1-seed.md` §7.2). Its outputs — the atlas
  image and the metrics blob — ship; the tool never runs on a target device, so
  there is no product binary to package it into. The first half is a narrower
  idea that addresses only the availability gap named above: distributing a
  pinned binary to contributors and CI. It stays open as a possible future
  convenience, not as a change to how the atlas is generated.
- **Link it as a library through Rust FFI.** This is Option 3 with a library
  boundary in place of a process boundary. It still forces a C++ toolchain into
  the workspace build, which is exactly why Option 3 was rejected.

The revisit condition is unchanged: reconsider the external-tool dependency only
if it becomes a real operational problem.

## The tool's output is deterministic per machine, not per architecture (story #654, 2026-08-01)

Choosing an external C++ binary carried a consequence that was not visible until
CI could run again after the #263 outage: **the tool's output depends on the CPU
architecture it runs on.**

Measured with the pinned commit built from source in each environment, over all
eight committed fixtures:

| environment                              | result                     |
| ---------------------------------------- | -------------------------- |
| macOS arm64, homebrew tool               | all 8 reproduce            |
| Debian bookworm aarch64, freetype 2.12   | all 8 reproduce            |
| Ubuntu 24.04 aarch64, freetype 2.13.2    | all 8 reproduce            |
| Ubuntu 24.04 **x86_64**, freetype 2.13.2 | 7 reproduce, Bold does not |

So the operating system, the freetype version and the tool build are all ruled
out; the architecture is the variable. The divergence is small and precisely
bounded:

- `atlas.metrics` is **byte-identical** on every environment above, so the
  packing, the per-glyph boxes, the atlas parameters and the provenance do not
  move at all;
- the Bold `atlas.png` decodes **4 pixels of 65536 apart** (0.006 %), each by
  **one channel step** — the smallest an 8-bit channel can express — at
  identical dimensions.

This is inherent to Option 1. A pure-Rust generator (Option 2) would not
necessarily be architecture-stable either, and vendoring the C++ source
(Option 3) changes who compiles it, not what its floating point does.

### Consequence for the reproducibility gate

Byte-identity of the image was the wrong assertion, not the wrong result: no
single committed fixture can satisfy it on two architectures, so it failed on
whichever fixture the drift happened to land on. The `atlas-repro` tests now
compare the metrics byte for byte and the decoded image under two bounds — no
channel moves by more than one step, and at most 0.1 % of pixels differ at all —
and the job runs on both x86_64 and arm64.

The bounds keep the gate the version pin exists to serve. A generator that
re-rasterises a glyph, moves the packing, or changes the distance range exceeds
one step; a systematic shift exceeds 0.1 % of the pixels. Both are
mutation-proven rather than asserted. Same-machine determinism is untouched and
remains byte-identity, with no tolerance.

Committing a second per-architecture fixture was considered and rejected: the
atlases are also loaded by the goldens (`goldens/tooling/src/render.rs` and five
golden tests), so a per-arch selection rule would make the committed golden
images architecture-dependent too, and seven of the eight pairs would be
byte-identical duplicates.
