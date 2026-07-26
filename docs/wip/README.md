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
six and exits non-zero. All six are deliberate, accepted exceptions rather than
ungardened debt, and they are recorded here so the gate's result is explained
rather than merely tolerated.

Four are forward-looking design captures for work that has not started. Every
one says so in its own `status` line — "Nothing here is implemented". Gardening
runs **after** tests are green by definition, so there is no as-built code to
reconcile any of them against; promoting one now would put a plan into
`docs/design/` describing a system that does not exist.

The other two are driver prompts: the brief a session is handed to carry out a
named piece of work. They are transient by construction — spent the moment their
work lands — and follow the convention of the three already in `docs/archive/`,
which is to archive them verbatim rather than garden them into records.

| capture                                            | gardened when                                                                                                                                                 |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2026-07-19-asset-pipeline-profiles-and-baking.md` | v0.11's own scope lands — the sectioned `.dsb` container and the content-addressed asset table (epic #344, story #107)                                        |
| `2026-07-19-backdrop-blur-v011.md`                 | backdrop blur lands: a profile-policy decision reversal, the first schema `Effect` representation, and the boundary-B contract a backdrop-sampling node needs |
| `2026-07-19-color-space-blur-and-msdf.md`          | the painter's working colour space is settled — one question in it is genuinely open                                                                          |
| `2026-07-19-wgpu-painter-direction.md`             | a wgpu painter is actually chosen. Explicitly a direction, not a commitment; it exists so the question is not researched from scratch when that slice opens   |

Each row's entry is removed when its capture is gardened.

Anything else tracked here is genuinely ungardened and should be gardened
before its branch targets `main`. For reference, the two pieces of working
memory from the v0.11 fidelity track were gardened on completion: the
font-weight design into four decision records plus the design records it
touched, and its driver prompt archived verbatim, both under `docs/archive/`.
