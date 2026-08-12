# Debt streams own artifact classes, not crates

    status   accepted (2026-07-27, the v0.12-close plan revision) — binds how
             v0.13's three parallel burn-down streams are drawn and dispatched
             (epics #475, #438, #439 under #362)
    scope    the parallel-stream protocol for a debt slice; which stream may
             regenerate a committed golden, oracle frame, or byte-exact
             payload
    related  docs/decisions/pre-v1-hardening-slice.md (the slice this governs),
             docs/decisions/v02-flex-goldens-per-construct.md (the per-construct
             byte-golden rule), docs/decisions/dsb-frozen-fixture-r7-guard.md,
             docs/technotes/tolerance-band-coverage.md, epics #362,
             #475, #438, #439

## Context

v0.12 ran three parallel streams. The split was drawn by **what a branch owns**:
Stream A was the packer epic and the only stream allowed to regenerate a golden;
Streams B and C were crate clusters chosen to have no overlap with it. The rule
behind it is sound and is not in question here — a regenerated binary golden
does not merge, it collides, and the second session cannot tell whether its
regeneration is correct without re-deriving the first's reasoning. Two
concurrent re-baselines would destroy the attribution property that v0.11 was
spent building.

Two things about v0.12 made that split easy, and neither holds for v0.13.

**v0.12's territory was one crate cluster.** The packer was new code. Drawing a
line around `dashpack`, `dashbuf`'s format surface, `goldens/` and the profile
preview left two clean crate clusters outside it. v0.13's territory is fifteen
crates and every existing artifact in the repo.

**v0.12 could hold zero golden movement across the whole slice, and did.** All
nine stories moved zero committed bytes, verified per file with
`git hash-object`. That property came free from the sequencing: RAW is the null
binding, so a RAW assembly must produce identical bytes, and only one story held
a re-baseline permit — which it then did not need. A slice-wide assertion was
available because nothing in the slice was supposed to change output.

v0.13 is not like that. Several of its items exist **precisely** to change
output: a glyph run that never honoured a clip (#275), a folded group opacity
that double-blends (#277), a shadow silently dropped on a text node (#396), a
Hug row that mis-sums under negative margins (#270). Fixing any of them moves
pixels or rects by definition. Carrying v0.12's slice-wide assertion forward
would either block the fixes or reduce the assertion to a formality — and a
formality is worse than no assertion, because it reads as evidence.

Splitting by crate does not work either. `dashc` alone contains both kinds:
items that write a golden `.dsb`, re-bake an atlas, or re-author a fixture, and
items that only touch a diagnostic or a parse-side default. A crate is not the
unit that collides.

## Decision

**Streams are drawn around artifact classes.** A stream owns a class of
committed output, and no two concurrent streams own the same class.

- **Stream A owns the painter and every committed artifact** — rendered goldens,
  oracle frames, byte-exact `.dsb` and KTX2 payloads. It is the only stream
  permitted to regenerate one.
- **Stream C owns the runtime** — the solver, the typesetter, and the document
  they operate on — and with it the layout and rect assertions, and nothing
  else.
- **Stream B owns producers and vocabulary** and moves nothing committed at all.

An item's stream follows the artifact it can move, not the crate it lives in.
`dashc` splits across A and B on exactly that test. `#354` is typeset work that
touches `goldens/`, so it is A's — the ruling v0.12 already made about it,
generalised.

**The slice-wide zero-movement assertion becomes a per-story one.**

- The default is zero movement, **asserted rather than assumed**: every story
  checks `git hash-object` per file against `origin/main` and records the
  result. A green suite is not evidence that nothing moved.
- A story that moves an artifact **declares it before it starts**, lands alone,
  and records the reason and both measurements — the discipline that made v0.12
  catch its one moved measurement rather than absorb it.
- **The permit does not travel with the item.** A story in B or C that finds it
  must move a committed artifact stops and hands the item to A. It does not move
  it, and it does not acquire the permit by having discovered the need.

**Three streams remains the ceiling.** Review is the throughput bottleneck, not
free crates: every one of v0.12's nine stories had a real defect found in
review, several of which no test could have distinguished from correct
behaviour. Adding a fourth stream adds review load to the same reviewer, not
capacity.

## Alternatives considered

- **Carry v0.12's split forward unchanged.** Rejected because its Stream A was
  the packer epic, which is closed, and because the crate clusters it left over
  do not partition v0.13's territory — `dashscene-core` alone was held out of it
  entirely.
- **Keep the slice-wide "zero goldens moved" assertion.** Rejected: v0.13
  contains fixes whose purpose is to change output. The assertion would have to
  be waived so often that it would stop being read, which is the failure mode it
  exists to prevent.
- **Split by crate, and forbid golden movement outside one stream.** Rejected
  because a crate is not the unit that collides. `dashc` would have to be either
  wholly inside the artifact stream, which pulls four unrelated producer-side
  items with it, or wholly outside it, which puts four artifact-moving items in
  a stream with no permit.
- **Serialize every artifact-moving story into a single queue, streams otherwise
  free.** Rejected as the same thing with worse ergonomics: the queue _is_
  Stream A, and naming it a queue loses the worktree and territory isolation
  that keeps the other two from colliding on files.
- **Four or more streams, one per crate cluster.** Rejected on v0.12's measured
  limit — review, not crate availability, is what bounds throughput.

## Consequences

- v0.13's 93 burn-down items split 35 / 30 / 28 across A / B / C. The split is
  recorded in each stream's epic (#475, #438, #439), and the counts are in #362.
- Stream C is the second stream with a permit, which v0.12 did not have. The two
  permits are disjoint by class — A may not move a rect assertion, C may not
  move a rendered golden — so the no-two-concurrent-re-baselines property is
  preserved rather than relaxed.
- A layout fix in C can move a rendered golden downstream. That is the hand-off
  case, and it is why the permit is stated as non-travelling: C stops and hands
  the re-baseline to A.
- The protocol is written down here rather than only in issue bodies. GitHub
  issues might not have survived the move to the public name — a fresh push
  would have carried none of them
  (`docs/decisions/repo-staging-and-public-facade.md`), and the v0.12 protocol
  existed only in three issue bodies. The move turned out to be a rename and the
  issues survived, but that was not knowable when this was written, and a
  protocol held only in an issue body is fragile for other reasons anyway.
