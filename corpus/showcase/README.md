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

## Driving them by hand

The scripted phase runs on its own, and three inputs drive the same scene at
the same time (story #573):

| input | what it does |
| --- | --- |
| pointer, left to right | scrubs the scene's own scalar signal across `0.0` to `1.0` |
| Left Arrow / Right Arrow | snaps that signal to `0.0` / `1.0` |
| Space | runs the scene's own variant switch, in the one scene that has one |

The host knows none of this by name. A scene carries the **name** of the signal
it wants driven and, optionally, a **function** the variant key calls, and the
host passes both through without reading them — `Showcase::signal` and
`Showcase::action` in `src/lib.rs`. That seam is what makes a variant switch
reachable at all; the "what the scenes do not cover" section below records what
it still does not reach.

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
the three scenes, driven them with the pointer and the keys above, and seen each
line.

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
| 31 | reflow on a topology change, through `Prop::Visible` | `layout` | the bottom row: the outlined middle chip leaves and rejoins, its siblings close up and re-open, and the gap between them animates |
| 32 | variant switch, through `Txn::set_variant` | `layout` | press **Space** in the bottom row: the rightmost chip narrows and turns teal, then leaves the laid-out set entirely, then comes back — three members overriding `Width`, `Fill` and `Visible`, and the row re-centres at each step. The same picture as line 31 by a different mechanism, which is the pairing `corpus/dsl-generated/variant-topology.md` already proves |
| 33 | signal driven by input | all three | move the pointer left and right, or press Left Arrow and Right Arrow: the same signal the scripted phase drives moves under the pointer, through the same springs |
| 34 | painter badge | all three | a dark pill in the top-left corner naming the painter that drew the frame: `dashscene-skia` by default, `dashscene-gpu` after pressing **P**. It is empty and fully transparent until the host announces a painter, which is why the still-image example renders nothing in its place |

## What the scenes do not cover, and why

Two items on the slice's list are **not** shown, and neither is an oversight.

**`VariantFlip`, which animates a variant switch.** The switch itself is line 32
above and is real. Animating it is not, and what stands in the way is no longer
the seam. FLIP needs the before and after rect slices around the switch plus an
`advance(dt)` and a commit composing its samples over the after layout **once
per frame** (`goldens/tooling/tests/v04_flip.rs` is the worked example), and
`LiveScene::tick` does all of that: it detects a **staged** `set_variant`,
re-solves for the after layout, binds the transition the member declares and
composes the samples on the frames that follow (story #771). Since issue #950
`layout::switch_variant` stages rather than commits, so the switch goes through
exactly that path.

What is missing is the declaration. No member of the chip's set carries a
`Txn::set_variant_transition`, and a member with none starts no track and lands
whole — which is what the switch does today, one tick after the key. Declaring
one on a member is now the whole of what animating this would take, where before
it needed a per-frame scene driver the seam did not have.

**`dashcue` keyframes and tweens.** `dashcue` carries `TransitionSpec::Tween`
and `TransitionSpec::Keyframes`, and `dashlang::Node::smooth` accepts only a
`Spring`. Every eased motion in these scenes is therefore a spring. Reaching the
other two specs from an authored scene is a `dashlang` widening, not a scene.

## The defect these scenes were written around — fixed at issue #621

**A tick that committed without solving published a scene with no glyph runs in
it.** `LiveScene::tick` commits through a rect-replaying solver whenever no
binding forced a re-solve; that solver took `LayoutSolver`'s defaulted `atlases`
and `stage_text`, and commit rebuilds the glyph-run table from whatever the
solver stages — so every run disappeared until the next frame that did solve. It
was not a text-specific path that was wrong; it was that text had never been
driven through the reactive layer before.

It bit hardest exactly where it was least expected: a `bind_text` write is itself
paint-only, so a scene whose only animation was a changing string blanked that
string the moment it changed.

**Issue #621 fixed it.** `CachedSolver` in `crates/dashlang/src/reactive.rs` —
the rect replay `LiveScene::tick` commits through when no binding forced a
re-solve — now borrows the scene's own solver and forwards both `atlases` and
`stage_text` to it, so a paint-only commit keeps its text.

Every scene here is still written so that **every signal drives at least one
layout-affecting channel**, and that is now a property of how the scenes were
tuned rather than a requirement. It is also why these scenes could not catch the
defect: every commit in them is a solving commit, so the run count never fell —
the tests that pin the fix are in `crates/dashlang/tests/reactive.rs`, on a scene
built for it. That is why `surfaces` animates the header's width rather than its
fill alpha, and why `typography`'s readout shares its signal with a bar that
reflows. Replaying the host's loop over all three scenes for eight phases at
both 960x600 and 1920x1200 — 1,200 ticks and around 700 commits each — the
staged run count never falls below its starting value.

That same property used to be what made `layout`'s variant switch safe, and it
is not what makes it safe now. The switch committed geometry from outside
`LiveScene::tick`, and the argument was that no tick in `layout` commits without
solving — `spread` binds `Channel::Gap`, always a solve in `dashlang`, and
`show_middle` is a visibility binding, always a reflow — so no tick could replay
a stale rect cache over it. The argument was sound for the cache it named and
too shallow for the one underneath: once each scene retained **one** solver, the
switch's own commit consumed the arena's layout-dirty set and the scene's solver
patched a Taffy tree that no longer described the row. The switch stages now and
`LiveScene::tick` publishes it, so nothing commits into these arenas but the
scene's own solver (issue #950,
`docs/decisions/one-solver-per-live-scene.md`).

A second defect in the same seam is still open, and it is smaller than the one
this section is titled for: `Scene::signal_named` declares and looks up
**scalar** signals only, so a bool signal — the kind `visible_when` takes — has
no runtime name, and a scripted phase handed only a `LiveScene` cannot ask for
one. `layout.rs` keeps its handle in a `OnceLock` as a result. Issue #623
tracks it.

## How a scene is built

In one pass, for the most part. `dashlang::Node` carries the whole v0 paint
vocabulary — fill, gradient, stroke, corner, shadow, blur, mask, clip, opacity,
vector-field and text-style setters all exist on it now — alongside geometry,
the flex vocabulary and the reactive bindings, so structure, layout, motion and
paint are authored together on one value tree. An image fill has a setter too
(`fill_with`, which takes any `FillSpec`), but authoring one still needs the
arena, for the reason the next paragraph gives.

Two constructs still need a short second pass over the built arena, staged
through `dashscene_core::Txn` and addressing nodes by the name they were given
on the tree: an image fill (including a cropped one and a baked vector field's
coverage mask), because each references an index `Txn::add_image` issues
against the arena, and no such index exists until the tree is built; and a
variant-set declaration, because `Txn::add_variant_set` is likewise an arena
operation. `surfaces` runs this second pass for its image fills and its vector
field; `layout` runs it only to declare its variant set; `typography` needs no
second pass at all.

The second pass is safe to run against a live scene's arena because everything
it stages is either paint intent or arena metadata, and none of it is resolved
by a solver: replaying the retained rect cache reproduces exactly the geometry
the pass committed against. It commits through a text-capable solver rather
than a rect replay, so the text the first pass already staged is not wiped out.

That safety matters more than it used to, because a retained Taffy tree is only
correct while nothing else consumes the arena's layout-dirty set, and this is the
one commit left in these scenes that does not go through the scene's own solver.
Consuming is now the solver's own answer rather than something every commit does
— `LayoutSolver::consumes_layout_dirty`, `true` by default and `false` for a
replay (issue #1148) — which does not soften the sentence above: this pass
commits through a real solver, so it answers `true` and drains, exactly as
before.
`tests/retained_tree.rs` builds each scene and compares its committed rects
against a from-scratch solve, at build time and after every scripted phase and
every variant press.

**It is a check and not a proof, and the file says which.** A stale tree reaches
the committed table only once some later commit's readback descends to the
affected node, so the test catches a second pass that stages layout intent on a
node the scripted phases reflow, and misses one on a node they never touch —
measured both ways, in `layout` and in `surfaces` respectively. Closing that gap
needs the detector in issue #1104 rather than a scene-level test; issue #1118
carries it. Until then, "stage paint intent and arena metadata, never geometry"
is a rule this pass keeps rather than one the suite enforces.

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

**These figures predate issues #639 and #644**, both of which removed per-frame
decoding after the table was taken: #639 moved the image decodes onto the
painter, and #644 did the same for the MSDF glyph atlases and the
resolve-shader compile. `docs/technotes/frame-budget.md` measures both fixes,
each against its own before-and-after run. Its figures are **not** this table
re-taken — that note times `paint` offscreen as a median of per-frame medians,
where this table is a mean over an interactive replay, and its own "before"
column for `surfaces` does not agree with the 28.3 ms here. Read the two as
separate measurements rather than as one series.

A measurement, not a threshold, and not the slice's frame budget — that is the
epic's to record, over both the static and the animated case. Three things this
does say:

- `surfaces` is forty times the cost of `layout`, and it is the scene with four
  image fills and a backdrop blur in it. At 28 ms it did not hold 60 Hz at
  1920x1200, and the loop degrades to running flat out, which is exactly what
  `crates/dashscene-desktop/src/host.rs` says a frame overrunning
  `FRAME_INTERVAL` does. **The debt this scene stood on, issue #101, is
  closed** — image assets are no longer decoded per rect, and since issue #639
  not once per frame either. What it carries now is two residuals rather than
  one, and `crates/dashscene-skia/src/lib.rs`'s `ImageCache` doc states both
  with their measured bytes: the cache holds one decode per asset until the
  table changes or the painter is dropped, but the *decoded* pixels are not
  held there — Skia keeps that raster in a global resource cache that purges
  under pressure, so a document whose images decode past the limit is
  re-inflated whatever this cache does — and establishing that the table has
  not changed walks its encoded payloads once per frame, a cost linear in the
  table's encoded size. Read that doc for the numbers rather than keeping a
  second copy of them here; there is no memory budget anywhere in the project
  to size any of it against (issue #462).
- A debug build measures the same, within a few per cent. The cost is in Skia
  and in the per-frame image decode, neither of which is Rust the profile
  affects.
- The scenes no longer rebuild Taffy's retained tree on every solve. They did
  until issue #950, because `LiveScene` holds a `'static` boxed solver and
  `TaffySolver` borrows its typesetter, so the crate owned the typesetter in a
  wrapper and built a solver inside every call. `TaffySolver::owning` holds the
  typesetter instead and keeps the tree, which is what
  `crate::resources::solver` now builds. Measured over 1,200 ticks with a
  layout-affecting write each, `--release` on macOS aarch64, median tick:
  `layout` 0.021 ms to 0.006 ms, `typography` 0.037 ms to 0.022 ms, `surfaces`
  unchanged at 0.010 ms. Roughly half the tick, and invisible against a paint
  measured in the milliseconds above: the reason to do it was one answer in the
  tree rather than the clock. Why `surfaces` did not move is **not** explained —
  the note taken with the measurement said its pulse drives paint channels, and
  it does not: `sweep` binds `Channel::Width` on the header and `Channel::X` on
  the frost panel, both of which force a solve. Anyone re-taking these numbers
  should treat that row as unexplained rather than as accounted for.
