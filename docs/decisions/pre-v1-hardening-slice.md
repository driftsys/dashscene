# The v1 debt backlog splits: a pre-v1 hardening slice, feature deferrals stay v1

    status   accepted (2026-07-19 debt triage) — creates the v0.13 slice
             (milestone #14, epic #362); binds where accumulated debt is
             resolved. Revised at the v0.12 close (2026-07-27): the dividing
             line gains a third term, and the slice gains a second track
             (epic #474) for the items that term names.
    scope    docs/roadmap.md, the v0.x slice plan, the debt backlog
    related  docs/decisions/debt-streams-own-artifact-classes.md (how the
             slice's burn-down is dispatched)

## Context

The 2026-07-19 debt-backlog triage swept every open `debt` issue and re-anchored
each to the slice where it next matters. A large cluster verified as independent
debt — perf and allocation micro-debt, cleanup, test-gaps, and
latent-correctness guards — with no near-term consumer, and was anchored to v1.

v1's scope is "Unity, full feature set, performance, production toolchain": a
large slice whose principal work is the Unity painter, the full feature set, the
rendering-performance pass, and the production toolchain. Debt anchored there
does not get a focused pass — it sits under those large deliverables and never
surfaces.

## Decision

The independent code-debt splits out of v1 into a dedicated pre-v1 hardening
slice, **v0.13** (milestone #14, epic #362): the items across the `dashcue`,
`dashlang`, `dashscene-core`, `dashscene-engine`, `dashscene-typeset`, paint,
goldens, and repo/importers clusters that are resolvable on their own — perf and
allocation, cleanup, test-gaps, and latent-correctness guards. They parallelize
by crate (one PR per cluster), and none is gated on a v1 deliverable.

Feature scope gated on a specific v1 consumer **stays on v1** — it is not debt
to burn down, it is work that unlocks when its consumer lands: STRING/BOOL and
smoothing binding serialization and the `Format` transform (gated on the v1
text-parity fixture #299), the cross-file and library importer maturation, the
strict waiver gate, the anti-aliased-clip decision (needs the second painter),
retained group composition (needs the v1 incremental painter), extended-Arabic
joining (needs an imported extended-Arabic charset), mixed style segments, and
the docs items that need v1 inputs — target-hardware numbers for the
measurable-requirements work, and their own v1 or v2 subject slices for the
decision-record ratifications and the open-question tracking.

The dividing line: **resolvable before v1 goes to v0.13; unlocks with a v1
consumer stays on v1.**

### The third term, added at the v0.12 close

That line has two terms and needs three. It has no place for an item which is
resolvable now, is not gated on a v1 consumer, and still cannot be worked —
because what it needs is a ruling or a measurement, not an edit. The 2026-07-19
triage did not separate those, so nine of them were filed as `debt` and counted
as burn-down.

**Needs a decision or an owner-supplied input is v0.13, but is not burn-down
work.** Such an item goes on the milestone and into its own track (epic #474),
beside the burn-down rather than inside it. The test is whether a session
working alone can finish it: if the next step is a ruling only the repository
owner can give, or an input only the owner can supply, it is not burn-down work
no matter how small the eventual edit.

Seven items met it when the term was written. **Four were settled the same
day**, which is the term's own justification: they were not hard, they were
merely unasked.

- **#462** — `dashpack` treats a profile exceeding the target memory budget as a
  validator error, and **no memory budget or target display resolution exists
  anywhere in `docs/specification/`**; `03-target-hardware-rules.md` carries
  R-T1 to R-T4 and no number. A profile that cannot fail is not a contract.
  **Deferred to v1**, set against target hardware alongside #170. This accepts a
  stated gap for the whole of v0: a document can pack successfully and still not
  fit the target, and nothing detects it. The per-asset bands still bind; only
  the aggregate contract is deferred.
- **#373** — the MSDF 14 px/em floor is enforced at import time against the
  authored size, while `docs/decisions/q1-msdf-below-14px.md` justifies
  MSDF-only text on sizes being static. dashscene animates, so a document can
  pass validation and still render below the floor mid-transition — the silent
  degrade **P4** exists to prevent. **Ruled: the validator computes the
  reachable minimum**, walking the `dashcue` specs bound to a node instead of
  trusting the authored size. It stays a compile-time named diagnostic, because
  a reachable scale is a document property and is knowable in advance — unlike
  painter capability, which is why backdrop blur reports at render time and this
  does not.
- **#422** — the `blur-falloff` band caught none of six measured mutations.
  **Ruled: split the number** into the residual it was written for and a
  separate gate chosen against those mutations.
- **#446** — **Ruled: correct the record in place, no new record.** Only half of
  it was ever a judgement; the rest is a live contradiction on `main`.

All three of the ruled items became ordinary burn-down work.

The three that remain are blocked on an **input**, not a decision. Issues #105
and #271 both need a Figma capture that does not exist, and #412 is blocked on
the painter's working colour space, itself undecided.

Two further items are blocked but not on a decision — #263 and #82, both on
GitHub Actions billing — and sit on the milestone outside both tracks.

### The fourth term, added with the tier split

The same day, a fourth kind surfaced with the same shape as #462. **Twenty of
the burn-down's items were perf and allocation debt with no measurement behind
them** — reuse a commit-path allocation, stop cloning a level vec per line,
prune FLIP targets in better than O(n²). Each names a genuine inefficiency and
each analysis is sound. None has a number saying it matters: there is no frame
budget, no target-hardware measurement, and no profile identifying which is on a
hot path.

Fixing one produces a PR whose success criterion is "the tests still pass",
which is not evidence the change was worth making, nor that it did not make
something else worse. That is the #462 argument applied to optimisation:
**resolvable is not the same as measurable.**

**Perf debt with no measurement behind it goes to v1's performance pass**, which
is where the profile that should select and order it gets produced. It goes with
a home and an entry condition (epic #476), not as a bare milestone move — a bare
move would recreate the "buried under v1" failure this whole record exists to
fix, and the entry condition lets "measured, not worth it" be a real outcome
rather than a silent drop.

Four perf-shaped items stayed in v0.13, each correctness wearing perf clothing:
**#197** (interners growing without bound is a leak, not a slow path), **#276**
(a clip-blind overlap test costs a wrong decision, not a slow one), **#200**
(hardening carrying a fill-completeness assert), and **#322** (it changes layout
output).

### The line, in full

    resolvable and measurable now  → v0.13 burn-down (#362)
    needs a ruling or an input     → v0.13, but not burn-down (#474)
    real but not yet measurable    → v1's performance pass (#476)
    unlocks with a v1 consumer     → v1

## Alternatives considered

- **Leave everything on v1.** Simplest, but conflates "unlocks with its
  consumer" with "resolvable debt", so the hygiene debt never gets a focused
  pass — the problem this decision exists to fix.
- **An unnumbered "pre-v1 debt" milestone outside the version sequence.** Keeps
  it off the v0.x numbering, but breaks the convention that every milestone maps
  to a roadmap slice.
- **An epic on the v1 milestone, no new slice.** Lightest, but the debt still
  reads as "v1" in milestone and board views — the drift this avoids.

Considered at the 2026-07-27 revision, for the third term:

- **Leave the seven in the burn-down and let stories skip them.** Rejected
  because that is what happened: each gets picked up, analysed, and put back
  down, repeatedly, since nothing about it changes between attempts.
- **Move them to v1.** Rejected for #462 in particular — it now gates real
  content, and deferring it means shipping an escalation ladder whose budget
  nothing checks.
- **A separate milestone for them.** Rejected on the same ground as the
  unnumbered-milestone alternative above: every milestone maps to a roadmap
  slice.

## Consequences

- v0 now runs through v0.13. The slice was provisional and **was revised at the
  v0.12 close (2026-07-27)**, like v0.11 and v0.12 before it.
- The v0.13 epic (#362) carries the full by-crate checklist and the out-of-scope
  rationale; the milestone (#14) holds the items.
- **v0.13 runs as two tracks**: the burn-down (#362, 76 items across three
  streams, in three tiers) and the inputs-and-rulings track (#474, 3 items). Two
  further items are blocked on Actions billing. The milestone holds 81.
- **The burn-down is tiered**, so it is worked in an order rather than as a
  list: 23 `t1-correctness` (wrong output, crash, silent drop), 20
  `t2-check-has-no-teeth` (test gaps and checks that cannot fail), 33
  `t3-cleanup`. T2 is the tier v0.12 earned — every one of its nine stories had
  a defect found in review, and the recurring kind was a check that could not
  fail.
- **Stream C halved**, from 28 items to 14, because 14 of them were perf debt.
  What is left there is almost entirely correctness.
- The v0 exit gate (#49) is unchanged — v0.13 is post-qualification hardening,
  not a qualification criterion. Nothing in the second track is a qualification
  criterion either; #271 is already a _disclosed_ limit under E3 rather than an
  unmet one.
- The 2026-07-19 item count in this record's original text (54) is superseded.
  The correction is not a re-scoping: 23 open issues carried no milestone at all
  and so were in nobody's count, 22 more were re-anchored from closed
  milestones, and five were stragglers on slices that had already closed. The
  count in #362 is the current one, and a milestone sweep for un-anchored issues
  is now part of the phase-end revision rather than assumed.
