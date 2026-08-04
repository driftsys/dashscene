# dashpaint — boundary B

    crate    crates/dashpaint
    covers   v0.1 walking skeleton (story #3) + v0.3 paint vocabulary
             (story #13) + resolved subtree clips (story #97) + v0.5
             glyph-run table (story #30) + v0.8 group opacity (story #44)
             + v0.11 backdrop contract (story #393)

## Purpose

`dashpaint` defines boundary B (`docs/design/architecture.md`): the
complete input a painter consumes, and the trait every painter implements.
Principle P2 (`AGENTS.md`) holds throughout — a painter only colors; it
never measures, wraps, kerns, or moves anything.

Boundary B is a rect table plus a paint table plus a clip table. The paint vocabulary is
the v0.3 slice's set (`docs/roadmap.md`'s v0.3, drawn from
`docs/specification/04-figma-vocabulary-profile.md`'s NOW list): solid fills, the four
gradient kinds, image fills with scale modes, stroke with align,
per-corner radii, and clip. The crate has no dependencies, including no
`dashscene-core` and no `dashbuf` — see
`docs/decisions/dashpaint-owns-boundary-b-types.md` for why.

Since v0.12 that "no dependencies" is enforced by a test rather than only
observed, because a second thing now lives here that depends on it.

## `image_id` — the shared image header parser

`crates/dashpaint/src/image_id.rs` identifies PNG, JPEG, and GIF by magic
signature and parses just the header for the intrinsic width and height. It
sits in this crate for a reason that is about publish order rather than about
painting: every crate that writes an `AssetEntry` (`dashc`, `dashpack`) and the
gate that checks them (`dashscene-validator`) must reach one implementation, and
`dashpaint` publishes before all three. It already owns `ImageFormat`, which is
the type the parser's answer is phrased in.

It **never decodes**. That boundary matters more here than it did in `dashc`,
because `dashc` depends on this crate — so anything reachable from `dashpaint`
is reachable from the compiler, and `docs/decisions/dashc-identifies-images-never-decodes.md`
keeps pixel reconstruction out of the compiler permanently. The guard is the
dependency-free property above: no production-grade decoder is written without a
dependency, so `manifest_carries_no_third_party_dependencies` fails on the
manifest line that would introduce one. The packer's decode belongs in the
packer, which publishes after everything here
(`docs/decisions/image-header-parser-lives-in-dashpaint.md`).

## The boundary-B contract

- The rect-table index is the document DFS node index
  (`docs/design/dashbuf.md`); a `RectEntry` carries no id field of its own.
- `RectEntry.paint` is an index into the `PaintTable`.
- `RectEntry.clip` is an index into the `ClipTable`. Clipping crosses
  this boundary already resolved: `dashscene-core` walks the clipping
  ancestors at commit, because a flat rect table carries none for a
  painter to walk (P2, story #97). **Masks reuse this table** (story #44):
  a mask node's box is added at commit to the clip regions of the siblings
  it stencils, so a painter needs no mask concept.
- `RectEntry.opacity` is the rect's resolved _free_-path group alpha
  (story #44): the product of the enclosing group opacities that took the
  free path, `1.0` when none applies. A painter multiplies the rect's
  paint alpha by it. The _render-target_ path crosses separately, as a
  `groups: &[GroupComposite]` parameter on `Painter::paint` — each names a
  subtree rect range `[start, end)` and the alpha its offscreen layer
  composites at, so an overlapping group at partial opacity flattens
  before its alpha applies. Both are resolved by `dashscene-core` at commit
  from `Prop::Opacity` intent (`docs/decisions/masks-and-group-opacity.md`).
- Text crosses as a `GlyphRunTable` — positioned glyph runs plus the
  MSDF atlases they sample (story #30,
  `docs/decisions/glyph-runs-cross-boundary-b.md`). Runs arrive already
  shaped, wrapped, and positioned in absolute document space by the one
  typesetter; the painter draws each glyph as a textured atlas quad and
  never moves anything (P2). The atlas is a plain mirror of the
  `dashscene-typeset` metrics blob, so `dashpaint` still depends on no
  crate.
- A `GlyphRun`'s quads are a `GlyphRange` — `(offset, count)` into the
  table's one flat quad array, read with `GlyphRunTable::quads` (story
  #578). Same shape and same two reasons as `ClipRegion`: a `Vec` per run
  has no C representation, and a flat array plus a range uploads as one
  buffer copy where a `Vec` per run is a pointer chase. `push_run` assigns
  the range and **refuses** a run that already carries one, because a
  producer cannot know where its quads will land in a table it has not
  entered — and commit sorts runs by anchor before pushing, so no offset a
  stager computed would survive. A stager therefore hands back a
  `StagedRun`, its quads beside the run rather than inside it.

  Each run carries `rect: u32`, the rect-table index of the text node it
  was shaped from — its **anchor**, stamped by `dashscene-core`'s commit
  (story #542). One field carries three facts a painter cannot otherwise
  recover: the run's clip is `rects[run.rect].clip`, its group membership
  is the `GroupComposite` whose range contains `run.rect`, and its z
  position is immediately after that rect. Like `RectEntry::paint` and
  `RectEntry::clip`, it resolves only against the rect table of the commit
  it came from. The table's atlases are held behind an `Arc`, because
  commit rebuilds the runs every frame while the atlas set behind them does
  not change.
- Solid-fill color is 4×f32 RGBA — the same shape as `dashbuf`'s `Color`
  struct (`crates/dashbuf/schema/dashbuf.fbs`), reproduced here as a
  plain type rather than shared by dependency.
- The generation stamp (`docs/design/architecture.md`) belongs to the double
  buffer `dashscene-core` owns, not to individual rect entries; it is out
  of scope for this crate.

## Public interface

All types and the trait live in `crates/dashpaint/src/lib.rs`:

- `Color` — `#[repr(C)]` RGBA, 4×f32 fields `r`, `g`, `b`, `a`.
- `RectEntry` — `#[repr(C)]`, fields `x`, `y`, `w`, `h: f32`,
  `paint: PaintIndex`, `clip: ClipIndex`, `opacity: f32` (28 bytes total,
  pinned by test).
- `GroupComposite` — a render-target group opacity: a rect subtree range
  `start`/`end: u32` and the `alpha: f32` its offscreen layer composites at
  (story #44).
- `PaintIndex` — `#[repr(transparent)]` newtype over `u32` (story #4,
  debt #54): a node index or other bare `u32` cannot cross into a
  paint index without an explicit wrap; layout unchanged.
- `PaintKind` — the fill vocabulary as `#[repr(C)]` tag plus row index:
  a `PaintTag` (Solid/Gradient/Image) and a `u32` into that kind's table
  on the `PaintTable` (story #578). It was a payload-carrying enum until
  then; a tag-plus-union has no clean C form, and the repo already
  mandates integer indices for cross-table references
  (`docs/decisions/dsb-sectioned-container.md`). Eight bytes, pinned by
  test.
- `FillSpec` — the same vocabulary as a producer authors it, with a
  gradient's stops still owned: `Solid { color }`, `Gradient { gradient,
  stops }`, `Image(ImageFill)`. A producer has no table to index, so it
  describes the fill and `PaintTable::intern_fill` assigns the row. Same
  split, and the same reason, as `GlyphRunTable::push_run`'s.
- `Fill<'a>` / `GradientView<'a>` — the borrowed view `PaintTable::fill`
  returns, and the form painters match on: `Solid(Color)`,
  `Gradient(GradientView)` (the gradient plus the stops its range names),
  `Image(&ImageFill)`. The flattened `PaintKind` is for uploading; this is
  for reading, and it is what keeps the tag-match plus bounds check from
  being repeated at every call site.
- Vocabulary value types — `Vec2`, `GradientStop`, `GradientKind`
  (Linear/Radial/Angular/Diamond), `Gradient` (kind + three normalized
  handle positions + a `StopRange` into the table's flat stop array),
  `ImageFill` (image index, `ScaleMode`, crop `Mat23`, tile scale),
  `ScaleMode` (Fill/Fit/Crop/Tile), `StrokeAlign`
  (Inside/Center/Outside), `Stroke` (width + align + solid color),
  `CornerRadii` (per-corner, `Default` = sharp). `ImageFill.transform`
  is a plain `Mat23` — `Option<Mat23>` has no C representation, and
  `Mat23::IDENTITY` is what its `None` meant.
- `MAX_GRADIENT_STOPS` — the gradient stop budget (story #15). It lives
  here, on boundary B, because it is a property of the paint vocabulary
  rather than of one backend: `dashscene-skia` asserts against it and
  `dashscene-validator` rejects it upstream (P4), and two hard-coded
  copies that drifted would make the validator's guarantee false.
- `BLUR_SIGMA_PER_RADIUS` — the Gaussian sigma one unit of blur radius
  maps to, 0.4375, Figma's measured constant
  (`docs/decisions/blur-sigma-is-figmas-mapping.md`). It lives here from
  story #584, when a second painter needed it: `dashscene-skia` measured
  it and `dashscene-gpu` applies it when it writes a shadow's row.
  **Unlike the blend space, it is not a contract term** — a painter
  should match it where it reasonably can, and one approximating the
  blur on constrained hardware will not match it exactly. What sharing
  it prevents is two copies of a measured number drifting apart, which
  is what the `Blur` doc comment says and what the scale-mode and
  gradient-kind pins exist to catch elsewhere.
- `PaintEntry` — the paint-table entry, `#[repr(C)]`, `Copy`, 64 bytes
  and seven fixed-width members since story #578: `fill: PaintKind`
  ([`PaintKind::NONE`] = a paint-less, layout-only node),
  `extra_fills: FillRange`, `stroke: StrokeRange`,
  `corners: CornerRadii`, `shadows: ShadowRange`, `blurs: BlurRange`,
  `shape: ShapeRange`. Every optional or repeated member is a range into
  an array the table owns, and `stroke` and `shape` carry arity 0-or-1 —
  a range rather than a sentinel, so an absent member needs no skip rule
  at the read site (`docs/decisions/optional-members-are-ranges-of-arity-one.md`).
  `EntryParts` is the producer-side shape, with the lists still owned.
  `PaintTable::push_solid(Color)` is the v0.1 shorthand, and replaced
  `PaintEntry::solid`, which could not survive a fill that only a table
  can name. See
  `docs/decisions/paint-entry-composition.md`. It carries no clip flag —
  whether a node clips its children is intent, and lives in the document
  and the arena, not in resolved painter input
  (`docs/decisions/resolved-clip-regions-at-commit.md`).
- `PaintEntry::samples_backdrop()` — whether a rect painted from the
  entry reads the already-composited backdrop beneath it, which is true
  when any of its `blurs` is a `BlurKind::Backdrop` (v0.11, story #393).
  Derived rather than stored: `blurs` already carries the fact, and a
  flag beside it would be a second copy of it that nothing keeps in
  agreement. It is the property the `Painter::paint` ordering guarantee
  is stated over, and it widens by itself if a further
  backdrop-sampling effect is added.
- `Shadow` / `ShadowKind` — a drop or inner shadow (v0.8, story #45):
  `kind` (`Drop`/`Inner`), `offset: Vec2`, `blur: f32` (Gaussian radius,
  non-negative), `spread: f32`, `color: Color`. Authored intent — the
  painter derives the shadow geometry from the rect's box and the entry's
  corners (P1). A list, not a fill kind, so a node stacks any number and
  `Paint.fill`/`.stroke` arity stays single-valued
  (`docs/decisions/effects-vocabulary-shadows.md`).
- `PaintTable` — a dense entry list behind a private field, indexed by
  `RectEntry.paint`: `new`, `push(&mut self, PaintEntry) -> PaintIndex`
  (returns the sequential index just assigned), `get(&self, PaintIndex)
  -> Option<&PaintEntry>`, `resolve(&self, PaintIndex) -> &PaintEntry`
  (the lookup painters use — panics on an out-of-range index), `len`,
  `is_empty`. Since story #578 it also owns every flat array its ranges
  and row indices name: `all_shadows`, `all_blurs`, `all_extra_fills`,
  `all_strokes`, `all_shapes`, and one array per fill kind
  (`all_solids`, `all_gradients`, `all_stops`, `all_images`). The
  matching readers — `shadows`, `blurs`, `extra_fills`, `stroke`,
  `shape`, `fill` — are the only way to resolve an entry.
- `PaintTable::push_with(entry, EntryParts)` — appends an entry over its
  parts, copying each into the flat arrays and assigning every range. It
  refuses an entry that arrives with a range already set, for the reason
  `GlyphRunTable::push_run` gives. `push` is the bare-entry shorthand and
  refuses the same way. An empty range is assigned `(0, 0)` rather than
  the offset it would have started at, so two entries that both draw
  nothing compare equal.
- `PaintTable::intern_fill(&FillSpec) -> PaintKind` — the only way a fill
  enters the table. Unlike `push_with`, which copies an entry's parts in
  without dedup, this **deduplicates**: a shadow
  list belongs to one entry and has no identity beyond it, while a fill
  is a shared value that `dashscene-core`'s retained interner re-stages
  every commit. Appending each time would grow the fill arrays for the
  life of a session, and the entry-level interner could not see it,
  because two equal fills would already have reached it as two different
  row indices.
- A `PaintKind` names a row in **the table that interned it**. `push`
  refuses an entry naming a row the table does not hold, by name (P4).
  That refusal is what catches the one place this can go wrong:
  `dashscene-core`'s table compaction rebuilds a fresh table from the
  entries still referenced, so a re-homed entry has to re-intern its
  fills from their contents (`Fill::to_spec`) rather than carry its old
  indices across.
- `ImageTable` / `ImageAsset` / `ImageFormat` — encoded, format-tagged
  image assets (the runtime side of `dashbuf`'s `Document.assets`, whose
  payloads the loader bound from the file's blob sections), indexed by
  `ImageFill.image`; same push/get/resolve contract as
  `PaintTable`. See
  `docs/decisions/image-assets-cross-boundary-b.md` (story #14).
- `Mat23` — row-major 2×3 affine; the image crop transform's shape.
- `ClipBox` — `#[repr(C)]`, one clipping ancestor's resolved box:
  `x`, `y`, `w`, `h: f32` plus `corners: CornerRadii` (all-zero radii =
  a sharp box).
- `ClipRegion` — the clip that applies to one rect: the boxes to
  **intersect**, outermost ancestor first (`boxes()`), behind a private
  field. No boxes = unclipped (`unclipped()`, `is_unclipped()`). The
  list is not pre-intersected into one box because the intersection of
  two rounded rects is not a rounded rect.
- `ClipTable` / `ClipIndex` — the region pool, same push/get/resolve
  contract as `PaintTable`, with one addition: `ClipTable::new()`
  reserves index 0 (`ClipIndex::UNCLIPPED`) for the unclipped region, so
  every rect resolves without a sentinel. `len()` counts it; a clip
  table is never empty, so there is no `is_empty`.
- `Painter` — the one trait every paint backend implements:
  `fn paint(&mut self, rects, paints: &PaintTable, images: &ImageTable,
  clips: &ClipTable, groups: &[GroupComposite], glyphs: &GlyphRunTable,
  dirty: Option<&[u32]>)` (an empty image table is valid input for
  image-less scenes; a fresh `ClipTable` for a scene that clips nothing;
  an empty `groups` slice for a scene with no render-target opacity).

`Color`, `RectEntry` and `ClipBox` are `#[repr(C)]` because
`docs/design/architecture.md` calls rect entries blittable and R-T4 plans
dirty-range instance-buffer uploads of per-frame painter input; fixing
the layout now costs nothing. A `RectEntry` is 28 bytes — four
coordinates, the paint and clip indices, and the free-path group alpha —
pinned by test.

`CornerRadii` is `#[repr(C)]` too, and was not until story #600.
`ClipBox` embeds one, so a `repr(C)` struct held a `repr(Rust)` field,
whose layout is unspecified — the blittability claim above was true only
by accident of what rustc does with four `f32`s. It was found by
`crates/dashscene-unity`'s `improper_ctypes_definitions` gate on the
first run of that gate, and adding the attribute moved nothing: all 77
committed binary artifacts stayed byte-identical.

That gate is why these attributes are now enforced rather than merely
intended. `dashscene-unity` declares an `extern "C"` surface over the
boundary-B value types under `#![deny(improper_ctypes_definitions)]`, so
removing a `repr` attribute, adding a `Vec` field, or putting a
payload-carrying enum on the surface stops the workspace compiling.
Boundary B is a language-neutral data contract, and the reason is G2 —
see `docs/design/architecture.md`. The surface is narrow today and widens
as story #578 flattens each type in turn. `ClipRegion`, `GlyphRun` (with
`GlyphRange`), `ShadowRange`, `BlurRange`, `PaintKind`, `Gradient` (with
`StopRange`), `ImageFill`, and now `PaintEntry` itself with the three
ranges it gained, are done and on the surface. `ImageAsset` remains, and
is a different problem: its `Vec<u8>` is a payload rather than a
reference into a table, so flattening it means deciding where a
decoded-ready blob lives.

`Painter::paint` is infallible and the trait is object-safe (`Box<dyn
Painter>` must work — backend selection is whole-scene, R3). Slice order
defines stacking — a later entry composites over an earlier one, since
DFS order encodes document stacking. The composited result is the
contract; iteration order is the implementation's choice (the lean
painter draws opaque cores front-to-back,
`docs/specification/03-target-hardware-rules.md`'s R-T2) — with the one
exception "Backdrop sampling" below states.
An out-of-range paint index is a broken contract between crates;
`PaintTable::resolve` centralizes the panic for that case, so no painter
invents its own failure path (a silent skip would be the silent drop P4
forbids). See `docs/decisions/painter-trait-infallible-slice-input.md`
for the alternatives considered on the trait's signature.

## Testing

`crates/dashpaint/tests/boundary_b.rs` exercises the public API only,
against hand-built fixtures, with no `dashscene-core` dependency. It
covers the `PaintTable` and `ClipTable` indexing contracts (including
both `resolve` panics and the reserved unclipped region), the
`PaintEntry` composition (solid shorthand, paint-less entry, full
gradient+stroke+corners entry, image fill), the recorded output of a
`RecordingPainter` test double over a two-rect fixture — including the
clip region it resolves per rect — the backdrop declaration and the
ordering barrier a painter reads out of the paint table for it, and
dyn-dispatch through `&mut dyn Painter`. The test file is
the executable statement of the boundary-B contract; this section
deliberately does not restate its cases.

## Subtree clipping

`Paint.clip` ("clips its children to its box", `docs/design/architecture.md`)
is a relation between a node and its descendants — the one construct a
painter cannot be handed directly, since the flat rect table has no
ancestors and P2 forbids re-deriving them. `dashscene-core` resolves it
at commit (issue #97): every rect carries the `ClipRegion` its clipping
ancestors add up to, and a painter intersects the boxes it is given
without asking which node each came from. A clipping node does not clip
itself — only its descendants; its own corner radii still shape its own
fill and stroke. The full contract and the rejected alternatives are
`docs/decisions/resolved-clip-regions-at-commit.md`.

## Masks and group opacity

Story #44 adds two more constructs a painter cannot be handed directly.
A **mask** node stencils the siblings that follow it in the same parent —
another producer-side relation — so `dashscene-core` resolves it at commit
into those siblings' `ClipRegion`s (the mask node's box added to each), and
the mask node itself resolves to the draws-nothing entry. Boundary B needs
no mask type: masks arrive as clip regions. **Group opacity** splits by the
overlap rule: a non-overlapping subtree folds its alpha into each rect's
`RectEntry.opacity` (the free path), while an overlapping one becomes a
`GroupComposite` the painter draws through an offscreen layer. The full
model, the overlap rule, and the render-target budget are
`docs/decisions/masks-and-group-opacity.md`.

## Backdrop sampling

Every effect before v0.11 is node-local: a shadow is built from the
node's own rounded-rect geometry, and a `GroupComposite` flattens a
subtree's **own** rects offscreen and composites that layer over what
lies beneath — it writes an isolated layer and never samples one. A
backdrop blur is the first effect that reads the already-composited
backdrop, so boundary B carries two things for it
(`docs/decisions/backdrop-blur-is-core-vocabulary.md`, story #393).

- **The declaration.** `PaintEntry::samples_backdrop()` answers whether
  a rect painted from the entry reads that backdrop. It sits in the
  paint entry rather than in `RectEntry` for the reason corners already
  do (`docs/decisions/paint-entry-composition.md`): `RectEntry`'s
  layout is pinned and blittable, and this is a paint-side effect
  parameter that shares the paint table's dedup pool. It is not a
  parallel table either — a `GroupComposite` spans a rect **range** and
  so cannot live on one entry, while a backdrop sample belongs to
  exactly one rect and already has a per-node home.
- **The ordering guarantee.** A painter still chooses its iteration
  order, except that every rect at a lower index than a
  backdrop-sampling rect is composited before that rect is drawn. The
  sampling rect is a barrier in any reorder, and the licence holds
  unchanged on either side of it. A painter that iterates in slice
  order satisfies this without doing anything, because it already
  composites back-to-front into one target; only a painter that
  reorders pays for the barrier.

The guarantee fixes order alone. Which surface the sample reads when a
barrier rect falls inside a `GroupComposite` range was left to the first
painter that implements the sampling, and `dashscene-skia` settled it
(story #393 stage B-3): **a render-target group is a backdrop root.** A
barrier rect inside a `GroupComposite` range reads that group's offscreen
layer, not the canvas beneath the group; outside such a range it reads
the canvas. Sampling through the group would composite the backdrop
twice — once directly through `1 - alpha` and once inside the layer's
own blurred copy — which is the defect that produced `GroupComposite` in
the first place, one level up. The reasoning and its disclosed cost are
`docs/decisions/backdrop-blur-is-core-vocabulary.md`. Glyph runs are
outside the guarantee for the same
reason they are outside `groups`: the v0.5 subset composites every run
over all rects, so no run is ever beneath a barrier and no run can enter
a sampled backdrop — a named limitation, not a silent drop.

## Trace

- Satisfies: `docs/design/architecture.md` painter trait (boundary B)
  and output shape, `docs/roadmap.md`'s v0.3 paint vocabulary (from
  `docs/specification/04-figma-vocabulary-profile.md`'s NOW list);
  issue #3, #13 and #97 acceptance
  criteria.
- Blocks: #4 (`dashscene-skia`, first `Painter` implementation), #6
  (golden harness), #14 (v0.3 painting).
- Related decisions: `docs/decisions/dashpaint-owns-boundary-b-types.md`,
  `docs/decisions/painter-trait-infallible-slice-input.md`,
  `docs/decisions/paint-entry-composition.md`,
  `docs/decisions/document-paint-pool-and-legacy-paint-field.md`,
  `docs/decisions/resolved-clip-regions-at-commit.md`,
  `docs/decisions/backdrop-blur-is-core-vocabulary.md`.
