# The frame delta is clamped, and the host owns the clock

    status   accepted (story #572, 2026-07-31); rule 1 amended by story #810,
             2026-08-08 — the clamp moved from each host into
             `LiveScene::tick`; the upper bound's first input noted as
             supplied for one device on 2026-09-04
    scope    crates/dashlang (MAX_FRAME_DELTA and the clamp, since story #810),
             demo/src/shell.rs and demo-web/src/host.rs (the frame loops, which
             own their clocks), demo/tests/clock_invariant.rs and
             demo/tests/host_policy_invariant.rs (the two invariants'
             enforcement), and the host of every future product painter
    binds    every animation test, because a test that wants a stall must pass
             the clamped value rather than a wall-clock one; and every product
             painter's host, because the clamp value is a cross-painter
             agreement rather than a per-host setting
    related  docs/decisions/dashcue-spring-uses-semi-implicit-euler.md,
             docs/decisions/dashcue-scheduler-storage-stays-vec.md,
             docs/decisions/crate-name-map.md (the dashscene-desktop section,
             which is where story #810's move was ruled),
             docs/specification/01-goals-and-requirements.md (G2, R4),
             docs/specification/02-principles.md (P3)
    refs     #572, #568, #476, #810, #803, #775

## The decision

Three rules, and the third is the one that outlives the other two.

1. **The frame delta is clamped**: `dt = min(elapsed, 100 ms)`, and there is
   **no accumulator**.
2. **No crate at or below `LiveScene` reads a clock.** This was already true and
   held by accident; it is now asserted by `demo/tests/clock_invariant.rs`.
3. **Both product painters clamp at the same value, and that value is configured
   rather than inherited from either engine's default.**

## Amended by story #810, 2026-08-08: who applies the clamp

Rule 1 was written as "**the host** clamps the frame delta", and both hosts did
— in two different units, `Duration::from_millis(100)` in the native one and
`f64 = 0.1` in the browser one, so keeping them equal already needed a unit
conversion that nothing performed. Rule 3 is what made that a defect rather than
a detail: it requires the two values to be equal, and nothing checked that they
were.

**The clamp now lives in `dashlang`, as `MAX_FRAME_DELTA`, applied inside
`LiveScene::tick`.** A host passes the raw interval its own clock measured.
Ruled with issue #803 and recorded in `docs/decisions/crate-name-map.md`,
because stories #741 and #794 turn both hosts into _published_ integration
crates — at which point a rule written twice becomes a semver-bound agreement
that nothing checks.

**This title still holds, and now describes a seam rather than a single owner.**
The host owns the clock: it decides what "elapsed" means, and when its clock is
stopped — the first frame, and the frames that end a parked loop, both of which
start from zero. Those are facts about a host's own timeline. What a host no
longer decides is how large a step is too large.

Rule 3 is now structural rather than an agreement: there is one value, so two
painters cannot disagree about it. `demo/tests/host_policy_invariant.rs` fails
if a host declares one of its own again, and
`crates/dashlang/tests/frame_policy.rs` pins what the clamp does.

**The generation-and-`shown` gate moved with it**, for the same reason and in
the same story. It was the other rule both hosts held privately, and the browser
host documented its own copy by citing the native host rather than this record
(issue #775). It is now `LiveScene::advanced` and `LiveScene::mark_shown`. That
also makes one rule structural that each host previously had to remember: a
rebuild produces a new `LiveScene` over a new arena whose generations restart,
and the gate starts clear with it.

## Why there is no accumulator

`docs/decisions/dashcue-spring-uses-semi-implicit-euler.md` already states that
`advance(dt)` steps in equal substeps below the stability bound
`h < 1 / ((2ζ + 1)·ω)`, so "a frame hitch splits into several, so the
integration cannot diverge". A host-side accumulator would reimplement that
substepping one layer up, in a second place, against a bound the host does not
know.

Reproducibility needs nothing from an accumulator either. `tick` takes `dt` as a
parameter and reads no clock, so a test passes an explicit sequence and never
involves a host at all; the same record pins the per-step arithmetic to IEEE
basic operations plus `sqrt`, so an identical sequence is bit-identical across
machines. **R4's reproducibility clause is already satisfied**, and a fixed
timestep would be paying for a property the stack already has.

## What the clamp guards: frame cost, not correctness

R4 also requires statically provable frame cost. Substep count scales with `dt`:

    omega    = 2π / response
    h_max    = 1 / ((2ζ + 1)·omega)   =  response / 18.85   for ζ = 1
    substeps = ceil(dt / h_max)

For a snappy spring (`response = 0.15 s`, so `h_max ≈ 7.96 ms`):

| `dt`               | substeps |
| ------------------ | -------- |
| 16.7 ms (60 Hz)    | 3        |
| 100 ms (the clamp) | 13       |
| 333 ms             | 42       |
| 30 s (unclamped)   | 3770     |

An unbounded `dt` — a debugger pause, an alt-tab, an operating-system deschedule
— is therefore an unbounded substep burst, which is exactly the property R4
forbids. The clamp bounds it. It changes no result that was correct without it.

## 100 ms is a convention, not a derived bound

Recorded plainly, because a constant that reads as computed does not get
revisited. This project has twice been bitten by exactly that shape: issue #549
(no display geometry is pinned, so FLIP's viewing condition is an assumption)
and issue #462 (no memory budget exists, so the raster quality bands were set
against a content class that will not dominate).

**The lower bound is real.** The clamp has to sit above ordinary hitches — a
garbage-collection pause, a page fault, a heavy re-solve — or it fires during
normal operation and animation visibly drifts behind the wall clock for no
reason. Anything under about 50 ms would be wrong for that reason alone.

**The upper bound is not chosen.** Nothing distinguishes 100 ms from 333 ms.
Deriving it needs two things: the stiffest spring the vocabulary permits, and a
frame budget for the animation update to fit the resulting substep burst inside.
**Epic #476 states there is no frame budget and no target-hardware
measurement**, which was true when this was written; since 2026-09-04
`the-gpu-frame-on-the-target-device-is-budgeted.md` supplies a frame budget for
one named device — the Pixel 5 at 1080x2340, one 60 Hz frame — so the second
input exists for that device and the bound can be derived against it there.

**What would settle it**, in the order the inputs become available:

- a measured frame budget on a named target device, which is what turns "13
  substeps" into "13 substeps costs X of Y milliseconds" — supplied for the
  Pixel 5 on 2026-09-04, and for no other device;
- the stiffest spring the authoring vocabulary allows, which today is unbounded:
  `Spring::new(stiffness, damping_ratio)` takes any stiffness a producer writes,
  so the worst case is not a property of the vocabulary yet;
- a measurement of how long a real stall lasts on that device, which decides how
  often the clamp fires in the field.

Until then 100 ms is a working value chosen in the design session, and this
record is where it says so.

## The binding clause is cross-painter agreement, not the number

**Unity's `Time.maximumDeltaTime` defaults to 0.3333 s.** If the native host
clamps at 100 ms while the Unity painter's host runs Unity's default, then the
same document, after the same stall, animates differently on the two product
painters — and G2 requires that multiple render backends show the same pixels.

So the rule this record binds downstream work to is not the value:

> Both product painters clamp at the same value, and it is configured rather
> than inherited from either engine's default.

Inheriting is the failure mode to name explicitly, because it is the path of
least resistance on the Unity side: a host that simply does not set
`Time.maximumDeltaTime` has silently chosen 0.3333 s and nothing reports it. The
constraint survives whatever number is eventually picked, and changing the
number is a change to both hosts or to neither.

## The product consequence: late rather than wrong

Under a clamp, **animation falls behind wall-clock time after a long stall**
instead of jumping forward to catch up. A three-second stall advances the
animation by 100 ms and leaves it 2.9 s behind where a wall-clock-driven
animation would be.

That is the right behaviour for a cockpit — a needle that sweeps through its
whole range in one frame to catch up is worse than a needle that arrives late —
but it **is a choice**, and the alternative (catch up by stepping the whole
elapsed time) is what an unclamped loop does by default. It is recorded here so
the next person finds it argued rather than inferring it from a constant.

## Idle-frame skipping does not make a large `dt` routine

This looks like an interaction and is not one, which is worth recording because
the two features land together.

While anything animates, the generation advances, so the loop runs at frame rate
and `dt` stays small. When the loop is genuinely idle there are no live tracks
to substep. The two states are mutually exclusive by construction.

The host closes the remaining gap directly: **the frame clock stops when the
loop parks**. `Host::previous_frame` is cleared on the way into
`ControlFlow::Wait`, so the frame that ends the wait starts from `dt = 0` rather
than from however long the window sat untouched. Without that, the first frame
after an idle period would hand `tick` the full clamp, and a spring started by
the very event that ended the wait would begin 100 ms into its own motion — a
visible artefact on the first interaction after every quiet period.

With it, the clamp guards **external** stalls only: a debugger, an
operating-system deschedule, a long synchronous load. Never the loop's own wait.

## The clock invariant, and how it is enforced

The invariant is: **no crate at or below `LiveScene` reads a clock** —
`dashlang`, `dashcue`, `dashscene-core`, `dashscene-engine` and
`dashscene-typeset`. It is what R4's reproducibility rests on, and until this
story nothing stopped someone adding a helpful `Instant::now()` inside the
runtime and silently removing the property.

**The enforcement is a source scan, committed as a test**
(`demo/tests/clock_invariant.rs`), not a reviewed convention. A convention is
enforced by whoever happens to read the diff; a test is enforced by the commit.

Three properties of the scan are deliberate.

- **It scans a hand-maintained crate list, because the correct list cannot be
  derived.** `dashscene-engine` reaches `tick` as an injected
  `Box<dyn LayoutSolver>` and `dashscene-typeset` through the engine's measure
  callback. Neither is a library dependency of `dashlang`, so a check built from
  Cargo's dependency graph would miss exactly the two crates the rule most
  needs.
- **It matches calls, not type names.** `Instant::now` and `SystemTime::now`,
  with `//` comments stripped first — the crates it scans are expected to
  _discuss_ the rule, and a doc comment naming the forbidden call is not a call.
  Forbidding the type names `Instant` and `SystemTime` would instead fail on a
  signature that merely accepted one, which is not reading a clock.
- **It fails when it scans nothing.** A path list that goes stale turns a source
  scan into a check that passes without having read anything, which is the
  `t2-check-has-no-teeth` failure the v0.13 test tiering exists to remove. The
  test asserts a non-empty file set per crate, and a second test in the same
  file exercises the matcher against two clock reads that must be caught and two
  legitimate lines that must not be.

**What it does not catch**, stated so the claim is not read as wider than it is:
a clock read reached through a third-party crate. Nothing in the workspace
depends on `chrono`, `time`, `quanta` or `web_time` today, so there is no
spelling to add; taking one of them on would need its call named in
`CLOCK_READS`, and the manifest change is what would surface that.

## Alternatives considered

- **A fixed-timestep accumulator in the host.** Rejected above: `dashcue`
  already substeps below its own stability bound, and reproducibility does not
  need one because `tick` takes `dt` as a parameter. It would add a second place
  where the substep policy lives, and the host is the one that knows least about
  the bound.
- **No clamp at all, and let the substeps run.** Rejected: R4 requires
  statically provable frame cost, and an unbounded `dt` makes substep count
  unbounded. A 30-second debugger pause is 3770 substeps per track.
- **Clamp at Unity's 0.3333 s, so the Unity host needs no configuration.**
  Rejected: it makes the native host inherit a value chosen by a different
  engine for a different purpose, and the point of the cross-painter clause is
  that neither host inherits. If 0.3333 s is later measured to be the right
  number, both hosts move to it together and both set it explicitly.
- **Record the clock invariant as a convention in `AGENTS.md`.** Rejected: a
  convention does not fail a build. The property is cheap to assert and
  expensive to lose, because losing it does not break a test — it makes the
  tests that pass mean less.
- **Put the clock-invariant scan in a workspace-level test crate of its own.**
  Rejected: it would be a new workspace member for one test. It lives in `demo/`
  because the host is the counterpart of the rule — `demo/src/shell.rs` is the
  one place in this repository that reads a clock — and `cargo test --workspace`
  already runs it.
