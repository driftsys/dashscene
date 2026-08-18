# A slice is planned against its inflow, and anchoring is checked below the milestone

    status   accepted (2026-08-18, the v0.20 phase-end revision) — the
             retrospective for slice v0.20; binds how a slice is planned,
             how the phase-end anchoring sweep is run, and how the rolling
             debt milestone is read
    scope    docs/roadmap.md, AGENTS.md, the v0.21, v0.22 and v0.23
             milestones
    related  docs/decisions/pre-v1-hardening-slice.md (the anchoring sweep
             this extends)
             docs/decisions/review-before-ready-not-before-open.md (the
             finding-triage rule that produces the inflow)
             docs/decisions/debt-streams-own-artifact-classes.md (lane
             territory)

## Context — what v0.20 measured

Epic #951 planned **13 issues** across three tracks. All 13 closed. The
milestone closed at **154**, which counts 12 pull requests alongside 142 issues.
Sixteen of those 142 were filed before 2026-08-13, the day the epic was filed;
**126 were filed while the slice ran**, and 125 of the 126 inside the three days
2026-08-14 to 2026-08-16.

Repository-wide over the same window, **184 issues were filed**, measured on
2026-08-18 before this revision acted: 126 to v0.20, 25 to v0.21, 24 to v0.23, 7
to v1 and 2 to v0.19.

**No query reproduces those figures now, and that is worth stating rather than
papering over.** The revision itself filed four issues into the same window —
#1242, #1243, #1246 and #1247 — and moved two between milestones, #1149 from
v0.21 to v0.23 and #1241 from v0.23 to `v1`, so the total reads **188** as this
revision closes and the v0.23 and `v1` rows are not what they were; and the
breakdown is by an issue's **current** milestone rather than the one it had on
the day, so every later move rewrites a row retroactively. The query below gives
the shape and a total, not this reading:

    gh issue list --state all --limit 400 --json number,milestone \
      --search "created:>=2026-08-13"

That a plainly-stated measurement stopped being re-derivable within hours is the
same drift this record's decision 4 is about, arriving in the record that
describes it.

The slice was worked as concurrent lanes with per-file territory. Eighteen
driver prompts are archived under `docs/archive/2026-08-15-v020-wave2-*` and
`docs/archive/2026-08-16-v020-wave3-*`, and **those eighteen files are not
eighteen lanes**, and the set is partial besides. They carry thirteen lane
letters — D to N, P and S — because lane G has three prompts and H and I have
two each, and `phase-0-doc-link-gate` is a serialised gate rather than a lane at
all. `docs/archive/README.md` adds that lanes A to C ran earlier in the slice
with no prompt kept. So nothing in the tree establishes the lane count, and this
record does not state one. Two branches landed during it carrying a silent
revert of files they had never edited, and CI stayed green both times, because
the older content still compiled and still passed its own tests. **Only one of
the two pairs is this slice's own**: #1037 over #1038, both `debt/v020-*` lanes.
The other, #978 over #961, was a v0.19 story branch over a fix branch, merged
inside this slice's window. The cost is a property of many concurrent branches
rather than of one milestone, which is why it is recorded here. **The merge
button did not create either one**: `AGENTS.md` records that each deletion lives
in a **single-parent** commit, made by the hand-run re-parenting before the
merge, which is why the remedy is the squash order and not the merge method.

## Decision

### 1. A slice's epic states the inflow it expects, and the revision measures it

The gap between 13 planned and 142 closed is not a planning error to correct. It
is what `review-before-ready-not-before-open.md` produces by design: a finding
is fixed in the pull request that found it unless it is blocked, or long and not
a correctness defect, in which case it is filed. A slice that reviews its own
work therefore files issues at a rate set by the review, not by the plan.

What was missing is that nothing said so in advance, so the milestone read as
about eleven times its plan with no record of which part _was_ the plan. From
v0.21 an epic states its planned issue count where it states its tracks, and the
phase-end revision records the closing count beside it. Neither number gates
anything, and neither is a target; the ratio is the measurement this record
exists to keep.

**The v0.21 counts this rule is instantiated with**, so the next close has a
before-number rather than a reconstruction. All three already show the effect
this rule exists to make visible, because this revision added to each of them:

| epic             | planned when filed               | held on 2026-08-18              |
| ---------------- | -------------------------------- | ------------------------------- |
| #1106 — Unity    | 8 stories                        | 9 stories, plus #1226 as a gate |
| #1107 — hardware | 5 Track A items, Track B unfiled | 7, of which 4 are closed        |
| #1120 — not MVP  | 3 issues                         | 8 issues, no stories of its own |

#1107's Track B stays deliberately unfiled until #1106 has a host that runs; a
breakdown written now would be written against nothing.

### 2. The anchoring sweep extends below the milestone, to the epic and the label

`pre-v1-hardening-slice.md`'s 2026-07-19 correction made a sweep for issues
carrying no milestone part of the phase-end revision, after 23 issues were found
in nobody's count. v0.20 was planned after that failure recurred at 55.

The same failure was present at this close, one level down. **Nine** open issues
on v0.21 were named by none of its three epics: #1029, #1149, #1191, #1195,
#1215, #1226, #1232, #1235 and #1236. A milestone query finds them; an
epic-driven session does not. **Story #859 is not an instance of this failure
and is the argument for a third level.** Epic #1106 named it in prose as "the
first build step", so an epic sweep passes it. What it lacked at the v0.19
revision was a **label**, so it appeared in no `story` or `debt` listing — and
it was missing from that epic's story table, so the only thing naming it was a
sentence. An unlabelled issue is invisible to every query anyone runs, whatever
its milestone and whatever its epic.

**The sweep now checks three levels.** The first and third are repo-wide — an
issue with no milestone appears under none, and an issue with no label is
returned by no listing wherever it sits — and the second runs over the slice
milestones the revision revises, since epic membership is defined only where
there are epics. An open issue carries a **milestone**; an open issue on a slice
is **named by one of that slice's epics**; and an open issue carries a **label**
that some listing returns. At each level, an issue that is an exception on
purpose is said to be one — #851 is the standing case, a tracking issue that is
deliberately not a story and that epic #1106 says so about.

**Level 2 needs a command, because epic membership in this repository is
prose.** `issues/<epic>/sub_issues` is empty for all three v0.21 epics: an epic
names its stories in a Markdown table and in `Refs` lines, and amends by
comment. So the sweep reads the epic bodies **and their comments**:

    # a fresh directory: a stale epic file from an earlier slice masks
    # whichever issues it happens to name
    D=$(mktemp -d)
    for e in <this slice's epic numbers>; do
      gh issue view "$e" --json body,comments \
        --jq '.body + (.comments | map(.body) | join("\n"))' > "$D/epic-$e"
    done
    gh issue list --milestone "<slice>" --state open --limit 200 \
      --json number --jq '.[].number' |
      while read -r n; do
        grep -qrE "#$n\b" "$D" || echo "#$n is named by no epic"
      done

**It answers "is this number mentioned by an epic", not "does an epic own it".**
An epic's "Not on this epic" section names issues in order to disclaim them —
#1107's names #872 and #708 — so a disclaiming mention passes the check. Read
each hit rather than trusting the exit status; the cheap version is the right
default because the failure it catches is an issue mentioned **nowhere**.

Naming a step without giving a command is what decision 3 below exists to
correct, and it would have been the same defect here.

### 3. Lanes keep their shape, and the post-merge check moves into AGENTS.md

The lane shape is kept for v0.21. It closed 142 issues in six days, and the two
reverts have a known cause and a known detector.

`AGENTS.md` already carried the pre-push detector — the three-dot
`git diff --stat` against the merge base — and named the post-merge step as
"confirming the previous lane's work survives" **without giving a command for
it**. That command lived only in the lane driver prompts, which are written per
slice and archived with it. This record moves it into `AGENTS.md`, beside the
rule it belongs to, so the sentence above is past tense as of this commit.

### 4. The rolling debt milestone is read as a population at each slice close

v0.23 held 58 open issues when this revision began, and had never been read as a
whole. It holds **57** as this revision closes: two closed as already repaired,
#1149 and #1247 moved in or filed onto it, and #1241 moved out to `v1` — its own
body argues it is not a quick fix, which is that milestone's whole threshold,
and `AGENTS.md` says a long finding never goes there. That last one is the
**wrong-milestone-for-its-size** kind below: an item sized against the gate it
came from rather than against the milestone it landed on. Its own rule — one
focused pull request each, under half a day — is what hides the three things
only the population shows:

- **Items on the wrong milestone for their size.** #1241 was filed at the v0.20
  close as the remainder of a gate and placed on v0.23, whose threshold is half
  a day; its own body is headed "Why this is not a quick fix". Moved to `v1`.
- **Duplicates that later work had already repaired.** Issue #511, filed
  2026-07-27, was repaired by #1193 on 2026-08-16; #647, filed 2026-07-31, was
  repaired by #1186 the same day. Both are closed by this revision, and neither
  of the newer issues referenced the older one.
- **Items that are one statement, filed as several.** #1033 and #1060 both say
  that `dashscene-desktop` and `dashscene-ffi` duplicate each other, were filed
  on the same day, and **both already cite the same cause, #925** — so the link
  exists and the split survived it anyway. One pull request could take both; the
  milestone's own rule asks for two, and nothing in a per-item view says
  otherwise.

The sharpest **cluster** is **not** in this milestone, and that is worth saying
rather than borrowing it: #1029, #1191 and #1232 are all on v0.21, and they are
three successive reviews of `assert-drew.py`, each finding the next route by
which the Android painter's only automated witness passes a frame it should
refuse. Read one at a time they are three small script defects; read together
they are one statement about what that witness can say. They were found by
decision 2's epic sweep, not by this one, and the two passes look for the same
thing at different scopes.

So the phase-end revision gains a **cluster pass** over the rolling debt
milestone: group the open population by subject, and act on the groups.

**It is not a re-verification pass, and this record does not claim one was
run.** Four of the 58 were checked against the tree here — #511, #512, #647 and
#752 — and **all four are about `crates/dashscene-skia/src/lib.rs`**, where
v0.20 worked heavily and where a duplicate was therefore likeliest. (#752 is
titled against `dashpaint` and its body pins `lib.rs:301`, which is how it came
to be in the same sample.) Two of the four were already repaired, and that is
not a rate for the milestone — the sample was chosen to find duplicates. A large
part of the population asserts an **absence** — "no test covers X" — and an
absence is checkable only by mutating the code and running the tier, not by
reading it. A full re-verification of v0.23 is therefore a slice's work rather
than a step inside a revision.

## Consequences

- `AGENTS.md` gains the post-merge lane check under the merging rules, and its
  "Plan tracking" section gains all three of the ritual additions below —
  decision 1's planned-versus-closed count, decision 2's three-level sweep and
  decision 4's cluster pass. All three are in `docs/roadmap.md`'s ritual section
  too, because that is the file a revision reads first, but a rule that lived
  only there would be invisible to a session reading `AGENTS.md`, which is the
  failure this record is otherwise about.
- `docs/roadmap.md`'s v0.23 entry stops stating an `owner-input` count. It said
  two, from 2026-08-16; as this revision closes there are **ten** on that
  milestone and eleven in the repository, two of them put there by this revision
  — which is why the entry now prints the query instead of a number.
- v0.21's nine unanchored issues are placed, and **#1226 was raised to a gate on
  epic #1106** rather than sitting beside it: it asked whether the C ABI's
  runtime handle stays a raw pointer, which changes the signature of every entry
  point and so every P/Invoke declaration a C# host writes. **It was ruled on
  2026-08-18** —
  [`the-c-abi-runtime-handle-is-generational.md`](the-c-abi-runtime-handle-is-generational.md)
  — and #1226 now carries the build, ahead of #859 and #1121.
- v0.22's four items are all filed as issues. Two of them — the SVG vocabulary
  profile and the census harness, now #1242 and #1243 — were carried as prose in
  `docs/roadmap.md` and filed as no issue at all. That is a **fourth** way work
  goes missing, beside no milestone, no epic and #859's no label, and it is the
  only one of the four that no query can find, because there is nothing to
  query.

## Alternatives considered

**Cap the inflow by raising the filing bar.** Rejected here rather than
re-argued: it is the sweep-versus-file question, it is open on #1194, and pull
request #1076 was closed unmerged because the measurement behind its proposal
could not be re-derived. This record measures the rate and does not change it.

**Run v0.21 with fewer lanes to reduce merge risk.** Rejected. Both reverts came
from the squash base being named as a moving ref, which the merge order in
`AGENTS.md` already corrects, and neither was found by review — only by
comparing file counts. Fewer lanes would lower the rate of an error whose cause
is known and whose detector is one diff.
