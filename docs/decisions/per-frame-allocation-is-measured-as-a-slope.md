# Per-frame allocation is measured as a slope, in bytes

    status   accepted (2026-08-16); **AS-BUILT the same day (v0.19, story
             #944)** — the term, its guard clause and the four measured
             steps that moved it shipped together. Measured 69 bytes per
             extra root against the unchanged commit and **17** against the
             one this story left.
    scope    the third term of the per-frame scaling criterion,
             `goldens/tooling/tests/per_frame_scaling.rs`, and how any later
             per-frame allocation cost is stated in this repository.

## Context

`docs/decisions/startup-scaling-is-measured-by-a-counter.md` D1 rules that in
this repository "a cost with no visible symptom needs a counter, not a
stopwatch". The per-frame band applied it and took two counters: Taffy layout
computations, and committed rect rows. Story #838 moved both from 65 to 1 over a
sixty-five-root document, and both have read 1.00x since.

**Neither can see what a commit allocates.** Story #838 confined the solve, the
committed table and the paint to the shown root and left the commit's own
per-node scratch sized by the document: nine vectors of sixty-five entries, and
a carry-forward loop over every node in the document, to produce a one-row
table. Both terms read 1.00x throughout. Issue #944 recorded that the band was
therefore "a true statement about the two costs it names and not about per-frame
cost in general", and required a third term as a condition of fixing the vectors
at all — a change with no term that can see it is unfalsifiable.

Issue #944 offered "an allocation count or a bytes-touched count" and did not
choose. Three measurements chose.

## Decision

**D1 — bytes, not an allocation count.** The count is blind here. A steady-state
layout frame makes exactly **21 allocation calls** over the one-root,
seventeen-root and sixty-five-root documents alike; only the sizes differ,
because what scales is the length of vectors the commit allocates once each. An
allocation-count term would have read 1.00x before and after the work it exists
to price.

**D2 — the layout frame, not the paint-only frame.** The paint-only frame's byte
count moves across repeats over one unchanged document — 884, 884, 1284, 1172 on
the one-root document — because the paint table's own interning and its
pooled-entry compaction (issue #197) move with it, and neither is a
document-scaled cost. The layout frame repeats bit-identically at every document
size.

**D3 — a slope, where the other two terms are levels.** The term is

    (many-root layout bytes − one-root layout bytes) / extra roots

and what is **asserted** is that difference against
`the constant × extra
roots`, not the quotient against the constant. Dividing
first truncates: over sixty-four extra roots, up to sixty-three bytes of new
document-scaled cost divides away and the band stays green, which is the failure
this term exists to close. The quotient is derived for the message and the log.
The two count terms are counts of work and a level states them exactly. A byte
**level** would also hold every fixed allocation a frame makes — the rect table,
the dirty sets, the solver's own per-frame working memory — and would move on
any dependency bump that added one, with nothing about the cost this term is for
having changed. The slope cancels every fixed cost and leaves exactly what grows
with the document, which is the claim being made.

This is the one place this repository states a measured criterion as a
difference rather than as a value, and D3 is the reason: it is the only term of
the three whose level is contaminated by costs that are not the subject.

**D4 — the instrument is a `#[global_allocator]` in the test binary, with a
const-initialised thread-local counter.** It lives in `per_frame_scaling.rs`
itself and never in `goldens/tooling/tests/common/`, which eighteen test
binaries compile (issue #932). The counter is thread-local because CI runs this
binary under `cargo test` (`.github/workflows/ci.yml`), which runs a binary's
tests as threads in one process; a global counter would report this frame plus
whatever the other two tests were allocating. Under nextest, which gives each
test its own process, the two would be equivalent — the suite is run both ways,
so the weaker assumption is the one held.

**D5 — the term states a residue rather than rounding it away.** It reads 17,
not 0. What remains is `dashscene-engine`'s per-frame scratch, named in the
constant's own documentation and carried by issue #1111. A band that reported
zero by measuring only the crate under change would be the same defect the term
was added to correct.

## Consequences

- The band has three terms, and `within_band` breaches on any of them.
  `the_confinement_is_what_makes_the_number_one` requires all three to move when
  the shown root is cleared, so the third term has the same committed upward
  injection the other two have and could land in the same change as the fix it
  measures without being inert.
- **The guard's byte figure is 136, not the 69 the story started from, and that
  is not a regression.** An unconfined commit really does draw sixty-five
  artboards, so the rect table and everything sized by rect rows grows with it.
  Those are costs the confined commit avoids by drawing less, not scratch it was
  wasting. A reader comparing the guard's number to the before-number is
  comparing two different things, which is why both are written down.
- **A level moving in the small case is not a breach, by construction.**
  Bounding `dfs_order`'s reserve took the one-root document's layout frame _up_
  by 8 bytes, because an unreserved `Vec` growing to one element takes a larger
  minimum allocation than a one-element reserve, while taking the
  sixty-five-root document's from 4705 bytes to 1385. A level would have had to
  argue about that trade; the slope does not see it.
- Anything later that adds a per-frame allocation sized by the document — in the
  commit, the engine, or a host — moves this term and fails the build, wherever
  it is written.

## Alternatives considered

- **An allocation count**, as issue #944 suggested. Ruled out by D1's
  measurement rather than by preference.
- **An absolute byte level**, matching the other two terms' shape. Rejected
  under D3: it churns on dependency bumps that have nothing to do with the cost.
- **A counter inside `dashscene-core` reporting the scratch it sized**, in the
  shape of `TaffySolver::solves()`. Rejected: it would report what the code says
  about itself rather than what it did, and would go quietly stale the moment a
  tenth vector was added without being counted — which is exactly how the ninth,
  `carried_out`, went unnoticed in issue #944's own list of eight.
- **Widening story #944 to `dashscene-engine` so the term could read zero.**
  Rejected for that story: a second crate, outside the issue's subject, in a
  file parallel stories are known to collide on. The residue is filed as #1111
  and named in the band instead (D5).
