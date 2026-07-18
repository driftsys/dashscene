# S0-impl — partial-emit mode (skip-and-diagnose, never approximate)

    status   design (working memory); human-approved 2026-07-18
    story    S0-impl of the "full real-file import" epic
             (docs/wip/2026-07-18-epic-full-real-file-import.md)
    scope    crates/dashc (lib, figma walk, abi wire), importers/figma
    revises  docs/decisions/unsupported-figma-constructs-refuse-the-compile.md

## Why

`dashc` today refuses the whole file if the exported subtree contains any
construct the document cannot express (the all-or-nothing posture). This makes
a real, media-rich Figma file (the epic's hero target) effectively unimportable:
one unsupported node anywhere refuses every byte, so "full import" degenerates
into "close the entire vocabulary first."

The epic's S0 gate decided (human, 2026-07-18) to add a **partial-emit** policy:
skip an unsupported node with a named diagnostic, but still emit the document
with the covered majority. A **strict** (all-or-nothing) mode stays available for
"correct or refused." The render oracle (story Sf) bounds the accumulated
omissions, so degradation is measured and visible, never silent.

The all-or-nothing behavior is **not R6**. R6 (`docs/specification/01-goals-and-requirements.md`)
says a vocabulary gap is "a named import diagnostic (warning/error), never a
silent drop" — it explicitly permits a warning. The refuse-the-file choice lives
in `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`
("Choice: Option 3", already revised once at #140). That record is what this
story revises.

## The key distinction: omission vs approximation

The codebase already carries two kinds of vocabulary diagnostic, and partial-emit
must treat them oppositely:

- **Omission diagnostics** — `figma.unsupported` (`crates/dashc/src/figma/mod.rs`,
  `unsupported_at`). The node's whole subtree is _skipped_; nothing is lowered.
  The result is a hole plus a named diagnostic — never an approximation.
- **Approximation-if-shipped diagnostics** — REJECT-band constructs triaged on the
  _success_ path (`figma/mod.rs`, the `for construct in constructs` push after the
  node is emitted; e.g. noise/texture, progressive blur). Here the node **is**
  lowered, just without the rejected feature. Shipping it would render a picture
  the designer never authored — the silent lie the original decision forbids.
  (LATER-band constructs such as a plain layer blur already degrade-and-warn in
  both modes; that is an accepted, _loud_ approximation and does not change.)

So "skip-and-diagnose, never approximate" maps exactly onto:

> Under partial-emit, **downgrade omission diagnostics to warnings; keep
> approximation-if-shipped diagnostics fatal.**

## Design

### The mode

    // crates/dashc/src/lib.rs
    pub enum EmitPolicy {
        /// All-or-nothing: any vocabulary gap refuses the whole file.
        Strict,
        /// Skip an unsupported node with a warning, still emit. Never
        /// approximates: a construct that could only ship approximately
        /// (a REJECT-band feature on a lowered node) still refuses.
        Partial,
    }

### Threading

1. `compile_figma` and `compile_figma_with_bindings` gain a `policy: EmitPolicy`
   parameter. **The Rust API default is `Strict`** — existing callers and tests
   keep today's behavior; the "correct or refused" posture stays the library
   default. Only the product surface (the importer) opts into `Partial`.
2. The policy threads into `figma::lower_with_bindings` → the `Walk` → the
   `unsupported_at` helper. Under `Partial`, `figma.unsupported` (and the
   `blockers`-path skips that feed it) are minted at `Severity::Warning`; under
   `Strict`, at `Severity::Error` (unchanged).
3. Nothing else in the gate changes. The node/subtree is omitted either way, and
   `compile_figma_with_bindings` still returns `Err` only when
   `report.has_errors()` (`lib.rs`). Under `Partial`, the downgraded gaps are
   warnings, so the gate passes and `Ok((bytes, report))` is returned with the
   warnings riding along.

### The ABI (wire v1 compatible)

The compile request (`crates/dashc/src/abi/wire.rs`) gains one field —
`strict: bool` (or `policy`) — **optional, defaulting to `Strict`** when a caller
omits it, so an old caller still refuses-hard (wire v1 stays compatible,
`docs/decisions/dashc-wasm-abi.md`). `compile_figma_response` maps it to
`EmitPolicy` and passes it through.

### The importer

`importers/figma/src/import.ts` **defaults to Partial** (sends `strict: false`).
A new `--strict` flag opts into all-or-nothing (sends `strict: true`). The
diagnostics already print to stderr; under Partial the run also writes the
document, so the `wrote …` success line and the warning lines both appear.

### Stays fatal in both modes

- Parse failure (`CompileError::Parse`).
- Unresolved image ref (`CompileError::UnresolvedImage`) — a caller-contract
  violation, not a designer's unsupported construct.
- No-root `figma.no-content` (a zero-node `.dsb` panics a downstream loader).
- Load-gate structural errors (dangling parent, unknown enum) — an emitter-bug
  class that must never ship.
- REJECT-band triaged constructs on a lowered node (the approximation-if-shipped
  class above).

## Tree integrity

Whole-subtree omission (today's behavior) is kept. Keeping a skipped node's
_children_ while dropping the node would leave `parent` indices pointing at a
node that was never pushed — which the load gate rejects
(`node.parent-out-of-range` / `node.parent-not-before-child`). So partial-emit
adds **no new IR node kind and no `.dsb` schema change**; it only changes the
severity of an existing diagnostic and adds a request field.

## P1 / P4

- **P4** (every gap a named diagnostic, never silent) is _strengthened_: a skip
  is still a named diagnostic; partial-emit only changes its severity. Nothing
  becomes silent.
- **P1** (the document carries intent, not results) holds: omission carries no
  solver results. A skipped node leaves nothing behind — no baked box, no
  placeholder extent.

## Consequence to accept

A file whose _only_ blocker is a REJECT-band construct on an otherwise-lowerable
node still refuses under Partial. We will not ship it approximated, and omitting
just that one node from the success path (moving its triage into the skip
decision) is a per-construct follow-up story, filed only if a real target hits
it. The first-light target hits none of these; the hero target is unlikely to.

## Alternatives considered

- **Partial as the sole policy (drop Strict).** Rejected at the S0 gate: it
  throws away the "correct or refused" guarantee the original decision
  deliberately chose, with no way to demand a fully-covered emit.
- **A placeholder/"unsupported" IR node kind** so a skipped flex child preserves
  its siblings' layout. Deferred: it is a genuine schema effort (`dashbuf.fbs` +
  `Node` + a load-gate rule) and buys only exact sibling-layout fidelity, which
  the render oracle already measures. Not a prerequisite for emit.
- **REJECT-band skips-the-node under Partial** (omit the whole node instead of
  refusing). Attractive but larger: it moves success-path triage into the skip
  decision for every REJECT construct. Deferred to a per-construct follow-up;
  the conservative "still refuses" is the faithful "never approximate" reading.

## Test strategy (TDD — detailed in the plan)

- Strict still refuses a file with a `figma.unsupported` construct (existing
  behavior, now pinned with an explicit `EmitPolicy::Strict`).
- Partial emits the same file, returns `Ok`, and the report carries the gap as a
  **warning** at the same node path.
- Partial still refuses a REJECT-band construct on a lowered node (the
  never-approximate pin) and still refuses parse / no-content / unresolved-image.
- The ABI round-trips the `strict` field; a request omitting it defaults to
  Strict.
- The importer defaults to Partial and `--strict` restores refusal (Deno test
  with a stub compile boundary).
