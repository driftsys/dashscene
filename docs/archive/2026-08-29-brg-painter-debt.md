# The BRG painter's two debts: #1297 and #1306

Working memory for branch `debt/v021-brg-painter-debt`, based on `origin/main`
at `0e818315`. Both issues are on epic #1120, which is not MVP, so a recorded
constraint with its reason is a legitimate outcome for either where the fix is
larger than the debt.

## Spec

### #1297 — the painter binds its paint heap globally

`BrgPainter.BindGlobals` binds `_DsPaints`, `_DsClipBoxes`, `_DsStrokes` and
`_DsGlyphs` with `Shader.SetGlobalBuffer` and `_DsGlobals` with
`Shader.SetGlobalVector`. Those are process-wide, so two painters in one process
share one heap and the last one to draw supplies the rows every painter's
fragments shade from. The painter reports it — a constructor warning when a
second painter exists — rather than drawing a wrong picture quietly.

**The question:** is a second painter in one process a supported configuration?

**The answer this branch takes: yes, and the buffers move off the global
namespace.** The reason it can be answered now and could not be when #1297 was
filed is that the issue named its own blocker — "a device drawing a frame … any
harness that draws one document and compares it to something" — and
`just unity-render` is that harness. It builds a **player**, draws
`goldens/dsb/v03-paint.dsb` through the painter, and asserts ink landed where
the committed tables place each node, with a negative control. #1297's routing
comment (2026-08-23) says an editor batchmode run cannot settle this; a player
build is what the recipe is.

**What replaces the global binding.** Every name the painter binds is set on the
materials the painter itself registered: `Material.SetBuffer` for the three heap
tables on every material, for `_DsGlyphs` on each text material, and
`Material.SetVector` for `_DsGlobals`. `_DsGlobals` moves into
`CBUFFER_START(UnityPerMaterial)`, which is where a per-material constant has to
be declared for the SRP Batcher to bind it per material — and `_DsCutoff` is the
measured precedent that a `UnityPerMaterial` member resolves under
`DOTS_INSTANCING_ON` on a BatchRendererGroup draw (issue #1307,
`just
unity-render`, 2026-08-23).

**Alternatives considered.**

- _Leave it global and record the single-painter constraint._ Rejected: the
  blocker the issue named is gone, so the constraint would be recorded for a
  reason that no longer holds.
- _A per-painter offset into one shared buffer._ #1297 says this is not to be
  designed before the material-scoped binding is measured, and it is the
  fallback if the measurement says the material binding does not reach a BRG
  draw.

### #1306 — the painter never reads `DsFrame.Dirty`

Verified by grep over `Runtime/**/*.cs`: the field's declaration is
`Native.cs:226`, the only code reading anything from it is `FrameLease.cs:207`,
which takes its stride for R-E17, and the remaining sites — `FrameLease.cs:171`,
`FramePacker.cs:21` and `BrgPainter.cs`'s `UploadInstances` — are comments
saying that nothing reads the rows. **No reader exists.** The issue's negative
claim holds.

**The outcome this branch takes: the non-use is recorded with the cost stated,
and the citation is left claiming only what the code does.** The fix is larger
than the debt and the issue says where it belongs: packing only the changed
rects needs the previous commit's tables held for comparison, because a rect's
instance _count_ can change between commits, so a dirty rect is not a fixed byte
range. `dashscene-gpu` has the same gap from the other side and it is issue
#708, where one design serving both painters belongs.

What was missing rather than wrong: the record named the gap and did not state
its cost. This branch states it.

## Plan

1. **RED** — a new `unity/package-gate` test asserting the painter binds the
   paint heap per material and makes no `Shader.SetGlobal*` call. Verify it
   fails on the current source, for the reason the calls are still there.
2. **GREEN** — move the five names to `PaintMaterialProperties`, bind them per
   material in `BrgPainter`, move `_DsGlobals` into `UnityPerMaterial`, delete
   the second-painter warning and the `_liveCount` counter, and drop the
   `Dispose` unbind block the global binding required. _Verify:_ `just test`,
   then `just unity-render` passes with the same 13-of-13 centres and the same 5
   centres judged against the instance's own colour.
3. **MUTATE** — bind `_DsGlobals` with a poisoned solid base through the same
   `Material.SetVector` call and confirm `just unity-render` FAILS. A value that
   did not reach the fragment stage would draw the same picture. Revert.
4. **#1306** — state the cost in `FramePacker`'s header and in
   `docs/design/unity-csharp-host.md`'s gaps list. _Verify:_ every number
   derived in the same command that states it.
5. **Prose** — record the #1297 answer in `docs/design/unity-csharp-host.md`'s
   painter section, and delete every claim in the tree that the heap is bound
   process-wide. _Verify:_ `grep -rn "SetGlobalBuffer\|SetGlobalVector"` over
   the package and the docs returns nothing that is not about the past.
