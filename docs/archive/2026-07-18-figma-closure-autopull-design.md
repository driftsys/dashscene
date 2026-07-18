# S3 — Figma component closure: auto-pull local masters, warn on the unplaceable

    status   design (working memory); human-approved 2026-07-18 (BOLD policy)
    story    S3 / #312 of the "full real-file import" epic
             (ledger .superpowers/sdd/epic-progress.md)
    scope    importers/figma/src/closure.ts (+ the import.ts remote path)
    base     main 27d8d80
    decision extends docs/decisions/figma-component-lowering.md

## Why

Under partial-emit the hero target (Landify, file `S30AJmYfnDKGeSQmzuXEUk`, root
`1973:6580`) never reaches `dashc`: the Deno **closure** stage throws
`ExportBlocked` first. The live probe: 79 instances → 56 masters — 38 local
component _sets_ buried under other top-level frames/canvases, 18 remote-library.
The closure blocks on `figma.closure.buried-component` (×14) and
`figma.closure.unresolved-component` (×2); the 18 remote refs are a latent second
wall (`cross-file-unresolved`).

Key fact (from `docs/decisions/figma-component-lowering.md`): Figma's REST API
**bakes each instance's fully-resolved subtree (overrides applied) into the
instance's own `children`.** So an instance _renders_ from its baked children;
its master (component set) is needed only to (a) validate the reference and
(b) ship the variant set for `image_refs` / the future v0.4 variant switcher —
**not to render the authored state.**

## Policy (approved — BOLD)

1. **Auto-pull buried LOCAL masters.** When a declared root's instance references
   a local (`remote:false`) component/set whose definition is in-tree but under
   an undeclared top-level node, pull _just that definition subtree_ into the
   closure and lift it as a top-level node of the pruned file. Never pull the
   containing frame.
2. **Downgrade ANY unplaceable master to a named WARNING** — both the local
   unresolved case (master absent from the tree, e.g. removed by `trim`) and the
   remote case (`cross-file-unresolved` with no library declared). The instance
   renders from its baked children; the missing master is a named warning, never
   a silent drop (P4). This clears the closure entirely, so the hero reaches
   `dashc` (where partial-emit handles any dashc-level gaps).

This is the closure-stage sibling of the S0 partial-emit decision:
**skip-and-diagnose, never approximate.** Baked children are Figma's own resolved
content, not an approximation. It **defers** proper remote-library resolution
(#259/#261) — still valuable for variant switching and complete library
fidelity, but not needed to render the authored state.

## The closure algorithm today (grounded, cite before editing)

- `computeClosure(file, manifest)` (`closure.ts:296`) is a declared-root
  reachability walk; it never throws — it returns `diagnostics`, and the caller
  (`import.ts`) throws `ExportBlocked` when any are error-severity
  (`import.ts:247/265/275`).
- `walk` (`closure.ts:326-355`) records node ids + image refs and, per
  `INSTANCE`, pushes `componentId` onto `pendingComponents` (the transitive
  worklist).
- Component closure (`closure.ts:427-538`) drains `pendingComponents`. Buried is
  minted at `410` (single component) and `523` (set) under
  `top.id !== self && !keptTop.has(top.id)`. Unresolved is minted at `432`
  (id absent from `file.components`), `462` (local, no set, absent from `byId`),
  `495` (set node absent from `byId`). Remote refs `continue` at `458` before the
  unresolved checks; the remote wall is `cross-file-unresolved` raised later in
  the remote-resolution step (`import.ts:261-265` / `resolveRemoteComponents`).
- The pruned file is built at `585-628`; remote defs are spliced in as top-level
  nodes at `closure.ts:1025-1034` (the exact splice pattern auto-pull reuses).

## Design

### Auto-pull (buried local masters)

At the buried check (`closure.ts:410` / `523`), for a **local** master: instead
of pushing `buried-component`, `walk(definitionNode)` the component/set subtree
only (this records its node ids + image refs and appends its nested instances to
`pendingComponents`, so transitivity is automatic) and collect the definition
node into a `pulled` list. In the pruned-file construction (`585-628`), splice
each pulled definition subtree in as an extra top-level child of a canvas —
mirror `resolveRemoteComponents`' splice (`1025-1034`), simpler because local ids
need no re-id. Discipline: pull `walk(setNode)` only; **never `keptTop.add(top.id)`
of the containing frame** (that would silently export undeclared content).

### Downgrade to warning (unplaceable masters)

- Local unresolved (`432/462/495`): mint a **warning** (e.g.
  `figma.closure.local-master-unplaceable`) instead of the error, with a message
  naming the instance and master and stating the instance renders from its baked
  children.
- Remote (`cross-file-unresolved`, the remote-resolution path): when the master
  is remote and no declaring library resolves it, mint a **warning** instead of
  throwing. (Keep genuine remote-resolution _failures_ — a declared library that
  errors — as errors; only the "no library, render baked" case downgrades.)
- Ensure `ExportBlocked` fires only on **error-severity** closure diagnostics, so
  these warnings do not block. Surface the warnings on stderr (like the dashc
  warnings the importer already prints) — never drop them silently (P4).

### Drift oracle invariant (must hold)

`closure.imageRefs` must equal dashc `figmaImageRefs` (`closure_test.ts`).
Auto-pulled defs become top-level nodes → dashc's `image_refs` scans them → equal.
Downgraded (not pulled) masters are in neither → still equal. Add a
`closure_test.ts` case asserting equality across an auto-pull.

## Guardrails

- **Deno-only.** No `dashc` change: `top_level_nodes` already lowers every canvas
  child as a root and skips `COMPONENT`/`COMPONENT_SET` definitions (#242). Touch
  `closure.ts` and the `import.ts` remote path only.
- **EmitPolicy does NOT reach the closure.** Partial-emit is a `dashc`
  compile-time policy passed only into `compileFigma` (`import.ts:337-343`), after
  the closure has already run. So this closure-level downgrade is a _separate_
  severity change in the closure/import path — do not try to reuse `EmitPolicy`.
- **E7 (v0.9 exit gate) untouched.** E7 fixtures are single-canvas with declared
  roots and no buried masters → auto-pull is a no-op on them. Do not modify E7
  fixtures/goldens. Add new `closure_test.ts` cases instead.
- **P4:** every downgrade is a named warning. **P1:** closure is structural; no
  results enter the document.
- **Corpus stays self-authored:** tests use synthetic in-repo fixtures; the hero
  is probed live only, never committed.

## Caveat (measured later, not blocking)

Whether remote-component instances have _complete_ baked children in the REST
response (full render) or empty ones (holes) is unverified. Either way the result
is omission-with-warning (P4-clean); the render oracle (Sf) measures the hero's
fidelity. During the empirical re-probe, note whether the remote instances lower
with content or as empty frames.

## Alternatives considered

- **Local-only (conservative).** Rejected at the fork (human chose bold): the
  hero would not emit — the 18 remote refs become the next wall.
- **Multi-root.** Rejected: the masters are scattered under ~13 frames across ~11
  canvases; declaring them would paint component galleries as document content.
- **Teach `trim` to preserve referenced masters.** More invasive than the
  warning; the warning is cleaner (the baked instance renders without the master).

## Test strategy (TDD — detail in the plan; `closure_test.ts`)

- Auto-pull: a declared root whose instance references a local set buried under
  another top-level frame → the set subtree is pulled and lifted top-level, a
  nested instance inside it is followed transitively, the container frame is NOT
  pulled, and the drift-oracle equality holds.
- Local-unplaceable → warning: a referenced local master absent from the tree →
  a named warning, closure succeeds (no `ExportBlocked`).
- Remote → warning: a remote-library instance, no library declared → a named
  warning (was `cross-file-unresolved`), closure succeeds.
- Regression: an existing single-root file with no buried masters is unchanged
  (byte-for-byte pruned output + no new diagnostics).
- Empirical: re-probe the hero → closure passes with warnings, hero reaches
  `dashc`; capture the new dashc-level frontier (that is Wave 2's input).
