# A review finding that is a sibling site of the fix's own invariant is swept, not filed

    status   accepted (2026-08-15)
    scope    the story workflow's handling of `/code-review` findings, measured
             over the nine pull requests that closed slice v0.20's parallel debt
             lanes: #1037, #1038, #1039, #1051, #1052, #1068, #1071, #1072, #1073

## Context

The story workflow says to fix critical findings before merging and to file each
minor finding as its own `debt`-labeled issue rather than fixing it inline. That
rule was written for findings about the change under review. It says nothing
about a finding that is **somewhere else**: a second site of the very invariant
the fix has just established, which the fix did not reach only because it was
scoped to one call site.

The question was raised on 2026-08-15 and parked for evidence, on three grounds:
the only sample was four pull requests written and classified by one author; the
sweep had no stated boundary, so "every site of the same shape" across nineteen
crates was as defensible a reading as "the neighbouring method"; and one sample
of same-shaped changes could not show whether the answer generalised.

The nine pull requests above supplied the evidence.

## The measurement

**Population.** Every issue whose body names one of those nine pull requests, or
one of their branches, as where it was found. That criterion is mechanical and
re-derivable; it deliberately includes issues found by a lane's own work as well
as by its `/code-review` fan-out, because the two are not distinguishable in
effect and separating them was where an earlier draft of this record went wrong.

**Denominator.** Each pull request records its own findings count:

    #1037  26 (three rounds)   #1051  10   #1071   8
    #1038  11                  #1052   9   #1072  14
    #1039  75 (seven passes)   #1068  12   #1073  13
                                                 ---
                                                 178

**178 findings produced 21 filed issues.** The other 157 were fixed in the
branch, or triaged as needing no change, before the pull request merged. **The
practice is already overwhelmingly to sweep**; this decision is about the
remaining 21, not about the 178.

Each of the 21 was read and classified from its own provenance line and opening
claim:

**A — a sibling site of the invariant the fix had just established, reachable by
one grep for that invariant's identifier. Eight of twenty-one.** Issues #1034
and #1044, #1045, #1047, #1050, #1055, #1065 and #1074.

**B — a defect in the fix's own additions, or created by it. Five of
twenty-one.** Issues #1040, #1041, #1043, #1049 and #1056.

**C — independent of the fix. Eight of twenty-one.** Issues #1036, #1042, #1046,
#1048, #1059, #1064, #1066 and #1067.

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

`Atlas` has had one fix applied four times:

    #724          px_per_em           checked, then made private after review
    #964, #966    distance_range_px   the same finding, the same fix (PR #983)
    #1001         width and height    the same finding, the same fix (PR #1073)
    #1074         image               open

**Round three is the one that matters, and not for the reason it first
appears.** Issue #1001's body opens: "Found by the sweep on the branch closing
issues #985 and #986, **before `/code-review` ran**." The sweep the decision
below prescribes had already found it. What failed was what happened next — it
was filed as `debt` rather than fixed in the branch that was editing that very
struct, so it cost its own pull request, its own review and its own merge two
days later.

That is the whole argument in one case. **The sweep works; filing its result is
what does not.**

`boundary-b-domain-checks-sit-at-the-table-seam.md` records round three as "the
same finding a third time, and it closes the type". It narrows that claim three
sentences later to "every divisor this type owns is now checked at its one
constructor", which is true — `image` is not a divisor, and #1074's concern is a
payload swapped out from under an extent check that only existed after PR #1073.
So round four is not something a sweep at PR #983 would have worded the same
way. The cost is nonetheless four pull requests, four reviews and four merges
against one struct.

## Decision

**A category-A finding is fixed in the branch that established the invariant. A
category-B finding is not debt at all — it is the fix, unfinished. Only category
C is filed.**

The boundary, which is what the parked question actually turned on:

> **the same invariant, at the sites one grep for its identifier reaches.**

That phrasing is derived from the cases rather than chosen: `field_draws` for
issues #1034 and #1044, `forget_uploaded` for #1050, the `u32::try_from` pair in
one file for #1045, the struct definition itself for #1074. It is **not** "the
same file" — an earlier draft said that, and #1044 falsifies it, since the
sibling of a `dashscene-gpu` predicate is in `dashscene-skia`. It is also not
"every site of that shape in the workspace"; see the alternatives below.

**Each pull request states which case each finding fell in, and — where it
sweeps — how many sites the sweep reached.** The second half is what makes the
falsification test below evaluable.

## What this does not change

- **Critical findings are still fixed before merging.** This is about where a
  _minor_ finding goes, not about severity.
- **A category-C finding is still filed as `debt` on a milestone** — the current
  slice, the next one, or `v1`.
- **The fix round is still reviewed.** Category B exists because that review
  keeps finding things: five of twenty-one were defects in additions made during
  the review itself, and PR #1039 needed seven passes.

## Alternatives considered

**File everything, as before.** Rejected on the `Atlas` evidence: the same fix
four times against one struct is a measured cost, not a hypothetical one. Filing
also produces a `debt` body that restates the invariant the closed issue already
stated, which is one way a stale or negated claim enters the record.

**Sweep without a boundary — every site of the same shape, workspace-wide.**
Rejected as unbounded. "A payload cloned while the source drops" is dozens of
sites across nineteen crates; "which fields of this struct are public" is one
definition. Every measured category-A case fell on the narrow side, so the rule
is written to the narrow side and claims nothing about the wide one.

**Raise category A to critical.** Rejected: severity describes what a defect
does, and several of these are latent — #1001 had no reachable divide-by-zero
because a painter guarded it. Changing where a finding is fixed does not require
changing what it is called.

**Do nothing, because 157 of 178 findings were already swept.** This is the
strongest objection, and it is why the decision is written as a clarification
rather than a reversal. The 21 filed issues are the residue where lanes were
uncertain, and the `Atlas` chain shows the residue is not randomly distributed:
it collects on exactly the sites a later round then has to pay for.

## The cost this creates, and it has already fired

A rule that widens a branch into sibling sites widens the files it touches, and
parallel lanes then collide. **This is not speculative.** On 2026-08-15 PR #1037
carried a reversion of PR #1038 across seven shared `dashpaint`,
`dashscene-skia` and `dashscene-validator` files, silently putting the #1000
painter divergence back on `main` while #1000 read closed; PR #1063 existed only
to restore them.

A lane that sweeps **states the files its sweep added to the diff in the pull
request body**, so the next lane's rebase has something to check against. The
post-merge check in the story workflow — diff the previous lane's files against
`main` — is what catches it when that fails.

## What would falsify this

A sweep that balloons: "the same invariant, one grep away" turning out to be
twenty sites rather than two, so the pull request doubles in size to hold them.
None of the eight measured here was more than a few lines, but none sat in a
hot, widely-shared seam either.

The rule above requires each sweeping pull request to record **how many sites it
reached**. Three such numbers are enough to say whether the boundary holds or
needs narrowing again.

Related: `boundary-b-domain-checks-sit-at-the-table-seam.md`,
`review-before-ready-not-before-open.md`, `ci-green-before-story-merge.md`.
