# A review finding that is a sibling site of the fix's own invariant is swept, not filed

    status   accepted (2026-08-15)
    scope    the story workflow's handling of `/code-review` findings; slice
             v0.20's Wave 1 and Wave 2 lanes; issues #985, #986 and the 18
             findings their successors produced

## Context

The story workflow says to fix critical findings before merging and to file each
minor finding as its own `debt`-labeled issue rather than fixing it inline. That
rule was written for findings about the change under review. It does not say
what to do with a finding that is **somewhere else**: a second site of the very
invariant the fix has just established, which the fix did not reach because it
was scoped to one call site.

The question was raised on 2026-08-15 and deliberately parked for evidence, on
three grounds: the only sample was four pull requests written and classified by
one author; the sweep had no stated boundary, so "every site of the same shape"
across nineteen crates was as defensible a reading as "the neighbouring method";
and one sample of same-shaped changes could not show whether the answer
generalised.

Slice v0.20's Wave 2 supplied the evidence. Four lanes closed 20 issues across
`dashscene-gpu`, `dashpaint`/`dashscene-skia`/`dashscene-validator`, `dashc` and
`dashscene-android`, and their `/code-review` fan-outs filed 18.

## The measurement

Each of the 18 was read and classified from its own provenance line and opening
claim. Three categories, and they did not need inventing — they are the ones the
first sample produced.

**A — a sibling site of the invariant the fix had just established, reachable by
one grep. Eight of eighteen.** Issues #1044, #1045, #1047, #1048, #1050, #1055,
#1065 and #1074.

**B — a defect in the fix's own additions, or created by it. Five of eighteen.**
Issues #1040, #1041, #1043, #1049 and #1056.

**C — independent of the fix. Five of eighteen.** Issues #1042, #1046, #1064,
#1066 and #1067.

Three of the eight in category A are the earlier defect verbatim, one identifier
away:

- **#1045** — `ClipTable::push` and `ImageTable::push` convert an offset and a
  count to `u32` separately. That is issue #1014's defect, in two neighbouring
  methods of the file #1014 was fixed in.
- **#1055** — `LayerTargets::prepare` carries the release-and-rebuild thrash
  that issue #1020 had just removed from `BlurTargets::prepare`, its sibling
  type.
- **#1074** — `Atlas::image` is still `pub` and unchecked.

## The case that decides it

`Atlas` has had one fix applied four times, each round's review finding the next
field:

    #724   px_per_em           checked, then made private after review
    #983   distance_range_px   the same finding, the same fix
    #1001  width and height    the same finding, the same fix
    #1074  image               open

`boundary-b-domain-checks-sit-at-the-table-seam.md` already records the third
round as "the same finding a third time, and it closes the type". It did not
close the type: one public field remained, and the fourth round found it.

The sweep that would have ended this is one question asked once — _which other
fields of this struct are public and unchecked?_ — and it is answerable by
reading a single struct definition. Filing instead has cost four pull requests,
four reviews and four merges, and the type is still not closed.

## Decision

**A category-A finding is fixed in the branch that established the invariant. A
category-B finding is not debt at all — it is the fix, unfinished. Only category
C is filed.**

The boundary, which is what the parked question actually turned on:

> **the same invariant, in the same file and its sibling types** — one grep, not
> a survey of the workspace.

All eight category-A findings sit inside one crate, and six inside one file or
its sibling type. None would have required a wider sweep than that, and the rule
claims nothing about wider ones.

**Each pull request states which category its findings fell in.** That is what
makes the next data point cost nothing to take; reconstructing these eighteen
after the fact took a full pass over every issue body.

## What this does not change

- **Critical findings are still fixed before merging.** This decision is about
  where a _minor_ finding goes, not about severity.
- **A category-C finding is still filed as `debt` on a milestone** — the current
  slice, the next one, or `v1`. Debt with no milestone is invisible at every
  slice close.
- **The fix round is still reviewed.** `review-before-ready-not-before-open.md`
  and the story workflow both require it, and category B exists because that
  review keeps finding things: five of eighteen were defects in additions made
  during the review itself.

## Alternatives considered

**File everything, as before.** Rejected on the `Atlas` evidence: the same fix
four times is not a hypothetical cost. It also produces debt whose issue body
has to restate the invariant the closed issue already stated, which is how a
negated or stale claim enters the record.

**Sweep without a boundary — every site of the same shape, workspace-wide.**
Rejected as unbounded. "A payload cloned while the source drops" is dozens of
sites across nineteen crates; "which fields of this struct are public" is one.
The measured cases all fell on the narrow side, so the rule is written to the
narrow side and says so.

**Raise category A to critical.** Rejected: severity is about what the defect
does, and several of these are latent rather than live — #1001 had no reachable
divide-by-zero because a painter guarded it. Changing where a finding is fixed
does not require changing what it is called.

## What would falsify this

A lane whose category-A sweep balloons — where "the same invariant in the same
file" turns out to be twenty sites rather than two, and the pull request doubles
in size to hold them. None of the eight measured here was more than a few lines,
but none sat in a hot, widely-shared seam either.

**Record the sweep's size in the first pull request that does one**, so the
boundary is refined from a case rather than from an argument.

Related: `boundary-b-domain-checks-sit-at-the-table-seam.md`,
`review-before-ready-not-before-open.md`, `ci-green-before-story-merge.md`.
