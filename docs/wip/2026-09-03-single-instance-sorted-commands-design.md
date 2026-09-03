# Single-instance sorted draw commands — the #1401 fix

    status  spec, approved in conversation 2026-09-03. Working memory: this
            file is archived to docs/archive/ by the pull request that lands
            the work.
    issue   #1401 (t1-correctness, v0.21)
    branch  debt/v021-single-instance-commands, off
            story/v021-showcase-host-parity at dd20a18
    evidence  driftsys/dashscene-v021-lanes/probe-1401/2026-09-03-arms/RESULTS.md
            (outside the repository; the measurement tables and the anomaly
            frame captures)

## The defect

Unity's sorted-transparent BatchRendererGroup path silently drops a contiguous
subset of draw commands for single frames when the commands carry
`BatchDrawCommandFlags.HasSortingPosition` with more than one visible instance
per command. Measured on macOS/Metal, Apple M3, Unity 6000.3.23f1, URP 17.3.0:
292–317 dropped-band frames per 20,000 on the typography scene as built, 115 per
20,000 with the host's whole per-frame path stopped, 0 per 60,000 with one
visible instance per command, 0 per 40,000 with the flag removed. The dropped
frame renders the affected region as bare backdrop; nothing is logged, and the
painter's own culling emission is byte-identical on the dropped frame.

Unity documents no restriction. The `visibleCount = 1` shape is what Unity's own
GPU Resident Drawer feeds this path; the restriction is established by the
measurements above, not by any contract.

## Requirement

The painter shall emit every draw command that carries
`BatchDrawCommandFlags.HasSortingPosition` with exactly one visible instance.

Unconditional across the three material classes: all three emit the hazardous
shape today, and one measured-safe shape everywhere beats an unmeasured branch.
A lit-class refinement (multi-instance runs without the flag, ordered by the
depth test) is filed as debt, not built here.

## Scope

`unity/com.driftsys.dashscene/Runtime/Engine/BrgPainter.cs` — the emission loop
in `OnPerformCulling` and the two counting methods (`RunEnd`, `CommandsInBatch`,
which collapse: the command count becomes the instance count). Key construction
(D4's behind-the-sheet layout) is unchanged. No document-format change, no C ABI
change, no shader change.

## Verification

- `unity/package-gate`: a bounded scan asserting no emission path pairs the flag
  with a run longer than one instance — written first, red against the current
  painter for that stated reason, green after. Mutation: restoring the
  material-run walk turns it red again.
- Count/emission agreement pinned: command count equals instance count, and the
  emission loop's bound agrees (the seam that failed in `c7f4fd7`).
- The band defect itself is probabilistic and Metal-bound, so it is a recorded
  measurement, not a CI gate: the 20,000-frame soak procedure (probe patch and
  script on the evidence shelf) run before and after, both tables quoted in the
  pull-request body.
- R-T4: a controlled before/after frame-cost pair on the typography scene
  (command count rises 11 → 381 per view), the number named against R-T4 in the
  pull-request body whatever it is.

## Records edited in place

- `docs/decisions/brg-draw-command-order-is-not-guaranteed.md` — the new binding
  constraint (a flagged command carries one visible instance), and "What is
  still owed" updated.
- `docs/technotes/batch-renderer-group.md` — the band defect and its isolation
  join §5; §7 gains the single-instance rule.
- `docs/design/unity-csharp-host.md` — the command-shape paragraph.
- R-E22's status checked at write time.

## Alternatives considered — all measured, 2026-09-03

- **Remove the flag (pre-#1391 emission).** Also eliminates the band (0/40,000)
  but restores issue #1389: material grouping hides every glyph behind the
  backdrop. Rejected.
- **Triple-buffer the instance and heap buffers.** No effect on the band
  (measured twice), and its premise is documented for `LockBufferForWrite`, not
  `SetData` — Unity's own GPU Resident Drawer writes one persistent buffer with
  `SetData`. Rejected; the experimental ring is not landed.
- **Depth writes on the overlay path.** Already rejected by D3 of
  `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`; nothing here
  reopens it.
- **Do nothing.** The defect is user-visible flicker at up to 1.5 % of frames.
  Rejected.

## Out of scope, filed as follow-ups

- Story 2: order semantics under the new shape — re-run the technote §5b
  measurements, build the sandwich fixture and the order gate, and revisit D1/D2
  of the decision record. The step-negation mutation must flip the composite
  there before any order claim is recorded.
- Device-lane soak on Android/Vulkan (one-device rule applies).
- A standalone BRG micro-reproduction, no dashscene code, filed upstream to
  Unity and re-run at every editor version bump.
- Lit-class command-shape refinement (debt).
