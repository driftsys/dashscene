# Unity painter uses BatchRendererGroup over GameObject-per-node

    status   **accepted (2026-08-18, owner's ruling)**. Ratified against a
             fallback ladder rather than against the two conditions this record
             originally set: one of them is carried as an assumption the owner
             is confirming with Unity directly, and the other escapes to a
             fallback this record already named. Both are stated under "What
             was carried" below.
    date     2026-07-13; ratified 2026-08-18
    source   docs/technotes/rendering-and-painters.md §10
    scope    the Unity C# package under unity/, and dashpaint-abi, which is the
             C representation of boundary B the package's declarations are held
             against
    related  docs/decisions/host-integration-in-three-layers.md (D1 layer 0,
             which the ruling of the same day gave a second form)

## Context

GameObject-per-node maintains a full scene-graph mirror (Transform hierarchy,
per-renderer culling, a managed object per node) — the "scene-graph duplication"
`docs/technotes/rendering-and-painters.md` §8.3 identifies as a cost.
BatchRendererGroup (BRG) draws N instances of a quad+material from a
`GraphicsBuffer` filled from a `NativeArray` (ideally a Burst job): no
GameObjects, no Transforms, per-instance SDF params in the buffer.

## Decision

**D1 — BRG over GameObject-per-node for the bulk SDF-quad UI, including lit
BRG.** Entities Graphics renders fully lit, shadow-casting instances via BRG
with zero GameObjects, so lighting is a shader-pass concern rather than a
rendering-path fork. Keep the 99 % on BRG and express the material classes
(unlit-overlay, lit-opaque, lit-cutout) as shader variants and passes on that
one path; reserve GameObjects for node replacement — arbitrary 3D, particles or
per-frame engine content in a layout box,
`docs/technotes/rendering-and-painters.md` §10.2 — and not for lit UI.

The property this buys is the one that matters downstream: it makes the Unity
painter's data model the same shape as the lean native painter, "instance buffer
→ SDF shader → GPU", so the dirty-set and double-buffer logic maps onto R-T4
directly. It is also the natural endpoint of the Burst and `Unity.Collections`
choice already made for the C# projection — the `NativeArray` the Burst jobs
fill **is** the BRG instance buffer.

**D2 — the target is Unity 6.5 (`6000.5.x`).** This is the version the painter
is to be built against. **No painter has run on it and no performance figure
comes from it** — no Unity painter exists anywhere, and story #1230's evidence
was all taken on `6000.3.22f1`. What _has_ been read from the 6.5 editor is its
shipped metadata: the API facts below and in D3, and the absence of an Android
module. Reading an assembly is not measuring a painter. It is a deliberate
departure from the editor that story installed.

**It does not settle the package's minimum version, which is issue #1125's.**
Epic #1106 recorded on 2026-08-18 that distribution form, minimum Unity version,
render pipeline, scripting backend, API compatibility level and native-plugin
layout are all that spike's, which is why `unity/`'s `package.json` carries no
`unity` field. A target and a floor are different numbers — a package can
support versions the painter is not developed against — and this clause fixes
only the first. That story asked for an LTS release and took `6000.3.22f1`,
Unity 6.3 LTS; Unity's release API on 2026-08-18 returns `6000.3` and `6000.0`
on the LTS stream and **not** `6000.5`, so 6.5 is a supported release rather
than an LTS one. Two consequences are recorded rather than left to be
discovered:

- **The Android modules are installed on 6.3 and not on 6.5.**
  `6000.3.22f1/PlaybackEngines/AndroidPlayer` exists; the 6.5 editor has no
  `PlaybackEngines/AndroidPlayer` at all. Whatever work first builds a Unity
  Android player on 6.5 installs them, and `docs/technotes/unity-toolchain.md`
  records what 6.3 has.
- **BRG is not what separates these versions.** `BatchRendererGroup` is present
  with the same public method set in 6.1, 6.3 and 6.5 — 6.3 and 6.5 read out of
  the installed editors' `UnityEngine.CoreModule.dll`, 6.1 from Unity's
  versioned scripting reference. Choosing among them is an LTS and platform-
  support question, not an API-availability one.

**D3 — the fallback ladder, and the failure mode each rung answers.**

| rung  | taken when                 | what it is                                                               |
| ----- | -------------------------- | ------------------------------------------------------------------------ |
| 1     | the design                 | BRG, per D1                                                              |
| 2     | **lit-BRG proves costly**  | the by-material-class hybrid: unlit-overlay via BRG, lit via GameObjects |
| 3     | **BRG is unsupported**     | instanced draws without BRG                                              |
| floor | rung 3 is also unavailable | GameObject-per-node, which D1 exists to reject                           |

**Descending the ladder costs R-T4, and that is a specification rule rather than
a preference.** `../specification/03-target-hardware-rules.md` R-T4 is "CPU
frame cost = dirty-range instance-buffer upload from the rect table +
submission. Nothing else." Rung 1 is what satisfies it. Rung 2 routes lit nodes
through GameObjects and the floor gives up the job-filled instance buffer
entirely, so each is a departure from R-T4 for the nodes it moves — partial for
rung 2, total for the floor. Rung 3 is the one lower rung that keeps the model
and so keeps R-T4 in reach.

**Ratifying the ladder is not a waiver of R-T4.** It records what is done if
rung 1 proves unavailable, and a rung below 1 being taken is the trigger to
raise the conflict rather than to proceed quietly — the specification is not
amended here and nothing in this record amends it.

**Rung 2 does not answer rung 3's failure, and this record previously implied it
did.** The hybrid's cheap half is still BRG, so if BRG is unavailable the hybrid
is unavailable with it. The two failure modes are separate and were conflated by
a single sentence naming one fallback.

Rung 3 preserves what D1 is actually buying — the instance-buffer data model and
therefore the R-T4 mapping — without BRG's batch and culling machinery.
`Graphics.RenderMeshInstanced`, `Graphics.RenderMeshIndirect`,
`Graphics.RenderPrimitives` and its indexed and indirect forms, and
`CommandBuffer.DrawMeshInstanced`, `DrawMeshInstancedProcedural` and
`DrawProcedural` are all present in the installed `6000.5.6f1` editor. **Nothing
is built for rung 3.** It is recorded so that the answer is not improvised under
pressure if the support question comes back badly.

**D4 — support is read from Unity, not inferred.**
`UnityEngine.Rendering.BatchRendererGroup.BufferTarget` is a static property
returning `UnityEngine.Rendering.BatchBufferTarget`, whose members are, verified
by reflection against both installed editors:

    Unknown                            =  0
    UnsupportedByUnderlyingGraphicsApi = -1
    RawBuffer                          =  1
    ConstantBuffer                     =  2

So "is BRG supported here" is a read on the target rather than a research
question. **Three of the four values select a rung**, and Unity documents the
fourth as one it never produces:

| value                                | what it selects                           |
| ------------------------------------ | ----------------------------------------- |
| `RawBuffer`                          | rung 1, instance data in a storage buffer |
| `ConstantBuffer`                     | rung 1, under the window constraint below |
| `UnsupportedByUnderlyingGraphicsApi` | **rung 3**                                |
| `Unknown`                            | never returned — see below                |

`Unknown` is documented in `UnityEngine.CoreModule.xml`, which ships beside the
assembly in both installed editors, as "the default uninitialized value for this
enum … Unity will never return this, and you will never use it". So it is not an
escape hatch and nothing should be written against it.

**The read is only valid with a graphics device, and this is the part that can
go wrong quietly.** `UnsupportedByUnderlyingGraphicsApi` is documented as "the
Batch Renderer does not support the **active graphics API**", so the answer is a
property of the device the editor or player actually obtained. The only Unity
harness this repository has is the batchmode invocation in
`../technotes/unity-toolchain.md`, and it passes `-nographics`. **What that
returns has not been measured**, and it must not be recorded as a verdict: a
`-nographics` run reporting `UnsupportedByUnderlyingGraphicsApi` would select
rung 3 and abandon BRG on a read taken with no device. Whoever discharges this
takes it on a player with a real device, on the target, and says which.

**This has not been read on any device**, and no value is claimed here. When it
is read, `ConstantBuffer` is not a synonym for supported-and-fine: it routes
per-instance data through a uniform-buffer window bounded by
`GetConstantBufferMaxWindowSize()` and aligned by
`GetConstantBufferOffsetAlignment()` rather than through a storage buffer, which
is a constraint on the instance layout D1 assumes. That is the same class of
limit as the four fragment-stage storage buffers the lean painter binds
(`host-integration-in-three-layers.md` D3a), and it is to be recorded whichever
way it lands.

**A window too small for the instance row is a third failure, and D3 has no rung
for it.** It is a constraint on the instance layout rather than on the rendering
path, so `instance-buffer-contract.md` — where the row is specified — is the
record to open if a target reports `ConstantBuffer` with a window that does not
fit.

**Whether rung 3 escapes that limit is not claimed here.** An earlier draft of
this clause asserted that it does not, on no reading; rung 3's APIs source
per-instance data from a buffer the shader binds rather than from BRG's batch
buffer, so the answer depends on what that shader binds and on the same device's
limits. It is a question for whoever meets it, and this clause exists so that
they meet it as a question rather than as a surprise.

## What was carried

This record set two conditions for its own ratification, and neither was met by
evidence. Both are carried, deliberately, and named here so that a later reader
does not mistake ratification for measurement.

- **"Confirm BRG platform support on the automotive GLES 3.2 target"** — carried
  as an assumption. The owner ruled on 2026-08-18 to assume the target board
  supports BRG and to confirm it with Unity directly, off this repository. The
  check that discharges it is D4's, and the failure it guards against selects
  rung 3.

  Unity's public documentation does not appear to answer it. The QNX overview
  page of the `6000.0` manual states no graphics API, no render pipeline and no
  restrictions list, and says the Editor for Embedded Platforms is available
  under separate terms. **That is one page read, not a survey of the QNX
  manual**, and it is the reason the question goes to Unity rather than to a
  document.

- **"Spike the lit + SDF-clipped-shadow-caster shader on the target SRP"** —
  carried as a risk with an escape. It is where the engineering risk sits, and
  its bad outcome is exactly rung 2's condition, so the ladder answers it
  without holding the slice. The limits already known are technique rather than
  BRG: a transparent or AA-fringe node casts only clipped, binary-threshold
  shadows and gets no SSR (§8.2), and every lit node adds passes and therefore
  tile-flush cost under R-T1, so most UI stays unlit-overlay and only genuinely
  physical nodes are marked lit.

## Consequences

**Entry condition 2 of epic #1106 is discharged**, which with the layer ruling
of the same day leaves that epic with none.

**A carried assumption is not a measurement, and nothing may describe BRG as
confirmed on target hardware until D4's read is taken there.** This is the same
rule `host-integration-in-three-layers.md` D3a lived under between its
ratification and issue #885, and it costs nothing to keep.

## Alternatives considered

**GameObject-per-node**, which is the floor of D3's ladder and the option this
record was written to reject. It is not merely slower: it maintains a second
scene graph whose Transform hierarchy and per-renderer culling duplicate what
the dashscene document already states, and Transforms cannot be written from
Burst except through `TransformAccessArray`, so it also gives up the job-filled
instance buffer that makes the R-T4 mapping work.

**Waiting for the support question before ratifying.** Rejected by the owner on
2026-08-18. It would have held epic #1106 behind a question this repository
cannot answer itself, when the ladder makes a bad answer survivable and D4 makes
it cheap to detect. **No count of the stories that would have waited is given**:
the epic's body says it "cannot be started until all three are settled", so the
answer is its whole story table. Counting the subset this condition gates
directly is what produced two wrong numbers already; read the table.
