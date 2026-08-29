# Unity host integration — what the existing records already decide, and what is still open

    status   WIP — design-discussion capture (2026-08-09, user + Opus).
             **Nothing here is implemented.** `dashscene-unity` is
             Rust-side FFI bindings only and the Unity project is a
             separate repository that does not exist yet, so every
             recommendation below is a proposal against unwritten code.

             The findings in §1 are different in kind: they are readings
             of records and code that already exist, each naming where it
             was checked on 2026-08-09 so a reader can re-derive it rather
             than trust this file.

             Gardened when the integration is built, not when it is
             decided. §5's reserved-node pair is the part most likely to
             become a decision record, because it is the only part that
             touches the document vocabulary, the Figma profile and the
             validator.
    scope    embedding a dashscene document in a Unity host that also
             draws 3D content: packaging, the painter and text seam, the
             composition mode, and how a node reserved for 3D content is
             expressed
    builds on docs/specification/03-target-hardware-rules.md (R-T1, R-T2),
             docs/design/architecture.md (boundary B),
             docs/decisions/backdrop-blur-is-core-vocabulary.md,
             docs/technotes/open-questions.md (Q-6),
             P1, P2, P3, P4, P5

## The premise this was worked out against

A Unity host where the continuously-changing content — gauges, dynamic
backgrounds — is 3D geometry Unity owns, and dashscene owns the 2D layout,
typography and chrome around it. The AssetBundle carries Unity-serialized
objects; the `.dsb` and its bank ship beside it. That split is what makes
the rest coherent: there is no second residency manager, and Addressables
is used as designed.

## 1. What the existing records already decide

Each was checked against the tree on 2026-08-09.

### 1.1 Interleaved compositing is ruled out, not merely expensive

`docs/specification/03-target-hardware-rules.md` R-T1: "One render pass per
frame; every mid-frame RT switch is a tile-memory flush + resolve. Blurs are
the only exception and are count-budgeted paint kinds."

Drawing UI in bands with 3D between them is N mid-frame render-target
switches. On the stated target hardware that is a spec violation rather
than a tradeoff. Where the motivation is "some UI must be occluded by 3D",
a world-space surface obtains that from the depth buffer in one pass.

### 1.2 The tiering axis is Core/Full, and it is not HiFi/LoFi

Two profile enums exist and they tier different things.

- `dashpack::Profile` (`crates/dashpack/src/profile.rs:182`) — `Raw`,
  `HiFi`, `LoFi`. Per-asset-class tolerance bands. Compression quality
  only.
- `dashscene_validator::Profile` (`crates/dashscene-validator/src/lib.rs:504`)
  — `Core`, `Full`. Paint vocabulary. `Core` is documented as "the subset a
  fixed-vocabulary painter can honor **without a render-target
  round-trip**"; `Full` is documented as **"Unity-class"**.

A capability that a Unity host has and a lean target does not is a Core/Full
question. The vocabulary profile was defined for this case and names it.

### 1.3 Backdrop blur cannot see a host-drawn 3D scene

The `Painter` trait (`crates/dashpaint/src/lib.rs:2746`) defines a backdrop
sample as reading "what is already composited beneath it" — within the
painter's own output — and settles that a render-target group is a backdrop
root, reading that group's layer rather than the canvas beneath it.

So a frosted-glass panel over 3D content is not a dashscene backdrop-blur
node. That effect belongs to the host material, outside boundary B. The two
produce visibly different results and the document vocabulary will accept
the one that cannot work, so which mechanism a design maps to has to be
decided rather than assumed.

### 1.4 A host painter must override `Painter::samples`

`Painter::samples` (`crates/dashpaint/src/lib.rs`) defaults to
`format.is_encoded()`, which claims PNG, JPEG and GIF only. The default is
deliberately the conservative half. A host painter that says nothing is
therefore handed encoded payloads and has to decode them per frame. A Unity
painter declares `Rgba8*` and the `Astc*` footprints its targets support.

### 1.5 The resolved box is on the committed scene, not on the arena

`Arena::layout(node)` (`crates/dashscene-core/src/arena.rs:774`) returns
layout **intent** — authored geometry plus the active variant overlay. It is
not the solved box, and it will look correct for fixed-position nodes while
being wrong for everything flex resolves.

The solved geometry is `CommittedScene::rects()`, reached by
`CommittedScene::rect_index_of(NodeId) -> Option<u32>`
(`crates/dashscene-core/src/committed.rs:154`), with `node_of` as its
inverse and `generation()` to detect when a cached index must be
re-resolved. Reading it per frame keeps P1 intact: the document still
carries no results.

There is no name-to-`NodeId` lookup. `Arena::name` is index-to-name only
(`crates/dashscene-core/src/arena.rs:780`), so a name-keyed binding needs a
map built by one walk at load.

### 1.6 A layout transition costs the repaint and almost nothing else

`crates/dashscene-engine/src/flip.rs:20-25`: "Each frame's cost is
`O(animated nodes)` with no per-frame allocation and no state that grows
with animation history." The solve runs twice — before and after — at
`start`; every step after that interpolates the moved nodes.

Two consequences. A transition is a burst, not a steady state: at 60 Hz a
300 ms transition is roughly 18 repaints. And interruption is already
handled — a second `start` mid-flight retargets through the scheduler's
existing rule, a spring-to-spring retarget keeps its velocity, nothing
snaps. A host must not reimplement that.

A named limit applies at any frame rate (story #283): the FLIP path animates
rect channels only, so a node a variant switch reveals or hides pops rather
than fading. Its reflowing siblings animate normally.

### 1.7 Glyph coverage is settled at build time

A `glyph_id` with no `AtlasGlyph` draws nothing — the reference painter
skips it (`crates/dashscene-skia/src/lib.rs`), and coverage is the
build-time atlas closure's responsibility
(`crates/dashscene-typeset/src/atlas/closure.rs`). There is therefore no
runtime atlas rebuild, which is the equivalent spike in a Unity-native text
path.

A host-side text fallback for uncovered scripts would put a second
typesetter behind a case the closure owns, reintroduce platform-dependent
output, and break the goldens for exactly the scripts least verified.
Extend the closure instead; the P4 diagnostic belongs at build time.

## 2. Packaging — recommendation, not a decision

**You cannot mmap through an AssetBundle.** The bundle is a container with
its own serialization and normally its own compression; `TextAsset.bytes`
additionally copies to the managed heap.

Separate what mmap buys. Zero-parse is flatbuffers, not mmap, and survives
any backing memory — `dashbuf` reads from a byte slice. Demand paging is
what a bundle destroys, and whether that matters depends entirely on
document size against the residency budget.

Recommended order, all preserving mmap:

1. **Play Asset Delivery with file storage** — a real filesystem path, zero
   copy, zero duplicate storage, designed for large binary payloads.
2. **Uncompressed in the APK, mapped in place** via
   `AAsset_openFileDescriptor`, which returns fd + offset + length for
   *stored* entries only. The mapped base inherits the **zip entry
   offset's** alignment, not the page's, so the entry must be aligned to
   the schema's requirement — AGP's default `zipalign` is 4. Verify with
   `zipinfo -v`; this is a build-step fix, and an unaligned base is silent.
3. **Extract to `persistentDataPath` at first run.** Always works, and
   alignment stops being a zip-layout question. Costs a first-run copy and
   permanent double storage. Needs an atomic write (temp, fsync, rename),
   a content-hash or version key so an app update does not leave a stale
   copy, and cheap header-only verification at open — a full verifier pass
   touches every page and defeats the demand paging it exists to protect.

**Unverified:** whether the packer's ASTC payloads carry the 16-byte
astcenc header. `ImageAsset::as_ref` panics on baked payloads because they
carry no discoverable extent and the `Atlas` supplies width/height
separately, which suggests headerless, but this was not confirmed. A header
offset produces garbled output rather than an error.

**The change that would dissolve the question:** `dashscene-web` already
loads a `.dsb` by HTTP byte range. If the load seam were a range-reader
rather than a mapping specifically, mmap, a range request and a bundle
chunk all become implementations of one seam. Two of the three already
exist.

## 3. The painter and text seam — recommendation

Unity contributes a GPU and a platform, not a UI framework. Pushing solved
geometry into UI Toolkit means fighting its layout and style resolution for
no benefit while inheriting its limits on stroke alignment, blend modes and
blur.

The shape: `.dsb` → arena → solve → typeset → commit → paint table →
one FFI crossing per frame (pointer + length to `#[repr(C)]` rows) →
`GraphicsBuffer` → instanced quads with the SDF math in HLSL.

Text stays in `dashscene-typeset`. The solver needs measurement *during*
layout — the measure callback — so handing shaping to a host typesetter
makes every measure call an FFI round trip inside the flex resolve, and
makes line breaks, goldens and cross-painter parity platform-dependent.
What a host contributes is the atlas upload and the sampler.

Specifics that were checked, and that a first implementation gets wrong:

- **The atlas texture must be linear, not sRGB.** MSDF channels are
  distances. The reference painter uses `raster_n32_premul` for this reason
  (`crates/dashscene-skia/src/lib.rs:907-911`). Bilinear, no mips.
- **`atlas_px` is bottom-left origin**, which already matches Unity's UV
  convention. Skia flips it because Skia images are top-left
  (`crates/dashscene-skia/src/lib.rs:974-977`); copying that code verbatim
  flips twice.
- **`plane_em` is y-up from the baseline** while document space is y-down,
  so the top edge subtracts. Descenders make the bottom term negative.
- `px_range = distance_range_px * run.size / px_per_em`, in screen pixels.
- The resolve is `median3(sample) - 0.5`, then
  `clamp(sd * px_range + 0.5, 0, 1)`
  (`crates/dashscene-gpu/src/shaders/sdf.wgsl:91-108`). It takes
  `px_range` as a uniform rather than from `fwidth` deliberately: the
  derivative form has a documented failure where `fwidth` returns zero and
  the division produces a NaN that paints a hole, and the uniform form is
  what makes the math conformance-testable without a GPU.
- Fold `GlyphRun::opacity` into the fill alpha before upload; the resolve
  modulates coverage by `color.a`.
- Pin the `#[repr(C)]` layout with `offset_of!` assertions and mirror it in
  the host's declaration. A silent layout drift produces garbled geometry,
  not a crash.

**Colour space is two separate questions in opposite directions.** The
atlas must be linear because it holds distances. A composite over a
linear-rendering 3D pipeline must respect that the paint is sRGB-encoded
and measured that way
(`docs/decisions/blur-blends-in-srgb-encoded-space.md`). Getting both wrong
in opposite directions is a plausible outcome and neither looks like an
error.

## 4. Composition — recommendation, contingent on a measurement

Three modes, of which one is ruled out by §1.1. Overlay and interleaved are
the same mechanism at different band counts; world-space is the different
one and composes with overlay trivially, because each is its own paint
target with its own arena, commit and `paint()` call.

A `.dsb` holds many roots — story #594's fixture has 65 — so one file with
one root per surface is the natural expression. A single root should not be
split across surfaces; there is no document-level concept for which rects go
where.

**But "the shown root" is a load bound today, not a paint bound, and that
matters to the cost argument below.**
`docs/decisions/the-shown-root-bounds-the-load-not-the-paint.md` (accepted
2026-08-08) rules that the runtime paints every root, verified at three
sites: the solve runs `for &root in arena.roots()`, `Arena::dfs_order` seeds
from all roots into one unfiltered committed table, and every painter walks
that table with no notion of which root is shown. Nothing selects a root —
both integration crates call `dashbuf::prefetch::first_root`, so "the shown
root" means "root 0". Confining the paint to the shown root is that
record's intended end state and lands at v0.19.

So splitting surfaces by root is the right structure and is **not** free
today: each root costs a solve and a committed table every frame whether or
not it is drawn. The same record names that cost unmeasured — 65 artboards
cost 65 artboards of solve and committed table per frame while one is
shown. Until the paint follows the shown root, a host with several surfaces
pays for all of them, which belongs in the same measurement as §6.1 rather
than being assumed away.

**Cost, on a tiler.** Overlay is one pass and compliant by construction. A
cached world-space surface costs nothing on a frame where `generation()` is
unchanged, and overlay-plus-one-resolve on a frame where it is not. So
world-space is not more expensive, it is *conditionally* cheaper, and the
condition is change rate rather than visual style.

**The lever is therefore the surface split.** Partial repaint is not
available — R-T1 forbids damage-region redraw and the `Painter` trait says
so explicitly about the dirty set, which exists for the instance-buffer
upload — so the surface is the caching granularity. Split roots by change
rate: static chrome on a cached surface, continuously-changing readouts on
their own. A speed readout on the chrome surface repaints the whole thing
at its tick rate, and story #798 fixed the case that would otherwise have
hidden it (a text change at equal glyph count now dirties correctly).

**Rate.** The transition rate is a host decision — how often commit is
called (P3). If a repaint fits in the frame budget, run transitions at full
rate and the question does not arise. If it does not, a reduced rate works
and anchored 3D stays coherent *provided it reads `committed().rects()`*,
because FLIP's samples overlay onto the committed rects and everything
anchored to them steps together. The way to break that is a separate
host-side tween at full rate inside a reduced-rate surface. At 60 Hz choose
a divisor — 15 or 20, not 16, which lands on 3.75 frames and adds an
uneven cadence to an already low rate.

**Lit and shadow-casting UI forces world-space**, since a screen-space
overlay has no world position. Then the painter's output is a texture
feeding a host material, which respects boundary B exactly and makes the
RT the mechanism rather than an optimisation. Two consequences: the
fidelity gate moves to the RenderTexture, because a lit result is not
Figma-identical by construction and must not be tested as if it were; and
MSDF still resolves correctly because rasterising into the RT is unit
scale, at the cost of capping text resolution — size the RT for the
surface's worst-case on-screen extent.

**A translucent lit panel over animated 3D pays the background's full fill
plus the panel's overdraw with no depth rejection.** R-T2's opaque-core
split applies to opaque surfaces only, so the two are alternatives rather
than layers. That is a fixed cost of the visual design.

## 5. The reserved-node and overload pair

This is the part most likely to become a decision record.

A 3D object placed where the layout says needs two different facts, and
conflating them is the error:

- **"This node is reserved for external content"** — producer-agnostic. It
  says nothing here is authored to be final. Any host can read it. This is
  defensible as document vocabulary.
- **"I fulfil it with this specific object"** — host-specific. This belongs
  in host code (`dashlang`), not in the IR. Putting it in the schema would
  make one producer's integration story part of the format, against P5.

Two modes are worth distinguishing: *anchor*, where the box positions an
external object and the node's own paint still draws; and *replaced*, where
the node's paint is suppressed and the host draws in its place.

**The state table is the diagnostic design.**

| document reserved | host overloads | state                    |
| ----------------- | -------------- | ------------------------ |
| yes               | yes            | fulfilled                |
| yes               | no             | **unfulfilled intent**   |
| no                | yes            | **undeclared overload**  |
| no                | no             | ordinary 2D node         |

Row 2 is a migration burn-down: count them and you have the remaining 3D
work. Row 3 is the one nothing else catches — host code covering a node the
designer believes ships, so they keep maintaining art nobody sees.

Both fit the existing severity model without new concepts. `Warning` is
already "deferred vocabulary with a declared degrade", release builds
already run strict, and waivers already cover the deliberate exception
(`crates/dashscene-validator/src/lib.rs`).

**The diagnostic must be profile-aware.** On a `Core` target nothing is
ever fulfilled, so row 2 is the correct state there and must not warn —
otherwise every Core build emits one warning per reserved node, and a
diagnostic that fires on every build is one readers learn to ignore. On
`Core` the 2D content is not a fallback at all, it is the product.

**Where the check runs.** Not `dashc`, which sees the document but not the
host's bindings. A test in the integration crate that loads both and
asserts they agree is the cheapest durable home, and it runs in CI.

### Why this also gives an incremental migration

An un-replaced node draws the designed appearance rather than a stub, so a
half-migrated build is shippable and each replacement is one declaration
with no document change and no re-import. It is reversible per node, which
is a kill switch. And it pulls the same direction as the caching story: a
live 2D instrument dirties the document every frame, so each replacement
makes the dashscene surface more static.

**The failure mode is decay.** The 2D content goes stale once nothing
exercises it, and nothing fails, because the host draws over it — until a
`Core` target or a rollback needs it. Put the un-replaced render in the
goldens; the harness exists and the cost is near zero. This is the single
piece of discipline that makes the approach durable rather than one that
works once.

Keep one data source behind both renderers, or the 2D and 3D versions end
up disagreeing about the value.

## 6. Open questions

1. **The repaint cost of one cached surface on target hardware.** Every
   §4 conclusion is contingent on it: the transition rate, the
   translucent-panel headroom, whether the surface split is needed at all.
   This is the same measurement as **Q-6**
   (`docs/technotes/open-questions.md`) — the render-target budget value,
   which `RENDER_TARGET_BUDGET_PLACEHOLDER` stands in for and which keeps
   `paint.render-target-budget` a warning rather than an error
   (`crates/dashscene-validator/src/lib.rs:287`). One measurement retires
   both.
2. **Which packaging path**, which depends on whether the documents are
   large enough for demand paging to matter, and on the ASTC header
   question in §2.
3. **Whether the reserved-node flag is document vocabulary or an import
   sidecar.** The flag is defensible in the schema as §5 argues; a sidecar
   regenerated by each import avoids a `dashbuf` change and index-stability
   concerns entirely. Not resolved.
4. **Who owns the host-side binding check across two repositories.** The
   test in §5 covers document-versus-bindings. Bindings-versus-actual-host-
   objects spans a repository that does not exist yet.
5. **How a designer marks a reserved node in Figma** — the annotator
   plugin's `sharedPluginData` is the obvious channel, and the Figma
   vocabulary profile would need an entry. Layer names are not stable
   across renames; Figma node ids are.
