# Technote — implementing a backend

    status   informative
    date     2026-08-08
    scope    boundary B (`dashpaint`), and the instance buffer behind
             `dashscene-gpu`
    story    #727, slice v0.17
    example  `goldens/tooling/tests/worked_example.rs`

Nothing in this repository told anyone how to implement a backend. The material
existed and did not do this job: `docs/technotes/rendering-and-painters.md`
explains the model, `docs/design/dashpaint.md` states the contract table by
table, and `crates/dashpaint/src/lib.rs`'s `Painter` doc comment is the real
specification — excellent, and reached only by someone who already knew to look.

This is the orientation. It does not restate the specification; it says which
job you are doing, what the rules are that are **not** visible from the trait,
and where to read next.

**A worked example compiles in the tree**, at
`goldens/tooling/tests/worked_example.rs`. Every rule below that can be asserted
is asserted there, because prose goes stale silently and this repository's
review history is dominated by that failure. If the guide and the example
disagree, the example is right.

## First: which of the two seams are you on

"Implement a backend" has two answers. They share almost no content, and picking
the wrong one costs a rewrite.

**Seam 1 — boundary B: implement `Painter`.** You receive finished rects,
positioned glyph runs and resolved paint, and you colour them. This is what any
wholly new painter does, and what the Unity painter is **planned** to do — that
painter does not exist yet, and `dashpaint-abi` today is the boundary-B C gate
only.

**Seam 2 — behind the lean painter: consume the instance buffer.** You receive
`dashscene-gpu`'s packed instance rows and draw them with your own API. This is
what a direct-GLES backend would do — the contingency
`docs/decisions/wgpu-is-the-lean-painter.md` names — and what the Unity painter
is expected to do for its _shaders_ while sitting on seam 1 for its input.
`docs/decisions/unity-painter-uses-brg.md` is `accepted` since 2026-08-18, so
that is a settled plan rather than a description — nothing is built. One rung of
that record's D3 ladder is instanced draws without BRG, which is this seam by
another route; D3 is where the rungs and their conditions are stated.

If you are writing a renderer for a platform, you are almost certainly on
seam 1. Choose seam 2 only when you need the SDF shading itself and not the
tables.

## Seam 1 — implementing `Painter`

The trait is two methods. `crates/dashpaint/src/lib.rs` documents both in full;
what follows is what an implementer gets wrong.

**You never measure, wrap, kern or move anything.** That is P2, and it is the
reason cross-backend identity is structural rather than tested for. Layout ran
once, in shared Rust; text was shaped once by the typesetter. A painter that
adjusted a glyph position would be producing a different picture from every
other backend, and nothing would catch it but a golden.

**Clip regions arrive ancestor-resolved.** You intersect the boxes you are given
and never ask which node they came from — the example asserts that. A
consequence worth stating: **you need no mask concept**, because a mask reuses
the clip table. That second half is not asserted anywhere; take it from
`docs/design/dashpaint.md`.

**`RectEntry::opacity` is not optional.** It is the resolved free-path group
alpha, and a painter must multiply it into the paint alpha. Miss it and every
partially-transparent free-path group draws at full strength — which no golden
in your own backend will catch until it is compared against another painter.

**`groups` is the other half of that.** A `GroupComposite` is a subtree that
must be drawn into an offscreen layer and composited at the group's alpha,
rather than having the alpha pushed onto each child. Free-path opacity and
render-target groups are different mechanisms and `RectEntry::opacity` covers
only the first.

**Glyph runs are neither clipped nor composited into group layers.** Two
limitations `Painter`'s own documentation states, and the two a text-drawing
implementer meets first.

**Slice order is stacking order**, and you may reorder. A later entry composites
over an earlier one; DFS order encodes document stacking. You are free to draw
opaque cores front-to-back (R-T2) because the composited result is the contract,
not the iteration.

**The backdrop barrier is the one narrowing.** A rect whose paint answers
`PaintTable::samples_backdrop` reads what is composited beneath it, so every
lower-indexed rect must be composited first. A painter that iterates in slice
order satisfies this by construction and pays nothing; only a reordering painter
pays. A render-target group is a backdrop root.

**`paint` is infallible, and that is why `samples` exists.** You cannot refuse a
payload once you are drawing, so the declaration has to be asked _before_ one is
bound rather than returned as a result
(`docs/decisions/painter-trait-infallible-slice-input.md`).

**No host calls it yet.** `Painter::samples` has no production caller in this
tree — only tests — and
`docs/decisions/baked-texel-payloads-cross-boundary-b.md` D6 states the
declaration while binding nothing to walking it a particular way. So answer
honestly because a host will come to rely on it, not because one does today.
Said plainly here, because a guide presenting this as as-built would be telling
you a host protects you when none does.

**The dirty set is advisory.** `None` is valid input, and ignoring it is correct
— you may always redraw everything. What you may not do is treat it as a
statement that nothing else changed.

Both v0 painters _do_ use it, in different places, which is worth knowing before
you decide your own painter will not. `dashscene-skia`'s retained mode skips a
group's subtree entirely when the set leaves its range alone, and
`goldens/tooling/tests/dirty_oracle.rs` is the differential test built for that.
`dashscene-gpu` honours it one level down, where it decides which byte ranges of
the instance buffer are uploaded — the same mechanism seam 2 describes below.

### Two rules that are invisible from the trait

Neither is discoverable from `Painter`'s own documentation, which is exactly why
this guide exists. Only the second was _added_ by v0.16; the first predates it,
and what v0.16 changed is where it is enforced.

**A painter must never receive bytes that have not been hashed.** That rule is
held at the _load_ boundary, not in the trait. `dashbuf::residency` proves each
payload against the hash the file names before anything binds it. If you are
writing a host as well as a painter, this is yours to preserve.

**The bytes may be a mapping's pages.** `ImageTable`'s pool is owned or mapped
and never both (`docs/decisions/assets-borrow-from-the-mapping.md`); `resolve`
is identical across the two and **a painter cannot tell them apart**,
deliberately. So nothing may assume the bytes outlive the region, and nothing
may write to them.

## Seam 2 — consuming the instance buffer

Read `crates/dashscene-gpu/src/pack.rs` and
`crates/dashscene-gpu/src/shaders/sdf.wgsl` alongside this.

**One ordered, kind-tagged stream.** `kind` carries the sub-kind, and it must be
mapped by an **exhaustive match, never a cast** — a cast silently accepts a
value a later version adds.

**`#[repr(C)]` is pinned by nothing unless you check it.** A size assertion
passes while a reorder moves every offset; assert `offset_of!` per member
against what your shader declares.

**Per-rect spans and dirty-range upload.** R-T4 budgets the upload as ranges of
the instance buffer rather than whole-buffer writes.

**`sdf.wgsl` is the single source** R-T5 promises. Porting it to another shading
language is the cost you are taking on, and keeping the port in step is the
obligation R-T5 creates.

**The binding budget is `downlevel_defaults`, not a desktop limit** — four
fragment-stage storage buffers, and the painter binds four. The paint-parameter
heap exists because of that ceiling; the next fragment-side table extends the
heap rather than adding a binding.

**An instance draws outside `Instance::bounds`, and the quad is grown to cover
it.** `Instance::outset` is how far past `bounds` the ink reaches, and the
vertex stage adds it — plus the antialiasing margin — so the geometry does not
clip the ink. Only the **lower** bound is a correctness property: a quad too
small clips, a quad too large draws the same picture more slowly. So `bounds` is
not a conservative bound and the quad is; a port that grows the quad by `outset`
is correct, and one that uses `bounds` as the quad clips every shadow and blur.

## What this guide does not settle

- **A portable conformance suite.** R-T5's promise is better served by a suite a
  second painter can port than by a description of one. That is its own story;
  layer 2's suite is `dashscene-gpu`'s today.
- **Whether this becomes the public book's chapter.** It would bind the shape to
  `docs/decisions/repo-staging-and-public-facade.md`, which was undecided and is
  now settled — one repository, the facade role folded in. The question is live
  again rather than blocked.
