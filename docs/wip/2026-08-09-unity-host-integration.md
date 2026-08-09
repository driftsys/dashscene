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
             decided. The part most likely to become a decision record is
             §5's **placeholder diagnostic** — not the placeholder surface
             itself, which `docs/design/architecture.md` and
             `docs/technotes/runtime-content.md` §7 already carry, but the
             absent check that no host filled a placeholder and that no
             host covered a node that is not one.

             **Revised after review, 2026-08-09.** Two sections asserted
             as new what a record already decided: §4 proposed a render
             target for lit UI where `unity-painter-uses-brg.md` rules
             lit BRG with three material classes, and §5 invented a
             vocabulary for the placeholder surface. Both now cite what
             they build on. The lesson is the check that was missing from
             the first review round — asking whether a record already
             covers the ground, not only whether the claims are true.
    scope    embedding a dashscene document in a Unity host that also
             draws 3D content: packaging, the painter and text seam, the
             composition mode, and the diagnostic a placeholder-filling
             host needs
    builds on docs/technotes/rendering-and-painters.md §9-§11 (the Unity
             painter's material classes and the lit/unlit concept),
             docs/decisions/unity-painter-uses-brg.md (proposed),
             docs/decisions/unity-separate-repo-deferred.md,
             docs/decisions/backend-tiering-unity-skia-lean.md,
             docs/decisions/host-integration-in-three-layers.md,
             docs/design/architecture.md ("Placeholders and node
             replacement", and boundary B),
             docs/technotes/runtime-content.md §7 (the placeholder contract),
             docs/specification/03-target-hardware-rules.md (R-T1, R-T2),
             docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md,
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

Each was checked against the tree on 2026-08-09. Citations name a file and a
symbol rather than a line: this file's first draft used line numbers, and four
of them were already stale by the time it was committed, because `main` moved
between the reads and the branch.

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

- `dashpack::Profile` (`crates/dashpack/src/profile.rs`) — `Raw`,
  `HiFi`, `LoFi`. Per-asset-class tolerance bands. Compression quality
  only.
- `dashscene_validator::Profile` (`crates/dashscene-validator/src/lib.rs`)
  — `Core`, `Full`. Paint vocabulary. `Core` is documented as "the subset a
  fixed-vocabulary painter can honor **without a render-target
  round-trip**"; `Full` is documented as **"Unity-class"**.

A capability that a Unity host has and a lean target does not is a Core/Full
question. The vocabulary profile was defined for this case and names it.

### 1.3 Backdrop blur cannot see a host-drawn 3D scene

The `Painter` trait (`crates/dashpaint/src/lib.rs`, "The backdrop barrier") defines a backdrop
sample as reading "what is already composited beneath it" — within the
painter's own output. That a render-target group is a **backdrop root**,
reading that group's layer rather than the canvas beneath it, is D3 of
`docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`.

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

`Arena::layout(node)` (`crates/dashscene-core/src/arena.rs`) returns
layout **intent** — authored geometry plus the active variant overlay. It is
not the solved box, and it will look correct for fixed-position nodes while
being wrong for everything flex resolves.

The solved geometry is `CommittedScene::rects()`, reached by
`CommittedScene::rect_index_of(NodeId) -> Option<u32>`
(`crates/dashscene-core/src/committed.rs`), with `node_of` as its
inverse and `generation()` to detect when a cached index must be
re-resolved. Reading it per frame keeps P1 intact: the document still
carries no results.

There is no name-to-`NodeId` lookup. `Arena::name` is index-to-name only
(`crates/dashscene-core/src/arena.rs`), so a name-keyed binding needs a
map built by one walk at load.

### 1.6 A layout transition costs the repaint and almost nothing else

`crates/dashscene-engine/src/flip.rs`, module docs: "Each frame's cost is
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

### 1.8 `commit` is on `Txn`, and its bare form ignores flex intent

`Arena` has no `commit`. It has `open(&mut self) -> Txn<'_>`, and `Txn` has two
forms. `Txn::commit(self)` resolves with core's internal `FixedSolver` —
authored offset and fixed size, **flex intent ignored** — and says so: "Product
code with flex layout commits through `commit_with` and a real solver."
`Txn::commit_with(self, solver)` is the one a runtime uses.

Same shape as §1.5: the wrong call looks correct for fixed-position nodes and
is wrong for everything flex resolves. A real solver also owns a `Typesetter`
and an atlas `Arc`, because `TaffySolver` borrows both and is built per solve —
`corpus/showcase/src/solver.rs` is the reference for that ownership split.

`dashscene-ffi` takes the correct branch (`attach_live` with a `TaffySolver`),
but with `TaffySolver::new()`, which has neither a typesetter nor atlases. That
is issue #863, and it is why a `.dsb` with text draws none.

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
   _stored_ entries only. The mapped base inherits the **zip entry
   offset's** alignment, not the page's, so the entry must be aligned to
   the schema's requirement — AGP's default `zipalign` is 4. Verify with
   `zipinfo -v`; this is a build-step fix, and an unaligned base is silent.
3. **Extract to `persistentDataPath` at first run.** Always works, and
   alignment stops being a zip-layout question. Costs a first-run copy and
   permanent double storage. Needs an atomic write (temp, fsync, rename),
   a content-hash or version key so an app update does not leave a stale
   copy, and cheap header-only verification at open — a full verifier pass
   touches every page and defeats the demand paging it exists to protect.

**The payloads are headerless, and this file called it unverified for a
round when one grep settled it.** `dashpack::astc::encode`'s own doc comment:
"The returned payload is exactly `BlockSize::payload_len` bytes: the blocks of
the grid, in raster order, 16 bytes each. **It carries no header** — a
container is the KTX2 writer's job (story #431)." `dashpack::preview` repeats
it. So `LoadRawTextureData` takes the payload as it stands, with no 16-byte
offset. Recorded as a lapse as well as an answer: §1 sets the discipline that
every finding names the file and symbol it was checked against, and this was
the one claim in the file that reasoned its way to the right answer and then
declined to confirm it.

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

**This is already a decided direction, not a fresh proposal.**
`docs/decisions/unity-painter-uses-brg.md` (proposed 2026-07-13) and
`docs/technotes/rendering-and-painters.md` §10 rule **BatchRendererGroup over
GameObject-per-node** for the bulk SDF-quad UI, with a reason this capture
would not have found: BRG makes the Unity painter's data model the same shape
as the lean painter — instance buffer, SDF shader, GPU — so the
dirty-set and double-buffer logic maps onto R-T4 directly. GameObjects are
reserved for node replacement only.

The shape, then: `.dsb` → arena → solve → typeset → commit → paint table →
one FFI crossing per frame (pointer + length to `#[repr(C)]` rows) → a
`NativeArray` a Burst job fills, which **is** the BRG instance buffer → SDF
shader.

Text stays in `dashscene-typeset`. The solver needs measurement _during_
layout — the measure callback — so handing shaping to a host typesetter
makes every measure call an FFI round trip inside the flex resolve, and
makes line breaks, goldens and cross-painter parity platform-dependent.
What a host contributes is the atlas upload and the sampler.

Specifics that were checked, and that a first implementation gets wrong:

- **The atlas texture must be linear, not sRGB.** MSDF channels are
  distances. The reference painter uses `raster_n32_premul` for this reason
  (the sampling comment in `draw_run`, `crates/dashscene-skia/src/lib.rs`). Bilinear, no mips.
- **`atlas_px` is bottom-left origin**, which already matches Unity's UV
  convention. Skia flips it because Skia images are top-left
  (`draw_glyph_quad`, `crates/dashscene-skia/src/lib.rs`); copying that code verbatim
  flips twice.
- **`plane_em` is y-up from the baseline** while document space is y-down,
  so the top edge subtracts. Descenders make the bottom term negative.
- `px_range = distance_range_px * run.size / px_per_em`, in screen pixels.
- The resolve is `median3(sample) - 0.5`, then
  `clamp(sd * px_range + 0.5, 0, 1)`
  (`msdf_coverage` in `crates/dashscene-gpu/src/shaders/sdf.wgsl`). It takes
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
world-space is not more expensive, it is _conditionally_ cheaper, and the
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
and anchored 3D stays coherent _provided it reads `committed().rects()`_,
because FLIP's samples overlay onto the committed rects and everything
anchored to them steps together. The way to break that is a separate
host-side tween at full rate inside a reduced-rate surface. At 60 Hz choose
a divisor — 15 or 20, not 16, which lands on 3.75 frames and adds an
uneven cadence to an already low rate.

**Lit and shadow-casting UI does not need a render target, and an earlier
revision of this section said it did.** That was wrong against a record
written three weeks earlier. `rendering-and-painters.md` §10:
"**Lit BRG is possible — lighting is a shader-pass concern, not a
rendering-path fork.** Entities Graphics renders fully lit, shadow-casting
instances via BRG with zero GameObjects." So lit UI stays on the one BRG
path and the difference is expressed as **three material classes**:

- **unlit-overlay** — unlit variant, no light or shadow passes, cheapest.
  The default, and what §11 says information should be: "the authored colour
  _is_ the on-screen colour".
- **lit-opaque** — lit forward/GBuffer plus a ShadowCaster pass.
- **lit-cutout** — SDF alpha-clip lit plus an SDF-clipped shadow-caster.
  This is the answer to translucent shadow casting, which this capture had
  called an open hard problem.

Two consequences the record already states and this capture should not
re-derive. Every lit node adds passes, so R-T1's tile-flush cost argues for
keeping most UI unlit-overlay and marking only genuinely-physical nodes lit.
And §11: unlit-overlay nodes match the flat design exactly "while lit nodes
intentionally do not" — so the fidelity gate applies to the unlit path, and
a lit node is out of scope for a Figma comparison by construction rather
than by an accident of compositing.

A render target is therefore **not** the mechanism for lit UI. It remains one
option for caching a mostly-static surface, which is a separate question
answered by change rate, and it costs the RT switch R-T1 penalises.

**A translucent lit panel over animated 3D pays the background's full fill
plus the panel's overdraw with no depth rejection.** R-T2's opaque-core
split applies to opaque surfaces only, so the two are alternatives rather
than layers. That is a fixed cost of the visual design.

## 5. The reserved-node and overload pair

**Most of this already exists, and an earlier revision of this section
proposed it as new.** `docs/design/architecture.md` carries "Placeholders and
node replacement" as a reserved schema surface: `Node` already holds
`contribution_id`, `fragment_ref`, `declared_size` and `interim_fill`, added
append-only, and the record states that "node replacement is an
engine-painter-only concept, so it binds to the Unity painter row above as
well" — naming this exact case. The runtime contract is designed at
`docs/technotes/runtime-content.md` §7: a declared-size box that never hugs,
an `interim_fill` shown while content resolves, and a `contribution_id` a
runtime producer binds against. Four decision records already build on it.

So the vocabulary question is settled and this capture should use its terms —
placeholder, `contribution_id`, `declared_size`, `interim_fill` — rather than
the ones an earlier revision invented ("reserved", "fulfil"). What follows is
what that surface does **not** yet cover.

The split the existing contract already makes, restated so the rest reads:

- **The placeholder itself** is producer-agnostic document vocabulary. It
  says nothing here is authored to be final, and any host can read it.
- **Which object fills it** is host-specific and binds through
  `contribution_id`, outside the IR. Putting a host's object identity in the
  schema would make one producer's integration story part of the format,
  against P5.

`interim_fill` is also the answer to the "does the placeholder paint?"
question this capture asked as though it were open: the distinction between
_anchor_ (the box positions an object and the node still draws) and
_replaced_ (the host draws in its place) is `interim_fill` present or absent,
not a new pair of modes.

**What is genuinely not covered** is the diagnostic — nothing today reports a
placeholder no host filled, or a host covering a node that is not a
placeholder. That is the part worth a decision record, and it extends the
placeholder contract rather than standing beside it.

**The state table is the diagnostic design.**

| node is a placeholder | a host contribution binds it | state                    |
| --------------------- | ---------------------------- | ------------------------ |
| yes                   | yes                          | filled                   |
| yes                   | no                           | **unfilled placeholder** |
| no                    | yes                          | **undeclared overload**  |
| no                    | no                           | ordinary 2D node         |

Row 2 is a migration burn-down: count them and you have the remaining 3D
work. It is also not an error on its own — `interim_fill` exists precisely
so an unfilled placeholder draws something. Row 3 is the one nothing else
catches: host code covering a node that is not a placeholder, so the
designer keeps maintaining art nobody sees.

Both fit the existing severity model without new concepts. `Warning` is
already "deferred vocabulary with a declared degrade", release builds
already run strict, and waivers already cover the deliberate exception
(`crates/dashscene-validator/src/lib.rs`).

**The diagnostic must be profile-aware.** On a `Core` target nothing is
ever filled, so row 2 is the correct state there and must not warn —
otherwise every Core build emits one warning per reserved node, and a
diagnostic that fires on every build is one readers learn to ignore. On
`Core` the 2D content is not a fallback at all, it is the product.

**Where the check runs.** Not `dashc`, which sees the document but not the
host's bindings. A test in the integration crate that loads both and
asserts they agree is the cheapest durable home, and it runs in CI.

### Why this also gives an incremental migration

An unfilled placeholder draws its `interim_fill` — the designed appearance
rather than a stub — so a half-migrated build is shippable and each
replacement is one binding
with no document change and no re-import. It is reversible per node, which
is a per-node way to revert without a rebuild. And it reduces the same cost
the caching story does: a
live 2D instrument dirties the document every frame, so each replacement
makes the dashscene surface more static.

**The failure mode is decay.** The `interim_fill` content goes stale once
nothing exercises it, and nothing fails, because the host draws over it — until a
`Core` target or a rollback needs it. Put the unfilled render in the goldens; the harness exists and the cost is near zero. This is the single
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
   (`RENDER_TARGET_BUDGET`, `crates/dashscene-validator/src/lib.rs`). One measurement retires
   both. The per-root solve and committed-table cost of a multi-surface
   document belongs in the same pass, for the reason §4 gives: until the
   paint follows the shown root, a host pays for every surface every frame,
   and that cost is unmeasured too.
2. **Which packaging path**, which depends on whether the documents are
   large enough for demand paging to matter. The ASTC header question that
   used to hang off this is answered in §2 — the payloads are headerless.
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
