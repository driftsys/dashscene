# dashscene

dashscene turns UI designed in Figma — or authored in code — into pixels on
screen, through one intermediate representation (the **dashscene document**,
written to disk as a `.dsb` file), one shared layout and text runtime, and
interchangeable paint backends behind a single trait.

It is aimed at embedded and automotive-class hardware, where the same screen has
to be drawn by a game engine on the product, by a lean native renderer on a
smaller device, and by a reference rasterizer in a test — and where all three
have to agree, to the pixel, about where every rectangle and every glyph sits.

![Sixteen tiles on a dark background: solid, linear, radial, angular and diamond fills; strokes inside, centred and outside their edges; a photograph under four fill modes; a star baked to a distance field; drop and inner shadows; a clipped disc; a masked gradient; two overlapping squares at group opacity; and a frosted panel blurring the tiles behind it.](docs/images/showcase-surfaces.png)

That is one frame of the `surfaces` scene, drawn by the Skia reference painter.
Every construct in it is v0 paint vocabulary, and the scene moves: the frosted
panel slides across the gallery, and what it blurs changes as it travels.

## Run it

You need [Rust](https://rustup.rs) 1.88 or newer, and `flatc`, the FlatBuffers
compiler, on `PATH` — the `dashbuf` crate's build script runs it (`brew install
flatbuffers`, or `apt-get install flatbuffers-compiler`).

```sh
git clone https://github.com/driftsys/dashscene-staging.git
cd dashscene-staging
cargo run -p demo --release
```

A window opens and the `surfaces` scene above animates in it. There are three
scenes, plus a compiled document:

```sh
cargo run -p demo --release                 # surfaces, the default
cargo run -p demo --release -- typography   # Latin and Arabic text
cargo run -p demo --release -- layout       # flex, grid, reflow, a variant switch
cargo run -p demo --release -- --all        # every scene in turn, cycling
cargo run -p demo --release -- --list       # what there is
cargo run -p demo --release -- --dsb        # a compiled .dsb, loaded and drawn
```

Three inputs drive the running scene, alongside the scripted animation: moving
the pointer left and right scrubs the scene's own scalar signal from `0.0` to
`1.0`, the Left and Right Arrow keys snap it to either end, and Space runs the
variant switch in the one scene that declares one (`layout`).

Build in release. The `surfaces` scene carries four image fills and a backdrop
blur, and it is by far the most expensive of the three;
[`corpus/showcase/README.md`](corpus/showcase/README.md) records what each scene
costs, measured, and lists every construct the three of them cover.

The still at the top of this page is not a screenshot. It is written by an
example that steps a fixed 1/60 s and never reads a clock, so the same arguments
reproduce the same picture:

```sh
cargo run -p showcase --example still -- surfaces docs/images/showcase-surfaces.png 1600 1000 0 0
```

To commit rather than only to run, `./bootstrap` installs
[git-std](https://github.com/driftsys/git-std) and wires up the repository's git
hooks. [`CONTRIBUTING.md`](CONTRIBUTING.md) covers the rest.

## What it is

Three stages and two boundaries. Everything that can be decided before the
device starts is decided in stage 1; everything that must be identical across
backends happens exactly once, in stage 2; stage 3 does only what must differ
per target, which is how a rectangle gets coloured.

```text
STAGE 1 — build time, offline
  Figma REST JSON --> dashc --> .dsb (sectioned container) + assets

  boundary A — the .dsb load gate: version and per-section hashes

STAGE 2 — common runtime, one instance, shared Rust
  arena + variants + text stack + Taffy solve + FLIP
    --> rect table + positioned glyph runs, double-buffered

  boundary B — the painter contract: rects, glyph runs, paint indices

STAGE 3 — painters, one trait, one per target
  Skia (built)     Unity (planned)     wgpu (planned)
```

A producer does not have to go through stage 1. The arena's staged-mutation API
(`open` / `set_prop` / `set_variant` / `commit`) is the real contract, and a
`.dsb` file is one way to fill it; the Rust DSL fills it directly. The two paths
are proven to converge: for the layout-and-solid-fill subset both producers
express, the same screen authored in Figma and in the DSL commits an identical
rect table and renders to a byte-identical image.

Five principles bind everything downstream. The first two account for most of
the shape of the codebase:

- **P1 — the document carries intent, never results.** No resolved x, y, width
  or height; no rasterized pixels; no glyph positions. Anything resolved would
  pin the document to one backend or one font build.
- **P2 — one solver, one typesetter; painters only colour.** Layout (Taffy) and
  text placement (rustybuzz) run exactly once, in shared Rust. Every painter
  consumes finished rects and positioned glyph runs, and never measures, wraps,
  kerns, or moves anything. Cross-backend identity is structural, not
  tested-for.
- **P3 — producers mutate, the runtime owns time.** Nothing producer-side runs
  inside the frame loop; all animation is descriptive data.
- **P4 — vocabulary is validated, never discovered.** Every out-of-profile
  construct is a named diagnostic, never a silent drop.
- **P5 — Figma compatibility is a property of one producer.** The dashscene
  document is a schema-first IR with its own specification; no producer's
  limitations define the format.

The full text is in
[`docs/specification/02-principles.md`](docs/specification/02-principles.md), and
the pipeline above is drawn out in
[`docs/design/architecture.md`](docs/design/architecture.md).

## What is built, and what is not

Built: the document format and its sectioned container, the arena and its
staged-mutation API, the Taffy solve, variants and FLIP, the text stack for
Latin and Arabic (bidi, shaping, MSDF glyph atlases), the descriptive animation
vocabulary and its scheduler, the Figma importer, the asset packer, and the Skia
reference painter. The three scenes above draw the whole of the v0 paint
vocabulary between them, and `--dsb` loads a document the Figma importer
compiled.

Not built:

- **The Unity painter.** `crates/dashscene-unity` is a stub; the Unity and C#
  project it will bind to is a separate repository that does not exist yet.
- **`dashscene-gpu`**, the lean painter for native and web. The v0.15 slice
  (epic #569). The crate exists and draws nothing — the `Painter` seam
  compiles, and no pixel path is built yet.
- **The web painter.** `crates/dashscene-web` is a stub, retired: `dashscene-gpu`
  reaches the browser from the same codebase as native.
- **The umbrella crate.** `crates/dashscene` is a stub; code in this repository
  depends on the individual crates, not on a facade.

Verify a document against the load gate — the one thing the compiler CLI does
today:

```sh
cargo run -p dashc -- check goldens/dsb/v03-paint.dsb
```

Compiling Figma REST JSON has no native subcommand. `dashc` also builds to
`wasm32-unknown-unknown`, and the Deno importer under `importers/figma/` calls
that wasm build rather than reimplementing the lowering
([`docs/decisions/figma-importer-deno-plus-dashc-wasm.md`](docs/decisions/figma-importer-deno-plus-dashc-wasm.md)).

## The workspace

Sixteen crates in one Cargo workspace, plus four members that are never
published — `demo/` (the window and the frame loop), `demo-web/` (the same
showcase in a browser, on a canvas), `corpus/showcase/` (the scenes they draw),
and `goldens/tooling/` (the golden-image harness).

| Crate                  | What it is                                                     |
| ---------------------- | -------------------------------------------------------------- |
| `dashbuf`              | the FlatBuffers schema — the `.dsb` document format            |
| `dashc`                | the compiler; also builds to wasm for the Deno Figma importer  |
| `dashscene-validator`  | profiles, diagnostics, waivers                                 |
| `dashscene-core`       | arena, node tree, layout and paint tables, staged mutation     |
| `dashscene-engine`     | Taffy solve, variants, FLIP, the measure callback              |
| `dashscene-typeset`    | bidi, shaping, the glyph atlas pipeline                        |
| `dashcue`              | the descriptive animation vocabulary and its scheduling        |
| `dashlang`             | the Rust DSL and the stress-corpus generator                   |
| `dashpaint`            | the paint table and the painter trait — boundary B             |
| `dashscene-skia`       | the Skia reference painter, and the whole of the v0 painter    |
| `dashpack`             | the asset packer — quality profiles, cold banks, manifests     |
| `dashpack-astcenc-sys` | bindings to the vendored astcenc encoder and reference decoder |
| `dashscene`            | the umbrella crate — a stub                                    |
| `dashscene-unity`      | Rust FFI bindings for the Unity painter — a stub               |
| `dashscene-web`        | the wasm and tiny-skia painter — a stub, retired               |
| `dashscene-gpu`        | the lean painter over wgpu — the seam only, draws nothing      |

Beside them: `importers/figma/` (the Deno and TypeScript Figma REST importer and
its annotator plugin), `corpus/` (the stress corpus, fonts, glyph atlases and
captured Figma fixtures), and `goldens/` (the committed images and the diff
tooling).

## Status

This is `driftsys/dashscene-staging`, a **private working repository**. Nothing
here is published: the crate names on crates.io are placeholder reservations
made before development started, and no code from this repository has been
released under any of them.
[`driftsys/dashscene`](https://github.com/driftsys/dashscene) is a
separate public repository, reserved as the project's future facade; it holds
those names and no working code. Staging's content is promoted into it once
there is a real version running, and the mechanism is deliberately still
undecided
([`docs/decisions/repo-staging-and-public-facade.md`](docs/decisions/repo-staging-and-public-facade.md)).

v0 is built one slice at a time. Slices v0.1 to v0.13 have closed, except v0.9,
which stays open on its exit gate — the first bullet below. v0.14 — the
demonstration above and this file — is the current slice; v0.15 and v0.16 are
planned.
[`docs/roadmap.md`](docs/roadmap.md) carries the slice map and what each one
delivered; GitHub issues carry the live state.

Two things about qualification, stated plainly because they are easy to read the
wrong way:

- Seven exit criteria, `E1` to `E7`, gate v0. Each one is met and each one is
  individually evidenced
  ([`docs/specification/05-qualification.md`](docs/specification/05-qualification.md)).
  What does **not** exist is the single CI job that asserts all seven together
  on one commit, so a regression in any of them fails a build rather than
  waiting for a person to notice.
- GitHub Actions on this repository is blocked at the account level. Jobs fail
  in seconds having executed zero steps. That is why this file carries no CI
  badge and makes no claim that CI is green, and it is what the exit gate above
  is waiting on.

## Where to read next

- [`docs/specification/`](docs/specification/) — goals, requirements,
  principles, the target-hardware rules, the Figma vocabulary profile, and the
  qualification chain from each requirement to its proof.
- [`docs/design/architecture.md`](docs/design/architecture.md) — the stack, the
  pipeline, its two boundaries, and a map from every crate to its own as-built
  design record.
- [`docs/decisions/`](docs/decisions/) — every decision taken since, each traced
  to what it affects.
- [`docs/features.md`](docs/features.md) — what the system does today and what
  is planned, feature by feature, written for a non-engineering reader.
- [`docs/roadmap.md`](docs/roadmap.md) — the v0, v1 and v2 plan.
- [`AGENTS.md`](AGENTS.md) — the working conventions this repository runs on.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to get a change in. `just --list`
  prints the recipe set; `just build` is the full local gate.

## Licence

MIT — see [`LICENSE`](LICENSE). One vendored source tree carries a different
licence; [`NOTICE`](NOTICE) records which and why.
