# `dashc` lowers Figma, it does not export from Figma

    status   accepted
    date     2026-07-13
    source   docs/technotes/producers-and-ir.md §1
    binds    docs/decisions/figma-importer-deno-plus-dashc-wasm.md

## Context

This re-affirms an existing decision rather than making a new one.
`docs/decisions/figma-importer-deno-plus-dashc-wasm.md` (gardened from
`docs/archive/2026-07-14-scope-decisions.md` §4) already draws the seam between
Deno-owned REST/auth/JSON I/O and `dashc`-owned lowering, validation, and `.dsb`
emission, and that split is already reflected in the code (`importers/figma/`,
`crates/dashc`).

## Choice

No new ruling: `dashc` owns only Figma≠CSS lowering, profile/vocabulary
validation, and deterministic `.dsb` emission. It never performs the REST fetch,
PAT rotation, rate-limit backoff, or the reachability/variant-set/ trim closures
— those stay in the Deno importer. See
[`figma-importer-deno-plus-dashc-wasm.md`](figma-importer-deno-plus-dashc-wasm.md)
for the full rationale.

## Consequences

- Keep `dashc`'s Figma-facing lowering named as Figma-specific, not generic "the
  lowering", so a future producer's lowering does not silently inherit Figma
  assumptions — tracked as an open item in `docs/technotes/producers-and-ir.md`
  §7.
