# Prior art and related work

Other projects that solve nearby problems, and the parts of the ecosystem
dashscene is built on.

**This page is the entry point for how dashscene relates to nearby projects,
and the only place a claim about one carries a pinned citation and a retrieval
date.** It is not the only page that mentions another project, and saying so
would be false: [`glossary.md`](glossary.md) defines eleven of them in one line
each, [`producers-and-ir.md`](producers-and-ir.md) §4–§5 carries the Penpot and
build-vs-adopt analyses, and
[`rendering-and-painters.md`](rendering-and-painters.md) §8 carries a
performance comparison written before this page existed and not yet reconciled
with it. Those are linked from here rather than duplicated, and where one of
them disagrees with this page, **this page is wrong until proven otherwise** —
it is the one with the dates on it.

**Every factual claim below is either checked against the project's own
repository on the date given, or it is not made.** Where a question would need
a judgement about another team's engineering to answer, this page does not
answer it. Nothing here scores projects against each other, and there is no
feature matrix: a matrix drawn by one project about its neighbours measures
the author's requirements, not the neighbours.

If you maintain something described here and the description is wrong or out
of date, please open an issue. That is a defect in this file.

Retrieved 2026-08-10 unless stated otherwise.

## Nearby problems

**[Automotive Design for Compose](https://github.com/google/automotive-design-compose)**
— Apache-2.0, written largely in Rust, actively developed. Google describes it
as "an extension to Jetpack Compose that allows every screen, component, and
overlay of your Android App to be defined in Figma, and lets you see the latest
changes to your Figma design in your app, immediately!"

This is the closest adjacent project: it also takes Figma as the design source
and keeps the design file authoritative rather than generating code once.
dashscene differs in what it renders into — the same document is drawn by a
game engine, by a lean native painter and by a reference rasterizer, which must
agree to the pixel — and in carrying its own schema-first intermediate
representation with a validator, rather than targeting one UI toolkit.
Those are different goals, not a ranking.

**[Slint](https://github.com/slint-ui/slint)** — "an open-source declarative
GUI toolkit to build native user interfaces for Rust, C++, JavaScript, or
Python apps". Its framework is triple-licensed and a user may choose any one:
a royalty-free licence covering proprietary desktop, mobile and web use, which
excludes embedded systems; GPL-3.0-only, covering open-source software on any
platform including embedded; or a commercial licence covering proprietary use
including embedded
([LICENSE.md](https://github.com/slint-ui/slint/blob/54774e5234e26be2c046024aad22521bc00121a2/LICENSE.md),
pinned to the commit that was current on the retrieval date).
The royalty-free tier carries a condition worth stating precisely: it is free
"as long as you disclose that you use Slint (for example with the `AboutSlint`
widget or the Slint badge); without that disclosure, use the Commercial
license."

Slint draws its own output; dashscene hands a document to renderers it does not
own. The full reasoning, including why Slint is not a code source for this
repository, is in
[`docs/decisions/no-gui-toolkit-dependency.md`](../decisions/no-gui-toolkit-dependency.md).

**[Penpot](https://github.com/penpot/penpot)** — MPL-2.0, "the open-source
design platform for Product teams that need scalable collaboration".
Self-hostable. Its layout model is literal CSS flexbox and grid, which maps
closely onto dashscene's layout table, so it is a candidate second producer;
that analysis is in [`producers-and-ir.md`](producers-and-ir.md) §4.

**[Flutter](https://github.com/flutter/flutter)** — BSD-3-Clause, Google's UI
framework, which ships its own renderer (Skia, and now Impeller). Used in
embedded and in-vehicle products: Toyota Connected publishes
[`ivi-homescreen`](https://github.com/toyota-connected/ivi-homescreen), "a
Flutter Linux C++ embedder for desktop and embedded/automotive displays".
Referenced in this repository as a frame-time reference point, and because
Impeller precompiles its shaders — a requirement dashscene shares, since a
shader compiled mid-frame costs time a fixed budget does not have.

## Animation and vector formats

**[Rive](https://github.com/rive-app/rive-runtime)** — MIT, "low-level C++ Rive
runtime and renderer". A state-machine-driven animation format with its own
runtime. dashscene's animation vocabulary (`dashcue`) is deliberately narrower:
transitions, springs, keyframes and FLIP, scheduled by the runtime, with no
scripting inside the document. A richer declared state-machine layer is a
question this project has recorded rather than answered.

**[Lottie](https://github.com/airbnb/lottie-android)** — Apache-2.0, Airbnb's
runtime for rendering After Effects animations. A JSON vector-animation format
in wide use. dashscene triages Lottie input by kind rather than adopting it
wholesale, and that triage is in [`runtime-content.md`](runtime-content.md)
§4–§6.

**[ThorVG](https://github.com/thorvg/thorvg)** — MIT, "a production-ready C++
vector graphics engine supporting SVG and Lottie formats". dashscene's use of
it is narrow by choice — a render-to-texture escape hatch for runtime vector,
and an offline frame renderer — because painting the document itself stays with
dashscene's own painters. That is a scope decision about dashscene.

## What dashscene is built on

None of the following is a competitor; all of it is work this project depends
on and would not exist without.

- **[Taffy](https://github.com/DioxusLabs/taffy)** — the flexbox and grid
  solver. dashscene has one layout engine and this is it.
- **[rustybuzz](https://github.com/harfbuzz/rustybuzz)** — HarfBuzz's shaping
  algorithm in Rust. Complex-script text is correct here because of it.
- **[ttf-parser](https://github.com/harfbuzz/ttf-parser)** and
  **[unicode-bidi](https://github.com/servo/unicode-bidi)** — font tables and
  the bidirectional algorithm.
- **[resvg / usvg / tiny-skia](https://github.com/linebender/resvg)** — the
  pure-Rust SVG and rasterization stack.
- **[Skia](https://skia.org)**, via
  [skia-safe](https://github.com/rust-skia/rust-skia) — the reference painter,
  and the oracle every other painter is checked against.
- **[wgpu](https://github.com/gfx-rs/wgpu)** — the lean painter's GPU
  abstraction, native and web from one codebase.
- **[msdf-atlas-gen](https://github.com/Chlumsky/msdf-atlas-gen)** and
  **[msdfgen](https://github.com/Chlumsky/msdfgen)** — the multi-channel signed
  distance field generator behind the glyph atlas.
- **[astcenc](https://github.com/ARM-software/astc-encoder)** — Arm's ASTC
  encoder, vendored and Apache-2.0. Attribution is in
  [`NOTICE`](../../NOTICE), with provenance and the steps to re-derive the copy
  in [`vendor/VENDOR.md`](../../crates/dashpack-astcenc-sys/vendor/VENDOR.md).

Content, not code, and equally depended on:

- **[Inter](https://github.com/rsms/inter)** and
  **[Noto Sans](https://github.com/notofonts/latin-greek-cyrillic)**, Latin and
  Arabic, under the SIL Open Font License 1.1. Noto Sans Arabic is what R1's
  shaped-Arabic verification renders; without it the requirement could not be
  tested at all.
- **Four CC0 photographic payloads from
  [Wikimedia Commons](https://commons.wikimedia.org)** — sources, retrieval
  dates and the verification method are in
  [`corpus/photo/README.md`](../../corpus/photo/README.md).

[`NOTICE`](../../NOTICE) is the canonical attribution record; this section is a
reader's summary of it and defers to it wherever the two differ.

**On lineage.** Earlier revisions of this repository credited a
"Taffy/Servo/Bevy/Slint/Zed lineage" for the ideas the architecture grew from,
and this page repeated it as though `docs/design/architecture.md` recorded it.
It does not — the phrase survives only in `docs/archive/`. Taffy is a real
dependency and is credited above; the others are projects whose designs were
read, which is not the same relationship and does not belong in a list headed
"what dashscene is built on". Slint in particular is **not** a dependency; see
[`no-gui-toolkit-dependency.md`](../decisions/no-gui-toolkit-dependency.md).
