# wip

Working memory: the spec and plan produced by a Superpowers session while
work is in progress. Transient by design — when a session's work lands and
its durable records are written into `docs/specification/`, `docs/design/`,
`docs/decisions/`, or `docs/technotes/`, the raw spec and plan move to
`docs/archive/` rather than being deleted.

Tracked in git (collaborative mode) rather than gitignored, per this
project's convention.

See the `sdd-working-memory-lifecycle` rule and the `sdd-gardening` skill.

## Why the WIP gate currently reports thirteen files

`wip-gate.sh` flags every tracked file here except this README, so it reports
thirteen and exits non-zero. All thirteen are deliberate, accepted exceptions
rather than ungardened debt, and they are recorded here so the gate's result is
explained rather than merely tolerated.

**The gate is deliberately not wired into CI, and this section is why.** Its
result here is non-zero by design and would stay non-zero for as long as any
forward-looking capture is held, so a CI job running it would be permanently
red — which trains a reader to ignore it, the opposite of a gate. The
`sdd-working-memory-lifecycle` rule that ships `wip-gate.sh` describes a
directory holding one finished session's working memory; this directory is
also a standing shelf for captures whose work has not started, which the same
rule requires ("forward-looking concepts stay in `docs/wip/` until implemented
and gardened in"). The gate cannot tell those two populations apart. What holds
the line instead is this file: every tracked entry is listed below with the
condition that empties it, and an entry that appears here without such a line
is ungardened debt.

**This count has been wrong before.** It read "seven" from the v0.13 close
until 2026-08-02, while eight files were tracked — the v0.14/v0.15 breakdown
was added and never listed. It went from seven to eight again later the same
day, when the v0.15 driver prompt was added — since archived, and described
below as one of the fifteen. Issue #663 was then filed against the count rather
than against this ledger, and asserted eight ungardened files when seven were
accounted for here. It went from eight to nine on 2026-08-05, when v0.16's
driver prompt was added while v0.15's was still held. It went back to eight on
2026-08-07, when epic #594 closed and v0.16's prompt was archived. Both of the
next two transitions fell on 2026-08-08: to nine when v0.17's prompt landed
(`0ab6547`, and note it is _named_ for the 2026-08-07 planning session that
wrote it, not for the day it was committed), and back to eight when epic #793
closed and that prompt was archived. **The two prompts never overlapped**, which
the file names alone would suggest they did. It went back to nine on 2026-08-09,
when v0.18's prompt landed — written on 2026-08-08 against a `main` that moved
underneath it, so its first revision counted ten and had to be re-derived on
the rebase. It reached a real ten later that same day, when
`2026-08-09-svg-as-a-second-producer.md` was added, and that addition read this
paragraph and updated the count in the same commit — which is what the rule at
the end of this section asks for.

**It then went stale on two additions in a row, and both were caught only at a
merge.** The second v0.18 driver prompt took the tracked count to eleven later
on 2026-08-09 and updated the paragraph below to read "nine of the eleven",
leaving this heading saying ten. The `FINISH-771` prompt took it to twelve the
same day and updated that paragraph again, to "nine of the twelve", leaving the
heading saying ten a second time. Story #771's merge archived the `FINISH-771`
prompt, re-derived the count, and set both to eleven. **The heading and the
paragraph below it are two copies of one number**, and an addition that edits
only the nearer copy leaves the gate's own explanation wrong — which is the
same failure as the archiving one above, reached from the other direction.

**Twelve, since `integration/v0.19-android` merged into `main`**, which brought
v0.19's Android driver prompt in beside the two v0.18 ones. That is the first
time **three** prompts are held at once and the first time **two slices'**
prompts overlap — which the paragraph below now says rather than repeating that
they never do.

It is also the second time the count moved through a **merge** rather than
through a commit that added a file, and both times the two branches had each
updated this paragraph correctly for themselves and neither number survived the
join. A count that only an addition can invalidate is one this ledger can hold;
a count two branches can each move is not, which is the argument for
re-deriving rather than editing. Re-derive the
number from `git ls-files docs/wip/` when touching this section rather than
trusting the prose.

**It went wrong once more on the same day, and the shape is worth naming
because it is the one this ledger is least able to catch.** The v0.15 close
(`2ae4326`) archived that slice's driver prompt and corrected the backdrop-blur
capture's status line, and did not touch this file — so this section still said
the gate reports nine when nine files were tracked and only eight were flagged,
and the paragraphs below claimed two driver prompts were held when one had just
left. **Tracked and flagged differ by exactly this README**, which is the
distinction the sentence above defines and the one the stale count lost.
Nothing here went stale through neglect; it
went stale because a commit that removes a file from this directory has to edit
this file in the same commit, and that is not enforced by anything. **Archiving
a capture and updating this ledger are one change, not two.**

**Nine of the thirteen are design captures**, described below, and **four
driver prompts are held**, across two slices.

Two are v0.18's: `2026-08-08-v018-DRIVER-PROMPT.md`, added the day v0.17 closed,
and `2026-08-09-v018-DRIVER-PROMPT.md`, which supersedes it and carries the
slice from issue #617 onward. The first is kept rather than replaced because its
story #770 material is now as-built and its gate section records why the slice
could start at all; the second is what a session should be handed for the slice
as a whole. Both archive together when epic #769 closes. A third was held for one
day — `2026-08-09-FINISH-771-DRIVER-PROMPT.md`, narrower than either, carrying
only what remained of story #771 — and left with the pull request it carried.
A third v0.18 prompt is held now: `2026-08-09-773-LOWERING-DRIVER-PROMPT.md`,
narrower again — it carries only story #773's lowering half, after pull
request #882 landed that story's capture half, and it archives when #773
closes.

The other, brought by an earlier merge, is v0.19's:
`2026-08-09-v019-ANDROID-DRIVER-PROMPT.md`, added at the close of that slice's
C ABI work to carry stories #841 and #842. **Both of those have landed**, and it
is still held rather than archived: its own status line says it archives at
v0.19's close, and that slice is open — D3a (#885), split-screen (#874) and
story #842's frame-rate number all wait on hardware. It archives when epic #833
does.
The two slices' prompts overlap because v0.18 and v0.19 ran at the same time, on
`main` and on `integration/v0.19-android` respectively; every earlier pair did
not.

Driver prompts — the brief a session is handed to carry out a named
piece of work — are transient by construction, spent the moment their work
lands, and the convention is to archive them verbatim rather than garden them
into records. Sixteen are in `docs/archive/`: fifteen `*-DRIVER-PROMPT.md`
plus the one `*-SPIKE.md`, which is counted there because it was archived
verbatim beside the prompt that carried out its design rather than gardened.

**The v0.17 prompt left when epic #793 closed** (2026-08-08), archived verbatim
as `docs/archive/2026-08-07-v017-DRIVER-PROMPT.md` on the same rule every prompt
before it followed. It held no decision and no design: it was current state, the
story order, the per-story loop, and the failure modes this repo has actually
hit — so there was nothing in it to garden into a record, and everything in it
went stale the moment the slice did. Its own `status` block said the same, and
said that removing it and editing this file are one commit, which is how it was
done. It was amended twice on 2026-08-08 rather than rewritten — first with the
four rulings that unblocked the slice, then again after five pull requests had
landed — and the archived copy carries that amendment above a body left unedited
on purpose, so what was known when it was written is still legible. By the close
its story states were stale and its traps section was not, which is why
story #796 was told to archive it rather than to garden anything out of it.

**Both v0.18 prompts empty when epic #769 closes**, on the same rule, and are
archived verbatim together. The 2026-08-09 one was written after three stories
closed rather than amended onto the first, because the first's story order and
its "where the slice stands" table had both been overtaken — amending would have
left a body whose two halves disagreed, where two files leave what was known at
each point legible. The earlier one carries a `status` line saying it is
superseded and by what.

The 2026-08-08 prompt empties on the same rule. It was written while v0.17 was one story from closing, because
v0.18's only remaining condition was that close — the slice has no technical
dependency on v0.17, and story #796 was the phase-end revision `AGENTS.md`
requires before the next slice starts. That story landed hours later, so the
prompt's gate section describes a condition already discharged and says so.

Five of its findings came from checking epic #769's own issue bodies against
the code, and **three of those bodies were wrong**: the `Prop` variant count is
37 rather than 34, stated wrongly in both epic #769 and story #770, and
story #773's premise that the importer discards Figma `reactions` names
something no captured fixture has ever carried. Counting the two _errors_
rather than the three _bodies_ is how an earlier revision of this paragraph
said "two", which is the same class of mistake as the count above it.

## The three animation captures, added 2026-08-07

They come from one design discussion and are split by concern rather than by
slice, because their dependency order is not their subject order:
`2026-08-07-motion-in-the-document.md` blocks
`2026-08-07-animated-content-import.md`, and
`2026-08-07-asset-sourcing-and-residency.md` is independent of both.

All three are forward-looking — each says "Nothing here is implemented" in its
own `status` line — so the same rule that holds the others here holds them:
gardening runs after tests are green by definition, and there is no as-built
code to reconcile them against.

Two things about them are worth stating here rather than only in the files.
**They extend `docs/technotes/runtime-content.md` rather than competing with
it**: that note already fixed the three-bucket triage and chose ThorVG, and the
import capture records only what a later session found on top of it. And **each
claim in them names where it was checked** — a five-agent review of the branch
that added them found eight claims that were not, including a variant count
derived twice with the same flawed command, so the repeat read as confirmation.

**The v0.16 prompt left when epic #594 closed** (2026-08-07), archived
verbatim as `docs/archive/2026-08-05-v016-DRIVER-PROMPT.md`, on the same rule
every prompt before it followed: it held no decision and no design — it was
current state, the story order, the per-story loop, and the failure modes this
repo has actually hit — so there was nothing in it to garden into a record, and
everything in it went stale the moment the slice did. Its own `status` block
said the same. It was rewritten once mid-slice, on 2026-08-07, because two
stories had landed and almost everything specific in it had become either done
or wrong; the archived copy is that revision.

**Holding two prompts at once is ordinary rather than exceptional**, and the
history says so plainly: `git ls-tree -r --name-only <sha> -- docs/wip/`
returns two `*-DRIVER-PROMPT.md` files at `dca9ec7`, `a99f3b3`, `33363f3` and
`902a4a3`, and two of those commits name the plural in their own messages
("track the **two** driver prompts for the next v0.11 sessions", "driver
**prompts** for the vector-blur and t2 handoffs"). It happened again from
2026-08-05, when v0.16 opened while epic #569 was still open, and ended the
same day when v0.15 closed and its prompt was archived. It nearly happened
again on 2026-08-08: v0.18's prompt was written while epic #793 was still
open, and the epic closed before that prompt landed, so only one is held.
An earlier revision of
this paragraph claimed that overlap was a first. It was not, and the paragraph
above already says why that happened — **re-derive from `git ls-tree` and
`git ls-files` rather than trusting the prose**, and that applies to a claimed
"first" exactly as it applies to a count.

One of the fifteen is `2026-07-27-t2-checks-that-cannot-fail-DRIVER-PROMPT.md`, v0.13's
`t2-check-has-no-teeth` tier — 19 items whose common property was an assertion
that could not distinguish right from wrong. It was archived on 2026-08-02, when
epic #362 closed.

**One item under that label, #499, was still open when it was archived, and the
milestones say why that is correct rather than an oversight.** The prompt's own
instruction was to archive it "when the tier is burnt down". The v0.13 milestone
holds 109 issues and none of them is open; #499 is not in it, having been moved
to the v1 milestone. The tier is burnt down, and #499 was re-scoped out of the
slice rather than left behind in it.

Issue #499 keeps the `t2-check-has-no-teeth` label because the label names the
kind of defect, not the slice that found it. It is also not a bookkeeping
entry: for the
corpus Arabic font, output is byte-identical with ligatures on and off, because
the lam-alef ligature is **required** and comes from `rlig`, which
`ligatures_off` does not disable. No assertion over this corpus can tell the two
settings apart, so the limitation is in the fixture rather than the test. It
closes when a font carrying an Arabic `liga` or `clig` ligature the corpus text
triggers is added under the corpus font rules — which is v1 work, and is why the
issue sits in the v1 milestone rather than being closed.

**Two are forward-looking design captures for work that has not started** —
`2026-07-27-glyph-coverage-sets-and-text-residency.md` and
`2026-07-27-indic-script-support.md`. Both say so in their own `status` line:
"Nothing here is implemented". Gardening runs **after** tests are green by
definition, so there is no as-built code to reconcile either against;
promoting one now would put a plan into `docs/design/` describing a system that
does not exist.

**The other three are partly gardened**, which is the honest state for a
capture spanning several slices, and each `status` line says which half is
which. They are described one at a time below.

**The two wgpu captures are no longer here**, and the distinction that decided
when they left is the one this directory has applied throughout: a decision
landing and a design being **built** are two events, and only the second empties
the file — the same rule the glyph-run spike was held to.
`2026-07-19-wgpu-painter-direction.md` and
`2026-07-29-v014-v015-showcase-and-wgpu-wbs.md` both originally said no crate
existed and nothing was implemented; those sentences went stale on 2026-08-02
when the breakdown was filed as epics #568 and #569, v0.14's showcase runtime
landed, and story #577 created `crates/dashscene-gpu`. Being chosen was not the
reason to garden them, and their `status` lines were corrected rather than the
files moved.

The second event happened on 2026-08-05: **epic #569 closed with the whole v0
paint vocabulary drawing through the lean painter**, native and web. The
as-built record is `docs/design/dashscene-gpu.md` — the instance buffer as the
painter's output, the four-storage-buffer wall that shaped the paint heap,
atlas residency, layers and the backdrop blur, the four-layer verification net,
and the limits stated with the issue that carries each. `docs/design/README.md`
and the crate map in `docs/design/architecture.md` point at it, and both files
are archived verbatim with their stale claims corrected in their own `status`
blocks rather than in the text.

**Three claims in them were wrong in a way worth carrying forward**, because
each was a prediction the slice disproved. The direction note's "most
repo-specific risk" was that a wgpu painter would not pixel-match Skia and the
oracle would need per-painter bands: measured, one band set serves both and
zero goldens moved. The breakdown asked for a WebGL2 fallback: it is not
buildable for this painter, because `downlevel_webgl2_defaults` allows zero
storage buffers per shader stage and the design is storage-buffer tables. And
the note pinned a helper stack of eight crates, of which three were adopted.
A capture is a record of what was believed, and what it got wrong is the part
that teaches.

One reading note that outlives them: the breakdown says `dashscene-wgpu`
throughout, and the crate is `dashscene-gpu`, named for the role rather than
the backend. The name was settled after the breakdown was written, and the text
is corrected in its `status` line rather than rewritten, so the file still
records what was actually planned.

Backdrop blur landed in v0.11 (story #393), so
`2026-07-19-backdrop-blur-v011.md` is spent in its decided half — the reversal,
the contract shape and all four of its open questions are now in
`docs/decisions/backdrop-blur-is-core-vocabulary.md`, and its own `status` line
says so. What is left, and stays here, is the unbuilt part of its per-painter
capability table — **Unity's row alone**, since the lean painter's row was
built by story #733 and the tiny-skia web painter was retired by story #588 —
and two of its three quality levers (dual-Kawase downsample,
re-blur cadence); the table's Skia row, previously stale the other way round
("unwired"), is corrected now that PR #403 wired the capability. A partial
gardening, the same shape as the asset-pipeline capture below — done at the
same pass, **#427**.

The photorealistic-3D capture is **partly gardened** as of 2026-08-05, and its
own `status` line carries the detail. Three of the five questions it traces are
ruled on: the profile bands, by issue #455's four real assets in `corpus/photo/`
— which made LoFi's budget the binding term on real committed content and
needed no retune; the ladder's fine end, by the same work, since two of those
assets terminate at astc-4x4 and astc-5x5 and every rung is now some fixture's
terminal choice; and the painter's working colour space, settled by measurement
on 2026-07-30. The memory budget (#462) and whether ASTC remains the right
family are open, both in the v1 milestone, and the file stays for them.
Issue #553 is evidence for the second rather than an answer to it.

The asset-pipeline capture is **partly gardened**: v0.11 and v0.12
each built the parts of it their scope covered, those parts are now as-built
records, and its `status` line says which and where. A partial gardening is the
honest state for a capture that spans several slices. The rule's own words are
what force it: "forward-looking concepts stay in `docs/wip/` until implemented
and gardened in", and gardening "runs after tests are green by definition".
Promoting the unbuilt half now would put a plan into `docs/design/` describing a
system that does not exist; archiving the whole file would lose it. The rule
models gardening as one atomic move because a capture spanning several slices is
a case it does not name — the reading here is that the _implemented_ half is
gardened, and the file leaves `docs/wip/` when its last half does.

Its `status` line named four residuals gated on the packer as of the v0.12
close; epic #345 has since closed. The profile-preview oracle was already
gardened into `docs/decisions/profile-preview-decodes-in-the-loader.md`, and
cold-bank assembly is now gardened too, into
`docs/design/dsb-container-format.md` and
`docs/decisions/derivation-manifest-section.md` (stories #433, #434). What is
genuinely still unbuilt is the vector bake's end-state fork and animated
content, neither scheduled to a slice yet, which is why the file stays.
Reconciling the line — and the backdrop-blur capture's above — is the
**#424 / #427** gardening pass this paragraph now records as done.

The colour-space capture, `2026-07-19-color-space-blur-and-msdf.md`, is **no
longer here**. It stayed while one genuinely open question in it stood —
whether the reference painter should blend blur in linear light or in
sRGB-encoded space — because issue #412 named that question as the dependency
its own sigma retune was blocked on. It was settled by measurement on
2026-07-30 and the file is archived verbatim: blur blends in sRGB-encoded
space, which is what Figma does, recorded in
`docs/decisions/blur-blends-in-srgb-encoded-space.md`.

Its last paragraph is worth keeping in view here, because this section is
where the cost of leaving a capture in place is paid. The question was held
for two slices on the capture's own sentence that a backdrop-blur frame over
multi-coloured content did not exist. That was true the day it was written and
false six days later, when story #393 committed exactly such a frame — and
nothing re-read it. A capture that sits here is not inert; its stale claims get
copied forward as fact. The other half of **#424**.

The glyph-run design spike is **no longer here**, and the reason is the
distinction this section draws throughout. It was partly gardened from
2026-07-28, when its decided half became
`docs/decisions/glyph-runs-cross-boundary-b.md`; the design it also carried —
the seam's shape, what a run carries, the ordering rule, the dirty-set change,
the per-frame cost question and the migration count — stayed, because a
decision landing and a design being **built** are two events and only the
second empties the file. The second happened on 2026-07-29: story #542 built
the commit-time seam and the anchor field, issue #275 the clipping and
issue #274 the z-interleave, and the as-built records now carry all of it
(`docs/design/dashscene-engine.md`, `docs/design/dashscene-skia.md`,
`docs/design/dashpaint.md`, `docs/design/dashscene-core-arena.md`, and the
decision record's two resolution sections). Both it and the driver prompt that
carried out its design are archived verbatim in `docs/archive/`.

One driver prompt is held here now, v0.18's, as the section above says. This
paragraph said "none" from 2026-08-07, when v0.17's prompt was added and this
sentence was not touched, until that prompt was archived a day later and the
sentence became accidentally true again. It is the same shape as the count
above it, and for the same reason: adding a file here and editing this file are
one change, and nothing enforces it. The most recent one archived is
`2026-08-05-v016-DRIVER-PROMPT.md`, on 2026-08-07 when epic #594 closed; before
it `2026-08-02-v015-DRIVER-PROMPT.md`, on 2026-08-05 when epic #569 closed;
before it `2026-07-27-t2-checks-that-cannot-fail-DRIVER-PROMPT.md`, on 2026-08-02 when
epic #362 closed; before that `2026-07-30-blur-colour-space-DRIVER-PROMPT.md`,
when the blur colour space was settled, and before that
`2026-07-27-glyph-runs-from-commit-SPIKE.md` and
`2026-07-29-glyph-runs-from-commit-DRIVER-PROMPT.md`, when the glyph-run
producer chain landed.

**The blur-colour-space prompt never entered this directory, and that is
deliberate.** Its work was already done when it was archived, so staging it
here first would have added a file to the gate for the length of one merge and
changed the count twice for no reader's benefit. A driver prompt whose work has
landed goes straight to `docs/archive/`.

Read that one as a historical brief rather than as a description of the
repository. **Its central premise was false** — it directs a session to author
a multi-coloured backdrop-blur fixture "which does not exist yet", and one had
existed since story #393 committed the `backdrop-blur` frame on 2026-07-26. It
is kept verbatim anyway, as every archived driver prompt is, because the
archive records what was actually asked for rather than a corrected version of
it. What was actually true is in
`docs/decisions/blur-blends-in-srgb-encoded-space.md`.

| capture                                                | gardened when                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2026-07-19-asset-pipeline-profiles-and-baking.md`     | partly gardened at the v0.11 close (epic #344) and again across v0.12 (epic #345, stories #432, #433, #434, #435, #436); the rest when the vector bake's end-state fork and animated content are built — its `status` line says which half is which                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `2026-07-19-backdrop-blur-v011.md`                     | partly gardened at the v0.11 close (story #393): the profile-policy reversal, the schema `Effect` representation, and the boundary-B contract now live in `docs/decisions/backdrop-blur-is-core-vocabulary.md`, and gardened further at the v0.15 close (story #733) — the lean painter's row is built, at `docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`, and tiny-skia web is retired (story #588). The rest when Unity's row is built, or when a constrained painter needs the two remaining quality levers — dual-Kawase downsample and re-blur cadence                                                                                                                         |
| `2026-07-28-photorealistic-3d-content.md`              | the remaining two questions are ruled on. Partly gardened 2026-08-05: the profile bands (#455), the ladder's fine end and the painter's working colour space are answered; the memory budget (#462) and whether ASTC remains the right family are open, both in v1. It records an input rather than a plan — photorealistic renders are target product content, and every number in the asset pipeline was chosen against content that is not representative of it                                                                                                                                                                                                                                          |
| `2026-07-27-indic-script-support.md`                   | Indic support is designed: the closure becomes text-driven and the unformed-cluster fallback is built. Its decided half — coverage is declared at build time, dynamic generation is a deferred painter capability — is already gardened into `docs/decisions/glyph-coverage-is-declared-at-build-time.md`                                                                                                                                                                                                                                                                                                                                                                                                   |
| `2026-07-27-glyph-coverage-sets-and-text-residency.md` | glyph-atlas residency is designed: the unit of residency is chosen and the runtime-supplied-string case is answered. Its decided half — that only raster is block-compressed — is already gardened into `docs/decisions/compress-raster-only.md`                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `2026-08-07-motion-in-the-document.md`                 | the vocabulary is **built**, not when it is decided. It holds three gaps — a rotation channel, motion rows in `dashbuf`, and loop tracks — and the rejected wasm-expression alternative. **All three closed on 2026-08-09** (stories #770, #771, #772) and each section says so, with the decisions gardened into `docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`, `docs/decisions/motion-is-document-data-keyed-on-the-destination.md` and `docs/decisions/a-loop-is-ambient-paint-anchored-at-load.md`. What remains is the rejected wasm-expression alternative and its counter-proposal, a standing input to the transform union rather than a gap; it empties when that is ruled on |
| `2026-08-07-animated-content-import.md`                | an animated-content importer is built. Extends `docs/technotes/runtime-content.md` §4-§6 rather than replacing it. Its ThorVG half may garden separately and later, or never — the standing decision already covers it and this file only records two notes against it                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `2026-08-07-asset-sourcing-and-residency.md`           | side-loading is built. Blocked on the schema's own deliberately-absent flavor/locator bit, so it cannot empty before that field has a producer and a consumer — the same condition the `AssetEntry` comment already states for itself                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `2026-08-09-svg-as-a-second-producer.md`               | gardened in two pieces — the profile half when the SVG importer is built, the reference-set half when the animation vocabulary closes. Three of its rulings are decision-shaped and marked **promote** in the file rather than gardened away: SMIL is not the animation reference set, SVG support is a profile rather than a version, and SVGO is rejected as a preprocessor                                                                                                                                                                                                                                                                                                                               |

Each row's entry is removed when its capture is gardened.

Anything else tracked here is genuinely ungardened and should be gardened
before its branch targets `main`. For reference, the working memory from the
v0.11 fidelity track was gardened on completion — the font-weight design into
four decision records plus the design records it touched, and its driver prompt
archived verbatim — and epic #344's and story #393's driver prompts were archived
the same way at the v0.11 close, as v0.12's and v0.13's were at theirs.
