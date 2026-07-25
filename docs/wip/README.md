# wip

Working memory: the spec and plan produced by a Superpowers session while
work is in progress. Transient by design — when a session's work lands and
its durable records are written into `docs/specification/`, `docs/design/`,
`docs/decisions/`, or `docs/technotes/`, the raw spec and plan move to
`docs/archive/` rather than being deleted.

Tracked in git (collaborative mode) rather than gitignored, per this
project's convention.

See the `sdd-working-memory-lifecycle` rule and the `sdd-gardening` skill.

## Why the WIP gate currently reports one file

`wip-gate.sh` flags every tracked file here except this README, so it
reports `2026-07-19-asset-pipeline-profiles-and-baking.md` and exits
non-zero. That one file is a deliberate, accepted exception rather than
ungardened debt, and it is recorded here so the gate's result is
explained rather than merely tolerated.

It is the design kernel for v0.11's own scope — the sectioned `.dsb`
container and the content-addressed asset table (epic #344, story #107).
That work has not started, so there is no as-built code to reconcile
records against, and gardening runs **after** tests are green by
definition. Forward-looking concepts stay here until implemented and
gardened in; promoting this one now would put a plan into
`docs/design/` describing a system that does not exist.

It is gardened when #344's implementation lands, and this section is
removed at the same time.

Anything else tracked here is genuinely ungardened and should be gardened
before its branch targets `main`. For reference, the two pieces of working
memory from the v0.11 fidelity track were gardened on completion: the
font-weight design into four decision records plus the design records it
touched, and its driver prompt archived verbatim, both under
`docs/archive/`.
