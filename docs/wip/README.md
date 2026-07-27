# wip

Working memory: the spec and plan produced by a Superpowers session while
work is in progress. Transient by design — when a session's work lands and
its durable records are written into `docs/specification/`, `docs/design/`,
`docs/decisions/`, or `docs/technotes/`, the raw spec and plan move to
`docs/archive/` rather than being deleted.

Tracked in git (collaborative mode) rather than gitignored, per this
project's convention.

See the `sdd-working-memory-lifecycle` rule and the `sdd-gardening` skill.

## Why the WIP gate currently reports six files

`wip-gate.sh` flags every tracked file here except this README, so it reports
six and exits non-zero. All six are deliberate, accepted exceptions rather
than ungardened debt, and they are recorded here so the gate's result is
explained rather than merely tolerated.

Five are forward-looking design captures for work that has not started. Every
one says so in its own `status` line — "Nothing here is implemented". Gardening
runs **after** tests are green by definition, so there is no as-built code to
reconcile any of them against; promoting one now would put a plan into
`docs/design/` describing a system that does not exist.

One of those five no longer fits that description, and this paragraph should
not be read as claiming it does. Backdrop blur landed in v0.11 (story #393), so
`2026-07-19-backdrop-blur-v011.md` is spent in its decided half — the reversal,
the contract shape and all four of its open questions are now in
`docs/decisions/backdrop-blur-is-core-vocabulary.md` — while its per-painter
capability table and two of its quality levers still describe painters that do
not exist. Correcting it is a partial gardening, the same shape as the
asset-pipeline capture below, and is tracked as **#427** rather than decided
here.

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

Its `status` line is now partly stale and this paragraph should not be read as
endorsing it. That line names four residuals gated on the packer, and epic #345
has since closed: the profile-preview oracle is gardened into
`docs/decisions/profile-preview-decodes-in-the-loader.md`, and cold-bank
assembly landed in stories #433 and #434. What is genuinely still unbuilt is the
vector bake's end-state fork and animated content, which is why the file stays.
Reconciling the line — and the backdrop-blur capture's, which #427 carries — is
the **#424 / #427** gardening pass, taken together in one PR rather than decided
here.

There is no driver prompt here at present. Driver prompts — the brief a session
is handed to carry out a named piece of work — are transient by construction,
spent the moment their work lands, and the convention is to archive them
verbatim rather than garden them into records. The seven now in `docs/archive/`
were, most recently `2026-07-27-v013-open-DRIVER-PROMPT.md` when the v0.13
plan revision landed.

| capture                                                | gardened when                                                                                                                                                                                                                                                                                             |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2026-07-19-asset-pipeline-profiles-and-baking.md`     | partly gardened at the v0.11 close (epic #344) and again across v0.12 (epic #345, stories #432, #435, #436); the rest when the vector bake's end-state fork and animated content are built — its `status` line says which half is which, and #424 reconciles the part of that line v0.12 made stale       |
| `2026-07-19-backdrop-blur-v011.md`                     | backdrop blur lands: a profile-policy decision reversal, the first schema `Effect` representation, and the boundary-B contract a backdrop-sampling node needs                                                                                                                                             |
| `2026-07-19-color-space-blur-and-msdf.md`              | the painter's working colour space is settled — one question in it is genuinely open                                                                                                                                                                                                                      |
| `2026-07-19-wgpu-painter-direction.md`                 | a wgpu painter is actually chosen. Explicitly a direction, not a commitment; it exists so the question is not researched from scratch when that slice opens                                                                                                                                               |
| `2026-07-27-indic-script-support.md`                   | Indic support is designed: the closure becomes text-driven and the unformed-cluster fallback is built. Its decided half — coverage is declared at build time, dynamic generation is a deferred painter capability — is already gardened into `docs/decisions/glyph-coverage-is-declared-at-build-time.md` |
| `2026-07-27-glyph-coverage-sets-and-text-residency.md` | glyph-atlas residency is designed: the unit of residency is chosen and the runtime-supplied-string case is answered. Its decided half — that only raster is block-compressed — is already gardened into `docs/decisions/compress-raster-only.md`                                                          |

Each row's entry is removed when its capture is gardened.

Anything else tracked here is genuinely ungardened and should be gardened
before its branch targets `main`. For reference, the working memory from the
v0.11 fidelity track was gardened on completion — the font-weight design into
four decision records plus the design records it touched, and its driver prompt
archived verbatim — and epic #344's and story #393's driver prompts were archived
the same way at the v0.11 close, as v0.12's and v0.13's were at theirs.
