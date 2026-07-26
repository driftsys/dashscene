# wip

Working memory: the spec and plan produced by a Superpowers session while
work is in progress. Transient by design — when a session's work lands and
its durable records are written into `docs/specification/`, `docs/design/`,
`docs/decisions/`, or `docs/technotes/`, the raw spec and plan move to
`docs/archive/` rather than being deleted.

Tracked in git (collaborative mode) rather than gitignored, per this
project's convention.

See the `sdd-working-memory-lifecycle` rule and the `sdd-gardening` skill.

## Why the WIP gate currently reports five files

`wip-gate.sh` flags every tracked file here except this README, so it reports
five and exits non-zero. All five are deliberate, accepted exceptions rather
than ungardened debt, and they are recorded here so the gate's result is
explained rather than merely tolerated.

Three are forward-looking design captures for work that has not started. Every
one says so in its own `status` line — "Nothing here is implemented". Gardening
runs **after** tests are green by definition, so there is no as-built code to
reconcile any of them against; promoting one now would put a plan into
`docs/design/` describing a system that does not exist.

The fourth, the asset-pipeline capture, is **partly gardened**: v0.11 built the
parts of it that this slice's scope covered, those parts are now as-built
records, and its `status` line says which and where. What remains is gated on the
packer and stays here as live input to epic #345. A partial gardening is the
honest state for a capture that spans two slices. The rule's own words are what
force it: "forward-looking concepts stay in `docs/wip/` until implemented and
gardened in", and gardening "runs after tests are green by definition". Promoting
the packer half now would put a plan into `docs/design/` describing a system that
does not exist; archiving the whole file would lose the half epic #345 needs. The
rule models gardening as one atomic move because a capture spanning two slices is
a case it does not name — the reading here is that the _implemented_ half is
gardened, and the file leaves `docs/wip/` when its last half does.

The fifth is a driver prompt: the brief a session is handed to carry out a named
piece of work. Driver prompts are transient by construction — spent the moment
their work lands — and the convention is to archive them verbatim rather than
garden them into records, as the four now in `docs/archive/` were.

`2026-07-26-story-393-b3-b4-DRIVER-PROMPT.md` is **spent**: story #393 is closed
and both its PRs are merged. It is left here rather than archived because it
belongs to a concurrent session that owns its own `docs/wip/` content, and moving
another session's working memory out from under it is worse than leaving one file
for it to archive. Named here so the gate's fifth file reads as a handover rather
than as debt nobody noticed.

| capture                                            | gardened when                                                                                                                                                 |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2026-07-19-asset-pipeline-profiles-and-baking.md` | partly gardened at the v0.11 close (epic #344); the rest when the packer lands (epic #345, v0.12) — its `status` line says which half is which                |
| `2026-07-19-backdrop-blur-v011.md`                 | backdrop blur lands: a profile-policy decision reversal, the first schema `Effect` representation, and the boundary-B contract a backdrop-sampling node needs |
| `2026-07-19-color-space-blur-and-msdf.md`          | the painter's working colour space is settled — one question in it is genuinely open                                                                          |
| `2026-07-19-wgpu-painter-direction.md`             | a wgpu painter is actually chosen. Explicitly a direction, not a commitment; it exists so the question is not researched from scratch when that slice opens   |

Each row's entry is removed when its capture is gardened.

Anything else tracked here is genuinely ungardened and should be gardened
before its branch targets `main`. For reference, the working memory from the
v0.11 fidelity track was gardened on completion — the font-weight design into
four decision records plus the design records it touched, and its driver prompt
archived verbatim — and epic #344's own driver prompt was archived the same way
at this close.
