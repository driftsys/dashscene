# dash — from Figma to code

    repo     driftsys/dash
    status   working draft (seed document for this repository —
             self-contained: goals, requirements, stack, format,
             producers, painters, features, plan)
    date     2026-07-11
    naming   DSB ("dash scene binary") is the intermediate
             representation, and `dashc` is the compiler. This document
             originally called them SCD and scdc, and said both were
             working names to be renamed freely. That invitation was
             taken up on 2026-07-13: the format that shipped is `.dsb`
             (schema in `dashbuf`), so the name follows the artifact.
             SCD and scdc are retired and appear nowhere in the code.

             The file extension is the one exception: the body below
             still says `.scb`. SCOPE_DECISIONS §3 already retired it in
             favour of `.dsb`, and other records quote this document's
             wording verbatim (§9 cites DESIGN §4's ".scb is one way to
             populate it"). Rewriting the extension here would make
             those quotations false, so the seed text keeps it and §3
             remains the ruling.

> **Superseded 2026-07-14** by
> `docs/decisions/dashscene-document-is-the-ir.md`. The IR is the dashscene
> document; `.dsb` is its file extension. The retirement of `SCD` recorded
> above stands; the naming of the IR as "DSB" does not.

dash turns UI designed in Figma — or authored programmatically in
code — into pixels on screen, through one intermediate
representation, one shared layout+text runtime, and interchangeable
paint backends. Primary targets: embedded/automotive-class devices
with tiling mobile GPUs (GLES 3.2) rendered by a game engine
(Unity) or a lean native renderer; a Skia backend serves as the
reference implementation, test oracle, and 2D path; a browser path
exists via wasm.

## 1. Goals and requirements

Goals:

    G1  Designers author in Figma; engineers author in code (Rust
        now, C# in-engine later); both produce the SAME document
        format and render identically.
    G2  Multiple render backends show the same pixels: Unity
        (product, lit/3D-capable), native (lean: far less memory
        and CPU than a game engine), Skia (reference/testing/2D),
        wasm (review).
    G3  Everything is testable: bit-exact goldens where possible,
        structural rect-table diffs everywhere, import-time
        diagnostics instead of runtime surprises.

Hard requirements:

    R1  Text: Middle-East scripts required (Arabic — shaping,
        ligatures, bidi/RTL). Perfect text quality. Identical text
        size and quality on every backend. High performance.
    R2  Layout: full Figma auto-layout vocabulary — all four modes
        (horizontal, vertical, wrap, GRID incl. spans), hug/fill/
        fixed sizing, min/max, gap, padding, alignment.
    R3  Backends: GPU is present on targets; GPU performance and
        memory bandwidth are the bottleneck. The native variant
        must consume far less memory and CPU than the engine
        backend. Backend selection is whole-scene, not per-node.
    R4  Animation must be reproducible in tests and have statically
        provable frame cost (no producer code in the frame loop).
    R5  Documents load fast: cold-start cost proportional to what
        is shown, not to file size (mmap + section discipline).
    R6  Unsupported design vocabulary is a named import diagnostic
        (warning/error), never a silent drop or runtime fallback.
    R7  Reproducible builds: same input → byte-identical document
        (hashing, signing, and CI depend on it).

## 2. Stack

    concern            choice                 why
    -----------------  ---------------------  -----------------------
    layout solver      Taffy (Rust)           only engine covering
                                              all 4 Figma modes; CSS
                                              Grid native; pure Rust
                                              (Servo/Bevy/Slint/Zed
                                              lineage). Yoga is
                                              flexbox-only; engine-
                                              internal solvers are
                                              inaccessible/lossy.
    text shaping       rustybuzz              HarfBuzz port, pure
                                              Rust; Arabic GSUB/GPOS
    bidi               unicode-bidi           run splitting, RTL
    font metrics       ttf-parser             same font tables as
                                              FreeType, same numbers
    glyph atlas        msdf-atlas-gen         MSDF quality, keyed by
                                              glyph id (contextual
                                              forms included)
    document format    FlatBuffers            zero-copy mmap load,
                                              schema evolution
                                              (optional fields,
                                              append-only ids), same
                                              schema as wire format
    vector baking      lyon (mesh) / SDF      arbitrary paths baked
                       atlas                  at compile time
    reference painter  skia-safe (Skia)       full 2D vocabulary;
                                              CPU raster = bit-exact
                                              deterministic goldens;
                                              GPU/GLES on target
    wasm painter       tiny-skia              pure Rust, Skia
                                              algorithms, plain
                                              wasm32 (no emscripten)
    engine painter     Unity + SDF shader     lit world-space
                       library                rendering; thin C#
                                              projection
    lean painter       custom instanced       bandwidth-optimal for
    (later)            SDF-quad renderer,     a quad vocabulary;
                       GLES 3.2               ~5-10 KLOC
    textures           KTX2/Basis (UASTC/     small at rest, GPU-
                       ETC1S) → ASTC/EAC      native in VRAM
    Figma source       @figma/rest-api-spec   official OpenAPI 3.1
                                              types; stable-additive

## 3. Principles

P1 — The document carries intent, never results. No resolved
x/y/w/h, no rasterized pixels, no glyph positions. Anything
resolved would pin the document to one backend or font build.

P2 — One solver, one typesetter; painters only color. Layout
(Taffy) and text placement (rustybuzz) run exactly once, in shared
Rust. Every painter consumes finished rects and positioned glyph
runs. Cross-backend identity is structural, not tested-for.

P3 — Producers mutate, the runtime owns time. Producers commit
structure, props, and variant switches whenever they like; nothing
producer-side executes inside the frame loop. All animation is
descriptive data.

P4 — Vocabulary is validated, never discovered. Paint profiles are
checked at import/commit; every out-of-profile construct is a named
diagnostic, never a runtime surprise.

P5 — Figma compatibility is a property of one producer. DSB is a
schema-first IR with its own spec and validator. The Figma exporter
is one client; the code DSLs are others. No producer's limitations
define the format.

## 4. Pipeline

    STAGE 1 — build time (dashc, offline)
      Figma REST JSON --> dashc --> .scb (flatbuffer) + assets

    STAGE 2 — common runtime (Rust, one instance)
      arena + variants + text stack + Taffy + FLIP
        --> rect table + positioned glyph runs (double-buffered)

    STAGE 3 — painters (one per target, one trait)
      Skia (v0 native)    Unity (v1 product)    lean GPU (later)

    boundary A = .scb load gate (version + per-section hashes)
    boundary B = painter contract: rect table + glyph runs + paint
                 indices. A painter never measures, wraps, kerns,
                 or moves anything.

Programmatic producers enter at stage 2: the in-memory arena + its
staged mutation API (open/set_prop/set_variant/commit) is the real
contract; .scb is one way to populate it.

Stage 1 does everything that can fail at compile time (validation,
lowering, atlas generation, shadow baking). Stage 2 does everything
that must be identical across backends, once. Stage 3 does only
what is legitimately per-target: how a rectangle gets colored.

## 5. The document (.scb)

Flatbuffer; flattened DFS node tree (doc index = rect-table index),
interned strings, dedup style pool.

    table          carries                          never carries
    -------------  -------------------------------  --------------
    layout table   mode NONE/H/V/WRAP/GRID, hug/    resolved rects
                   fill/fixed, min/max, gap,
                   padding, align, grid spans
    paint table    paint-kind enum, fill/stroke/    pixels
                   effect params, token refs,
                   material class
    variant table  sparse per-variant overrides     duplicate trees
    text           strings + style refs             glyph positions

Figma≠CSS lowerings happen in dashc: negative gap → margins, canvas
stacking → explicit order, strokes-in-layout → size adjustment,
scale constraints → insets.

Sections + mmap (R5): hot sections (tree, tables, strings) packed
at the file head; cold sections (heavy decor) page-aligned at the
tail. Per-section hashes with a signed root — the load gate
verifies hot sections without touching cold pages (whole-file
hashing would fault everything in and cancel the mmap win). Cold
pages are prefetched-and-verified on a loader thread; the render
path never takes a page fault. Section flavors: resident-raw (mmap
in place), resident-compressed (read once, decode at prefetch,
discard), external (fragment ref — reserved). Embed-vs-external is
a packaging bit, not a format fork.

Profiles (R6): a named paint-vocabulary subset a target honors.
profile:full (Unity-class) vs profile:core (lean/native painters:
runtime blur banned or budgeted, shadows baked at compile time,
backdrop blur banned or degraded to a declared pre-baked texture).
The validator runs from day one even while the permissive Skia
painter can draw everything — otherwise design files accumulate
vocabulary the lean painter will never support, and a painter swap
becomes design-file remediation instead of a re-golden.

Reserved surface (schema now, feature later): placeholder node kind
with contribution_id / fragment_ref / declared_size / interim_fill
(§10). Flatbuffer fields are optional and ids append-only, so this
costs nothing and keeps old loaders reading new documents.

## 6. Producers — the paths into DSB

### 6.1 Path 1: Figma

Source of truth: figma/rest-api-spec (OpenAPI 3.1, npm
@figma/rest-api-spec). The REST API is stable-additive; pin the
spec version, re-capture fixtures on deliberate upgrades, and make
the importer forward-tolerant: unknown node types / paint kinds are
diagnostics with declared handling, never crashes or silent drops.
Operational notes: personal access tokens expire (90 days — CI
rotation); use granular scopes (file_content:read); rate limits are
seat-gated (a paid Dev/Full seat is required for real use); the
Variables REST endpoint is Enterprise-gated (see tokens below).

Export = declared roots + reachability closure:

- An export manifest lists root frames by stable id. Roots say what
  ships; the closure proves what that requires; nothing else enters
  the document.
- Variant closure is per component SET (runtime can select any
  member). A frozen subset is an explicit declaration, never an
  inference.
- Glyph coverage comes from declared per-locale charsets, never
  from document text (slot-bound strings arrive at runtime).
- The closure spans files (library components resolve by key);
  unresolvable = error naming file and key.
- Emission is byte-reproducible (stable-id ordering, R7).

Diagnostics: every out-of-profile construct → {rule id, node path,
severity, workaround hint}. error blocks .scb; warning = deferred
vocabulary with a declared degrade; release builds run strict (zero
warnings or explicit waiver entries). Trim layers: root scoping,
slot-children auto-replaced (slot content in Figma is sample
content by definition), `_` name prefix as sugar, sharedPluginData
roles as machine truth (a small annotator plugin writes role =
placeholder|sample-content|redline|spec; the REST API returns it
via ?plugin_data=shared). Hidden ≠ trimmed: hidden nodes export as
visible:false (they may be variant states).

Design tokens, two phases: GET /file returns resolved values plus
boundVariables IDs on any paid plan (names/collections/modes need
the Enterprise Variables endpoint). Phase 1 emits resolved literals
and preserves the IDs in a sidecar; phase 2 joins IDs to names via
a plugin-exported table (or naming convention) and switches to
token refs. Token refs are in the schema from day one.

Fixtures: record-and-replay. Capture real GET /file JSON for
purpose-built corpus files, commit as fixtures, CI runs offline;
one nightly live smoke test catches schema drift. No public fixture
corpus is recent enough (grid mode, boundVariables, 2025 effects).

### 6.2 Path 2: code DSLs

All code DSLs are typed skins over ONE producer surface (the same
staged-mutation / describe-buffer contract sandboxed plugin
producers would use). No second runtime scene API.

    skin        runs             transport
    ----------  ---------------  --------------------------------
    Rust DSL    in-process       direct arena calls, no
    (v0)                         serialization — components are
                                 fns, tokens are consts, loops are
                                 repeaters, the type checker is
                                 the validator's first line
    C# decl.    engine process   describe buffer built C#-side,
    (v1)                         ONE commit across the FFI seam
                                 (no per-prop FFI; struct/Span,
                                 pooled, GC-free; typed keys via
                                 codegen — component ids and slot
                                 names compile-checked)
    Kotlin /    other process/   same flatbuffer schema used as
    remote      VM (future)      scene-fragment messages (one
                                 schema, two roles: file + wire);
                                 untrusted fragments pass an
                                 admission policy

The validator is one shared crate called at every entry: dashc at
compile time, builder commit at runtime, fragment admission on the
wire. The Rust DSL doubles as the stress-corpus generator
(wrap/hug-in-fill/grid-span/bidi/variant-topology edge cases are
generated, not hand-built in Figma). A code-authored screen that
uses engine-only features is engine-only — a declared property of
the screen, like a profile. DSLs mostly instantiate document-
compiled components and bind data into slots; raw node construction
is available when needed.

Ownership: both paths are strictly one-way (no DSB→Figma
round-trip, ever). Each component has exactly one authoring home,
declared in a manifest; a shared symbol namespace (component ids,
token names) lets either side instantiate the other's components.

### 6.3 Time invariant and animation

Event-driven mutation is unrestricted: data changes → set_prop +
commit, at signal rate if need be. Banned: frame-synchronous
callbacks (the renderer asking producer code "where should this be
now"). The test is directional — producers push in on their own
schedule; the runtime never calls out mid-frame.

Descriptive animation vocabulary, executed by the runtime:

    variant transition   per-prop targets + spec (tween/spring/
                         keyframes) + stagger; FLIP for layout
                         deltas
    per-prop smoothing   declared spring/filter on a bound prop
                         (gauges, live values)
    loop track           period + spec (spinners, pulses)
    keyframe track       sampled curve — data (exotic easing)
    enter/exit           declared specs on slot mount/unmount

Required properties (R4): interruptibility (mid-flight retarget is
defined — springs give it naturally) and statically bounded cost
(frame budget provable from the document).

Compose calibration: updateTransition / animateXAsState /
AnimatedVisibility / animateContentSize / InfiniteTransition /
keyframes{} map one-to-one onto the table — their specs are data,
and the DSL skins may expose Compose-shaped ergonomics that lower
to it at commit. Compose's closure escape hatches (Animatable +
suspend, withFrameNanos, arbitrary AnimationSpec functions) do not
survive: they become sampled curves, upstream mutation (gesture
handlers commit at input rate), or an engine-side slot doing its
own per-frame work outside layout authority. Choreography ("toast,
wait 3 s, dismiss") is app logic — timed commits. A declared
state-machine layer (Rive-class) is deliberately deferred (Q-3).

## 7. Common runtime

### 7.1 Layout — Taffy

Taffy is the sole solver for all backends (R2). Decisive: Figma
auto-layout has four modes (H, V, wrap, GRID); Taffy implements CSS
Grid natively; Yoga is flexbox-only (grid would be emulated and
break on spans). Unity's internal Yoga is inaccessible (all types
internal; exposure declined pending their layout refactor). uGUI
LayoutGroup is a lossy box model. layoutMode NONE = passthrough,
not a second engine. Check-item: baseline alignment is Taffy's
least-exercised corner — one mixed-size baseline row in the corpus
before final sign-off (Q-4).

### 7.2 Text — one typesetter, painters as glyph painters

R1 forces this design and removes engine text systems (e.g.
TextMeshPro) from typesetting entirely — such systems are three
things: a FreeType metrics extractor (replaced by ttf-parser, same
font tables, same numbers), a typesetter (replaced by rustybuzz,
strictly better for Arabic), and an SDF quad renderer (the idea is
kept, driven from our own atlas).

    build:   font.ttf --> msdf-atlas-gen --> glyph atlas keyed by
             GLYPH ID (contextual forms are just glyphs via GSUB)
             + metrics blob (ttf-parser)
    runtime: text+style --> bidi split (unicode-bidi) --> rustybuzz
             shape --> line break --> positioned glyph runs
             (glyph id, x, y, size, atlas page)

Every painter draws the same atlas quads (engine: SDF material;
Skia: textured quads via SkTextBlob — Skia's native text is a debug
overlay only). Identity across backends is by construction: one
typesetter, one atlas. Taffy's measure callback reads the
shaped-run cache (keyed string+style; numerals fast path), so
layout and paint cannot disagree. Performance: cockpit-class UI
text is mostly static labels + changing numerals — the cache plus a
pre-shaped numerals path makes shaping cost negligible; rendering
is batched quads, one draw per atlas page. Open: MSDF vs per-size
baked bitmap atlases for small fixed sizes, < ~14 px (Q-1).

### 7.3 Output

Generation-stamped double buffer of blittable rect entries + the
glyph-run table + a dirty set. That triple plus paint-table indices
is the entire painter input (boundary B).

## 8. Painters

One trait (boundary B). Consequences: painter swap = re-golden, not
redesign; diffs bisect by construction (same .scb + same commits →
bit-identical rect tables, so a pixel diff can only implicate the
paint layer); CPU painters are their own golden generators (CPU
raster is deterministic; GPU painters use tolerance-based
perceptual diffs).

### 8.1 Skia — v0 native painter, reference forever

skia-safe. Two execution modes from one API: CPU raster = bit-exact
CI goldens; GPU on GLES = the on-target native path until the lean
painter exists. Full paint vocabulary available on day one — which
is why the profile validator must run from day one (see §5).
Lowerings, all non-structural: diamond gradient via SkSL (not a
Skia primitive), stroke align inside/outside via path expansion
(Skia strokes are center-only), shadow-spread math. Graphite (the
next-gen Skia backend) changes performance, not fidelity — don't
couple to it; bindings lag. When the lean painter ships, Skia
retreats to dev-side oracle.

### 8.2 Unity — v1 product painter

Lit world-space rendering (UI toolkit paths rejected: unlit-only in
world space; native-plugin direct drawing rejected: exits the SRP
and forfeits lighting). The C# layer is a projection, not a
renderer: transforms written from the rect table to
pre-instantiated GameObjects; paint entries resolve to materials
from the SDF shader library — rounded rect, ellipse, all four
gradient types (diamond is easier here than in Skia), stroke align
native in SDF; text = atlas quads (TMP-style rendering without TMP
typesetting).

Material class per paint entry:

    lit-opaque     full lighting, casts/receives shadows,
                   reflection probes / SSR
    lit-cutout     SDF alpha-clip; shadow-caster pass with clip
    unlit-overlay  HUD-style, excluded from lighting

Transparent surfaces neither receive SSR nor cast clean shadows —
physics of the technique; designers choose lighting participation
per node via the class. Blur = RT + separable passes,
count-budgeted; backdrop blur is profile:full-only and additionally
a compositor-policy question on multi-layer display stacks. Node
replacement (§10) is an engine-painter concept only.

### 8.3 Lean native painter — later

For targets where memory + bandwidth are the constraint (R3) and
the vocabulary has collapsed to quads (SDF parametric + atlas +
baked), a custom instanced-quad renderer is bandwidth-optimal — no
intermediate RTs, no saveLayer round-trips, ~5–10 KLOC because the
hard 90 % of a 2D renderer (typesetting, layout, paths, effects)
lives upstream in dashc and the runtime. The SDF shader source is
single-sourced with the engine painter, so it is debugged on real
hardware before this painter exists. Resource profile vs an engine:
no managed heap, no GC, no scene graph duplication; CPU per frame ≈
dirty-row upload + submit; memory = binary + atlases + swapchain.

Rejected alternatives, with reasons: CPU rasterizers as product
painters (tiny-skia stays a CI-reference/wasm option; vello_cpu
worth re-checking yearly); Blend2D (JIT = W^X/certification problem
on locked-down OSes); wgpu/Vello (no story on the target OS; the
vocabulary doesn't need runtime path rendering); Impeller
(engine-shaped, someone else's roadmap); Skia-GPU long-term
(different AA model than SDF quads — permanently non-identical to
the engine painter — plus saveLayer bandwidth).

### 8.4 Web — parked

Ladder, climb only when pushed: Rust painter core + tiny-skia
behind the trait on wasm32-unknown-unknown (wasm IEEE-754
determinism → browser pixels match CPU goldens by construction) →
server-rendered exact frames (native painter renders PNG, zero new
painter code) → CanvasKit (a second painter implementation in TS,
~7 MB) only if offline-interactive full fidelity is a demonstrated
need. skia-safe to wasm is emscripten-only; avoid.

## 9. Target-hardware rules (tiling GPUs, GLES 3.2)

    R-T1  One render pass per frame; every mid-frame RT switch is a
          tile-memory flush + resolve. Blurs are the only exception
          and are count-budgeted paint kinds.
    R-T2  Split SDF quads into an opaque core (front-to-back,
          z-tested — hidden-surface rejection kills covered pixels)
          and a thin blended AA fringe. Converts mostly-opaque UI
          from blended overdraw to rejected pixels.
    R-T3  Framebuffer/texture compression on everything the driver
          offers (e.g. UBWC-class).
    R-T4  CPU frame cost = dirty-range instance-buffer upload from
          the rect table + submission. Nothing else.
    R-T5  SDF shader math single-sourced (common include) into both
          painters' shading languages. If engine and native painter
          share the same GLES driver, parity upgrades to "same
          math, one compiler."

Texture policy: GPU-native compressed formats for product assets
(ASTC/ETC2 family; single-channel SDF atlases in EAC-R11 — BC
formats are desktop-only, absent on mobile GPUs). KTX2/Basis as the
distribution format: UASTC for quality-critical (transcode to ASTC
at install time — no transcoder in the trusted load path), ETC1S
for bulk/disposable content (transcode at prefetch). Never
lossy-compress distance fields (block quantization mangles the
field gradient exactly on glyph and icon edges) — validator error.
Memory bandwidth is typically shared with everything else on the
SoC — frugality is systemic, not a local KPI.

## 10. Features

### 10.1 Figma vocabulary triage

    NOW (v0/v1)    all four gradient types (angular = gauges),
                   image fills + scale modes, baked drop/inner
                   shadows, shape masks, group opacity (compiler
                   detects non-overlapping children → per-node
                   opacity free; overlapping → budgeted RT),
                   axis-aligned + rounded clip, full text stack,
                   static variable-font instances, full auto-layout
                   (R2). Renders ~95 % of real product design
                   files.
    LATER (warn)   layer blur (budgeted), backdrop blur + advanced
                   blend modes (profile:full; spike
                   KHR_blend_equation_advanced first — it may make
                   multiply/screen nearly free), corner smoothing
                   (squircle), luminance masks, clip-on-rotated,
                   kashida justification.
    REJECT (error) noise/texture/progressive-blur effects, animated
                   boolean ops, animated variable-font axes — each
                   with a documented workaround (bake it, slot it,
                   design without it).

Deferred items are a negotiation surface with design, not a
compatibility debt: every LATER item has a designer-visible
workaround today, and the validator says so at import time.

### 10.2 Placeholders and node replacement (provisioned, later)

A component marked role=placeholder compiles to a STUB in the main
document (contribution id, declared layout box, variant axes+props,
interim fill, fragment ref); its authored fallback subtree + heavy
decor live in a sideloaded fragment, lazy-loaded at scene prefetch.
Resolution at bind: a registered native implementation wins and the
fragment is never loaded (zero bytes, zero parse); otherwise the
fallback renders as designed. Rules when active: placeholder size
is declared, never hug (or the compiler bakes measured per-variant
sizes — lazy load must not reflow); the Figma variant enumeration
is the acceptance contract for the native implementation
(CI-checked); fallback fragments are the most evictable assets; a
certified implementation lets packaging drop the fragment (a
packaging decision, not a document change); a stub with neither is
a strict-build load error. Node replacement is an engine-painter
concept only (arbitrary 3D/per-frame content in a layout box);
other painters always render the authored subtree.

## 11. Plan

Exit criteria for v0 (concept validation, Skia):

    E1  Same screen authored in Figma and in the Rust DSL →
        bit-identical rect tables and Skia renders.
    E2  Arabic screen (RTL, shaped, mixed numerals) golden-stable
        across machines.
    E3  Stress corpus green (wrap, hug-in-fill, grid spans,
        baseline, variant topology change, negative gap).
    E4  A deliberately dirty Figma file → full diagnostic report,
        no document emitted.
    E5  Variant switch animates via FLIP; goldens at t = 0/0.5/1.
    E6  Same input → byte-identical .scb (twice, two machines).

Slices (each ends in a rendered, golden-tested PNG in CI; the Figma
importer trails the DSL by at most one slice once it enters):

    v0.1  walking skeleton: schema, minimal DSL, fixed rects, solid
          fills, .scb round-trip, painter trait, golden harness
    v0.2  flex core: H/V, hug/fill/fixed, gap/padding/align,
          min/max, negative-gap lowering
    v0.3  basic paint: 4 gradients, rrect + stroke align, images,
          clip — importer enters (single frame, minimal)
    v0.4  variants + staged mutation + minimal FLIP        (E5)
    v0.5  text I: Latin — metrics, atlas, measure callback (text
          drives hug sizing)
    v0.6  text II: bidi/Arabic + charsets                  (E2)
    v0.7  importer catch-up: roots, reachability, cross-file, trim,
          deterministic emission, waivers            (E4, E6)
    v0.8  fidelity: wrap, grid spans, baseline, masks, group
          opacity, shadows                                 (E3)
    v0.9  parity: same-screen-both-ways fixture + CI gate  (E1)

Spikes: flatbuffer section-ordering control (v0.1), Arabic atlas
coverage in msdf-atlas-gen (start of v0.5), Taffy baseline (by
v0.8), real-file import (v0.7).

v1 — Unity + full feature + performance + production toolchain:
engine painter (SDF shader library, material classes, C# declare
skin), LATER features landing per priority (shadow baking switches
on and profile:core is enforced on target documents), loading
performance (mmap sections measured, prefetch choreography,
placeholder activation, KTX2 pipeline), rendering performance
(tiler rules measured on target; whether the lean painter lands
here or later is decided on measurements, not in advance),
production toolchain (dashc as a product: stable CLI, versioned
diagnostics, waiver workflow, linter rule packs, golden/report
tooling for design review).

v2 — remote/streaming: scenes and scene UPDATES streamed to
displays not local to the renderer. The architecture is already
shaped for it: streaming a scene is streaming the .scb once plus
the staged-mutation commit stream (descriptive animation makes
updates tiny — specs, not frames); the wire format is the same
flatbuffer schema as messages; the remote end runs a painter behind
the same trait. Open then: transport, remote painter choice,
latency budgets, admission policy for untrusted producers.

## 12. Open questions

    Q-1  MSDF vs per-size bitmap atlases below ~14 px (visual
         check, not an argument).
    Q-2  KHR_blend_equation_advanced availability/perf on target
         drivers (one-day spike; decides blend-mode phasing).
    Q-3  Declared state-machine layer (Rive-class) for sandboxed
         plugin producers — or is instantiate+bind+transitions
         enough?
    Q-4  Taffy baseline alignment behavior (corpus test).
    Q-5  Remote producer admission scope: composition+binding only,
         or raw node construction from untrusted peers?
    Q-6  Group-opacity RT budget value on target hardware
         (measure, then fix the number in profile:core).

## 13. Workspace layout

This section originally suggested a `scd-*` crate family. Those names
were never adopted: SCOPE_DECISIONS §2 mapped the roles onto the crate
names actually reserved on crates.io, and that map is authoritative.
The layout below is the one that exists.

    dashbuf/              flatbuffer schema + generated code + format
                          doc (section table, hashes, reserved fields)
    dashscene-validator/  shared validation crate (profiles,
                          diagnostics, waivers)
    dashlang/             Rust typed builder + corpus generator
    dashscene-core/       arena, node tree, layout + paint tables, and
                          the staged-mutation producer API
    dashscene-engine/     Taffy solve, variants, FLIP, measure callback
    dashscene-typeset/    metrics blob, bidi/shape/break, atlas
                          pipeline, glyph runs
    dashpaint/            painter trait + the paint table (boundary B)
    dashscene-skia/       Skia CPU/GPU painter
    dashc/                Figma importer + compiler CLI (roots,
                          closure, lowering, diagnostics)
    importers/figma/      the Deno/TypeScript Figma importer, which
                          calls dashc compiled to wasm
    corpus/               DSL-generated stress corpus + Figma fixture
                          captures (record-and-replay)
    goldens/              CI golden images + diff tooling

One role split in two on contact with the code: the original
`scd-layout/` is now `dashscene-core` (the model a producer mutates) and
`dashscene-engine` (the runtime that solves it) — see SCOPE_DECISIONS §9.
`dashcue` (the animation vocabulary) has no counterpart in the original
suggestion at all.
