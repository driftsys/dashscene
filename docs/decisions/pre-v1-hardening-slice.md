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

The 2026-07-19 debt-backlog triage swept every open `debt` issue and
re-anchored each to the slice where it next matters. A large cluster verified
as independent debt — perf and allocation micro-debt, cleanup, test-gaps, and
latent-correctness guards — with no near-term consumer, and was anchored to v1.

v1's scope is "Unity, full feature set, performance, production toolchain": a
large slice whose principal work is the Unity painter, the full feature set,
the rendering-performance pass, and the production toolchain. Debt anchored
there does not get a focused pass — it sits under those large deliverables and
never surfaces.

## Decision

The independent code-debt splits out of v1 into a dedicated pre-v1 hardening
slice, **v0.13** (milestone #14, epic #362): the items across the `dashcue`,
`dashlang`, `dashscene-core`, `dashscene-engine`, `dashscene-typeset`, paint,
goldens, and repo/importers clusters that are resolvable on their own — perf
and allocation, cleanup, test-gaps, and latent-correctness guards. They
parallelize by crate (one PR per cluster), and none is gated on a v1
deliverable.

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
owner can give, or an input only the owner can supply, it is not burn-down
work no matter how small the eventual edit.

Seven items meet it. Two are the shape the term was written for and are worth
naming, because both are specification gaps that were mistaken for code debt:

- **#462** — `dashpack` treats a profile exceeding the target memory budget as
  a validator error, and **no memory budget or target display resolution
  exists anywhere in `docs/specification/`**. A profile that cannot fail is
  not a contract, so the escalation ladder and the band contracts currently
  bind nothing. The fix is a number in the specification, and the number comes
  from measuring a representative target design file, which only the owner can
  supply.
- **#373** — the MSDF 14 px/em floor is enforced at import time against the
  authored size, while `docs/decisions/q1-msdf-below-14px.md` justifies
  MSDF-only text on sizes being static. dashscene animates, so a document can
  pass validation and still render below the floor mid-transition. That is the
  silent degrade **P4** exists to prevent, which makes it a principle-level
  ruling about where the check lives.

The other five are #422 and #412 (both change a pinned rule the render oracle
depends on), #446 (a docs judgement), #271 (an unruled layout semantics
question, disclosed under E3), and #105 (needs a Figma fixture parked with the
owner on #265).

Two further items are blocked but not on a decision — #263 and #82, both on
GitHub Actions billing — and sit on the milestone outside both tracks.

## Alternatives considered

- **Leave everything on v1.** Simplest, but conflates "unlocks with its
  consumer" with "resolvable debt", so the hygiene debt never gets a focused
  pass — the problem this decision exists to fix.
- **An unnumbered "pre-v1 debt" milestone outside the version sequence.** Keeps
  it off the v0.x numbering, but breaks the convention that every milestone
  maps to a roadmap slice.
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
- The v0.13 epic (#362) carries the full by-crate checklist and the
  out-of-scope rationale; the milestone (#14) holds the items.
- **v0.13 runs as two tracks**: the burn-down (#362, 93 items across three
  streams) and the decisions and owner-supplied inputs track (#474, 7 items).
  Two further items are blocked on Actions billing. The milestone holds 102.
- The v0 exit gate (#49) is unchanged — v0.13 is post-qualification hardening,
  not a qualification criterion. Nothing in the second track is a qualification
  criterion either; #271 is already a _disclosed_ limit under E3 rather than an
  unmet one.
- The 2026-07-19 item count in this record's original text (54) is superseded.
  The correction is not a re-scoping: 23 open issues carried no milestone at
  all and so were in nobody's count, 22 more were re-anchored from closed
  milestones, and five were stragglers on slices that had already closed. The
  count in #362 is the current one, and a milestone sweep for un-anchored
  issues is now part of the phase-end revision rather than assumed.
