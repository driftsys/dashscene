# wip

Working memory: the spec and plan produced by a Superpowers session while
work is in progress. Transient by design — when a session's work lands and
its durable records are written into `docs/specification/`, `docs/design/`,
`docs/decisions/`, or `docs/technotes/`, the raw spec and plan move to
`docs/archive/` rather than being deleted.

Tracked in git (collaborative mode) rather than gitignored, per this
project's convention.

See the `sdd-working-memory-lifecycle` rule and the `sdd-gardening` skill.

## Why the WIP gate currently reports nine files

`wip-gate.sh` flags every tracked file here except this README, so it reports
nine and exits non-zero. All nine are deliberate, accepted exceptions rather
than ungardened debt, and they are recorded here so the gate's result is
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
day, when the v0.15 driver prompt below was added. Issue #663 was then filed against the count rather
than against this ledger, and asserted eight ungardened files when seven were
accounted for here. It went from eight to nine on 2026-08-05, when v0.16's
driver prompt was added while v0.15's was still held. Re-derive the number from
`git ls-files docs/wip/` when touching this section rather than trusting the
prose.

**Seven are design captures**, described below. The other two are driver
prompts: `2026-08-02-v015-DRIVER-PROMPT.md`, the brief that carries v0.15's
remaining stories, and `2026-08-05-v016-DRIVER-PROMPT.md`, the brief that
opens v0.16. Driver prompts — the brief a session is handed to carry out
a named piece of work — are transient by construction, spent the moment their
work lands, and the convention is to archive them verbatim rather than garden
them into records. Twelve are already in `docs/archive/`: eleven
`*-DRIVER-PROMPT.md` plus the one `*-SPIKE.md`, which is counted there because
it was archived verbatim beside the prompt that carried out its design rather
than gardened.

**Holding two at once is ordinary rather than exceptional**, and the history
says so plainly: `git ls-tree -r --name-only <sha> -- docs/wip/` returns two
`*-DRIVER-PROMPT.md` files at `dca9ec7`, `a99f3b3`, `33363f3` and `902a4a3`,
and two of those commits name the plural in their own messages ("track the
**two** driver prompts for the next v0.11 sessions", "driver **prompts** for
the vector-blur and t2 handoffs"). What is true here is narrower: v0.16 opened
while epic #569 was still open, so the two slices overlap and each has a live
brief. The v0.16 prompt's own "v0.15 is still open" section records the overlap
and what it costs a session working either side of it.

An earlier revision of this paragraph claimed the overlap was a first. It was
not, and the paragraph above this one already says why that happened —
**re-derive from `git ls-tree` and `git ls-files` rather than trusting the
prose**, and that applies to a claimed "first" exactly as it applies to a
count.

**The v0.16 prompt leaves when epic #594 closes**, archived verbatim to
`docs/archive/` beside the twelve, on the same rule as the v0.15 prompt below:
it holds no decision and no design, so there is nothing in it to garden. Its
own `status` block says the same.

**The v0.15 prompt leaves when epic #569 closes**, archived verbatim beside
those twelve. It holds no decision and no design — it is current state, the
story order, the per-story loop, and the failure modes this repo has actually
hit — so there is nothing in it to garden into a record, and everything in it
goes stale the moment the slice does. Its own `status` block says the same.

The twelfth is `2026-07-27-t2-checks-that-cannot-fail-DRIVER-PROMPT.md`, v0.13's
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

Four are forward-looking design captures for work that has not started. Every
one says so in its own `status` line — "Nothing here is implemented". Gardening
runs **after** tests are green by definition, so there is no as-built code to
reconcile any of them against; promoting one now would put a plan into
`docs/design/` describing a system that does not exist.

**Two more have since started, and this is the distinction that decides when
they leave.** `2026-07-19-wgpu-painter-direction.md` and
`2026-07-29-v014-v015-showcase-and-wgpu-wbs.md` both said no crate existed and
nothing was implemented. Both sentences went stale on the same events: the
breakdown was filed as epics #568 and #569, v0.14's showcase runtime landed,
and `crates/dashscene-gpu` was created by story #577. Their `status` lines were
corrected on 2026-08-02 and now say what is true.

Neither is gardened yet, and being chosen is not the reason to garden one. The
rule this directory has applied throughout is that a decision landing and a
design being **built** are two events, and only the second empties the file —
the same rule the glyph-run spike was held to. v0.15 is in progress: the
painter is chosen, the seam compiles, and the painter is not built. Both files
leave when it is.

One note on reading the breakdown: it says `dashscene-wgpu` throughout, and the
crate is `dashscene-gpu`. The name was settled after the breakdown was written,
and the text is corrected in its `status` line rather than rewritten, so the
file still records what was actually planned.

One of the four no longer fits that description, and this paragraph should
not be read as claiming it does. Backdrop blur landed in v0.11 (story #393), so
`2026-07-19-backdrop-blur-v011.md` is spent in its decided half — the reversal,
the contract shape and all four of its open questions are now in
`docs/decisions/backdrop-blur-is-core-vocabulary.md`, and its own `status` line
says so. What is left, and stays here, is its per-painter capability table
(Unity, the parked tiny-skia web painter, and a future wgpu painter — none of
which exist) and two of its three quality levers (dual-Kawase downsample,
re-blur cadence); the table's Skia row, previously stale the other way round
("unwired"), is corrected now that PR #403 wired the capability. A partial
gardening, the same shape as the asset-pipeline capture below — done at the
same pass, **#427**.

The seventh, the asset-pipeline capture, is **partly gardened**: v0.11 and v0.12
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

Two driver prompts are held here now, v0.15's and v0.16's, for the overlap
recorded above. The most recent one archived is
`2026-07-27-t2-checks-that-cannot-fail-DRIVER-PROMPT.md`, on 2026-08-02 when
epic #362 closed; before it `2026-07-30-blur-colour-space-DRIVER-PROMPT.md`,
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

| capture                                                | gardened when                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2026-07-19-asset-pipeline-profiles-and-baking.md`     | partly gardened at the v0.11 close (epic #344) and again across v0.12 (epic #345, stories #432, #433, #434, #435, #436); the rest when the vector bake's end-state fork and animated content are built — its `status` line says which half is which                                                                                                                        |
| `2026-07-19-backdrop-blur-v011.md`                     | partly gardened at the v0.11 close (story #393): the profile-policy reversal, the schema `Effect` representation, and the boundary-B contract now live in `docs/decisions/backdrop-blur-is-core-vocabulary.md`; the rest when a second painter (Unity, tiny-skia web, or a future wgpu painter) needs the per-painter capability table or its two remaining quality levers |
| `2026-07-19-wgpu-painter-direction.md`                 | the painter is **built**, not when it was chosen. Choosing it happened on 2026-08-02 — v0.15 opened as epic #569 and story #577 created `crates/dashscene-gpu` — and this row previously said "a wgpu painter is actually chosen", which that event satisfied without emptying the file. The research below stays the authority for the parts v0.15 has not reached yet    |
| `2026-07-28-photorealistic-3d-content.md`              | each question it traces is ruled on. It records an input rather than a plan: photorealistic 3D renders are target product content, and every number in the asset pipeline was chosen against content that is not representative of it. Its first measurable consequence is #455's fixture                                                                                  |
| `2026-07-27-indic-script-support.md`                   | Indic support is designed: the closure becomes text-driven and the unformed-cluster fallback is built. Its decided half — coverage is declared at build time, dynamic generation is a deferred painter capability — is already gardened into `docs/decisions/glyph-coverage-is-declared-at-build-time.md`                                                                  |
| `2026-07-27-glyph-coverage-sets-and-text-residency.md` | glyph-atlas residency is designed: the unit of residency is chosen and the runtime-supplied-string case is answered. Its decided half — that only raster is block-compressed — is already gardened into `docs/decisions/compress-raster-only.md`                                                                                                                           |
| `2026-07-29-v014-v015-showcase-and-wgpu-wbs.md`        | the wgpu painter is **built**. Ratified and partly built: filed as epics #568 and #569, and v0.14's showcase runtime has landed, so its v0.14 half is spent. Held for v0.15, which is in progress. Reads `dashscene-wgpu` throughout; the crate is `dashscene-gpu`                                                                                                         |

Each row's entry is removed when its capture is gardened.

Anything else tracked here is genuinely ungardened and should be gardened
before its branch targets `main`. For reference, the working memory from the
v0.11 fidelity track was gardened on completion — the font-weight design into
four decision records plus the design records it touched, and its driver prompt
archived verbatim — and epic #344's and story #393's driver prompts were archived
the same way at the v0.11 close, as v0.12's and v0.13's were at theirs.
