# Token resolution: phase split; the join table comes from the Plugin API

    status   accepted
    date     2026-07-12
    scope    the importer's Figma variable/token handling
    binds    docs/decisions/annotator-plugin-contract-frozen.md's phase-2
             trigger

## Context

`docs/design/dashc.md` planned a two-phase token resolution. This decision
refines that plan against what the Professional plan
(`docs/decisions/figma-access-plan-and-pat-policy.md`) actually allows:
`file_variables:read` is Enterprise-only, so the REST Variables endpoint
is unavailable.

## Decision

**Phase 1 — resolved literals plus a sidecar receipt.** The importer
emits resolved literal values into the `.dsb` and writes a
`<out>.vars.json` sidecar preserving the `boundVariables` IDs. The
sidecar is fully derivable from the captured `GET /file` JSON — an
R7-safe receipt (re-deriving it is byte-reproducible), not a second
source of truth. Phase-1 documents are single-theme by construction: one
resolved mode, no runtime theme switching.

**Phase 2 — id → name/collection/mode join**, switching the `.dsb` to
token refs. On Professional there is no naming-convention fallback:
variable names, collections, and modes are exposed only by the
Enterprise-gated Variables REST endpoint, so the join table must come
from the Figma Plugin API. Concretely: one command on the annotator
plugin (`docs/decisions/annotator-plugin-contract-frozen.md`) exports
the id → name/collection/mode table — token export becomes the
annotator plugin's first mandatory job, ahead of the rest of the
plugin.

The table format is source-agnostic: if Enterprise REST access ever
becomes available, it is a drop-in replacement producer for the same
table. Staleness is guarded by stamping the table with the Figma file
version it was exported from; a version mismatch against the capture is
a diagnostic. For fixtures, the table is committed as
`corpus/figma-fixtures/<file>.vartable.json`.

## Why

The Professional plan's Variables-endpoint gate is a hard fact, not a
design choice: a naming-convention fallback (`docs/design/dashc.md`'s
original "or naming convention" plan) would have quietly broken the moment a
variable's Figma name didn't match the convention. Sourcing the join
table from the Plugin API instead of guessing from names keeps
resolution correct rather than convention-dependent, and it stays
correct if Enterprise access is added later, since the table format
does not change.
