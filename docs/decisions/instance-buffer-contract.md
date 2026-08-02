# The lean painter's frame is one ordered instance buffer, and layer 1 is stated over it

    status   accepted (2026-08-02)
    scope    dashscene-gpu's Instance, InstanceBuffer and packer; layer 1 of
             epic #569's verification net; the shape the Unity painter shares

## Context

Epic #569 decomposes fidelity into four layers so that only the smallest part
needs real hardware, and layer 1 — "instance-buffer goldens, bit-exact" — is
the widest part of that net. It only works if the instance buffer really is
the painter's whole output.

`docs/specification/03-target-hardware-rules.md` R-T4 already says it is: "CPU
frame cost = dirty-range instance-buffer upload from the rect table +
submission. Nothing else." Story #578 is where that stops being a budget and
becomes a data structure.

Two consumers, not one. G2 names a Unity painter and R-T5 asks for the SDF
shader math to be single-sourced into both painters' shading languages. The
epic plans that painter as instanced SDF quads too, which is what makes its
claim — "layers 1 and 2 are shared with the Unity painter" — concrete. How that
painter reaches the GPU is still open: `unity-painter-uses-brg.md` is
**proposed**, pending a lit-BRG shader spike. So the struct is designed for a
second consumer whose rendering approach is planned rather than settled, and
nothing below depends on which way that lands.

## Decision

**D1 — one ordered stream, tagged by kind.** Every quad the painter draws is
one `Instance` in one buffer, in draw order. Not one buffer per primitive
kind, which is what GPUI does.

**D2 — the instance is 64 bytes of fixed-width members.** Eight 4-byte scalars
(`kind`, `tag`, `row`, `shape`, `clip_offset`, `clip_count`, `layer`,
`opacity`) followed by two four-float vectors (`bounds`, `corners`).
`#[repr(C)]`, no implicit padding, no `bool`, no payload enum, no nested
collection — story #578's rules for anything crossing a language seam.

**D3 — a parameter set is named by a row, never inlined.** A fill instance
carries a `PaintTag` and a row of the per-kind table that tag names; a shadow
carries a row of `PaintTable::all_shadows`; a stroke a row of `all_strokes`; a
backdrop a row of `all_blurs`. The same tag-plus-index idiom boundary B itself
uses, and the reason is the same: a parameter change then moves one table row
rather than every instance that references it.

**D4 — a rect's instances are contiguous, and `spans[i]` names them.**
`InstanceBuffer::spans` is index-aligned with the rect table, so a dirty rect
index resolves to a byte range with no search. That is the shape R-T4's
dirty-range upload needs.

A span is `(offset, count)` like every range on boundary B, with one deliberate
difference: boundary B canonicalizes an empty range to `(0, 0)` so that two
draws-nothing values compare equal
(`optional-members-are-ranges-of-arity-one.md` D2), and a span cannot. Spans
partition the buffer, so an empty one still records where the next rect's
instances begin. Two draws-nothing spans therefore compare unequal, and nothing
compares them — a rect's identity is its index, not its span.

**D5 — the per-node order is the reference painter's.** Backdrop blurs, then
drop shadows, then the fill and the layers stacked over it, then the stroke,
then inner shadows. Story #582's glyph instances go at the end of that list,
where `dashscene-skia` draws a run anchored to the rect.

**D6 — `layer` and `shape` are index-plus-one, with zero meaning none.**
`row` is not biased, because it is never absent.

**D7 — a layer-1 golden is committed text, not committed bytes.**
`crates/dashscene-gpu/tests/goldens/*.txt`: one line per rect span and one per
instance, floats printed through Rust's shortest round-tripping form. The
fixtures are hand-built boundary-B tables in the test beside them.

**D8 — a drop shadow's `bounds` are the node's _stroked_ silhouette.** A drop
shadow casts from what the node draws, so an Outside stroke grows it by the
full stroke width and a Center stroke by half
(`effects-vocabulary-shadows.md`). The packer resolves that growth into the
instance; the shadow's own offset, spread and blur stay on the row it names.
An inner shadow takes no outset.

## Why

**D1, against per-kind buffers.** Boundary B says "slice order defines
stacking", and a node's shadow, fills and stroke interleave with its
neighbours'. Per-kind buffers would have to be re-interleaved at submission to
composite correctly, which is the batching that separating them was supposed to
buy, given back. One ordered stream makes draw order the buffer's own order —
so there is no depth field, because a second record of the same fact could
disagree with the first — and it makes a layer-1 golden a single readable
table.

**D2, on 64 bytes.** Both float vectors land at a 16-byte offset, so a consumer
binding this as a storage-buffer element repacks nothing. `kind` is `u32`
rather than `u8` because a shader's smallest addressable scalar is 32 bits and
the struct has no byte to save by narrowing it.

**D5, on why the order is copied rather than derived.** Two painters that
composite one node's parts in different orders produce different pixels from
the same document, and epic #569's layer 4 measures a perceptual band against
`dashscene-skia`. Leaving each painter to read the order out of boundary B is
how that band comes to measure a disagreement about ordering rather than about
rendering.

**D6, against a range, and why that does not contradict
`optional-members-are-ranges-of-arity-one.md`.** That record chose a range over
a sentinel for boundary B, because boundary B is read by every painter and a
skip rule is a rule each of them has to remember and can diverge on. This
struct is one painter's own upload format with one reader, and a range here
would cost a second 4-byte member to express an arity `kind` already fixes.

The bias rather than a value at the top of the range: `0` is a valid row of
every table, so an unbiased index cannot say "none" at all, and biasing puts
the absent value where an unwritten member already sits. It buys nothing
beyond that — `row` is unbiased, so a zeroed instance does name row 0 of a real
table, and what makes such an instance inert is its `opacity` of `0.0`.

**D7, against binary goldens.** A golden is reviewed truth (`goldens/README.md`)
and nobody reviews 64-byte rows in a diff. Text costs nothing in exactness:
`{:?}` on an `f32` is the shortest representation that round-trips, so a
one-bit change in a coordinate changes the line.

**D7, against driving the fixtures from a committed `.dsb`.** What is under
test is the translation _from_ boundary B, so boundary B is the input. A
document would put the compiler, the solver and the typesetter upstream of the
assertion, and a golden that moved would no longer say which of them moved it.
The cost is that the fixtures are Rust rather than data, so a Unity painter in
its own repo ports them rather than loading them; the golden text itself is
data and does transfer.

## What the packer deliberately does not emit

Each is a named gap with the story that closes it, not a silent drop (P4).

- **Glyph quads.** A glyph's texel rectangle is a coordinate in the painter's
  _residency_ atlas, not the `atlas_px` boundary B carries, and residency is
  story #581. Packing one now would pin coordinates story #581 is going to
  reassign. Story #582 adds the kind; D5 already says where its instances go,
  and because they extend the anchor rect's span rather than move a boundary,
  D4's contract does not change when they arrive.
- **`BlurKind::Layer` blurs.** Node-local layer blur is budgeted at v1 and
  nothing in this tree produces one — `dashc` lowers only `BACKGROUND_BLUR`.
  The reference painter skips it by the same filter.
- **The flatten-for-a-stroked-node layer** `dashscene-skia` opens when a node
  carries both a fill and a stroke below full opacity (debt #277). What this
  packer emits there is not a missing layer but a _different alpha_: each
  instance carries the group alpha, where the reference draws opaque instances
  into one layer composited at that alpha. The two differ wherever an Inside or
  Center stroke overlaps its own fill. Story #583 owns group compositing and
  decides it for this painter.
- **A masked node's stacked layers and its stroke**, and a masked node whose
  fill is an image. `dashscene-skia` draws a baked-vector node's own `fill`
  through the coverage field and nothing else, and draws nothing at all for an
  image fill — "an image-filled vector is not in the measured census; it draws
  nothing rather than an unmasked rectangle". A masked node's _effects_ are not
  in this list: the reference painter draws its backdrop blurs and its shadows,
  and so does the packer. Both omissions are matching the reference painter,
  not a rule anyone wrote down: `dashc` does not lower a VECTOR node with a
  stroke or with stacked fills, so no document in this tree reaches the case.
  If one ever does, the reference painter drops the same ink and both would
  have to change together.

## What layer 1 does not catch

The buffer names rows; it does not carry the parameters in them. So a layer-1
golden pins that the packer named the right row of the right table, and a wrong
_value_ in that row is a defect in the table, which boundary B's own tests own.
"The GPU is a pure function of the instance buffer" is true only together with
the tables the rows index, and those cross to the GPU as their own buffers at
story #580.

## Consequences

- Story #579's shader library reads `kind`, `tag` and `row` to select what it
  evaluates, and layer 2 is stated over the same rows.
- Story #580 binds this struct — as vertex attributes or as a storage-buffer
  element — and that binding is where a 16-byte alignment, if one is wanted,
  is declared. Nothing here depends on it.
- **R-T2 is not decided here.** It asks this painter to draw opaque cores
  front-to-back with a blended fringe, which is a reorder `Painter::paint`
  licenses. D1's "no depth field" does not stand in the way of it: draw order
  is the buffer's own index order, so a depth value is derived from the index
  rather than stored beside it, and a reordering submission derives its own.
  What R-T2 costs is the backdrop barrier — an instance whose `kind` is
  `Backdrop` is a barrier in any reorder — and that is story #580's to pay.
- Story #583 decides how `clip_offset`/`clip_count` are evaluated, and carries
  issue #133 (the quadratic clip-region storage) as the second consumer that
  gets to weigh the two representations.
- `dashscene-unity` does **not** gain this type. The instance buffer is what a
  painter builds _from_ boundary B; a C# painter builds its own, and what the
  two share is this shape and these goldens, not a Rust symbol across an FFI
  seam.
