# Technote — runtime-provided content & placeholders

    status   design note, 2026-07-13. Captures conclusions from a design
             discussion; extends docs/archive/2026-07-14-design-1-seed.md
             and docs/archive/2026-07-14-scope-decisions.md without
             superseding them. DECISION = settled; CANDIDATE / OPEN = not.
    scope    how content supplied at runtime (downloaded images, streamed UI,
             Lottie, arbitrary SVG) enters a scene and fills a placeholder,
             without pre-rendering.

## 1. The decision rule

Every runtime-provided content case resolves on one question — _is it
expressible in what the runtime can already draw?_ — and there are exactly three
buckets, cheapest first:

    content                              path
    -----------------------------------  ------------------------------------------
    already a bitmap (PNG/WebP)          decode -> GPU texture -> image fill (§2)
    expressible in resident vocabulary   stream dashscene intent via arena/wire (§3)
    arbitrary runtime vector / anim      render to texture with ThorVG (§4/§5)

"Resident vocabulary" means: the target profile's validated paint kinds, plus
only already-resident atlas assets (glyphs from declared charsets, pre-baked
icons) and the parametric SDF primitives (which need no baking). New arbitrary
vector that would require an _offline bake_ cannot be introduced at runtime —
there is no runtime baker — which is exactly what pushes a case from bucket 2
into bucket 3.

All three fill a placeholder under one contract (§6); none requires
pre-rendering pixels — buckets 1 and 2 stream _intent/resources_, bucket 3 is a
bounded texture escape hatch.

## 2. Downloaded raster (PNG / WebP) — the easy case

DECISION →
[`downloaded-raster-needs-no-vector-engine.md`](../decisions/downloaded-raster-needs-no-vector-engine.md).
A PNG/WebP is already a bitmap, so no vector engine, no per-frame RT. An **image
fill is already NOW vocabulary**
(`docs/specification/04-figma-vocabulary-profile.md`: "image fills + scale
modes"); a downloaded image is just an image fill whose _source_ is bound at
runtime. Path: **decode → upload → bind.** Decode to RGBA with a small pure-Rust
decoder (`png`, `image-webp`/`image`) — _not_ by re-enabling Skia's codecs, so
the §6 Skia trim stays intact; upload as a GPU texture; point the node's image
fill at it; show `interim_fill` while loading.

Notes: decoded RGBA is uncompressed → more VRAM/bandwidth than pre-transcoded
ASTC assets; fine for occasional images (avatars, album art, map tiles), and you
may runtime-transcode to ASTC/EAC at scale (a photo, not a distance field, so
§9's "never lossy-compress distance fields" does not apply). It is a bitmap, so
it scales like a photo — download at the box's size. Fully cross-backend (image
fill is in every profile; Skia draws the quad, Unity binds the texture to a
material slot) and consistent across tiers because it is static. Fits P1: the
document carries "image fill → source slot," pixels arrive at runtime like a
slot-bound string; no §10.2 node-replacement machinery, just a runtime texture
swap.

## 3. Streamed vocabulary content — a Glance-like cross-process producer

DECISION direction →
[`streamed-content-is-a-cross-process-producer.md`](../decisions/streamed-content-is-a-cross-process-producer.md)
(this is the provisioned "Kotlin/remote" skin + v2 streaming, pulled forward to
one node). You can compute a scene in realtime from a Compose/ Glance-like DSL
and stream it into a placeholder **without pre-rendering** — but you stream
_intent_, and via the **wire role of the schema, not the `.dsb` file role**. The
file role's mmap/section/hashing packaging (`docs/design/architecture.md`) is
for fast cold-load of a whole document; streaming uses plain length-prefixed
flatbuffer messages (`docs/decisions/dsb-format-and-one-schema.md`: one schema,
two roles).

Two placements:

- **In-process** — translate each (re)composition directly into arena staged
  mutations (`open`/`set_prop`/`set_variant`/`commit`), no serialisation, like
  the Rust DSL.
- **Cross-process** (Glance's real situation: Compose on the JVM, runtime in
  Rust) — the "Kotlin/remote" skin: composition emits a describe buffer, one
  commit across the FFI/IPC seam per recomposition (struct/Span, GC-free, typed
  keys via codegen), ongoing changes as tiny commit deltas.

Glance fits because it _already works this way_: it runs the Compose runtime but
does not render — its composition emits an abstract tree that is translated to
RemoteViews / a protobuf and rendered by another process. Swap that translator
for "Compose tree → dashscene fragment" and the runtime renders it.

Hard constraint (P3): **producers mutate, the runtime owns time.** Recomposition
that changes structure/props is unrestricted (commits at data/event rate). What
does _not_ survive: Compose frame-loop escape hatches (`withFrameNanos`,
`Animatable`+ `suspend`, arbitrary per-frame `AnimationSpec`). They must lower
to descriptive specs (dashcue), or become input-rate mutations, or engine-side
slot work. So: recompose → commit stream yes; animation between states is
declared data, not a Glance coroutine driving frames.

Boundary (this is what "stays in the baked capabilities" means): the streamed
fragment must use only the target profile's validated vocabulary + resident
assets + parametric primitives; it cannot introduce content needing an offline
bake. Within that box the win holds — because it streams _intent_, it renders on
every painter identically (trimmed-Skia entry and Unity high-end), unlike
node-replacement which is engine-only. The placeholder must be **declared-size,
never hug** so streamed content does not reflow the scene (§10.2). Open: if the
producer is remote/untrusted, the streamed fragment clears the admission policy
(Q-5), still undecided.

## 4. Lottie — bake if you can, ThorVG only when you must

DECISION direction →
[`lottie-bake-when-possible.md`](../decisions/lottie-bake-when-possible.md).
Lottie is not one thing; where a given animation sits decides whether it bakes.
dashc should **triage each Lottie and emit a named diagnostic** for the path
taken (P4/P5 — validated, never discovered), including a VRAM-budget check for
the sprite-sheet case and a reject-or-flag on profile:core for the VG case.

- **Transform-only** (spinner, pulse, slide-in — most UI Lottie): bake the
  shapes into the SDF atlas and lower the keyframes into **dashcue** tracks
  driving their transforms. No runtime VG. Keeps resolution independence, is
  cheap, cross-backend, interruptible, and **data-drivable** (a live progress
  ring only works this way). Needs a Lottie _parser_ offline, not a renderer.
  Prefer this whenever it applies.
- **Canned full-frame, no runtime params** (small/short): bake a
  **sprite-sheet** — offline-render frames to a texture atlas, play as textured
  quads. No runtime VG. Cost is VRAM (frames × resolution) → small/short only;
  loses resolution independence and parameterisation. ThorVG is a fine _offline_
  frame-renderer here.
- **Path morphing / masks / mattes / runtime-dynamic**: no faithful bake →
  **ThorVG at runtime** (§5). Budgeted escape hatch.

## 5. Arbitrary runtime vector (SVG/Lottie) — ThorVG-to-texture

DECISION →
[`runtime-vector-via-thorvg-to-texture.md`](../decisions/runtime-vector-via-thorvg-to-texture.md).
For genuinely runtime-provided, non-bakeable vector (arbitrary SVG,
morphing/masked Lottie), render it to a texture with **ThorVG** and fill the
placeholder with that texture as an image fill (every painter can draw an
image). ThorVG fits: lightweight (~150KB), MIT, SW+GL backends, native SVG _and_
Lottie, embedded-proven (LVGL, Tizen, Crank Storyboard).

Treat it as the bounded escape hatch it is:

- The node becomes a **bitmap** → loses crisp-at-scale; re-render on resize.
  Only for genuinely runtime content — anything bakeable should be baked (SDF),
  which stays crisp and free per frame.
- **Lottie = per-frame re-render → a per-frame offscreen RT** for that node,
  exactly the tiling-GPU cost the lean painter avoids (§9). **Count-budget like
  blurs** (relates to Q-6). Use ThorVG's **GL backend** to render into a GL
  texture on the painter's context and avoid a CPU→GPU upload per frame.
- Respect **P3**: ThorVG runs its own clock inside the fixed placeholder box;
  the runtime never calls into it mid-frame — the §6.3 "engine-side slot doing
  its own per-frame work outside layout authority" case.
- It is primarily the **native/entry-tier** mechanism; on Unity high-end you
  would more likely use node-replacement. So such a node is _not_
  pixel-identical across tiers — nor should it be, it is dynamic content (same
  as node-replacement).

## 6. ThorVG's role, summarised

- **Runtime**: only in bucket 3 (§5) — a scoped, RT-budgeted, render-to-texture
  escape hatch for one node. dashscene keeps painting behind boundary B to its
  own painters; the scope limit is dashscene's, not a limitation of ThorVG.
- **Build time**: a candidate offline Lottie/SVG frame-renderer for sprite-sheet
  baking (§4). For SVG baking into the atlas, the same-ecosystem Rust path
  (`usvg`/`resvg`/`tiny-skia`, same author as
  `ttf-parser`/`rustybuzz`/`tiny-skia`, already in-stack) is preferred over
  adding a C++ dependency; ThorVG earns its place mainly for Lottie.

So ThorVG is mostly the answer to a question the architecture is built _not_ to
ask at runtime (arbitrary runtime vector); keep it out of the steady-state
render path.

## 7. The placeholder contract

All three buckets fill a placeholder (§10.2) under one contract: a
**declared-size** box (never hug — lazy content must not reflow the scene), an
`interim_fill` shown while content loads/resolves, and a `contribution_id` the
runtime producer binds against.

**The schema surface for those three now exists and nothing resolves it.** Story
#1126 added `table Placeholder` and `Node.placeholder`, which `dashc`'s emitter
lowers and `dashscene-core` reads back. **No producer lowers one** — the `dashc`
CLI only has `check`, so a `.dsb` carrying a placeholder is authored by building
a `dashc::Document` in code. Figma's annotation vocabulary does have a
`dashscene/role = placeholder`, which the importer recognises and whose sample
children it trims; the lowering drops it and sets `placeholder: None`. And
nothing resolves one: no measure callback reads `declared_size`, no host binds a
`contribution_id`, and no painter draws an `interim_fill`. The contract below is
what activation will implement, and activation stays in v1
([`../specification/05-qualification.md`](../specification/05-qualification.md),
[`../decisions/a-placeholder-is-a-table-and-declares-its-measure-size.md`](../decisions/a-placeholder-is-a-table-and-declares-its-measure-size.md)).
Then either a streamed dashscene subtree (§3), a decoded image texture (§2), or
a ThorVG texture (§5) drops in. Buckets 1–2 preserve cross-backend identity;
bucket 3 is engine-clock, tier-specific, and budgeted.

## 8. Open items

- Admission policy (Q-5, §3) for remote/untrusted streamed fragments.
- dashc Lottie triage + VRAM budget + profile:core reject rule (§4).
- Group-opacity / RT budget value (Q-6) also bounds runtime-Lottie RTs (§5).
- Confirm colour/emoji-font scope (monochrome-SDF atlas can't hold them) — bears
  on whether some "text" content must route through bucket 3.
