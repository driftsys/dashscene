# The showcase scenes

The content the windowed demonstration draws (v0.14, story #574, epic #568).
Three scenes, authored in `dashlang` against its reactive API, covering the v0
paint vocabulary.

They live under `corpus/` and not under `demo/` because they exercise the full
vocabulary, which is what the stress corpus is for; two parallel scene sets
would be two sets that drift. `demo/` holds the host — a window, a clock, and a
frame loop. This holds what it draws.

## Running them

```text
cargo run -p demo                # surfaces, the default
cargo run -p demo -- typography
cargo run -p demo -- layout
cargo run -p demo -- --list
```

Every 2.5 s the host advances the scene's scripted phase, the scene animates to
its new values, and the loop parks again once nothing is moving. The log says
so:

```text
demo: settled at generation 48 after 50 ticks and 49 presents — waiting for an event
demo: woken by pulse 2 after 0.96 s parked — 0 ticks and 0 presents ran while parked
```

The second line is the idle-frame skip reporting itself: nothing ran for the
second the scene sat still. Every scene is tuned to settle inside the 2.5 s
pulse interval so that gap exists to be seen.

To take a still — the picture the entry-path documentation shows:

```text
cargo run -p showcase --example still -- surfaces docs/images/showcase-surfaces.png 1600 1000 0 0
```

That is the exact command `docs/images/showcase-surfaces.png` was produced
with.

The arguments are the scene, the output path, the width and height in physical
pixels, how many seconds of animation to run first, and which scripted phase to
run towards. The example steps a fixed 1/60 s and reads no clock, so the same
arguments produce the same picture.

## Coverage is a checklist a person walks, not a test

**There is no coverage test in this crate and there are no golden images for
these scenes. Do not add either.**

Epic #568 records why: a demonstration wired into CI becomes a suite whose green
state reads as evidence it never established — the `t2-check-has-no-teeth`
failure the v0.13 test tiering exists to remove. `goldens/` holds the frames the
project pins; these are frames it shows. The only automated claim this crate
makes is that it compiles.

So the coverage claim is the table below, and it is true when a person has run
the three scenes and seen each line.

| # | construct | scene | what to look for |
| --- | --- | --- | --- |
| 1 | solid fill | `surfaces` | tile 1, the dark panel inside the amber outline |
| 2 | linear gradient | `surfaces` | tile 2, crimson to violet, left to right |
| 3 | radial gradient | `surfaces` | tile 3, amber centre falling to crimson at the corners |
| 4 | angular gradient | `surfaces` | tile 4, the sweep around the centre with the seam at the right |
| 5 | diamond gradient | `surfaces` | tile 5, a rotated square of teal inside violet |
| 6 | stroke, `Inside` | `surfaces` | tile 1, the amber outline sits wholly within the tile's box |
| 7 | stroke, `Center` | `surfaces` | tile 2, the white outline straddles the edge, half in and half out |
| 8 | stroke, `Outside` | `surfaces` | tile 3, the teal outline sits wholly outside, so the tile reads larger |
| 9 | corner radii | `surfaces` | tile 16, top-left and bottom-right deeply rounded, the other two square |
| 10 | image fill, `Fill` | `surfaces` | tile 6, the photograph covers the tile with no border |
| 11 | image fill, `Fit` | `surfaces` | tile 7, the photograph sits inside a panel with the panel showing either side |
| 12 | image fill, `Crop` | `surfaces` | tile 8, the same photograph at half the sampled region, so the mountains are larger |
| 13 | image fill, `Tile` | `surfaces` | tile 9, the photograph repeated four times over |
| 14 | baked vector MSDF field | `surfaces` | tile 10, a star with a pentagonal hole, its edges clean at any window size, filled by a gradient the field masks |
| 15 | drop shadow | `surfaces` | tile 11, the white tile casts down and to the right |
| 16 | inner shadow | `surfaces` | tile 12, the green tile is darkened inside its own edges |
| 17 | clip | `surfaces` | tile 13, a circle: the child is half again the tile's size and the rounded clip is what makes it a disc |
| 18 | mask | `surfaces` | tile 14, the angular gradient appears only inside the rounded mask sibling, which draws nothing itself |
| 19 | group opacity | `surfaces` | tile 15, two overlapping squares at 0.55 — the overlap is **not** darker, which is what makes it a render-target group and not a per-rect alpha |
| 20 | backdrop blur | `surfaces` | the frosted panel, sliding across the gallery: what is under it is blurred, what is beside it is not, and the blur changes as it travels |
| 21 | MSDF text, Latin | `surfaces`, `typography` | the header title and subtitle; the whole of `typography` above the Arabic panel |
| 22 | MSDF text, Arabic (bidi) | `typography` | the panel's three runs: the greeting flushes **right**, the second lam joins its alef into a lam-alef ligature, the harakat stack above the letters, and the authored European digits in the third run render as Arabic-Indic shapes because their context is Arabic |
| 23 | text style axes | `typography` | the paragraph wraps inside its own box at a fixed line height with letter spacing; the readout is vertically centred in its box |
| 24 | clipped text | `typography` | the bottom line is cut mid-word at the box edge — glyphs take the same resolved clip regions rects do |
| 25 | signal-driven text | `typography` | the readout changes with each phase (0, 108, 240, 168 km/h) and the bar beside it tracks the same signal |
| 26 | springs | all three | nothing steps: every change eases in and settles, and interrupting one mid-flight resumes from where it is |
| 27 | flex, `Vertical` + `Hug` | `layout` | the first panel is exactly as tall as its three chips and no taller |
| 28 | flex, `Fill` split | `layout` | the second panel: one fixed chip, then two that share what is left equally |
| 29 | flex, `Wrap` | `layout` | the third panel: seven chips over two lines, the line spacing wider than the spacing within a line |
| 30 | grid, tracks and spans | `layout` | one fixed column and two fractional ones; the violet cell spans two rows, the crimson one spans two columns, and the last cell keeps its own fixed size at its cell origin instead of stretching |
| 31 | reflow on a topology change | `layout` | the bottom row: the outlined middle chip leaves and rejoins, its siblings close up and re-open, and the gap between them animates |

## What the scenes do not cover, and why

Three items on the slice's list are **not** shown, and none of them is an
oversight.

**A variant switch, and the FLIP that animates one.** `Txn::set_variant` needs
the arena, and `VariantFlip` needs the before and after rects plus a per-frame
`advance` and a composed commit. The host's scene seam hands a builder an
`&mut Arena` once, at build time, and then hands the scripted phase only an
`&mut LiveScene` — so neither is reachable from a scene. Line 31 above shows the
committed *effect* a variant switch has (a child leaves the laid-out set and its
siblings reflow), driven through `Prop::Visible`, which is the same change
`corpus/dsl-generated/variant-topology.md` proves through a variant set. What is
missing is the variant machinery, not the picture. Story #573 needs the same
seam for "one key cycling a variant set", so this is one gap and not two.

**`dashcue` keyframes and tweens.** `dashcue` carries `TransitionSpec::Tween`
and `TransitionSpec::Keyframes`, and `dashlang::Node::smooth` accepts only a
`Spring`. Every eased motion in these scenes is therefore a spring. Reaching the
other two specs from an authored scene is a `dashlang` widening, not a scene.

## The defect these scenes are written around

**A tick that commits without solving publishes a scene with no glyph runs in
it.** `LiveScene::tick` commits through a rect-replaying solver whenever no
binding forced a re-solve, that solver takes `LayoutSolver`'s defaulted
`atlases` and `stage_text`, and commit rebuilds the glyph-run table from
whatever the solver stages — so every run disappears until the next frame that
does solve. It is not a text-specific path that is wrong; it is that text has
never been driven through the reactive layer before.

It bites hardest exactly where it is least expected: a `bind_text` write is
itself paint-only, so a scene whose only animation is a changing string blanks
that string the moment it changes.

Every scene here is written so that **every signal drives at least one
layout-affecting channel**, which keeps every commit a solving commit and every
glyph run staged. That is why `surfaces` animates the header's width rather than
its fill alpha, and why `typography`'s readout shares its signal with a bar that
reflows. Replaying the host's loop over all three scenes for eight phases at
both 960x600 and 1920x1200 — 1,200 ticks and around 700 commits each — the
staged run count never falls below its starting value.

The fix belongs in `dashlang`: the rect replay should delegate `atlases` and
`stage_text` to the solver the live scene already owns. It is filed rather than
made here, because `crates/dashlang/src/reactive.rs` is under change on another
branch.

A second, smaller gap: `Scene::signal_named` declares and looks up **scalar**
signals only, so a bool signal — the kind `visible_when` takes — has no runtime
name, and a scripted phase handed only a `LiveScene` cannot ask for one.
`layout.rs` keeps its handle in a `OnceLock` as a result.

## How a scene is built

In two passes.

`dashlang`'s builder carries geometry, the flex vocabulary, one solid fill and
the reactive bindings, and none of the paint vocabulary — no gradient, stroke,
corner, shadow, blur, image, mask, clip, opacity, vector-field or text-style
setter exists on `dashlang::Node` (the DSL's vocabulary gap, issue #118). So
structure, layout and motion are authored through `dashlang`, and then a second
pass stages the paint intent onto the named nodes through
`dashscene_core::Txn`. `corpus/dsl-generated` already splits this way, where two
of its six cases go through core's `Txn` for constructs the builder does not
carry.

The second pass is safe to run against a live scene's arena because everything
it stages is paint intent, which no solver resolves: replaying the retained rect
cache reproduces exactly the geometry the pass committed against. It commits
through a text-capable solver rather than a rect replay, for the reason above.

## What is reused from the corpus, and what is new

Reused as-is:

- `corpus/fonts/inter/` (Regular and SemiBold) and
  `corpus/fonts/noto-sans-arabic/` — the cascade the scenes shape with.
- `corpus/atlas/inter-ascii`, `corpus/atlas/inter-ascii-semibold` and
  `corpus/atlas/arabic` — the committed MSDF glyph atlases, unchanged.
- The three Arabic strings, verbatim from `goldens/tooling/tests/v06_arabic.rs`.
  They are reused rather than re-authored because the committed Arabic atlas was
  baked for exactly their glyph closure: a new sentence would need a new atlas
  bake to render at all.
- `corpus/photo/dawn-mountains.png` — the CC0 payload the asset pipeline
  measures its quality bands against, drawn here by all four image fills.
- The golden scenes' colour palette
  (`goldens/tooling/tests/common/mod.rs`), so the showcase reads as the same
  project rather than as a second visual identity.

Newly authored, because nothing in the corpus covers it:

- The three scenes themselves. The existing corpus is a set of *edge cases*
  proved against hand-computed rects and pixel goldens — wrap, hug-in-fill, grid
  spans, baseline, variant topology, negative gap — plus captured Figma
  fixtures. None of them is a scene meant to be watched, none is animated, and
  none covers the paint vocabulary breadth-first.
- The star outline in `resources.rs`, baked to a distance field by the same
  `dashc` generator a Figma VECTOR node lowers through. The corpus carries
  `vector-shapes.json` as a captured Figma fixture, but no reusable baked field.

## Known costs

Measured on this branch, `cargo run --release`, on an Apple-silicon macOS
machine (M-series, 2026-07-31), replaying the host's loop over eight scripted
phases at 1920x1200 and timing `SkiaPainter::paint` alone:

| scene | mean | max | commits of 1,200 ticks |
| --- | --- | --- | --- |
| `surfaces` | 28.3 ms | 148.6 ms | 724 |
| `typography` | 6.7 ms | 21.0 ms | 646 |
| `layout` | 0.7 ms | 3.0 ms | 366 |

At 960x600 the same three read 15.0 ms, 4.1 ms and 0.2 ms.

A measurement, not a threshold, and not the slice's frame budget — that is the
epic's to record, over both the static and the animated case. Three things this
does say:

- `surfaces` is forty times the cost of `layout`, and it is the scene with four
  image fills and a backdrop blur in it. Issue #101 (image assets re-decoded per
  rect, per paint) is the debt this scene stands on: at 28 ms it does not hold
  60 Hz at 1920x1200, and the loop degrades to running flat out, which is
  exactly what `demo/src/shell.rs` says a frame that overruns the interval does.
- A debug build measures the same, within a few per cent. The cost is in Skia
  and in the per-frame image decode, neither of which is Rust the profile
  affects.
- `crate::solver` rebuilds Taffy's retained tree on every solve, because
  `LiveScene` holds a `'static` boxed solver and `TaffySolver` borrows its
  typesetter. It is a bounded cost, unlike leaking one typesetter per window
  resize, but it is a cost, and it is inside the tick rather than in the numbers
  above.
