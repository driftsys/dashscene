# Unity painter should use BatchRendererGroup over GameObject-per-node (proposed)

    status   proposed — a direction, not yet ratified
    date     2026-07-13
    source   docs/technotes/rendering-and-painters.md §10
    scope    dashscene-unity, the future Unity/C# painter project

## Context

GameObject-per-node maintains a full scene-graph mirror (Transform hierarchy,
per-renderer culling, a managed object per node) — the "scene-graph duplication"
`docs/technotes/rendering-and-painters.md` §8.3 identifies as a cost.
BatchRendererGroup (BRG) draws N instances of a quad+material from a
`GraphicsBuffer` filled from a `NativeArray` (ideally a Burst job): no
GameObjects, no Transforms, per-instance SDF params in the buffer.

## Leaning

BRG over GameObject-per-node for the bulk SDF-quad UI, including lit BRG:
Entities Graphics renders fully lit, shadow-casting instances via BRG with zero
GameObjects, so lighting is a shader-pass concern, not a rendering-path fork.
Keep the 99% on BRG and express the material classes (unlit-overlay, lit-opaque,
lit-cutout) as shader variants/passes on that one path; reserve GameObjects only
for node-replacement (arbitrary 3D/particles/per-frame engine content in a
layout box, `docs/technotes/rendering-and-painters.md` §10.2), not for lit UI.

## Why this is a leaning, not a decision

It makes the Unity painter's data model the same shape as the lean native
painter ("instance buffer → SDF shader → GPU"), so the dirty-set/ double-buffer
logic maps onto R-T4 directly, and it is the natural endpoint of the
Burst+`Unity.Collections` choice already made for the C# projection. It is not
ratified because the engineering risk is unverified: BRG is low-level and thin
on docs, platform support on the exact automotive GLES 3.2 target is
unconfirmed, and the lit + SDF-clipped-shadow-caster shader path is unspiked.
There is also a stated fallback if lit-BRG proves costly: a by-material-class
hybrid (unlit- overlay via BRG, lit via GameObjects).

## What ratifies this

- Spike the lit + SDF-clipped-shadow-caster shader on the target SRP.
- Confirm BRG platform support on the automotive GLES 3.2 target.

Both are tracked as open items in `docs/technotes/rendering-and-painters.md`
§13. This record moves to `accepted` only once they are resolved in BRG's favor;
until then the Unity painter's actual backend choice is undecided.
