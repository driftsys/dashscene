# The C# host: P/Invoke over the C ABI, lifetime and the tick

    status  as-built at story #1121 (2026-08-21), with the painter added at
            story #1122 and the packed-document load at story #1124
            (both 2026-08-23)
    source  stories #1121, #1122 and #1124, epic #1106. The requirements are
            [../specification/07-embedding-and-distribution.md](../specification/07-embedding-and-distribution.md)
            R-E1, R-E2, R-E10 through R-E17 and R-E19 through R-E20, settled by
            story #1125, and
            [../specification/03-target-hardware-rules.md](../specification/03-target-hardware-rules.md)
            R-T5.
    why     [../decisions/the-native-library-ships-inside-the-unity-package.md](../decisions/the-native-library-ships-inside-the-unity-package.md)
            rules the library name and where it sits;
            [../decisions/host-integration-in-three-layers.md](../decisions/host-integration-in-three-layers.md)
            puts a Unity host at layer 0 in its host-draws form;
            [c-abi.md](c-abi.md) carries the ABI itself;
            [../decisions/unity-painter-uses-brg.md](../decisions/unity-painter-uses-brg.md)
            D1 chooses BatchRendererGroup and names the three material classes,
            and D3 and D4 carry the fallback ladder the painter reads its rung
            off;
            [../decisions/r-e10-is-checked-in-two-halves.md](../decisions/r-e10-is-checked-in-two-halves.md)
            splits the compile gate the painter broke;
            [../decisions/clip-edge-semantics.md](../decisions/clip-edge-semantics.md)
            fixes what a clip contributes;
            [../decisions/the-document-is-mapped-where-it-is-packed.md](../decisions/the-document-is-mapped-where-it-is-packed.md)
            rules how the document's bytes reach memory.

The managed half a Unity host sits on: version negotiation, runtime lifetime,
document load, the tick, the committed frame under a lease, and — since story
#1122 — the painter that draws those tables through `BatchRendererGroup`.

**What it draws is a subset, and every gap is a named diagnostic.** Fills, both
solid and gradient, corner radii, strokes, clips, per-node opacity and rotation
— and, since story #1123, text. Not shadows, not blurs, not image fills, not
baked vector nodes and not render-target groups. `PackDiagnostic` names each one
and the painter reports it, which is P4.

    Samples~/FrameLoop --+--> CommitPacer      (when to commit)
    Samples~/Showcase ---+    (both samples)
                         |
                         +--> DocumentRange    (where the .dsb is)
                         |
                         +--> DashsceneRuntime --+--> Native (a forwarder per
                         |    FrameLease --------+     entry point, over a
                         |                       |     private DllImport each)
                         |    TextAtlasSet ------+     NativeText (the atlas)
                         |                       |
                         |                 dashscene_ffi (cdylib)
                         |
                         +--> BrgPainter ---+--> FramePacker  (what to draw)
                              (Runtime/     |    PackDiagnostics
                               Engine/)     |
                                            +--> BatchRendererGroup
                                                 GraphicsBuffer x5
                                                 AtlasTexture (one per sheet)
                                                 Dashscene/* shaders

## Compile territory: three tiers, and why each file sits where it does

`Runtime/` is checked in **two halves**, and where a file goes is a design rule
rather than a filing convention
([../decisions/r-e10-is-checked-in-two-halves.md](../decisions/r-e10-is-checked-in-two-halves.md)).

| where             | references the engine | checked by                             | runs in CI |
| ----------------- | --------------------- | -------------------------------------- | ---------- |
| `Runtime/`        | no                    | `unity/package-compat`, netstandard2.1 | yes        |
| `Runtime/Engine/` | yes                   | `just unity-editor`, a Unity editor    | no         |
| `Samples~/*/`     | yes                   | `just unity-editor`, copied in         | no         |

R-E10 requires every C# type under `Runtime/` to compile against
`netstandard.dll` 2.1.0. `unity/package-compat` carries no Unity reference
assemblies, so a `UnityEngine` type fails there with `CS0246` whatever its API
compatibility level actually is — which is what story #1121 predicted as issue
#1286 and story #1122 met with 26 errors on its first build.

**The split is drawn so that what decides the picture stays in the checked
half.** `FramePacker` reads the committed tables, resolves each rect's kind and
row, packs the paint heap and produces the five per-instance arrays — all of it
engine-independent, all of it compiled on any pull request whose diff is not
documentation-only, and all of it **executed** by `unity/ffi-check`: against a
real committed frame, and against synthetic frames built from pinned managed
arrays, which is what lets a single property be varied at a time.
`Runtime/Engine/BrgPainter.cs` holds the `BatchRendererGroup` lifetime, the
buffer upload and the culling callback, and decides nothing about what is drawn.
A file that could be written without the engine and is not is a defect against
that rule.

**The exclusion is itself asserted**, in `unity/package-gate`: the `Exclude` in
`PackageCompat.csproj` is exactly `Runtime/Engine/**/*.cs` and is the only one,
and every file under that directory references the engine. Without those,
meeting a `CS0246` by widening the exclusion would narrow R-E10 to whatever was
left, with nothing saying so — the repair #1286 predicted.

`Samples~/FrameLoop/` stays a sample for the reason it always was:
`Time.deltaTime` and a component lifecycle need an editor to mean anything, and
the `~` suffix hides the directory from Unity's importer — one of the four
shapes R-E2 enumerates — so it needs no `.meta`.

**No CI job compiles it, and that is the third row's cost.** It was compiled by
nothing at all until issue #1298 put the painter's wiring there.
`just
unity-editor` now copies it into its throwaway project's `Assets/` and
asserts it compiled, which is a developer's gate and not CI's.

## The painter, and where each rule it obeys comes from

`BrgPainter` is one `BatchRendererGroup`, one mesh, one material for the class
it was built with plus one per glyph atlas, and five `GraphicsBuffer`s. Per
frame it packs the lease's tables, writes one staging buffer and issues draw
commands from the culling callback.

**The instance layout is the lean painter's in size and in what it carries, and
that is the point.** Five `float4`-sized per-instance properties — the node's
box, its corner radii, `(opacity, outset, rotation)`, the rotation pivot, and
`(kind, row, clip offset, clip count)` — eighty bytes, the same eighty
`dashscene_gpu::Instance` occupies. **The member order is not the same and the
member set is not identical**: that struct also carries `shape`, `layer` and a
pad word this painter emits nothing for, and puts `kind`/`row` at offset 32
rather than in the last word. What matches is the stride and the meaning of
every field both carry.

The paint heap **is** packed word for word as `paint.wgsl` reads it: a gradient
row is twelve `float4`s in `gradient_colour`'s own order, a clip box is two, and
a stroke is two — colour first, then `(width, align, 0, 0)`, which is the order
`struct Stroke { color, width, align, _pad }` declares. A first version of the
packer wrote those two words the other way round; it was internally consistent
with its own shader and was not the row the other painter reads, which is what
this claim has to mean. One word is deliberately not byte-identical: `align`
crosses as an `f32` here and as a `u32` there, forced by this heap being a
`StructuredBuffer<float4>` — both painters hand it to `stroke_coverage` as a
float, so the conversion happens on write here and on read there.

Two painters written in different languages have a chance of drawing the same
picture only if they are stated over the same rows, and issue #828's portable
conformance suite is what will judge whether they do. `unity/package-gate` holds
the gradient row width across all three, the clip and stroke widths between the
C# packer and the Unity shading, the stop bound between the packer and the
generated library, **and the stroke row's field order** against `paint.wgsl`'s
`struct Stroke` — that last one because a width cannot see a permutation, which
is exactly how the reversed stroke row survived the first version of this gate.

**The transforms are shared, not per instance.** A BatchRendererGroup requires
`unity_ObjectToWorld` and `unity_WorldToObject` as instanced properties, but a
metadata value without the high bit addresses one value every instance reads. A
document is one sheet, so its object-to-world is one matrix — which is what
keeps an instance at eighty bytes rather than a hundred and seventy-six.

**The rung is read from Unity, never inferred** (D4, R-E14, R-E19). The painter
refuses to construct anything when `SystemInfo.graphicsDeviceType` is `Null`,
because a `-nographics` process reports `UnsupportedByUnderlyingGraphicsApi` and
that is not a verdict — it is the hazard D4 names and story #1125 measured.
`RawBuffer` and `ConstantBuffer` are rung 1;
`UnsupportedByUnderlyingGraphicsApi` selects **rung 3**, and since nothing is
built for rung 3 the painter records the rung on its `Rung` property and draws
nothing rather than pretending to be on rung 1. **It warns there as well as
recording the rung** (issue #1326): `Rung` is availability rather than a report,
so a host that never reads it would otherwise get a blank screen and a clean
console. R-E6's default produces a blank frame too, and Unity itself names that
one on every frame, so rung 3 was the blank frame a host could not tell apart
from a defect in its own document. The `Samples~/FrameLoop` component reads
`Rung` and escalates to an error on top of that. `Unknown` is documented as a
value Unity never returns, so D4's table assigns it none: the painter names it
and constructs no group.

**R-E5 is read from `Draw`, not from the constructor** (issue #1317). URP
assigns `GraphicsSettings.useScriptableRenderPipelineBatching` inside its own
pipeline instance's constructor, which Unity runs at the first render — so the
global is `false` in `Awake` of the first frame whatever the project is set to,
and a painter built there warned about every correctly configured host.
`ReportBatcherOnce` guards the read on `RenderPipelineManager.currentPipeline`
and reports once per pipeline instance: while that is null the read decides
nothing and the painter says nothing, and a later instance — a quality-level
switch, a different asset — is decided again rather than latched.

`just unity-render` holds all of this: a pipeline instance is live once a frame
has rendered, and the painter logged no R-E5 warning on a project that meets
R-E5. Restoring the read to the constructor makes that run report one warning
and fail, which is the negative control for it.

**Two hosts this does not decide correctly, and they fail in opposite
directions** — both named in the method's own remarks. A process that draws
every frame but renders through no pipeline is told nothing: the guard never
opens, and nothing says "undecided" (issue #1340). A host on a pipeline that
does not assign the global is told something false — it would warn about a
project that meets R-E5, which is issue #1317 on a different pipeline. The
second is out of scope — this package depends on URP and R-E5 names URP's own
`m_UseSRPBatcher` — and the guard is deliberately not narrowed to a URP type.

**Under `ConstantBuffer` both bounds come from the device** (R-E15). The window
size and the offset alignment are read with `GetConstantBufferMaxWindowSize()`
and `GetConstantBufferOffsetAlignment()` and never compared against a literal;
the batch stride is rounded up to the alignment, and the instances per batch are
what the window holds after the 112-byte shared head — the two transforms plus
the mandatory zero `float4` at offset 0. R-E20's 256 **is** a literal, and
correctly so: it is `unity_DOTSVisibleInstances[256]` in the SRP core shader
library, a property of the shader rather than of the adapter. The painter splits
a batch into several draw commands rather than asserting the bound, because
asserting it would refuse documents it can draw.

**The lease does not cross into the culling callback.** Issue #1267 measured
`OnPerformCulling` on Unity's main thread under `6000.3.22f1`, URP, macOS and
Metal, so acquiring inside it is allowed — and this painter does not need to.
`Draw` copies the borrowed tables into arrays the painter owns, so the caller's
lease can end the moment it returns. Whoever moves that read into the callback
inherits the rule `FrameLease` states: release after Unity completes the
`JobHandle`, not on return from the callback, because the workers are still
reading the borrowed rows when the callback returns.

**A clip contributes anti-aliased coverage and multiplies into the shape's**
([../decisions/clip-edge-semantics.md](../decisions/clip-edge-semantics.md)),
and boxes within one region combine with `min` — which that record leaves open
and `paint.wgsl`'s `clip_coverage` settles by doing. Agreeing with the painter
this one is meant to match is worth more than picking again.

**That `min` is an interim default and issue #1281 says so.** The reference
painter pushes one anti-aliased clip per box and lets Skia's clip stack combine
them; `min` and a clip stack agree wherever at most one box covers a pixel
fractionally, and can differ where two clip edges cross one pixel. Nothing here
measures that case — `v03-clips` looks like it draws it and does not, because
every box in it is integer- and axis-aligned — so this painter is the **third**
implementation of a combination rule that has never been compared. #1281 exists
so the choice does not become the ruling by being the thing that shipped.

**The material class is a host setting, not a document property.** D1 names
three — unlit-overlay, lit-opaque, lit-cutout — and nothing on boundary B says
which node is lit. Inventing a per-node lighting flag would be discovering
vocabulary rather than validating it, which P4 forbids, so `MaterialClass` is
chosen once by whoever constructs the painter. The three differ in render state
as well as in shading: the overlay class blends and writes no depth, so coverage
is its alpha; the opaque class writes depth and does not blend, so it **cannot**
express partial coverage or partial alpha, and the painter refuses five things
rather than drawing them away — a corner radius, a clip, a stroke, a per-node
opacity below one, and a fill whose colour or any gradient stop is not fully
opaque; the cutout class discards below a threshold, so the silhouette survives
and the edge is hard.

## Where the package's shaders live, and why (issue #1313)

**Every `.shader` the package ships sits under `Runtime/Resources/Dashscene/`,
named by the shader's own `Shader "…"` name, and `BrgPainter` loads each with
`Resources.Load<Shader>`.** That is the whole of the layout rule, and a shader
added later goes there too.

**It is a fix, not a preference.** Unity strips a shader that no scene and no
material references out of a **player** build. The painter resolved its three
with `Shader.Find` until 2026-08-23, which finds a shader in an editor — where
nothing is stripped — and returns null in a player. Measured twice on
`6000.3.22f1`, macOS/Metal: a throwaway probe hit it first and worked around it
host-side by adding the three shaders to the host project's Always Included
Shaders list; `just unity-render` then reproduced it against the package as
installed, with the painter throwing

    the shader 'Dashscene/UnlitOverlay' was not found

A `Resources` folder is included in a build whether or not anything references
it, so the load succeeds with the host configuring nothing. URP's own core
package ships a runtime `Resources` folder inside the package and loads from it
the same way, so this is the ordinary arrangement rather than a novelty.

**The shader's name doubles as the load path**, so `PaintShaders.For` is the
only string involved: `Dashscene/UnlitOverlay` is what `Shader "…"` declares,
what `Resources.Load<Shader>` is handed, and where the file sits under
`Runtime/Resources/`. `unity/package-gate`'s
`every_shader_sits_where_resources_load_will_find_it` holds all three together,
in both directions — a shader a class names and that is not at its path, and a
shader at a path no class names.

**The `.hlsl` files stay in `Runtime/Shaders/`.** They are `#include`d at
compile time and never loaded at run time, so they need no `Resources` folder;
`just sdf-hlsl` still writes `Runtime/Shaders/Sdf.hlsl`. The three `.shader`
files reach them through the absolute
`Packages/com.driftsys.dashscene/Runtime/Shaders/…` include form, which is what
URP's own shaders use for their library.

**Issue #1313's four candidates were not four alternatives**, and this is the
correction the run produced. Candidate 3 — a material asset per class, shipped
in the package — is a _shape_, and it cannot stand alone: nothing loads a loose
asset out of a package at run time, so it needs candidate 1's `Resources` folder
or candidate 2's preloaded-asset reference to be reachable at all. With
candidate 1 in place, candidate 3 adds an asset per class and buys a material
whose render state a human can inspect; it is not needed to make the shader
reachable, and this design takes the shader route because it is the same
mechanism with one fewer asset kind. Candidate 4 — documenting the defect — was
never a fix. The diagnostic's own half of it is done: it named `.meta` files and
nothing else, which sent the first investigation the wrong way, and it now names
the path the shader ships at and the mechanism that loads it before it gets to
the two `.meta` causes. Those two causes are still what makes the file absent,
so they stay.

**No tree-derived check can catch this class**, which is why `unity/render-gate`
exists: `unity/package-gate`'s own comment said "a gate over the files alone
would pass while nothing drew", and it passed while nothing drew. Both sides of
every assertion there are read out of this repository, and stripping happens at
build time in someone else's project.

## The SDF math is generated, not ported (R-T5)

[../specification/03-target-hardware-rules.md](../specification/03-target-hardware-rules.md)
R-T5 asks for the SDF shader math to be single-sourced into both product
painters' shading languages. This painter is the first consumer of
`crates/dashscene-gpu/src/shaders/sdf.wgsl` that is not WGSL, so the mechanism
that carries it across is what makes R-T5 true rather than aspirational.

**It is compiled, by the compiler that already compiles it.**
`unity/package-gate` runs `naga` — wgpu's own shader translator, the one the
lean painter's WGSL goes through on its way to Metal, Vulkan and GLES — over
`sdf.wgsl` with its HLSL backend, and writes `Runtime/Shaders/Sdf.hlsl`.
`just sdf-hlsl` regenerates it; a test re-derives it on every run and fails if
the committed file is not byte-identical. So the HLSL a Unity shader includes is
not a port of the WGSL: it is the WGSL, compiled.

    sdf.wgsl ──naga(wgsl-in)──> module ──naga(hlsl-out, SM 5.0)──> Sdf.hlsl
        │                                                             │
        └──include_str!──> dashscene-gpu pipelines            #include ─┘
                           layer-2 compute harness         DashsceneInstance.hlsl

**naga's options are its defaults, with one departure.** `shader_model` is 5.0
rather than 5.1, because `#pragma target 4.5` is Unity's spelling of Shader
Model 5.0 and that is what R-E11 requires of the shaders that include this.
Nothing else is changed — `restrict_indexing` and `force_loop_bounding` stay on,
because they are on for the lean painter too, and turning either off here would
make the generated code _differ_ from what the same module compiles to for the
other painter.

**Two names differ, and both are naga's namer rather than a port.** `median3` is
emitted as `median3_` and `msdf_coverage`'s `sample` parameter as `sample_`.
They come from two different clauses of one condition in
`naga::proc::Namer::call` (`src/proc/namer.rs`), which appends an underscore
when the sanitized base **ends with a digit**, is in the backend's reserved set,
is in its case-insensitive reserved set, or is in its builtin set:

- `median3` ends with a digit. It is in none of naga's reserved lists —
  `grep -c '"median3"'` over `back/hlsl/keywords.rs` returns 0.
- `sample` is reserved; that same grep returns 2.

**The fourth clause is empty for HLSL**, which is why it does not appear above
as a third possibility: `back/hlsl/writer.rs`'s `reset` passes
`proc::KeywordSet::empty()` in the `builtin_identifiers` position. A backend
that populated it would rename more.

**And a suffix is not the only rewrite.** `Namer::sanitize` runs first and
changes the **front** of a name as well: it drops leading digits, keeps only
alphanumerics and `_`, collapses runs of `_` — so a legal WGSL `a__b` becomes
`a_b` — and rewrites a name beginning with a reserved prefix to `gen_<name>`.
Separately, a base already used gets a `_<n>` suffix rather than a bare
underscore.

**One of those four reserved prefixes is `naga_`**, plain, from
`back/hlsl/writer.rs:54`; the others are `naga_query_init_tracker_for_`,
`LoadedStorageValueFrom` and `__dynamic_buffer_offsets`. So a WGSL function
called `naga_anything` is emitted as `gen_naga_anything` — the only one of these
rules whose trigger is a string somebody might plausibly choose. None of them
fires on this library today. They are named so that a reader meeting a third
renamed symbol can tell which rule produced it instead of assuming this record's
two are the whole set — **re-derive against whatever naga version is in the
lockfile rather than trusting this paragraph.**

All of the above was read out of the pinned `naga-30.0.0` source. An earlier
revision of this paragraph, and of the generated file's own banner, attributed
both renames to a reserved word — false for one of them, and caught by issue
#828's lane rather than here.

A test asserts every one of the library's seventeen functions is present under
the name the HLSL uses — without it, two empty files would satisfy the byte
comparison.

**What is NOT single-sourced, and why.** The composition around the math — which
row an instance names, the order coverage multiplies in, the clip loop — is
hand-written in `DashsceneInstance.hlsl`, because `paint.wgsl` is a pipeline
with bindings, entry points and texture samples and none of that survives
translation into a Unity `.shader`. The line is exactly where
`docs/decisions/shader-library-and-layer-2.md` draws it: `sdf.wgsl` is float
arithmetic over its arguments with no texture sample, no derivative and no
uniform, which is what makes it both compute-testable and translatable. Adding a
function that computes a distance or a coverage ramp to `DashsceneInstance.hlsl`
is what R-T5 forbids; it goes in the WGSL and is regenerated.

## Text: the sheet crosses on its own call, and the six rules it obeys

Story #1123. `ds_runtime_acquire_frame` hands over the glyph runs and their
quads, and a quad is a glyph id and a pen position — from that alone neither the
quad's corners nor its texture coordinates can be computed. The other half
crosses through `ds_runtime_atlas_count` and `ds_runtime_atlas`, which
`DashsceneRuntime.ReadAtlases` wraps.

**Once per load, not once per frame.** An atlas set is installed by a load and
replaced only by another, which is why it is a call rather than three more
`DsSlice` members on `DsFrame` — that shape would say the set can change per
tick, and would change `DsFrame`'s layout besides.
[../decisions/the-glyph-atlas-crosses-the-c-abi-as-a-call.md](../decisions/the-glyph-atlas-crosses-the-c-abi-as-a-call.md)
carries all of it, including why the **sheet** crosses as well as the glyph
table: an atlas index is the typesetter's font slot, the cascade groups faces by
family — trimmed and ASCII-case-insensitive — before flattening, and a host
pairing sheets by its own array index samples another face's glyphs rather than
failing.

    DashsceneRuntime.LoadDocumentWithText(bytes, faces)   the cascade
    DashsceneRuntime.ReadAtlases()  -> TextAtlasSet       the sheets
    BrgPainter.SetAtlases(set)                            one texture and one
                                                          material per sheet

**Read the atlases and install them on the frame that reports
`DocumentReplaced`, before `Draw`** — which is what the two calls' own doc
comments describe, though neither spells out the ordering against `Draw`.
**`Samples~/FrameLoop` does not**: it loads without a font cascade and installs
no atlas set, so it demonstrates none of this (issue #1337). `Samples~/Showcase`
does both, on the entries its manifest marks as carrying text. `Draw` drops a
set that was installed for a previous document, and what tells the two apart is
whether `SetAtlases` has been called since the last `Draw`, not the flag alone:
the flag is raised by every load and cleared by the acquire that reports it, so
a painter keying on it alone would destroy the set the host had just minted.

`NativeText` declares its two imports in a private nested type and reaches them
through a forwarder each, calling `Native.SymbolMissing` — the shape `Native`'s
own documentation prescribes for a sibling file and `unity/ffi-check` requires
of every import in the package (story #1308).

**`LoadDocumentWithText` had to land with it.** The package wrapped only the
loaders that pass no cascade, so nothing on this side could produce a document
carrying glyph runs at all — the seam would have had no reachable input.

**The row split is per run and per glyph, and it is the lean painter's.** A
run's fill and its screen-pixel MSDF range are one value for every glyph it
places, so they are a row in `_DsGlyphs` that the instance's `_DsPaint.y`
indexes. Which texels one quad samples is per glyph, so it rides on that
instance's own `_DsCorners` — the member the other kinds spend on corner radii,
and which a glyph has no use for. `dashscene-gpu` spends `Instance::corners` the
same way on the same kind, and writes a **different value**: a top-left
rectangle, because wgpu's texture coordinates are top-left. Both reference
painters flip; Unity's coordinates and `atlas_px` are both bottom-left, so this
one does not, which is why rule 2 says copying either of them is wrong.

**`PaintHeap.GlyphWords` is 2 where the lean painter's row is three words**, and
the difference is stated rather than tidied: that row carries the atlas
payload's own rectangle inside a shared residency texture, and this painter
gives each sheet its own `Texture2D`, so the origin is structurally `(0, 0)` and
only the scale survives.

**Text is not a fourth `MaterialClass`.** A class decides how a node is drawn —
blended, opaque, or thresholded — and MSDF coverage is partial coverage by
construction, so a glyph cannot be drawn by the non-blending opaque class at
all. `Dashscene/Text` is therefore blended whichever class the painter draws its
nodes with, and `PaintShaders.For` does not answer it because no class selects
it. It carries **one material per atlas**, because a sheet is a texture and a
texture is a per-material binding.

**Two consequences are worth stating rather than discovering, and both are on
the two lit classes rather than on the overlay class a UI takes.**

`EmitRun` applies none of `PackRect`'s coverage refusals, because those are the
class's and text does not take a class. So under `MaterialClass.LitOpaque` a
text node whose anchor rect is refused — a corner radius, a clip, an opacity
below one — still has its glyphs drawn: the text appears and its background does
not, and the missing background is reported by name. That is the intended
reading of "text is always blended whichever class the nodes take", and it is
the one place a class's refusal applies to half a node.

And `Dashscene/Text` sits in the **transparent** queue, as the overlay class
does. Against the overlay class that changes nothing — both are in one queue and
one pass, so submission order decides the picture as it always did. Against
`LitOpaque` (geometry) or `LitCutout` (alpha test) it does: a render pipeline
sorts by queue before it sorts by anything a draw command carries, so every
glyph is submitted after every node fill whatever order the document put them
in. A later node that overlaps a text node covers its glyphs in both reference
painters and does not here. This is not fixed by the per-material command split,
which preserves order only inside one pass, and it is a property of drawing
blended text beside opaque geometry rather than of this painter.

**The culling callback emits one command per contiguous run of instances that
share a material**, which preserves the emission order that decides the picture
— and it is the first thing here that makes the command count depend on
**which** nodes a document holds rather than on how many, which a 256-instance
split already did.

### The six rules, and where each one is held

`#851` verified six mechanical rules for this seam on 2026-08-09, and each is
silent when broken — thin stems, upside-down glyphs, or text at the wrong
baseline is a plausible wrong picture rather than a failure. **Five of the six
are held by something that runs in CI. The exception is rule 1**, whose guard is
a run-time check inside `Runtime/Engine/`, which no CI job compiles and only a
device runs — so a mutation to it reaches hardware before it reaches a gate. The
list says which each one is rather than implying they are alike:

1. **The sheet is linear, bilinear, no mips.** `AtlasTexture.Decode` **reads the
   format back** and refuses an sRGB one, rather than trusting the
   `linear: true` it asked for — the same posture the painter takes to
   `BatchRendererGroup.BufferTarget`. **Nothing here runs it**: that file is
   `Runtime/Engine/`, so a mutation to `mipChain` or `filterMode` reaches a
   device before it reaches a gate.
2. **`atlas_px` is bottom-left and so is a Unity UV, so nothing is flipped.**
   `unity/ffi-check`'s geometry check asserts the bottom edge is the bottom edge
   and not `height - top`, which is `dashscene-skia`'s convention and would flip
   every glyph.
3. **`plane_em` is y-up from the baseline while document space is y-down.** The
   same check, on all four components of the quad.
4. **`px_range = distance_range_px * size / px_per_em`.** The same check,
   against the row the packer wrote.
5. **The resolve is `median3(sample) - 0.5` then
   `clamp(sd * px_range + 0.5, 0, 1)`, with `px_range` a uniform.** Not written
   here at all: `DashsceneInstance.hlsl` calls `msdf_coverage` out of the
   generated `Sdf.hlsl`, and `unity/package-gate` re-derives that file from the
   WGSL on every run of the sanity tier — which is in CI. **What is not held is
   the caller**: a text arm rewritten to take its range from `fwidth` would
   leave every gate green, and rule 5 exists because that form has a documented
   failure where `fwidth` returns zero and the division paints a hole.
6. **`GlyphRun::opacity` reaches the fill alpha.** The same check, on
   `_DsShade.x`, which `DsShade` multiplies into the coverage.

Two of those were **mutated** rather than reasoned about, on 2026-08-23 against
`just unity-ffi` on the branch that landed them: reverting the y-up flip fails
with `quad y: expected 36.40625, got 58.71875`, and adopting Skia's atlas flip
fails with `atlas bottom: expected 153.5, got 74.5`. Each fails on its own.

**What the sampling composition is, and why it is not generated.** R-T5
single-sources the _resolve_; `paint.wgsl`'s `msdf_sample` — the map from a
point in the glyph's quad to a texel in the sheet — has no generated twin,
because a texture sample's binding does not survive translation. `DsMsdfSample`
in `DashsceneInstance.hlsl` is that mapping rewritten, and the section above
draws the line it stays on: the mapping is composition, the resolve is
arithmetic, and only the second is generated.

## Decisions in the binding, each of which is a defect if reversed

**Every `[DllImport]` is private, and every caller goes through a same-named
forwarder that translates a missing entry point.** .NET binds an import lazily,
at the first call, so a package built after a symbol arrived and loaded against
a library from before passes the `ds_abi_version` handshake — adding a symbol
deliberately does not move `DS_ABI_VERSION` — and then fails at that call with
an `EntryPointNotFoundException`, which is neither a `DashsceneException` nor a
`DashsceneAbiMismatchException` and escapes every catch a host is told to write.
The forwarder turns it into `DashsceneSymbolMissingException`. Story #1124 wrote
that catch by hand for the one symbol it added; issue #1308 was the other
fourteen, which had the same exposure and no catch at all.

Three details are the design rather than the mechanism:

- **A forwarder per import, not one wrapper taking a delegate.** The issue asked
  for the second, and it allocates: a call passed as a closure costs a display
  class and a delegate, and `Tick`, `AcquireFrame` and the lease release are
  per-frame calls. R-T4 bounds a frame's CPU cost and this document tracks the
  allocation half of it, so a per-frame allocation to serve a first-call failure
  is the wrong trade. **The trade is an allocation against a call frame, not
  against nothing**: a `try` block entered and not thrown through allocates
  nothing and adds no work at run time, but a method carrying one may not be
  inlined, so each of these is a real call. Neither side of that is measured
  here, and the runtime that ships this package is not the one any gate here
  runs (issue #1322).
- **The symbol name comes from `[CallerMemberName]`**, so it is the forwarder's
  own name. A literal per forwarder is one copy-paste from naming a symbol that
  resolved perfectly well, and nothing downstream could tell.
- **`Actual` is read from the library, and a library that exports neither the
  symbol nor `ds_abi_version` gets no translation at all.** That one is not a
  build of this library, so there is no version to report and no disagreement to
  describe; its `EntryPointNotFoundException` travels beside the
  `DllNotFoundException` a host already handles, which is this package's shipped
  state.

`Dispose` is the one place the translation must not surface as a throw, and two
routes had to be closed for that to be true. A `ds_runtime_free` that never
reached the library is recorded on `LastDisposeDetail` with `LastDisposeStatus`
left at `Ok`, because no call answered a status — and `LastMessage`, the channel
`Dispose` asks for the text of a free the library **did** refuse, absorbs the
same failure rather than raising it. A diagnostic channel that throws replaces
the diagnosis: without that, a library refusing the free and exporting no
`ds_last_error_message` hands a host a symbol-missing exception where its
`DsStatus` should be, out of a method running during unwinding.

**`LastDisposeStatus` is not a "was it freed" flag**, and the lease is why: a
lease release that fails records its own status there and the free that follows
can still succeed. What holds in every case is that `Dispose` may be called
again — it returns at once when the runtime was freed and retries when it was
not.

**A host needs the R-E16 catch wherever it calls, not only around the
constructor**, and that is the cost of this shape rather than a detail.
`DashsceneAbiMismatchException` derives from `Exception`, so a
`catch (DashsceneException)` does not see it — and the translation now reaches
every entry point, so a tick and a frame acquire can raise it where only a load
could before. `Samples~/FrameLoop/` carries the catch in all three places —
around the constructor, around the load and around the frame loop. The third was
issue #1315, filed when that file belonged to another lane and fixed by that
lane's own pull request.

**Every `bool` on the surface binds as `byte`.** C's `bool` is one byte and
.NET's default marshalling for `bool` is the four-byte Win32 `BOOL`, so an
out-parameter left to the default writes three bytes past its target. Binding
them as `byte` also keeps every type blittable, so nothing on the surface needs
a `[MarshalAs]` and `DsFrame` crosses with no marshalling at all.

**`ds_runtime_release_frame`'s `drawn` binds as `int`, not `bool`.** The header
spends a paragraph on it: a `bool` crossing _into_ the library has two valid bit
patterns and any other is undefined behaviour where the arguments bind, before
anything in the library can turn it into a status. The `bool`s above are ones
the library writes through an out-pointer, which is the opposite direction and
not the same hazard.

**`DashsceneRuntime` is not a `SafeHandle` and has no finalizer.** A
`SafeHandle` releases on the GC's finalizer thread, and the runtime is
thread-affine — that `ds_runtime_free` answers `DS_WRONG_THREAD` and the runtime
leaks with nothing reported. A type that cannot be collected correctly should
not carry the machinery that claims it can, so `Dispose` is explicit and
documented as owning-thread-only.

**`Dispose` reports and does not throw, and clears the handle only when the free
succeeded.** It runs during unwinding, so a throw there replaces whatever fault
caused it — and the sample's own teardown runs inside a `catch` for the
`DsStatus.Panic` that makes a free most likely to fail. The status goes to
`LastDisposeStatus` with the library's text on `LastDisposeDetail`, and the
handle is left live so a later `Dispose` on the owning thread can retry; zeroing
it would make that retry hit the `_handle == 0` guard and do nothing, turning a
reported failure into an unrecoverable leak.

Every status the FREE answers means the runtime was not freed — `WrongThread`
from a foreign thread, `FrameLeased` from a release that did not complete,
`BadHandle` from a double dispose — and all of them are recorded. A status the
lease release answered is a different thing and reads the same, which is why the
property above is not a "was it freed" flag. An earlier form reported only
`WrongThread` and discarded the rest, and a later one threw, which contradicted
the reason written beside it. A failing lease release is caught rather than
propagated for the same reason, and its detail is kept alongside the free's
rather than overwritten, because it is the usual cause of a `FrameLeased` free.

**The stride table is derived from `frame_of`, not from the member names.** Two
of the nineteen arrays do not hold the type their name suggests: `extra_fills`
holds `PaintKind` and `shapes` holds `VectorField`. Most of the rest are what
they look like — `strokes` holds `Stroke`, `shadows` holds `Shadow`, `blurs`
holds `Blur` — with two carrying primitives rather than boundary-B rows, `dirty`
a `uint` and `image_payload` a `byte`. The seven `*Range` types are rows of no
array here at all: five are index ranges inside `PaintEntry`, and `StopRange`
and `GlyphRange` sit inside `Gradient` and `GlyphRun`. A table written from the
names would have compared two arrays against the wrong size and reported a
mismatch on a correct build.

**A stride mismatch releases the lease before it throws.** The acquire has
already succeeded at that point, so throwing straight out would leave the lease
held and refuse every later tick for the life of the runtime — turning a
diagnosable version mismatch into a runtime that never advances again.

## How the document's bytes reach memory

**Three managed entry points, and the third is why this section exists.**
`LoadDocument(byte[])` copies. `LoadDocumentMapped(string, uint)` maps a file
and costs the artboard rather than the file.
`LoadDocumentMapped(DocumentRange, uint)` maps a byte range **inside** a file.

**Only the third loads a `StreamingAssets` document on Android**, and that is a
statement about paths rather than about the others being unavailable. The byte
loader works there — a host reads the asset with `UnityWebRequest` and hands
over the bytes — and it is rejected on cost, not on capability:
[../decisions/the-document-is-mapped-where-it-is-packed.md](../decisions/the-document-is-mapped-where-it-is-packed.md)
costs it as giving up demand paging, which is what R5 asks for. The string
loader is the one that genuinely cannot: there is no path.

**`Application.streamingAssetsPath` is not a directory there.** It resolves to
`jar:file:///data/app/<pkg>/base.apk!/assets`, and the document is an
uncompressed entry inside that APK, so the whole-file mapped loader answers
`DsStatus.Map`. That was the state the frame-loop sample shipped in — issue
#1288 — with the sample's own default of `documentPath = "scene.dsb"`.

`DocumentRange` is the value that says which of the two shapes a document is in:
`WholeFile(path)`, which routes to the path loader, or
`Window(container,
offset, length)`, which routes to
`ds_runtime_load_document_mapped_range`. **They are separate C entry points
rather than one with a sentinel length**, so the routing is a branch on
`IsWholeFile` and not a magic zero — the library refuses a zero length with
`DsStatus.Map`.

**The split between `Runtime/` and the sample is drawn at what a gate can
execute.** `DocumentRange` and the loader over it are in `Runtime/`, where
`unity/ffi-check` builds a container holding the fixture at a deliberately
unaligned offset, loads it against the real library, and compares **all nineteen
slice counts** against the same document loaded from its own path. That
comparison is what proves the two `ulong` slots cross correctly; an earlier form
asserted only that the frame had rows, which any range that happens to parse
satisfies. The JNI query that asks the APK where the entry is needs
`UnityEngine`, so `StreamingAssetDocument` sits in `Runtime/Engine/`, the half
`just unity-editor` compiles.

**It was in `Samples~/FrameLoop` when story #1124 wrote it**, because
`Runtime/Engine/` did not exist and no gate here could compile a `UnityEngine`
reference at all. Story #1122's two-halves ruling landed in the same slice and
made that both unnecessary and wrong, so the resolver moved before #1124 merged
— a package whose Android path a customer has to copy out of a sample is not the
package this story set out to ship.

**The compile is a gate; the behaviour is not.** Whether `openFd` reports the
offset an APK actually holds is answered by a device, and the decision above
records the run that answered it.

**Two things the resolver does that are not obvious**, and both were measured
rather than assumed:

- **It reads the container path off `/proc/self/fd/<n>` rather than taking
  `Application.dataPath`.** The two were the same `base.apk` on the install
  measured, and they do not have to be: `AssetManager` serves an asset out of
  whichever APK holds it, which for a split install is not the base.
- **It closes the file descriptor before the load is issued**, and that is safe
  because no descriptor of the host's is ever the one mapped: the resolver hands
  back a path, and the library opens that path itself. That is also why the C
  ABI takes a path rather than a descriptor at all. `AndroidJavaObject.Dispose`
  is not what closes it — that releases the JNI global reference — so the
  resolver calls `close()` explicitly, in a `finally`, logging rather than
  throwing so a cleanup failure cannot discard a range already built.

**One build setting this depends on, and a stock Unity build already satisfies
it.** `AssetManager.openFd` refuses a compressed entry, and Unity's shipped
`mainTemplate.gradle` sets `noCompress` from `unityStreamingAssets`, so a
`StreamingAssets` file is `Stored`. Measured on a built APK: `assets/scene.dsb`
was `Stored` where every one of Unity's own `assets/` entries was `Defl:N`. A
custom gradle template that drops that list breaks the Android path, and the
resolver lets `openFd`'s exception through rather than falling back to a copy.

## What the gates see, and what none of them does

| gate                     | question                                                                                                                                                         | recipe                   | in CI  |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ | ------ |
| `unity/abi-check`        | do boundary B's C# types match the Rust ones?                                                                                                                    | `just unity-abi`         | yes    |
| `unity/package-compat`   | would Unity compile the engine-free half at netstandard2.1?                                                                                                      | `just unity-abi`         | yes    |
| `unity/ffi-check`        | do the P/Invoke declarations match the library?                                                                                                                  | `just unity-ffi`         | yes    |
| `unity/package-gate`     | is the HLSL the WGSL's? do the shaders carry the pragmas? is the R-E10 split intact? does the painter still take R-E5's read behind its guard and report rung 3? | `just test`              | yes    |
| `unity/editor-compat`    | does the WHOLE package compile, shaders included?                                                                                                                | `just unity-editor`      | **no** |
| `unity/hlsl-conformance` | does the generated HLSL evaluate to the committed probe table?                                                                                                   | `just unity-conformance` | **no** |
| `unity/render-gate`      | does the package DRAW, in a player, as a consumer installs it?                                                                                                   | `just unity-render`      | **no** |

`unity/editor-compat` and `unity/package-gate` are story #1122's,
`unity/hlsl-conformance` is issue #1312's and `unity/render-gate` is issue
#1298's. `unity/package-gate` is Rust and rides in the sanity test tier
deliberately: the .NET gates are outside `just check` because bootstrap installs
no SDK, and an editor is outside CI entirely, so it is the only check over the
package that runs on every pull request without a prerequisite.
`unity/editor-compat` is the only thing in this repository that compiles a Unity
`.shader` **without building a player**, and the only thing whose **purpose** is
to compile `Runtime/Engine/` — `unity/hlsl-conformance` imports the same package
into an editor and so compiles that assembly incidentally, which is why a
compile error there stops it too, and `unity/render-gate` compiles both as a
side effect of `BuildPipeline.BuildPlayer` and costs tens of minutes, so neither
is a substitute; it needs an editor install, which
[../decisions/the-native-library-ships-inside-the-unity-package.md](../decisions/the-native-library-ships-inside-the-unity-package.md)
D4 records no CI runner here can host. `unity/render-gate` needs one too, and
also builds and runs a player.

**`unity/editor-compat` compiles the variant rather than trusting the import.**
Unity builds a shader's variants lazily, so `ShaderUtil.GetShaderMessages` after
an import reports on whatever the editor happened to need — which does not
include `DOTS_INSTANCING_ON`, the one variant a BatchRendererGroup actually
draws with. The gate calls `ShaderData.Pass.CompileVariant` with that keyword
for Vulkan and GLES3x on Android and Metal on macOS, and refuses a `Success`
that produced no bytes, because the API reports success for a variant it
declined to build as well as for one it built.

`unity/ffi-check` is the one story #1121 added. Before it, nothing compiled a C#
P/Invoke against `crates/dashscene-ffi/include/dashscene.h` — issue #1266 item
2. It is not, however, the only gate that executes: `abi-check` declares sixty
`[DllImport]`s and round-trips structs by value through `dashpaint-abi`.
`package-compat` is the one that only compiles.

`just unity-ffi` reports the count it ran, and `unity/ffi-check/Program.cs` is
the list. **No enumeration of them lives here**: one did, it was wrong about
which statuses the gate produces, and it was falsified by an edit in the same
pass that wrote it.

**Two of them perform the mutation their requirement's own _Check_ asks for.**
R-E16 says "build a host against a mismatched value and assert it refuses" and
R-E17 says "mutate a row type's size and assert the host refuses rather than
drawing" — so the gate does exactly that, rather than a developer doing it once
by hand and the record claiming the gate does it. `CompareAbiVersion` exists as
the seam for the first, because `Native.AbiVersion` is a `const` the compiler
inlines and no reflection can move it; the second mutates `FrameLease.RowSizes`
in place, since the field is readonly and the array's contents are not.

**Every entry point is declared, including the four a Unity host never calls.**
`ds_runtime_attach_surface`, `ds_runtime_detach_surface`, `ds_runtime_resize`
and `ds_runtime_draw` belong to a host that hands dashscene a surface. They are
declared because an unbound symbol is an ungated one — but **.NET binds a
`DllImport` lazily, at the first call**, so declaring them gates nothing on its
own. The symbol-resolution check is what makes it real: it looks every
declaration up in the loaded library, so a rename fails now rather than in the
story that first calls one. A lookup proves the name and not the signature; the
behavioural checks prove the signatures of what they exercise.

**And the lazy bind is provoked rather than argued about.** `just unity-ffi`
compiles `unity/ffi-check/older-library.c` several ways and hands each build to
its own copy of the package assembly in its own `AssemblyLoadContext`. A
resolver is consulted once per library name per assembly and the module is
cached, and `SetDllImportResolver` throws on a second call for one assembly, so
the copy that already resolved the real library can never be shown a different
one; a second context is what can, and a second process would do as well and
costs more. Each reaches a different failure — a package newer than its library,
one reporting a version it was not built against, one exporting nothing at all,
one whose free refuses while the channel describing the refusal cannot bind, one
whose lease release fails while the free succeeds. **That file is where they are
enumerated**, and this record deliberately does not count them.

**None of them reads a shipped binary.** The ones that build a Rust half build
both halves from one tree, and the others read this repository's own sources —
so all of them observe only a disagreement it already contains, the older
libraries above included: those are built here too, from a C file in this tree.
A stale committed library is what `DsSlice::stride` catches at run time, which
is why R-E17 makes that check mandatory in the host rather than advisory.

**One of them draws, and it is the last row.** The rest compile, link or execute
on the CPU; `unity/hlsl-conformance` executes on a graphics device, but as a
compute dispatch over the shader library's arithmetic — there is no rasteriser,
no blend and no picture in it, which is what makes layer 2 meaningful on
whatever adapter a machine offers. `unity/render-gate` builds a player, runs it,
renders through `RenderPipeline.SubmitRenderRequest` into a `RenderTexture` and
reads that back.

**What it asserts is weaker than "the picture is right", and differs by frame
and by instance.** Every sampled centre is asked whether it differs from the
frame's own clear colour — "something drew here". Some centres are also asked
the stronger "THIS node drew": the centre must be nearer that node's own colour
than the clear colour, so a parent frame's fill behind it will not do.

**Three things exclude a centre from the stronger question, and the run prints
how many reached it.** The count matters because a change that quietly emptied
that set would print an otherwise identical report.

- **The material class.** Only `UnlitOverlay` puts the node's albedo on the
  pixel. `DsLit` multiplies the albedo by the light, which moves the pixel
  toward the clear colour while leaving both references where they are — so the
  two cutout frames are judged by the weak question alone. So the low-cutoff
  frame's "13 of 13" reads "something drew at each centre" and not "each node
  drew" — and the high-cutoff frame is required to be 0 of 13, which is the
  discriminator rather than a weaker pass.
- **The paint.** A gradient's colour at a point is the shading arithmetic issue
  #828's suite judges, and a translucent fill's centre is a composite. Neither
  is predictable here.
- **A later instance's quad reaching the centre**, taken over the quad rather
  than over its inked shape — so a solid whose centre merely falls inside a
  later node's box is excluded too. That is over-broad on purpose: narrowing it
  means evaluating each later instance's silhouette, which is the shading
  arithmetic this gate exists not to re-implement. The cost is a smaller
  stronger-form set, which the run prints.

## The demonstration, and what it is not

`just unity-demo` builds a windowed player from this package and runs it. It is
the fourth throwaway Unity project in this repository and a fourth copy of the
bring-up the other three carry, which issue #1316 factors out together rather
than any one of them doing it to the others.

**What it draws**: committed documents staged into `StreamingAssets` with a
manifest, switched on the arrow keys or on a `-cycle <seconds>` argument, with
the font cascade — `corpus/fonts/inter/Inter-Regular.otf` and the
`corpus/atlas/inter-ascii` sheet and metrics — beside them for the document that
carries text.

**Measured on 2026-08-24**, `6000.3.22f1`, macOS/Metal, Apple M3, in a player
built by `just unity-demo 6000.3.22f1 cycle`: `v03-paint.dsb` packed 16
instances on rung `RawBuffer` and reported its one image fill refused;
`v07-variant-topology.dsb` 2 instances; `v018-variant-shelf.dsb` 3; and
`v07-text-hug-in-fill.dsb` 16 with **no diagnostic at all**, which is the text
seam of story #1123 shading glyphs through the cascade in a player rather than
in an editor.

**The `cycle` action re-derives that** rather than leaving it to a person
watching: the player walks every entry once, quits, and the recipe fails unless
the log carries the line the sample writes when all of them have drawn. It is
bounded at 90 seconds, because a document that never draws and a player that
never exits look the same from outside — the foreground shape it replaced drew
the first document and then sat for one hour and fifty-four minutes.

**It is a demonstration and not a gate.** `cycle` asserts that every document
reached the painter and nothing more: there is no negative control, no ink
predicate, and a person still decides whether the picture is right.
`unity/render-gate` is what puts a frame through an ink predicate and fails on a
frame the painter did not draw. A green `unity-demo run` says a player built and
ran; a green `cycle` adds that every document reached the painter, and nothing
about what landed on the screen.

**Three things it deliberately does not do.**

- **It runs on a staged library.** The recipe copies the cdylib into the
  project, as `unity-render` does. The shipped form — `Runtime/Plugins/`, the
  `.meta` files and the tag R-E18 wants — is issue #1334, and nothing here
  stands in for it.
- **It does not draw the showcase scenes** the desktop, web and Android hosts
  draw. Those are built in Rust against a live arena and are emitted as no
  `.dsb`; issue #1342 carries what it would take, including the wall that is the
  painter's rather than the demo's.
- **It shows no motion a host has to drive.** The C ABI has no producer-side
  entry point, and signal binding is layer 1, which is `v1` for every host
  (issues #1261 and #1262). What ticks is the runtime's own time.

**One thing it did not find, and the correction is the point.** A first run of
this demo logged `the SRP Batcher is off` on a project that meets R-E5, and that
was written up here as issue #1317 reproducing in a player. It was measured
against `463febd5`, before the fix for #1317 landed — commit `937e539a`, merged
as `539096ed`, which moved the read into `ReportBatcherOnce` behind
`RenderPipelineManager.currentPipeline` and which `unity/package-gate`'s
`r_e5_is_latched_on_the_pipeline_instance_not_on_a_flag` pins. On a tree
carrying that fix the warning does not appear — measured rather than inferred:
the `cycle` run above logged no R-E5 line at all, on a project configured by the
same sequence that produced the original observation. The paragraph that said
otherwise would have sent the next reader to reopen a closed issue.

**And one coincidence worth keeping.** Its text document and the cascade beside
it are the ones `crates/dashscene-android/harness/build.sh` already stages, so
that harness and this demo draw the same file with the same fonts through two
different painters — the lean one on Android, the BRG one here. Track B of epic
#1107 wants exactly that comparison, and this is the shared content it can be
taken over.

## The `.meta` files, and how they were made

R-E2 requires a committed `.meta` beside every path Unity imports, because a
Git-URL package lands in `Library/PackageCache` immutable, where Unity ignores
an asset with no `.meta` rather than generating one. The package ships one per
imported path; `the_unity_package_meta_files_are_all_or_nothing` counts them so
this record does not have to.

**They were generated by an editor**, on `6000.3.22f1`, by importing the package
into a throwaway project as a `file:` dependency — a local package is mutable,
so Unity writes the `.meta` beside each asset. Hand-writing them would have
meant guessing an importer class per extension, and the guid is the load-bearing
part: it is what an asset reference resolves through, and nothing can mint one
later inside an immutable package.

**A script's `.meta` is two lines, and that is canonical rather than a truncated
write.** Unity emits `fileFormatVersion` and `guid` with no `MonoImporter` block
for a script in a package; two independent batchmode passes produced
byte-identical files, and 1119 of the 4805 `*.cs.meta` files in the editor's own
`BuiltInPackages` carry exactly those two keys and nothing else.

## Known gaps, named

- **Draw order is submission order, and nothing has confirmed that** — and since
  issue #1313's branch, `unity/render-gate` rests on it. A document is drawn
  back to front, so order is the property that decides the picture. The painter
  emits its draw commands in rect order inside one `BatchDrawRange` with
  `allDepthSorted` false, which is what should preserve it — but every quad in
  an overlay sits at the same depth, and whether URP's transparent pass re-sorts
  a BRG range at equal depth is a question only a drawn frame answers. If it
  does, overlapping nodes will be in an arbitrary order and the failure will
  look like a z-fighting artefact rather than like a painter bug.

  **A drawn frame has not answered it, and the gate does not test it.** What the
  gate does is exclude a node's centre from its stronger per-instance assertion
  when a HIGHER-indexed instance's quad reaches that centre, which is sound only
  under this assumption. Widening the exclusion to any other instance would
  remove the dependence and is not available: a filled parent frame reaches
  every child, so that predicate leaves the stronger form nothing to judge. The
  failure it would permit is bounded — a centre hidden under a LOWER-indexed
  node could pass the stronger assertion on ink that is not its own — and it can
  never produce a false failure. Settling it needs a frame drawn from a document
  built for the question, with two known overlapping fills and no parent under
  them.
- **The corner silhouette is checked by nothing.** `unity/render-gate` probes a
  point inside a node's box and outside its rounded corner, and skips any probe
  another instance's quad can reach — because a document is drawn back to front
  and a parent frame's fill sits under every child, so "outside this node's
  corner" is not "background". On `v03-paint.dsb` no probe survives that test,
  so the run says so and asserts nothing. A fixture with one isolated rounded
  node would change it; issue #828's suite is where that belongs.
- **No frustum culling.** `OnPerformCulling` ignores its `BatchCullingContext`
  and emits every instance for every camera. For a full-screen overlay that is
  the right answer and costs nothing; for a document placed in a 3D scene it is
  work per camera proportional to the whole document.
- **R-T4's dirty-range upload is not implemented.** The painter repacks every
  rect and re-uploads the whole instance buffer — including capacity past the
  live instances — on every frame, and `DsFrame.Dirty`'s rows are read by
  nothing (`FrameLease` reads its stride for R-E17; nothing reads the indices).
  The arrays are reused, so a steady frame allocates nothing; that is the half
  of R-T4 about allocation, not the half about transfer. Issue #1306, and issue
  #708 is the same gap in the lean painter.
- **Two code paths have never been exercised by any gate or any device**, and
  they are worth naming as a class rather than one at a time: the
  `ConstantBuffer` rung (this adapter reports `RawBuffer`, so
  `InstancesPerBatch` and `BatchStrideBytes` have never run) and the opaque
  material's alpha handling. A defect in either looks like a plausible picture
  rather than a failure — the window-size clamp in `BatchStrideBytes` was one,
  found by reading rather than by running. **The cutout material's `clip()`
  threshold was the third and is now measured**: `just unity-render` draws that
  class at 0.5 and at 2 — the second above any coverage a fragment can have —
  and got 13 of 13 sampled node centres inked at the first and none at the
  second, with 601144 of 786432 pixels differing. A `_DsCutoff` that did not
  reach the fragment stage would have drawn the same picture twice, whatever the
  stage read instead, so **it resolves**, on Metal, and issue #1307 is answered.
  GLES 3.2 and Vulkan are untested.
- **The painter draws, and what checks it is `unity/render-gate`.** Measured on
  `6000.3.22f1`, macOS/Metal, Apple M3, 2026-08-23, in a player built from this
  package: `goldens/dsb/v03-paint.dsb` packs to 16 instances on rung
  `RawBuffer`, and all 13 of them whose box centre the gate may assert on carry
  ink. **Two numbers, because two thresholds govern them.** The smallest
  distance any of the 13 kept from the frame's own clear colour was 0.514, on a
  scale where that threshold is 0.016; **5 of the 13** also cleared the stronger
  per-instance question, and the smallest amount by which one of those was
  nearer its own instance's colour than the clear colour was 0.599, where the
  threshold is zero. The other 8 are excluded by the paint rule or the
  later-quad rule — the class rule excludes nothing on an `UnlitOverlay` frame,
  which is the whole reason this is the frame the stronger form is asked on —
  and the run prints the split so that a change which quietly emptied that set
  would not read as an identical report. The three instances not asserted on at
  all are strokes, which ink a band around a box and not its middle. The one
  rect carrying an image fill is refused and reported, which is P4.

  **The corner probes were vacuous on this run**, and the gate says so rather
  than passing quietly: `0 of 0` reached the assertion, so the run states that
  it says nothing about whether corner radii are drawn round. Two guards can
  produce that zero — a radius outside the band a probe is meaningful in, and a
  point another instance's quad reaches — and the printed report does not
  distinguish them, so this run does not say which emptied it. That is the gap
  named below, measured rather than assumed.

  **What that is not** is an oracle. It says ink landed where the committed
  tables place a node, on one graphics API, over one document; whether the ink
  is the right colour is issue #828's portable conformance suite. And a
  screenshot is still not the substitute story #1122's text warns about — what
  makes this a check rather than a screenshot is that its predicate is evaluated
  on a frame the painter deliberately did not draw first, and the run fails if
  that frame passes.
- **Six paint constructs are not drawn, each by name.** Shadows, layer blurs,
  backdrop blurs, image fills, baked vector nodes and render-target groups.
  `PackDiagnostic` names each, the painter reports the set when it changes, and
  P4 is what makes reporting mandatory rather than optional. **Backdrop blur is
  the one that is not merely unbuilt**: it reads what the painter itself
  composited, and a Unity host's target also holds the engine's own scene, so
  frosted glass over Unity 3D is a host material effect outside boundary B
  whatever this painter does.
- **The SDF library's arithmetic is checked in this language on one backend, in
  an editor.** `just unity-conformance` evaluates every probe of
  `conformance/layer2-probes.json` through the generated `Sdf.hlsl` as a compute
  shader and compares against the recorded expectations — issue #1312, and it is
  what makes R-T5's single-sourcing a measured property here rather than a byte
  comparison over a generated file. What it has run on is Metal, on the
  developer's machine; neither GLES 3.2 nor Vulkan has evaluated a probe, no
  player build has, and no CI job runs the gate because it needs an editor.
  Issue #1314 carries all three. Two narrower gaps sit beside it: nothing holds
  the harness's own pinned probe counts against the table (issue #1323), and
  layer 2's properties are not ported alongside its table (issue #1324).
- **No glyph has been drawn, and the half that is checked is the half with no
  Unity type in it.** Story #1123 landed the seam: the atlas crosses, the packer
  turns runs into instances, and `unity/ffi-check` executes the geometry, the
  run heap and the atlas lookup on any pull request whose diff is not
  documentation-only. The material, the texture and the draw commands are
  `Runtime/Engine/`, which only a Unity editor compiles and only a device runs —
  so the sampling itself, the linear texture and the per-atlas draw-command
  split rest on reading rather than on running.
- **The `px_range` formula has two copies and nothing compares them.**
  `dashscene-gpu`'s `gpu_glyph_run` computes
  `distance_range_px * size /
  px_per_em` in Rust and `TextAtlas.PixelRange`
  computes it in C#. The same shape the heap row widths had before
  `unity/package-gate` held those together; issue #828's portable conformance
  suite is where the comparison belongs.
- **The C ABI has no `samples` channel.** `dashpaint::Painter::samples` is how a
  painter declares which image formats it can be handed, and the default —
  `format.is_encoded()` — is what makes a silent painter pay a decode. A Unity
  host sits behind `ds_runtime_acquire_frame` rather than behind the Rust trait,
  so it cannot declare anything: `crates/dashscene-ffi` holds a `GpuPainter` and
  that painter's declaration is the one in force. It costs nothing today because
  this painter uploads no image FILL at all — story #1123's atlas is a sheet the
  host decodes itself with `ImageConversion.LoadImage`, which asks the library
  nothing about formats — and it is the first thing an image-fill story meets.
  Issue #1292 carries it.
- **Every forwarder is driven against a library that does not export it**, and
  watched reporting its own symbol — so one that rethrows untranslated, or
  catches and swallows, fails whichever entry point it belongs to, including the
  five no managed code calls. `ds_abi_version`'s is the one whose absence is
  handed back rather than translated, because translating needs a version read
  from the library and it IS the read; that hand-back is driven too, and it is
  asserted to name the symbol that failed rather than the version read. Which
  library stages which absence is `unity/ffi-check/older-library.c`. Measured on
  pull request #1319: before this drive existed, a bare rethrow in
  `ds_runtime_draw`'s forwarder, a swallow in `ds_runtime_attach_surface`'s and
  an import called outside its own `try` in `ds_runtime_resize`'s each left the
  gate green, and each turns it red now.
- **The translation is verified on CoreCLR and on no Unity runtime.**
  `unity/ffi-check` runs under the .NET SDK, and `just unity-editor` loads no
  native library — so nothing here observes what Mono or IL2CPP does when a
  symbol is absent. If IL2CPP raises a different type, or resolves at load
  rather than at the first call, every forwarder's catch is dead code on the
  target that ships. It cannot be closed while the package ships no native
  library; issue #1322 carries it.
- **No native library and no release.** R-E3, R-E18 and R-E21 stay unmet, and
  they are about shipping rather than about this directory: `just host-lib`
  builds the cdylib and nothing places it into the package. Committing it was
  considered and deferred — it is about 9.6 MB of undeltifiable binary in a
  public repository's permanent history for a package that cannot yet draw.
- **The two mapped loaders still pass `(null, 0)` for their cascade.** Story
  #1123 wrapped `ds_runtime_load_document_with_text`, so the byte-taking path
  can load a document with fonts and sheets — and it is driven against a library
  that lacks it, like the other five. The mapped pair cannot take a cascade at
  all, which is the path a shipped document takes: a host that wants both a
  bounded load and text has neither call.
- **`ReleaseLease` clearing its managed handle only after the library has
  released is pinned by nothing**, and `Dispose` is pinned for both of its
  failing frees. Both need `ds_runtime_release_frame` or `ds_runtime_free` to
  fail with a live handle, which the older libraries now supply: one where the
  free never reaches the library, one where it answers a refusal while the
  channel describing it cannot bind, and one where the lease release answers a
  failure and the free that follows succeeds. What stays out of reach is
  `ReleaseLease`'s ordering itself and `DsStatus.WrongThread`, which IS
  reachable by disposing from a second thread — so a check can be written and
  has not been. An earlier draft of this bullet said it could not, which was
  wrong. Mutating the release ordering back leaves the gate green, measured
  rather than assumed. Issue #1289 carries the threaded harness it needs.
- **No CI job compiles `Samples~/`, and two developer gates do** —
  `just unity-editor`, whose purpose it is, and `just unity-demo`, which
  compiles the Showcase sample while building its player. Not because of the
  `~`, which only hides it from Unity's importer: `package-compat` and
  `ffi-check` glob `Runtime/**/*.cs`, so anything outside `Runtime/` is out of
  scope wherever it sits, and no CI job runs an editor. That is why
  `CommitPacer` sits in `Runtime/` rather than in the sample: the pacing
  arithmetic carries a numeric claim, so it lives where a gate can reach it.
  Story #1124's Android resolver briefly carried one too, and moved to
  `Runtime/Engine/` once the two-halves ruling gave engine-referencing code a
  gate.

  **The sample stopped being claim-free at issue #1298**, which put the
  painter's construction, the `Draw`-then-`MarkDrawn` ordering inside the
  lease's `using`, and the dispose order into it. Until then a syntax error in
  the file would have survived every gate this repository has — measured. So
  `just unity-editor` now copies it into its throwaway project's `Assets/` and
  asserts it compiled. That is a compile and nothing more: the ordering it
  carries is still pinned by nothing, because the render gate drives its own
  component rather than this one.
- **The thread-affinity question is narrowed, not closed.** Story #1125 measured
  `OnPerformCulling` on the main thread under `6000.3.22f1` with URP on macOS
  and Metal, so a host can bracket its job dispatch — but the target is Android,
  where no reading has been taken. Issue #1267 question 2, whether
  `DS_WRONG_THREAD` should distinguish a dead thread from a foreign one, is
  untouched and remains an owner's ruling.
