# wip

Working memory: the spec and plan produced by a Superpowers session while
work is in progress. Transient by design — when a session's work lands and
its durable records are written into `docs/specification/`, `docs/design/`,
`docs/decisions/`, or `docs/technotes/`, the raw spec and plan move to
`docs/archive/` rather than being deleted.

Tracked in git (collaborative mode) rather than gitignored, per this
project's convention.

See the `sdd-working-memory-lifecycle` rule and the `sdd-gardening` skill.

## Why the WIP gate currently reports seven files

`wip-gate.sh` flags every tracked file here except this README, so it reports
seven and exits non-zero. All seven are deliberate, accepted exceptions rather
than ungardened debt, and they are recorded here so the gate's result is
explained rather than merely tolerated.

Six are design captures, described below. **One is a driver prompt**, the
brief a session is handed to carry out a named piece of work. Driver prompts are
transient by construction — spent the moment their work lands — and the
convention is to archive them verbatim rather than garden them into records, as
the ten now in `docs/archive/` were.

- `2026-07-27-t2-checks-that-cannot-fail-DRIVER-PROMPT.md` — v0.13's
  `t2-check-has-no-teeth` tier, 19 items whose common property is an assertion
  that cannot distinguish right from wrong.

It is archived when its work lands.

Five are forward-looking design captures for work that has not started. Every
one says so in its own `status` line — "Nothing here is implemented". Gardening
runs **after** tests are green by definition, so there is no as-built code to
reconcile any of them against; promoting one now would put a plan into
`docs/design/` describing a system that does not exist.

One of those five no longer fits that description, and this paragraph should
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

The sixth, the asset-pipeline capture, is **partly gardened**: v0.11 and v0.12
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

The driver prompt still here is listed at the top of this section rather than
here. The most recent ones archived were
`2026-07-27-glyph-runs-from-commit-SPIKE.md` and
`2026-07-29-glyph-runs-from-commit-DRIVER-PROMPT.md`, when the glyph-run
producer chain landed.

| capture                                                | gardened when                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2026-07-19-asset-pipeline-profiles-and-baking.md`     | partly gardened at the v0.11 close (epic #344) and again across v0.12 (epic #345, stories #432, #433, #434, #435, #436); the rest when the vector bake's end-state fork and animated content are built — its `status` line says which half is which                                                                                                                        |
| `2026-07-19-backdrop-blur-v011.md`                     | partly gardened at the v0.11 close (story #393): the profile-policy reversal, the schema `Effect` representation, and the boundary-B contract now live in `docs/decisions/backdrop-blur-is-core-vocabulary.md`; the rest when a second painter (Unity, tiny-skia web, or a future wgpu painter) needs the per-painter capability table or its two remaining quality levers |
| `2026-07-19-wgpu-painter-direction.md`                 | a wgpu painter is actually chosen. Explicitly a direction, not a commitment; it exists so the question is not researched from scratch when that slice opens                                                                                                                                                                                                                |
| `2026-07-28-photorealistic-3d-content.md`              | each question it traces is ruled on. It records an input rather than a plan: photorealistic 3D renders are target product content, and every number in the asset pipeline was chosen against content that is not representative of it. Its first measurable consequence is #455's fixture                                                                                  |
| `2026-07-27-indic-script-support.md`                   | Indic support is designed: the closure becomes text-driven and the unformed-cluster fallback is built. Its decided half — coverage is declared at build time, dynamic generation is a deferred painter capability — is already gardened into `docs/decisions/glyph-coverage-is-declared-at-build-time.md`                                                                  |
| `2026-07-27-glyph-coverage-sets-and-text-residency.md` | glyph-atlas residency is designed: the unit of residency is chosen and the runtime-supplied-string case is answered. Its decided half — that only raster is block-compressed — is already gardened into `docs/decisions/compress-raster-only.md`                                                                                                                           |

Each row's entry is removed when its capture is gardened.

Anything else tracked here is genuinely ungardened and should be gardened
before its branch targets `main`. For reference, the working memory from the
v0.11 fidelity track was gardened on completion — the font-weight design into
four decision records plus the design records it touched, and its driver prompt
archived verbatim — and epic #344's and story #393's driver prompts were archived
the same way at the v0.11 close, as v0.12's and v0.13's were at theirs.
