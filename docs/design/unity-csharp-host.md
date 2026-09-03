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

**The paint heap is bound per material, so two painters in one process no longer
share one** (issue #1297). `_DsPaints`, `_DsClipBoxes`, `_DsStrokes`,
`_DsGlyphs` and the `_DsGlobals` scalars were bound with
`Shader.SetGlobalBuffer` and `Shader.SetGlobalVector` until 2026-08-29, both of
which are process-wide: the last painter to draw supplied the gradients, strokes
and clip boxes every painter's fragments shaded from, and the painter reported
that with a constructor warning rather than drawing a wrong picture quietly.
`BindHeap` now sets them on the materials this painter registered itself — the
three heap tables and the scalars on the class material and on every glyph
atlas's material, and `_DsGlyphs` on the atlas materials alone, because the
shading declares it under `DASHSCENE_CLASS_TEXT` and no other class can reach a
glyph run. So **a second painter in one process is a supported configuration**,
and the live-painter counter that warned about one is gone. It runs on every
frame rather than once, for two reasons: a heap buffer is reallocated when its
table outgrows it, and `SetAtlases` mints text materials long after the
constructor, so no earlier moment holds the whole set.

**`_DsGlobals` had to move into `CBUFFER_START(UnityPerMaterial)` and into all
four `Properties` blocks, and the second half of that is measured rather than
reasoned.** A uniform declared outside every CBUFFER lands in `$Globals`, which
is one namespace for the process — so the scalars could not be per material
while they sat there. Moving them into the material's constant buffer alone
produced a blank frame — ink at 0 of 13 sampled node centres — and a player log
in which every draw command was refused for the reason
`UnityPerMaterial var is not declared in shader property section`.
`Runtime/Shaders/DashsceneInstance.hlsl` carries the run. So the rule the
shading states in one direction — a `Properties` entry must appear in
`UnityPerMaterial` — holds in the other for any member a pass reads. `_DsCutoff`
sat in the buffer for all four shaders — three material classes plus
`Dashscene/Text`, which is deliberately not a class — and in one `Properties`
block, and the three that omitted it drew all the same. The explanation those
two runs support is that a uniform no pass statement reads does not survive that
pass's compile, and neither run measured it, so the rule is now held over every
member rather than over the one that was measured: `unity/package-gate`'s
`every_per_material_member_is_declared_by_every_shader` requires each
`UnityPerMaterial` member in every including shader's `Properties` block, and
all four shaders declare `_DsCutoff` and `_DsGlobals` alike.

**A drawn frame is what says the binding reaches the fragment stage, which is
why this could not be fixed when it was filed.** Issue #1297 named its own
blocker — a harness that draws one document and compares it to something — and
`just unity-render` is that harness. On the fixed tree it returns the numbers
the global binding returned: ink at 13 of 13 sampled node centres, five of them
judged against the instance's own packed colour, smallest distance from the
clear colour 0.514, smallest colour advantage 0.599, and 601144 of 786432 pixels
differing between the two cutout thresholds. An unbound `StructuredBuffer` reads
zeros, so those five centres are what says `Material.SetBuffer` reached the
stage. **The same run cannot say it for `_DsGlobals`**, whose `Properties`
default is `(1, 0, 0, 0)` while the solid base really is 0 — so that value was
poisoned instead: written one row high through the same `Material.SetVector`
call, the frame fell to 11 of 13 centres, 0.420, and a colour advantage of
-0.109. Unity 6000.3.23f1, macOS/Metal, Apple M3, 2026-08-29, in a player built
from this package. **One graphics API**, as every other measurement here is.

**No frame has drawn two painters, and the supported-configuration claim rests
on the binding rather than on a picture.** What `just unity-render` measures is
that a painter's own materials carry its own heap; that two painters therefore
do not collide follows from each painter minting its own `Material` in its own
constructor, which is read rather than drawn. The gate keeps exactly one painter
alive by disposing before constructing, so nothing in this repository draws two
documents at once. Settling it needs a harness that builds two painters over two
documents and judges each frame against its own packed colours — the same shape
issue #828's suite has, and the same shape the corner-silhouette gap below
needs.

**The text materials take the same binding in the same loop, and no gate has
drawn one.** `just unity-render` draws a document with no glyph runs, so
`_DsGlyphs` and the three tables on a text material are held by reading rather
than by a picture. A frame HAS now drawn one — issue #1389 drew glyphs from
three atlases in a macOS/Metal player build on 2026-08-31 — but that was a
measurement taken by hand, not a gate, and R-E22 is the requirement that would
make it one. That is a gap the change opened as well as one it inherited: while
the heap was global a text material needed no binding of its own, and now it
needs the loop in `BindHeap` to reach it.

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
library, a property of the shader rather than of the adapter. The painter meets
it by emitting a batch as several draw commands rather than by asserting the
bound, because asserting it would refuse documents it can draw — and it now
emits one instance per command (issue #1401), so no document can approach the
bound at all.

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
does. Against the overlay class that changes nothing about the QUEUE — both sit
in one queue and one pass — though within that pass submission order does not
decide the picture either, which is issue #1389 and the paragraph below. Against
`LitOpaque` (geometry) or `LitCutout` (alpha test) it does: a render pipeline
sorts by queue before it sorts by anything a draw command carries, so every
glyph is submitted after every node fill whatever order the document put them
in. A later node that overlaps a text node covers its glyphs in both reference
painters and does not here. This is not fixed by the per-material command split,
which preserves order only inside one pass, and it is a property of drawing
blended text beside opaque geometry rather than of this painter.

**The culling callback emits one draw command per visible instance**, each
carrying `BatchDrawCommandFlags.HasSortingPosition` and one sorting key, so the
command count is the instance count and R-E20's 256 is met without splitting
anything. Until issue #1401 it emitted one command per contiguous run of
instances sharing a material, split at 256: Unity's sorted-transparent path was
measured dropping a contiguous subset of commands for single frames under that
shape, rendering the affected region as bare backdrop with nothing logged.
[../decisions/brg-draw-command-order-is-not-guaranteed.md](../decisions/brg-draw-command-order-is-not-guaranteed.md)
D5 is the constraint and
[../technotes/batch-renderer-group.md](../technotes/batch-renderer-group.md) §5d
carries the tables. **The emission order is still not the order the picture is
drawn in**: BRG groups the commands by material without the keys, which is what
issue #1389 found, and whether the keys reproduce the emission order was
measured unsettled under the old shape and has not been re-measured under this
one.

**What that shape costs against R-T4, and what the figure does not cover.** On
the showcase typography scene the command count rose from 11 to 381 per view,
one per instance. `Samples~/Showcase/DashsceneFrameCost.cs` reported these two
lines on the 20,000-frame soaks of 2026-09-03 — macOS/Metal, Apple M3, Unity
6000.3.23f1, the last reporting window of each run, wrapped here and one line
each in the log:

    [showcase] frame cost — scene typography at 3024x1832 over 240 frames —
    tick 0.07 ms, draw mean 0.19 p50 0.16 p95 0.41 max 1.02 ms      686ef1f

    [showcase] frame cost — scene typography at 3024x1832 over 240 frames —
    tick 0.09 ms, draw mean 0.19 p50 0.15 p95 0.42 max 1.15 ms      23dd62d

A command count 34.6 times higher left the mean and the median where they were
and moved the maximum by 0.13 ms, which is well inside the spread one run shows
against itself — the before run's last two windows report a maximum of 2.49 ms
and then 1.02 ms.

**What that pair does not measure is the loop that grew.** The figure covers the
frame lease, `BrgPainter.Draw` and the release; Unity runs `OnPerformCulling`
after `Update` returns, so the emission loop is outside it, and so is the GPU's
execution of 381 draw commands rather than 11. What the pair bounds is the
packing and the upload, which the command shape does not change at all. The
emission loop's own cost, and the GPU's, are unmeasured.

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

| gate                     | question                                                                                                                                                                                                                                                                                                                                                                                                                                      | recipe                   | in CI  |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ | ------ |
| `unity/abi-check`        | do boundary B's C# types match the Rust ones?                                                                                                                                                                                                                                                                                                                                                                                                 | `just unity-abi`         | yes    |
| `unity/package-compat`   | would Unity compile the engine-free half at netstandard2.1?                                                                                                                                                                                                                                                                                                                                                                                   | `just unity-abi`         | yes    |
| `unity/ffi-check`        | do the P/Invoke declarations match the library?                                                                                                                                                                                                                                                                                                                                                                                               | `just unity-ffi`         | yes    |
| `unity/package-gate`     | is the HLSL the WGSL's? do the shaders carry the pragmas? is the R-E10 split intact? does the painter still take R-E5's read behind its guard and report rung 3? does each shipped library's `.meta` and header match D3's row, per R-E21? and does the prose over the editor-only gates still match the code in them — R-E7/E8/E9's recorded status, `unity-android-negative`'s mutation table, and what `just unity-editor` is said to ask? | `just test`              | yes    |
| `unity/editor-compat`    | does the WHOLE package compile, shaders included? and what does this runtime do with a `[DllImport]` naming a symbol no library exports?                                                                                                                                                                                                                                                                                                      | `just unity-editor`      | **no** |
| `unity/hlsl-conformance` | does the generated HLSL evaluate to the committed probe table?                                                                                                                                                                                                                                                                                                                                                                                | `just unity-conformance` | **no** |
| `unity/render-gate`      | does the package DRAW, in a player, as a consumer installs it?                                                                                                                                                                                                                                                                                                                                                                                | `just unity-render`      | **no** |
| `unity/android-probe`    | on a device: which rung does `BufferTarget` select, does the APK carry only arm64-v8a with the shipped `.so` inside it, and does that library load?                                                                                                                                                                                                                                                                                           | `just unity-android`     | **no** |

`unity/editor-compat` and `unity/package-gate` are story #1122's,
`unity/hlsl-conformance` is issue #1312's and `unity/render-gate` is issue
#1298's. Since issue #1297 `unity/package-gate` also holds the paint heap's
binding — that no `SetGlobal…` setter survives in the compiled half, that each
name reaches a setter through a static property id, that the bindings sit inside
`Draw`'s own call to `BindHeap` and after the upload that can replace the
buffers, and that every `UnityPerMaterial` member is declared in every shipped
shader's `Properties` block. `unity/package-gate` is Rust and rides in the
sanity test tier deliberately: the .NET gates are outside `just check` because
bootstrap installs no SDK, and an editor is outside CI entirely, so it is the
only check over the package that runs on every pull request without a
prerequisite. `unity/editor-compat` is the only thing in this repository that
compiles a Unity `.shader` **without building a player**, and the only thing
whose **purpose** is to compile `Runtime/Engine/` — `unity/hlsl-conformance`
imports the same package into an editor and so compiles that assembly
incidentally, which is why a compile error there stops it too, and
`unity/render-gate` compiles both as a side effect of
`BuildPipeline.BuildPlayer` and costs tens of minutes, so neither is a
substitute; it needs an editor install, which
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

**Since story #1342 it runs the whole program twice.** The second pass sets
`-p:DemoProducer=true` and points at `unity/demo-producer`, and it is the only
thing that compiles or binds `Runtime/DemoProducer.cs`'s P/Invokes — they sit
behind a `#if` the shipped pass never defines, so without it they would be read
by no gate at all, which is issue #1308's class. That pass drives the seven
`ds_demo_*` through the missing-symbol context alongside the shipped set and
adds checks over the producer itself.

**The pass refuses to run vacuously.** Misspell the MSBuild property or the
symbol and the program compiles with every demo block removed, runs the shipped
checks a second time and exits 0 — measured during the review of PR #1365, which
reported "49 checks passed" against a comparison that never happened. The recipe
now sets `DASHSCENE_FFI_EXPECT_DEMO=1` on that pass and the program refuses when
the two disagree, in both directions.

**`unity/package-compat` gained a second pass for the same file**, because that
project is the one that asks the netstandard2.1 question. `unity/ffi-check`
compiles `DemoProducer.cs`'s real body at net10.0, a superset of what Unity
accepts; before story #1342 the only thing that compiled it the way Unity will
was `just unity-demo`'s player build.

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

**Two of them read a shipped binary, both since story #1334.**
`unity/render-gate` was de-staged, so the player it builds loads the library the
package ships and runs the version handshake and the per-array stride comparison
against it; and `package-gate` reads each shipped library's header far enough to
compare its architecture and container against D3. The ones that build a Rust
half still build both halves from one tree, and the others read this
repository's own sources — so those observe only a disagreement it already
contains, the older libraries above included: those are built here too, from a C
file in this tree.

**What none of them does is compare a shipped binary against the sources of the
commit that carries it.** An architecture match is not a freshness check. A
stale committed library is what `DsSlice::stride` catches at run time, which is
why R-E17 makes that check mandatory in the host rather than advisory.

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

**What it draws**, in one list the arrow keys walk: the three `corpus/showcase`
scenes first, then committed documents staged into `StreamingAssets` with a
manifest, with the font cascade — `corpus/fonts/inter/Inter-Regular.otf` and the
`corpus/atlas/inter-ascii` sheet and metrics — beside them for the document that
carries text. A `-cycle <seconds>` argument advances without a key press.

**The scenes arrive through a native producer** (story #1342). They are Rust,
built into a live arena by `dashlang`, and their motion is host-driven — so a C#
host cannot animate them while layers 1 and 2 are `v1`, and re-authoring them in
C# would be a second definition that drifts from the one `demo-android` draws.
`unity/demo-producer` is `dashscene-ffi` linked as an rlib plus six `ds_demo_*`
entry points; `just unity-demo` builds and stages it in place of the shipped
library and defines `DASHSCENE_DEMO_PRODUCER` for the player build.
`docs/decisions/the-demo-producer-links-the-abi-rather-than-shipping-in-it.md`
carries why it is a separate crate rather than a feature of the shipped one, and
`just demo-exports` is what holds it to being the shipped seventeen plus a set
carrying only the `ds_demo_` prefix. **That recipe pins the prefix, not the
cardinality**: `unity/ffi-check`'s demonstration pass is what names the seven
and drives each one.

**The camera is framed per entry class, and it was not at first.** A scene is
built for the drawable in physical pixels, as the three Rust hosts build it, so
its extent is the window's; a committed document carries a fixed extent of a few
hundred units, and `unity/demo/DemoBuild.cs` picks a size that frames those. The
sample framed only the second, so a scene drew at the ratio between the two —
**measured at 2.25x on a 2994x1802 drawable against a camera showing 800
units**, and found by a person opening the window rather than by any gate.
`cycle` asserts that every entry reached the painter and says in its own output
that it asserts nothing about pixels, so the whole branch passed with the scenes
at that size. A scene is now framed at one document unit per pixel, from the
size it was built at rather than from `Screen`, and the drawable is printed
beside the census so the two numbers exist somewhere.

**The scenes have motion; the documents do not**, and the difference is the
boundary rather than the sample. A scene's scripted pulse runs on
`demo/src/shell.rs`'s own 2500 ms cadence and its variant switch is on the space
bar, both through `ds_demo_*`. A document has neither, because no **shipped**
entry point mutates a document — that is layer 1, `v1` for every host (issues
#1261 and #1262).

**What a viewer will see missing from `surfaces`, and why that is P4 working.**
`PackDiagnostic` names six refusals, five of them paint kinds both Rust painters
draw (the sixth, a layer blur, both of them skip as well), so the image fill,
the baked vector field and the shadows do not arrive; the backdrop blur is
outside boundary B for a Unity host whatever this painter gains
(`a-backdrop-blur-snapshots-the-target-it-draws-into.md` D3). The readout names
what was refused for whatever is showing, so the difference from `demo-web` is
visible in the demonstration instead of surprising. Issue #1344 is the painter
request.

**Measured on 2026-08-24**, `6000.3.22f1`, macOS/Metal, Apple M3, in a player
built by `just unity-demo 6000.3.22f1 cycle`. **It covers the document half
only** — the scenes were added on 2026-08-26 by story #1342 and this run
predates them, so it is four entries of what is now a longer list rather than
the whole of it: `v03-paint.dsb` packed 16 instances on rung `RawBuffer` and
reported its one image fill refused; `v07-variant-topology.dsb` 2 instances;
`v018-variant-shelf.dsb` 3; and `v07-text-hug-in-fill.dsb` 16 with **no
diagnostic at all**, which is the text seam of story #1123 shading glyphs
through the cascade in a player rather than in an editor.

**Measured on 2026-08-26**, same editor, machine and API, in a player built by
`just unity-demo 6000.3.22f1 cycle` — the whole list this time:

    scene surfaces      56 instances, rung RawBuffer, five kinds refused
                        over 9 rects: shadows, backdrop blurs, image fills,
                        baked vector nodes and render-target groups
    scene typography   381 instances, no diagnostic
    scene layout        29 instances, no diagnostic
    v03-paint.dsb       16 instances, its one image fill refused
    v07-variant-…       2 instances
    v018-variant-shelf   3 instances
    v07-text-hug-…      16 instances, no diagnostic

`typography`'s 381 instances are the seam this producer exists to cross: the
scene's own solver carries a typesetter and its atlases, so the glyphs shade
here without the document-side cascade the last entry in the table above needs.
`surfaces`'s five refusals are P4 working and are what issue #1344 is for.

**The `cycle` action re-derives that** rather than leaving it to a person
watching: the player walks every entry once, quits, and the recipe fails unless
the log carries the line the sample writes when all of them have drawn. It is
bounded in two stages — up to 90 s for the player's own census line, then three
seconds per entry plus thirty — because an entry that never draws and a player
that never exits look the same from outside — the foreground shape it replaced
drew the first document and then sat for one hour and fifty-four minutes.

**It is a demonstration and not a gate.** `cycle` asserts that every entry
reached the painter and nothing more: there is no negative control, no ink
predicate, and a person still decides whether the picture is right.
`unity/render-gate` is what puts a frame through an ink predicate and fails on a
frame the painter did not draw. A green `unity-demo run` says a player built and
ran; a green `cycle` adds that every entry reached the painter, and nothing
about what landed on the screen.

**One thing it deliberately does not do, and two properties worth naming.**

- **It runs on a staged library.** The recipe copies the cdylib into the
  project, as `unity-render` does. The shipped form — `Runtime/Plugins/`, the
  `.meta` files and the tag R-E18 wants — is issue #1334, and nothing here
  stands in for it.
- **It draws the scenes through a library a customer does not install.** The
  producer's six entry points are not on the shipped ABI and are not proposed
  for it: when layers 1 and 2 land, the demonstration moves to C# and
  `unity/demo-producer` stops existing. Until then a C# demonstration would
  advertise a capability the product does not ship.
- **Its motion is the scenes' own script, not a host's.** The scene declares the
  signal and owns the variant switch; this host binds a key to them and
  constructs nothing, which is the seam `showcase::Showcase` exists to give it.
  A document in the same list still shows only the runtime's own time.

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

- **Draw order is NOT submission order — a drawn frame has now answered it, and
  the answer is no** — and since issue #1313's branch, `unity/render-gate` rests
  on the assumption that it is. A document is drawn back to front, so order is
  the property that decides the picture. The painter emits its draw commands in
  rect order inside one `BatchDrawRange` with `allDepthSorted` false, which is
  what should preserve it. It does not: `BatchRendererGroup` groups commands by
  material first, and under that grouping the painter drew every surface and no
  glyph on every platform (issue #1389). The failure did not look like
  z-fighting; it looked like text that was never drawn.

  The painter now sets `HasSortingPosition` and writes a sorting key per
  command, which is what makes glyphs reach the screen. **It is not established
  that those keys impose the painter's order**, and
  `docs/decisions/brg-draw-command-order-is-not-guaranteed.md` records why, with
  the measurements in `docs/technotes/batch-renderer-group.md` §5b. So the
  assumption below is still unmet, and is now known to be unmet rather than
  merely unconfirmed.

  **The gate does not test it.** What the gate does is exclude a node's centre
  from its stronger per-instance assertion when a HIGHER-indexed instance's quad
  reaches that centre, which is sound only under this assumption. Widening the
  exclusion to any other instance would remove the dependence and is not
  available: a filled parent frame reaches every child, so that predicate leaves
  the stronger form nothing to judge. The failure it would permit is bounded — a
  centre hidden under a LOWER-indexed node could pass the stronger assertion on
  ink that is not its own — and it can never produce a false failure. Settling
  it needs a frame drawn from a document built for the question, with two known
  overlapping fills and no parent under them.
- **The corner silhouette is checked by nothing.** `unity/render-gate` probes a
  point inside a node's box and outside its rounded corner, and skips any probe
  another instance's quad can reach — because a document is drawn back to front
  and a parent frame's fill sits under every child, so "outside this node's
  corner" is not "background". On `v03-paint.dsb` no probe survives that test,
  so the run says so and asserts nothing. A fixture with one isolated rounded
  node would change it; issue #828's suite is where that belongs.
- **No frustum culling.** `OnPerformCulling` reads one field of its
  `BatchCullingContext` — `lodParameters.cameraPosition`, which issue #1389's
  sorting keys are built from, so the command stream is camera-dependent — and
  uses none of the culling planes. It emits every instance for every camera. For
  a full-screen overlay that is the right answer and costs nothing; for a
  document placed in a 3D scene it is work per camera proportional to the whole
  document.
- **R-T4's dirty-range upload is not implemented, and this is what the full
  repack costs.** The painter repacks every rect and re-uploads the whole
  instance buffer — including capacity past the live instances — on every frame,
  and `DsFrame.Dirty`'s rows are read by nothing (`FrameLease` reads its stride
  for R-E17; nothing reads the indices). For the document `just unity-render`
  draws, `goldens/dsb/v03-paint.dsb`, every commit walks all fourteen of its
  rect entries, rebuilds all four heap tables and re-uploads their live rows,
  and sends one instance batch. **On the `RawBuffer` rung** — the only rung any
  device has ever reported here — that batch is the 64-slot floor
  `InstancesPerBatch` chooses: the shared head plus sixty-four eighty-byte
  slots, 112 + 5120 = **5232 bytes**, of which 112 + 16 × 80 = **1392** carry
  the sixteen instances the frame's draw commands read. All of it goes up on a
  commit whose dirty set is empty, and none of it is derived from that set. The
  `ConstantBuffer` rung sizes its batch from the device's own window instead, so
  the same document costs more there: a 16 KB window with 256-byte alignment
  fits `(16384 - 112) / 80` = 203 slots, and `BatchStrideBytes` rounds
  `112 + 203 x 80` = 16352 **up** to the alignment — so the stride the buffer is
  sized from, and the upload sends, is **16384 bytes**. That arithmetic has run
  on no device, which is why the figure to quote is the `RawBuffer` one.

  **The fourteen is two tests, not one.**
  `crates/dashc/tests/figma_lowering.rs`'s
  `the_fixture_compiles_loads_and_renders` asserts fourteen rects over the scene
  compiled from `corpus/figma-fixtures/v03-paint.json`, and
  `the_fixture_emits_the_golden_dsb` in the same file asserts that compiling
  that fixture reproduces `goldens/dsb/v03-paint.dsb` byte for byte. Neither
  alone says the file the painter loads holds fourteen rects. The arrays are
  reused, so a steady frame allocates nothing; that is the half of R-T4 about
  allocation, not the half about transfer. Issue #1306, and issue #708 is the
  same gap in the lean painter, where the design serving both belongs — packing
  only the changed rects needs the previous commit's tables held for comparison,
  because a rect's instance count can change between commits and a dirty rect is
  therefore not a fixed byte range.
- **Five code paths have never been exercised by any gate or any device**, and
  they are worth naming as a class rather than one at a time. Three of them are
  `AddBatches`'s rung split, added by issue #1389: the `window` local and both
  `AddBatch` window arguments have a `ConstantBuffer` arm no measured adapter
  reaches, Their `RawBuffer` arms DO run on every measured adapter; what no
  adapter reaches there is a non-zero `b`, because `InstancesPerBatch` doubles
  until one batch covers the document. The other two are the `ConstantBuffer`
  rung (**two** adapters now report `RawBuffer` — Metal on an Apple M3, and
  Vulkan on an Adreno 620 measured 2026-08-28 — so the `ConstantBuffer`
  **branch** of `InstancesPerBatch` and `BatchStrideBytes` has still never run.
  Both methods run on every capacity change and take an early return when the
  rung is not `ConstantBuffer`; it is the windowed arithmetic behind that return
  which is unexercised. A second agreeing adapter lowers the chance of reaching
  it rather than raising it) and the opaque material's alpha handling. A defect
  in either looks like a plausible picture rather than a failure — the
  window-size clamp in `BatchStrideBytes` was one, found by reading rather than
  by running. **The cutout material's `clip()` threshold was another and is now
  measured**: `just unity-render` draws that class at 0.5 and at 2 — the second
  above any coverage a fragment can have — and got 13 of 13 sampled node centres
  inked at the first and none at the second, with 601144 of 786432 pixels
  differing. A `_DsCutoff` that did not reach the fragment stage would have
  drawn the same picture twice, whatever the stage read instead, so **it
  resolves**, on Metal, and issue #1307 is answered. GLES 3.2 and Vulkan are
  untested.
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
- **A glyph has now been drawn, once, by hand — and the half that any GATE
  checks is still the half with no Unity type in it.** Issue #1389 drew glyphs
  from three atlases in a macOS/Metal player build on 2026-08-31, which is a
  measurement rather than a gate; R-E22 is the requirement that would make it
  one. Story #1123 landed the seam: the atlas crosses, the packer turns runs
  into instances, and `unity/ffi-check` executes the geometry, the run heap and
  the atlas lookup on any pull request whose diff is not documentation-only. The
  material, the texture and the draw commands are `Runtime/Engine/`, which only
  a Unity editor compiles and only a device runs — so the sampling itself, the
  linear texture and the per-instance choice of atlas material rest on reading
  rather than on running.
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
- **The translation is verified on Mono and CoreCLR, and not on IL2CPP, which is
  what ships.** `unity/ffi-check` runs under the .NET SDK. `just unity-editor`
  calls into the shipped library since 2026-08-29 and observes Mono raising
  `EntryPointNotFoundException` — the paragraphs below carry that reading — so
  what is left is the AOT backend. **`just unity-render` is the one that now
  could**: since story #1334 its player loads the shipped library, so a Unity
  runtime reaches the C ABI for the first time. **Which runtime is not
  recorded** — that project sets no scripting backend, and issue #1360 is what
  makes it set one and report it, and R-E7's IL2CPP requirement is stated for
  Android while this gate builds a macOS standalone player. So the gap is
  narrower than it was and is not closed, and naming the backend needs a reading
  nobody has taken. If IL2CPP raises a different type, or resolves at load
  rather than at the first call, every forwarder's catch is dead code on the
  target that ships. It cannot be closed while the package ships no native
  library for a platform a player runs on. **Story #1334 met that condition** on
  2026-08-24, so issue #1322 became reachable rather than blocked: a player can
  load a shipped library. Mono has since answered its half; IL2CPP has not.

  **An IL2CPP player reached the C ABI on a device on 2026-08-28**, through
  `just unity-android` (story #1367). That recipe sets the scripting backend
  explicitly and reports it, so "which runtime is not recorded" is answered
  **for this recipe**: IL2CPP, because R-E7 requires it and because Unity ships
  no arm64 Mono runtime. On a Pixel 5 the shipped `libdashscene_ffi.so` loaded
  and the runtime constructed, so a **present** library resolves under IL2CPP.

  **That is a gated claim rather than an observation**, and it took two review
  rounds to become one. The recipe's first verdict was the read line alone —
  which the probe emits _before_ constructing the runtime — so a library that
  failed to load gave a green run. Requiring the **absence** of the error line
  was the second version, and it was still fail-open: a native abort inside
  `ds_runtime_new` raises no managed exception, so no error line is written at
  all. The verdict is now three **positive** markers — the read, the runtime
  line and `DONE` — plus a refusal when the process reported `api=Null`, which
  is R-E14's premise rather than a decoration.

  **What it catches is a missing library or an ABI-version mismatch**, not
  staleness in general. `DashsceneRuntime`'s constructor runs `ds_abi_version`
  and `ds_runtime_new` only; R-E17's per-array `DsSlice::stride` comparison —
  which `unity/README.md` names as what catches a stale library of the right
  architecture — runs in `AcquireFrame`, and this probe acquires no frame. A
  library rebuilt from another commit with an unchanged `DS_ABI_VERSION` still
  constructs. The **painter** line stays non-fatal, because rung 3 and a
  stripped shader are answers this probe reports rather than failures of it.

  **The APK's own contents are asserted too**:
  `lib/arm64-v8a/libdashscene_ffi.so` present, and no other ABI directory. That
  is R-E8 read off the artifact rather than off a value the build script
  assigned itself, and it is the Android half of what
  `AssertLibraryReachedThePlayer` does for macOS — where the importer reported a
  plugin compatible, the build produced no errors, and Unity copied nothing.

  **That run does not change the claim above**, and the distinction is the whole
  of issue #1322: nothing has yet removed a symbol from a library an IL2CPP
  player loads. What was shown is that a present symbol resolves, which is not
  what the forwarders' catches are written for. `just unity-render` still builds
  a macOS standalone player, so its backend remains unrecorded and issue #1360
  stands for it.

  **Mono answers its half, measured on 2026-08-29.** `just unity-editor` makes
  two `[DllImport("dashscene_ffi")]` calls of its own against the library the
  package ships: `ds_abi_version`, which returned `2`, and one naming
  `ds_no_library_exports_this_symbol`, which raised
  `EntryPointNotFoundException`. The editor is a Mono runtime, and the check
  REFUSES a runtime these records do not name rather than assuming one — so
  Unity's move to CoreCLR cannot leave this paragraph false and the gate green.
  So the behaviour every forwarder in `Native.cs` catches is now observed on a
  runtime Unity ships, and not on CoreCLR alone.

  **The positive control is what makes that a statement about the symbol.** A
  library that did not load raises `DllNotFoundException`, and a check that only
  caught `EntryPointNotFoundException` would report the same pass whether the
  library was there or not. Mutated by pointing the second import at
  `ds_abi_version`: the gate reported that the call RETURNED and exited 1.

  **It does not exercise the package's own forwarders**, because `Native` is
  internal to the package assembly and the two imports are declared in the gate.
  What was unmeasured was the runtime, not the `catch`; `unity/ffi-check`
  executes the translation itself on every pull request.

  **IL2CPP is what is left, and it is the backend that ships.** It AOT-compiles
  each P/Invoke through a generated resolver, so it may resolve at build or at
  load rather than at the first call — and an editor is Mono whatever its player
  is. Two routes reach it. `just unity-android` builds an IL2CPP player and
  needs an attached device. A macOS standalone player built with
  `ScriptingImplementation.IL2CPP` needs none, and **Mac Build Support (IL2CPP)
  is present**: `mac-il2cpp` is selected in the editor's `modules.json` and
  `Unity.app/Contents/PlaybackEngines/MacStandaloneSupport/Variations/` carries
  the `il2cpp` player variations. An earlier version of this paragraph said that
  module was absent; it was read from `Editor/<version>/PlaybackEngines/`, which
  holds the Android and Linux players and never holds macOS support. So the
  cheapest route to closing issue #1322 is open, and it is a player build rather
  than a prerequisite.
- **The editor-side gate sources are compiled by nothing that runs on a pull
  request** — issue #1350. **Eight C# files, 5,743 lines at `0e818315`**, under
  `unity/android-probe/`, `unity/demo/`, `unity/editor-compat/`,
  `unity/hlsl-conformance/` and `unity/render-gate/`. The issue names two of
  them and its own correction names three.

  **Two populations, and the difference is that issue's own scope.** #1350 says
  in terms that it is "not issue #1331", which is about files that ship to a
  customer; these eight ship to nobody and are gate code. Widening the criterion
  to "every `.cs` under `unity/` no `.csproj` compiles" adds five more — the
  package's `Runtime/Engine/` and its `Samples~/`, 3,327 lines — for a union of
  thirteen files and 9,070 lines, and those five are #1331's. Both counts are
  derived with `git show 0e818315:<path> | wc -l`; a first version of this
  paragraph published a ten-file middle set that belonged to neither, with one
  row's line count taken from the working tree rather than from `0e818315`. They
  compile only inside the throwaway projects `just unity-editor`,
  `just unity-render`, `just unity-conformance`, `just unity-demo` and
  `just unity-android` build, so a file that stops compiling is found by a
  developer minutes or tens of minutes into an editor run rather than on the
  pull request that broke it.

  **Three measurements narrow the choice the issue leaves open, and none of them
  closes it.** Referencing an editor's own managed assemblies makes CI depend on
  an installed editor, which
  [`../decisions/the-native-library-ships-inside-the-unity-package.md`](../decisions/the-native-library-ships-inside-the-unity-package.md)
  D4 rules out, and those assemblies are Unity's to license rather than this
  repository's to vendor. A formatter is not a syntax gate:
  `dotnet format
  whitespace --verify-no-changes` runs over these files with no
  reference assemblies at all and reports formatting drift, and it exits **0**
  over a copy of `DashsceneAndroidProbe.cs` carrying `error CS1026: ) expected`
  — so the cheap form of a Roslyn pass would need Roslyn as a library, which
  would be the first `PackageReference` in any `.csproj` here. And a vendored
  facade is not a small one: these files reach `UnityEngine`, `UnityEditor`,
  `UnityEditor.Build`, `UnityEditor.Build.Reporting`,
  `UnityEditor.SceneManagement`, `UnityEngine.Rendering`, URP's
  `UnityEngine.Rendering.Universal` — which lives in a package assembly rather
  than in the editor install — and the package's own `Runtime/Engine/` types.
  With no references the compiler stops after about a hundred errors, so that
  surface cannot even be enumerated in one pass.

- **No release, and therefore no tag.** Story #1334 landed the library on
  2026-08-24: the package ships macOS arm64 and Android arm64 under
  `Runtime/Plugins/`, each with a committed `.meta`, and **R-E21 is met**. R-E3
  and R-E18 are not, and both now wait on the same thing — a release to name.
  The version in `package.json` is the placeholder `0.0.0`, so the tag R-E18
  composes would be `v0.0.0`.

  **The committed binaries measured 3,118,792 and 6,541,424 bytes**, which is
  9,660,216 together against D4's estimate of 9,598,896 — 0.64 % over it, not
  under. Two rows ship because two have a consumer; the Windows and Linux editor
  rows have none, and committing them would spend a public repository's
  permanent history on nothing.

  **A committed binary can go stale, and nothing here catches that.** It is
  refreshed by `just unity-plugins` and the defences are the ones already named
  in this document: `ds_abi_version`'s handshake, and `DsSlice::stride` read per
  array at run time.
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

  **Issue #1346 did not take the Android reading either, and the reason is
  mechanical.** The only place the callback's own thread can be observed is
  inside `OnPerformCulling`, which is in `Runtime/Engine/BrgPainter.cs` — a file
  another lane held for the whole of that work. A probe `BatchRendererGroup`
  registering no batch is not a substitute: Unity is not obliged to call the
  callback of a group with nothing to cull, so an absent reading would be
  indistinguishable from a reading of "not the main thread". One line beside the
  `Malloc` calls there is what it costs, and it is put to the owner on #1346.
- **A `StreamingAssets` file is not a file on Android, and the sample met that
  on a device.** Measured on a Pixel 5 on 2026-08-29 (issue #1346's run):
  `Application.streamingAssetsPath` is
  `jar:file:///data/app/<pkg>/base.apk!/assets`, so `File.Exists` answered false
  for a manifest that was present, the sample reported it missing, `Awake` ended
  — and the showcase SCENES, which need no manifest at all, never loaded either.
  The manifest read now goes through `StreamingAssetDocument.Resolve`, which
  asks the APK's own `AssetManager` where the entry is.

  **The cascade reads are still `File` and are still broken there**, which is
  why `just unity-demo-android` stages the three mapped documents and not the
  text one: `LoadDocumentWithText` takes owned bytes and the font, sheet and
  metrics beside it are read with `File.ReadAllBytes`. That is issue #1332, and
  the resolver above is the shape its fix would take.
