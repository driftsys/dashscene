# Garden `specs/` into the `docs/` taxonomy — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Delete `specs/`, landing `DESIGN_1.md` and `SCOPE_DECISIONS.md` into
`docs/specification/`, `docs/design/`, `docs/decisions/`, `docs/technotes/`,
`docs/roadmap.md`, and `docs/archive/`, with every live citation repointed and
the IR's name settled.

**Architecture:** Two branches, in order. `fix/ir-naming` settles what the IR is
called and renames the Rust type to match — it touches source, so it stays small
and separately revertable. `docs/garden-specs` then rebases onto it and does the
migration, which is docs-only. The order is forced: the garden pass writes prose
that names the IR, so the name must be settled first or the prose gets written
twice.

**Tech Stack:** Rust (`cargo`, edition 2024), `just` (task runner), `dprint` +
`markdownlint` (markdown lint, run by `just lint`), `convco` (commit-message
lint, run by `just verify`).

**Design:** `docs/wip/2026-07-13-garden-specs-design.md`. Section references
below (§N) point into it.

## Global Constraints

- **Commit scopes are a fixed list** (`.git-std.toml`). Allowed here: `dashc`,
  `docs`, `repo`, `dashbuf`, `goldens`, `corpus`, `importers`, `ci`. `wip` is
  **not** allowed and will be rejected by the commit hook.
- **Commit style:** conventional, `type(scope): subject`. Subject in lowercase,
  no trailing period.
- **`docs/archive/` is never edited.** It is verbatim history. Every `grep`
  verification excludes it, and a match there is never a failure.
- **`docs/wip/` is not touched by this work**, except for this plan and its
  design (§11, user instruction 2026-07-14). Two files in it belong to other
  sessions' in-flight work.
- **`just build` must be green before any commit.** It runs `cargo test`,
  `clippy -D warnings`, `cargo fmt --check`, `dprint check`, and `markdownlint`.
- **Markdown lint rules that will bite:** ordered lists must run `1/2/3` from 1
  (MD029 — a list restarting at 6 fails); emphasis uses `_underscore_`, not
  `*asterisk*` (MD049); tables must be pipe-aligned (MD060). Run
  `dprint fmt <file>` after writing any markdown; it fixes tables and spacing but
  **not** MD029 or MD049 — fix those by hand.
- **Requirement identifiers are preserved verbatim** (`G1`, `R1`, `P4`, `R-T2`,
  `E3`, `Q-4`). They are cited across the codebase. Do not renumber or reword
  them; this pass moves them, it does not rewrite them (§9).
- **`.dsb` is not renamed.** It is the flatbuffer extension and it stays.
- **Locate every section by its `## N.` heading, never by line number.** Any line
  numbers this plan quotes were measured before Branch 1 landed, and Branch 1's
  superseded banners shifted `DESIGN_1.md` by five lines. Headings are stable;
  line numbers are not.
- **Branch 1 has landed.** The IR is **the dashscene document**; `.dsb` is the
  flatbuffer extension; the Rust type is `Document`. `SCOPE_DECISIONS.md` §20 and
  `DESIGN_1.md`'s naming note each carry a superseded banner, and
  `docs/decisions/dashscene-document-is-the-ir.md` is the ruling. Write all new
  prose in that vocabulary — never "DSB" or "SCD" as the name of the IR.

---

## Branch 1 — `fix/ir-naming`

Branch from `origin/main`. Small, mechanical, touches source.

**The ruling being implemented (§5):** dashscene is the IR; `.dsb` is the
flatbuffer extension. `SCOPE_DECISIONS.md` §20 said the IR is "DSB" and is
overturned. The Rust type `Dsb` is therefore misnamed — it is the in-memory
document, not the buffer.

### Task 1: Record the ruling

**Files:**

- Create: `docs/decisions/dashscene-document-is-the-ir.md`
- Modify: `docs/decisions/README.md` (add the index entry)
- Modify: `specs/SCOPE_DECISIONS.md:1015-1061` (mark §20 superseded — `specs/`
  still exists on this branch)
- Modify: `docs/technotes/glossary.md:7` (cite the new record)

**Interfaces:**

- Produces: `docs/decisions/dashscene-document-is-the-ir.md` — Task 2's doc
  comments and Branch 2's prose both cite this path.

- [ ] **Step 1: Write the decision record**

Create `docs/decisions/dashscene-document-is-the-ir.md`:

```text
# The dashscene document is the IR; `.dsb` is its file extension

    Status      accepted
    Date        2026-07-14
    Supersedes  SCOPE_DECISIONS.md §20 ("The IR is named DSB; SCD is retired")

## Decision

The intermediate representation is **the dashscene document**. `.dsb` is the
extension of the flatbuffer that serializes it, and nothing more.

In Rust, the in-memory document is `Document` (in `dashc`), and its nodes are
`Node`. The generated flatbuffer types of the same name are aliased `FbDocument`
and `FbNode` where both are in scope.

## Why §20 is overturned

§20 retired the working name `SCD` — correctly — and then named the IR after the
file format, `DSB`. Its argument was that "two names for one thing is a cost this
removes".

The IR and its serialization are not one thing. `.dsb` is one way to carry the
document; the arena in `dashscene-core` is another, and a producer can populate
the arena without a `.dsb` ever existing. Naming the IR after one of its
encodings makes the other encoding read as secondary, and it makes P5 —
"DSB is a schema-first IR with its own spec and validator" — assert that a file
format has a validator. What is validated is the document.

The drift was already visible within a day of §20 landing. Three documents
described the same name and no two agreed:

| Source                                   | The IR is called | `DSB` expands to    |
| ---------------------------------------- | ---------------- | ------------------- |
| `SCOPE_DECISIONS.md` §20, `crates/dashc` | DSB              | —                   |
| `specs/DESIGN_1.md` naming note          | DSB              | "dash scene binary" |
| `docs/technotes/glossary.md`             | dashscene        | "dashscene buffer"  |

## What this binds

- `crates/dashc` — the IR type is `Document`, not `Dsb`.
- Every prose reference to the IR — "the dashscene document", or just "the
  document".
- `.dsb` — unchanged.
- `SCD`, `scdc`, `.scb` — already retired by PR #152. Nothing to do.
```

- [ ] **Step 2: Index it**

In `docs/decisions/README.md`, add to the list:

```text
- [dashscene-document-is-the-ir.md](dashscene-document-is-the-ir.md) — the IR is
  the dashscene document; `.dsb` is its file extension. Supersedes
  `SCOPE_DECISIONS.md` §20; binds `crates/dashc`'s type names.
```

- [ ] **Step 3: Mark §20 superseded**

In `specs/SCOPE_DECISIONS.md`, directly under the `## 20.` heading (line 1015),
insert:

```text
> **Superseded 2026-07-14** by
> `docs/decisions/dashscene-document-is-the-ir.md`. The IR is the dashscene
> document; `.dsb` is its file extension. The retirement of `SCD` recorded below
> stands; the naming of the IR as "DSB" does not.
```

Leave the rest of §20 unedited. It is history, and Branch 2 archives it verbatim.

- [ ] **Step 4: Point the glossary at the record**

`docs/technotes/glossary.md` is already correct — it says the IR is dashscene.
It just has no authority to cite.

Its `note` value (in the `status` block at the top, around line 7) currently
ends with: `Older drafts said "SCD" and "scdc"; those are not used here.`

**Append one sentence to that value:**

```text
Ruling: `docs/decisions/dashscene-document-is-the-ir.md`.
```

Do not retype the block. That `status` block is an indented code block, and its
lines are column-aligned under `note` — match the existing continuation indent
exactly and add nothing else. (This is why the sentence is given here as a
fragment rather than as a rewritten block: reproducing the block in this plan
would lose its indentation to the markdown formatter, and you would copy the
broken version.)

- [ ] **Step 5: Lint and commit**

```bash
dprint fmt docs/decisions/dashscene-document-is-the-ir.md docs/decisions/README.md docs/technotes/glossary.md
markdownlint docs/decisions/dashscene-document-is-the-ir.md docs/decisions/README.md docs/technotes/glossary.md
```

Expected: no output from `markdownlint`.

```bash
git add docs/decisions/ docs/technotes/glossary.md specs/SCOPE_DECISIONS.md
git commit -m "docs(docs): the dashscene document is the IR, not DSB"
```

### Task 2: Rename `Dsb` to `Document` in `dashc`

**Files:**

- Rename: `crates/dashc/src/dsb.rs` → `crates/dashc/src/document.rs`
- Modify: `crates/dashc/src/lib.rs:15,24,41,49,63,68,69,97,103,104,105,129,133`
- Modify: `crates/dashc/src/emit.rs:12-20,24,25,33,34,45,47,73`
- Modify: `crates/dashc/src/figma/mod.rs` (17 sites)
- Modify: `crates/dashc/src/figma/rest.rs:85,97,101,106`
- Modify: `crates/dashc/src/figma/triage.rs:17,61,143`
- Modify: `crates/dashc/Cargo.toml:8` (the `description` — published metadata)
- Modify: `crates/dashc/tests/round_trip.rs:6,8,19,91,92,93` (10 sites)
- Modify: `crates/dashc/tests/figma_lowering.rs` (10 sites)

**Interfaces:**

- Consumes: `docs/decisions/dashscene-document-is-the-ir.md` (Task 1) — cited
  from the module doc comment.
- Produces: `dashc_wasm::{Document, Node}` — the public IR types. `Box2D`,
  `Paint`, `compile`, `emit` keep their names. Note the **lib** crate is named
  `dashc_wasm` (the binary is `dashc`); tests import from `dashc_wasm`.

**This is a pure rename. No behavior changes, and the test suite must pass
unaltered — same tests, same assertions, before and after. A rename that needs a
test's assertions edited is not a rename; if that happens, stop and re-read the
diff.**

- [ ] **Step 1: Confirm the tests are green before touching anything**

```bash
cargo test -p dashc
```

Expected: PASS. Record the test count — the same count must pass at the end.

- [ ] **Step 2: Rename the module file**

```bash
git mv crates/dashc/src/dsb.rs crates/dashc/src/document.rs
```

- [ ] **Step 3: Rename the types inside it**

In `crates/dashc/src/document.rs`:

- `pub struct DsbNode` → `pub struct Node`
- `pub struct Dsb` → `pub struct Document`
- `impl Dsb` → `impl Document`
- `pub nodes: Vec<DsbNode>` → `pub nodes: Vec<Node>`
- `pub fn push(&mut self, node: DsbNode) -> u32` → `... node: Node ...`

And the intra-doc link in the node's doc comment:

```rust
// before:  /// One node of the document. `parent` is an index into [`Dsb::nodes`],
// after:   /// One node of the document. `parent` is an index into [`Document::nodes`],
```

- [ ] **Step 4: Fix the two collisions in `emit.rs`**

`emit.rs` is the only file that imports the flatbuffer types, and it imports
**four** names that now collide: `Document`, `DocumentArgs`, `Node`, `NodeArgs`.
Alias the flatbuffer side. Replace the import block at `crates/dashc/src/emit.rs:12-20`:

```rust
use dashbuf::{
    Color, CornerRadii, Document as FbDocument, DocumentArgs as FbDocumentArgs,
    FixedSizeLayout, Gradient, GradientArgs, GradientStop, Image, ImageArgs, ImageFill,
    ImageFillArgs, Mat23, NO_PAINT, NO_PARENT, Node as FbNode, NodeArgs as FbNodeArgs,
    Paint as BufPaint, PaintArgs, SolidFill, SolidFillArgs, Stroke, StrokeArgs, Vec2,
};
use dashpaint::{ImageAsset, PaintEntry, PaintKind};
use flatbuffers::{FlatBufferBuilder, WIPOffset};

use crate::document::{Document, Node, Paint};
```

Then update the body: every use of the flatbuffer `Document`/`DocumentArgs`/
`Node`/`NodeArgs` becomes the `Fb`-prefixed name, and the parameter `dsb: &Dsb`
becomes `doc: &Document`. Specifically:

- `pub fn emit(dsb: &Dsb) -> Vec<u8>` → `pub fn emit(doc: &Document) -> Vec<u8>`
- `dsb.nodes`, `dsb.images` → `doc.nodes`, `doc.images`
- `let nodes: Vec<WIPOffset<Node>>` → `Vec<WIPOffset<FbNode>>`
- `node: &DsbNode` → `node: &Node`
- `Document::create(...)` / `DocumentArgs { ... }` → `FbDocument::create(...)` /
  `FbDocumentArgs { ... }`
- the doc comment `/// Deterministic: the same [`Dsb`] always produces the same
  bytes (R7).` → `[`Document`]`

- [ ] **Step 5: Update `lib.rs`**

In `crates/dashc/src/lib.rs`:

- `mod dsb;` → `mod document;`
- `pub use dsb::{Box2D, Dsb, DsbNode, Paint};` →
  `pub use document::{Box2D, Document, Node, Paint};`
- `fn emit_and_validate(dsb: &Dsb)` → `fn emit_and_validate(doc: &Document)`
- `pub fn compile(dsb: &Dsb)` → `pub fn compile(doc: &Document)`
- every mention of the old type in a doc comment, whether an intra-doc link or
  plain code span, becomes `Document`

And the module doc's pipeline line, plus a pointer at the ruling:

```rust
// before:
//!   source             →  lower  →  Dsb  →  emit  →  validate  →  .dsb

// after:
//!   source             →  lower  →  Document  →  emit  →  validate  →  .dsb
//!
//! The IR is the dashscene document; `.dsb` is its file extension
//! (`docs/decisions/dashscene-document-is-the-ir.md`).
```

`lib.rs` and `main.rs` use `dashbuf::root_as_document`, a free function — it does
not collide and does not change.

- [ ] **Step 6: Update `figma/`**

`figma/mod.rs` (17 sites), `figma/rest.rs` (4 doc comments), `figma/triage.rs`
(3 comments). All are `Dsb` → `Document` / `DsbNode` → `Node` in code and prose.
`figma::lower` returns the IR, so its signature changes:

```rust
pub fn lower(...) -> Result<(Document, Found), Report>
```

- [ ] **Step 7: Update the published crate description**

`crates/dashc/Cargo.toml:8` — this is published metadata, so the retired name
would reach crates.io:

```toml
description = "Compiler CLI: Figma importer orchestration target, Figma-to-dashscene lowering, diagnostics, .dsb emission. Also builds to wasm32-unknown-unknown for the Deno importer."
```

(The old text also cited `DESIGN_1.md §4, §6.1`. Drop the citation here — Branch 2
deletes that file, and a `Cargo.toml` description is not the place for a doc
cross-reference.)

- [ ] **Step 8: Update the tests — imports and constructors only**

`tests/round_trip.rs:19`:

```rust
use dashc_wasm::{Box2D, Document, Node, Paint, compile, emit};
```

`tests/round_trip.rs:91-93`:

```rust
fn v03_document() -> Document {
    let mut doc = Document::new();
    let image = doc.push_image(ImageAsset {
```

Same treatment in `tests/figma_lowering.rs` (10 sites).

**Assertions are not touched.** Only type names, the local binding `dsb` → `doc`,
and doc comments.

- [ ] **Step 9: Verify the rename is complete and behavior-preserving**

```bash
grep -rn "\bDsb\b\|\bDsbNode\b" crates/ importers/ goldens/ || echo "clean"
```

Expected: `clean`. The only surviving `dsb` spelling anywhere is the `.dsb`
extension and the `dashbuf` crate name.

```bash
cargo test -p dashc
```

Expected: PASS, **with the same test count as Step 1**.

```bash
just build
```

Expected: green (this runs clippy `-D warnings` and `cargo fmt --check`).

- [ ] **Step 10: Commit**

```bash
git add crates/dashc/
git commit -m "refactor(dashc): the IR type is Document, not Dsb"
```

### Task 3: Land branch 1

- [ ] **Step 1: Squash and verify**

```bash
git rebase -i origin/main   # squash to one commit
just verify
```

Expected: green. `just verify` runs the commit-message lint over the branch range
and then `just build`.

- [ ] **Step 2: Open as draft, review, then ready**

Per AGENTS.md the review gate is "ready for review", not "PR opened":

```bash
git push -u origin fix/ir-naming
gh pr create --draft \
  --title "refactor(dashc): the IR type is Document, not Dsb" \
  --body "$(cat <<'BODY'
Settles what the IR is called, so the `specs/` gardening pass (#TBD-garden-PR)
can write prose that names it.

`SCOPE_DECISIONS.md` §20 named the IR **DSB**. `docs/technotes/glossary.md`,
landed the same day, named it **dashscene** and called `.dsb` the flatbuffer
extension. `specs/DESIGN_1.md` agreed with §20 on the name but expanded it a
third way ("dash scene binary" vs the glossary's "dashscene buffer"). Three
documents, one concept, no two agreeing.

Ruling: **dashscene is the IR; `.dsb` is the flatbuffer extension.** §20 is
superseded by `docs/decisions/dashscene-document-is-the-ir.md`.

`Dsb` was therefore misnamed — it is the in-memory document, not the buffer.
Renamed to `Document`, and `DsbNode` to `Node`. `emit.rs` aliases the four
flatbuffer types it imports (`FbDocument`, `FbDocumentArgs`, `FbNode`,
`FbNodeArgs`); it is the only file where both vocabularies are in scope.

Pure rename: no behavior change, and `cargo test -p dashc` passes with the same
test count and unaltered assertions before and after.
BODY
)"
```

Replace `#TBD-garden-PR` with the garden PR's number once it exists, or drop the
sentence if this lands first.

Run `/code-review` against the PR, capture every finding as a checklist in the
description, fix criticals, then `gh pr ready`.

- [ ] **Step 3: Merge**

```bash
gh pr merge --merge   # name the method explicitly; never trust the preselection
```

---

## Branch 2 — `docs/garden-specs`

Rebase onto `main` once Branch 1 lands. Docs-only: **no file under `crates/`,
`importers/`, or `goldens/` changes except its comments.**

The branch already exists and carries the design (§0) and this plan. Rebase it
first:

```bash
git rebase origin/main
```

### Task 4: Archive the originals

Do this **first**. Every later task deletes content from `specs/`, and the
archive is the only thing that keeps it.

**Files:**

- Create: `docs/archive/2026-07-14-design-1-seed.md` (copy of `specs/DESIGN_1.md`)
- Create: `docs/archive/2026-07-14-scope-decisions.md` (copy of
  `specs/SCOPE_DECISIONS.md`)

- [ ] **Step 1: Copy both verbatim**

```bash
cp specs/DESIGN_1.md docs/archive/2026-07-14-design-1-seed.md
cp specs/SCOPE_DECISIONS.md docs/archive/2026-07-14-scope-decisions.md
```

**Byte-for-byte. Do not lint, format, or fix them.** They keep their `SCD`,
`scdc`, and `.scb` — the archive is the record of what was actually written
(§3.3).

**`SCOPE_DECISIONS.md` §20 will already carry a "Superseded" banner** — Branch 1
put it there. **Do not strip it.** "Verbatim" here means as-of-retirement, not
as-of-first-writing: the archive records what the document said when it was
retired, and by then §20 had been overturned. Copy the file exactly as you find
it.

`dprint` and `markdownlint` must be configured to skip `docs/archive/`; verify
that they already are:

```bash
dprint check 2>&1 | grep archive || echo "archive is excluded — good"
```

If `docs/archive/` is **not** excluded, add it to `.dprintignore` and
`.markdownlintignore` as part of this task, and say so in the commit.

- [ ] **Step 2: Commit**

```bash
git add docs/archive/
git commit -m "docs(docs): archive the seed spec and the scope-decisions log verbatim"
```

### Task 5: `docs/specification/` — the requirements

**Files:**

- Create: `docs/specification/01-goals-and-requirements.md` — from `DESIGN_1.md`
  §1 "Goals and requirements" (G1-G3, R1-R7)
- Create: `docs/specification/02-principles.md` — from `DESIGN_1.md` §3
  "Principles" (P1-P5)
- Create: `docs/specification/03-target-hardware-rules.md` — from `DESIGN_1.md`
  §9 "Target-hardware rules" (R-T1..R-T5 + the texture policy)
- Create: `docs/specification/04-figma-vocabulary-profile.md` — from
  `DESIGN_1.md` §10.1, the NOW / LATER / REJECT vocabulary triage
- Create: `docs/specification/05-qualification.md` — from `DESIGN_1.md` §11
  "Plan", **its exit criteria E1-E6 only**. The slice map in the same section
  goes to `docs/roadmap.md` in Task 8, not here.
- Rename: `docs/specification/dashc-figma-lowering.md` →
  `docs/specification/06-dashc-figma-lowering.md`
- Modify: `docs/specification/README.md` (rewrite — it promises the migration in
  the future tense)

**Interfaces:**

- Produces: the five numbered specification files. Tasks 6, 7, 8, and 10 cite
  them by path. `05-qualification.md` is the one Task 8's roadmap links into (a
  slice closes named E-criteria).

- [ ] **Step 1: Move §1 verbatim into `01-goals-and-requirements.md`**

Identifiers `G1`-`G3` and `R1`-`R7` are copied **exactly as written**, including
`R1`'s "Perfect text quality" and `R3`'s "far less memory and CPU". They are not
measurable and that is a known, filed defect (§9) — rewriting them is the
MarkSpec slice's job, not this one. Add a header:

```text
# Goals and requirements

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §1

Requirement identifiers (`G1`-`G3`, `R1`-`R7`) are cited across the codebase and
are preserved verbatim. Several of `R1`-`R7` are not independently verifiable as
written; making them measurable is tracked separately and deliberately not done
in this pass.

Each requirement's proof lives in [05-qualification.md](05-qualification.md).
```

- [ ] **Step 2: `02-principles.md`** — DESIGN §3, P1-P5, verbatim.

P5's text currently says "DSB is a schema-first IR". Under Branch 1's ruling this
becomes:

```text
P5 — Figma compatibility is a property of one producer. The dashscene document
is a schema-first IR with its own spec and validator. The Figma exporter is one
client; the code DSLs are others. No producer's limitations define the format.
```

- [ ] **Step 3: `03-target-hardware-rules.md`** — DESIGN §9, R-T1..R-T5 plus the
      texture policy, verbatim.

- [ ] **Step 4: `04-figma-vocabulary-profile.md`** — DESIGN §10.1's NOW / LATER /
      REJECT triage.

This is a profile specification: it defines what the validator must accept, warn
on, and reject, which is what makes P4 ("vocabulary is validated, never
discovered") checkable. Cross-link it to `crates/dashscene-validator` and to
`docs/decisions/validator-three-gates.md`.

- [ ] **Step 5: `05-qualification.md` — the verification layer**

This is the file that makes the specification drive the tests. Exit criteria
E1-E6 from DESIGN §11. An exit criterion is not a requirement — it is the _proof_
of one. Each row states what it verifies and what executes it:

```text
# Qualification

    status  as-built, gardened 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §11

A requirement with no proof is indistinguishable from a requirement with one.
This file is the chain that closes that gap:

    requirement (R1)
      → criterion  (E2)                    this file
        → case     (an RTL corpus scene)   corpus/
          → proof  (a golden test)         goldens/ or a crate test

Criteria whose slice has not landed are listed as **open**, not omitted — a
missing proof must be visible.

## v0 exit criteria

| Criterion                         | Verifies | Status                                                                                           |
| --------------------------------- | -------- | ------------------------------------------------------------------------------------------------ |
| E1 same screen authored both ways | G1       | open — v0.9                                                                                      |
| E2 Arabic golden-stable           | R1       | open — v0.6                                                                                      |
| E3 stress corpus green            | R2       | partial — see #46                                                                                |
| E4 dirty Figma file → report      | R6       | open — v0.7                                                                                      |
| E5 variant switch via FLIP        | R4       | open — v0.4                                                                                      |
| E6 byte-identical `.dsb`          | R7       | **met** — guarded by the frozen `.dsb` fixture (`docs/decisions/dsb-frozen-fixture-r7-guard.md`) |
```

**Verify E3's and E6's status against the repo before writing them** — v0.3
closed since the design was written, and the fixture-corpus and R7-guard work
landed. Check `docs/decisions/dsb-frozen-fixture-r7-guard.md` and the epic #12
checklist. Do not copy the table above without checking; the statuses are the
part most likely to be stale.

The file carries no version in its name. "v0 exit criteria" is a heading inside
it; v1's criteria will be a second heading, not a second file.

- [ ] **Step 6: Renumber the file that landed under us**

```bash
git mv docs/specification/dashc-figma-lowering.md docs/specification/06-dashc-figma-lowering.md
```

Its content is **not** touched (its citations are repointed in Task 10). Then
find its inbound links and fix them:

```bash
grep -rn "specification/dashc-figma-lowering" --exclude-dir=.git --exclude-dir=target .
```

- [ ] **Step 7: Rewrite `docs/specification/README.md`**

It currently says "Nothing lives here yet". Replace with an index of `01`-`06`,
and state the numbering rule so the next author does not renumber on insert:

```text
# specification

Requirements: what the system must do.

Read in order. A new topic takes the next free number; nothing is renumbered to
make room, so gaps and out-of-sequence arrivals are expected and cost nothing.

- [01-goals-and-requirements.md](01-goals-and-requirements.md) — G1-G3, R1-R7
- [02-principles.md](02-principles.md) — P1-P5, binding on all downstream work
- [03-target-hardware-rules.md](03-target-hardware-rules.md) — R-T1..R-T5
- [04-figma-vocabulary-profile.md](04-figma-vocabulary-profile.md) — the
  NOW/LATER/REJECT triage the validator enforces
- [05-qualification.md](05-qualification.md) — E1-E6; requirement → criterion →
  case → proof
- [06-dashc-figma-lowering.md](06-dashc-figma-lowering.md) — the Figma lowering
  spec (story #16)
```

- [ ] **Step 8: Lint and commit**

```bash
dprint fmt docs/specification/
markdownlint docs/specification/
git add docs/specification/
git commit -m "docs(docs): garden the requirements into docs/specification"
```

### Task 6: `docs/design/architecture.md`

**Files:**

- Create: `docs/design/architecture.md`
- Modify: `docs/design/README.md` (it promises this migration in the future tense)

**Interfaces:**

- Consumes: `docs/specification/02-principles.md` (Task 5) — links to it rather
  than restating P1-P5.

- [ ] **Step 1: Write it thin**

Sources: DESIGN §2 (stack), §4 (pipeline, boundaries A and B), §5 (the document),
§6 (producers), §7 (common runtime), §8 (painters), §13 (workspace layout — note
this was _repaired_ by PR #152 and is current, not stale).

**The test to apply to every paragraph: does this exist anywhere else? If it
does, link — do not restate.** Four technotes landed on `main` (~990 lines) that
already cover producers, painters, and runtime content in depth:

- `docs/technotes/producers-and-ir.md`
- `docs/technotes/rendering-and-painters.md`
- `docs/technotes/runtime-content.md`
- `docs/technotes/glossary.md`

So `architecture.md` carries only what nothing else does — the stack, the
three-stage pipeline, boundaries A and B, and the component map — and links out
for the rest. It links down to the ten existing per-component records
(`dashbuf.md`, `dashpaint.md`, `dashscene-core-arena.md`, `dashscene-engine.md`,
`dashscene-skia.md`, `dashlang.md`, `dashcue.md`, `atlas-pipeline.md`,
`typeset-latin.md`, `goldens.md`) and to `dashc.md`.

- [ ] **Step 2: Record the one deliberate rule deviation**

The `sdd-working-memory-lifecycle` rule says shipped docs describe the system
as-built, and forward-looking concepts stay in `docs/wip/`. The Unity painter,
the lean native painter, the web painter, placeholders, and remote streaming are
all unbuilt — but they are _why the built parts have the shape they do_. Boundary
B exists precisely so painters are interchangeable; deleting the painters that do
not exist yet would delete the justification for the seam that does.

Each unbuilt component is marked **planned** and names the requirement or decision
that binds it. Add a note in the file saying the deviation was chosen, not
overlooked, so a later reader does not "fix" it.

- [ ] **Step 3: Rewrite `docs/design/README.md`** to describe what is there,
      replacing the future-tense promise. Add `architecture.md` at the top of the
      index.

- [ ] **Step 4: Lint and commit**

```bash
dprint fmt docs/design/
markdownlint docs/design/
git add docs/design/
git commit -m "docs(docs): add the system architecture record"
```

### Task 7: `docs/decisions/` — the ten records from `SCOPE_DECISIONS.md`

**Files:**

- Create: `docs/decisions/repo-staging-and-public-facade.md` (§1, lines 30-66)
- Create: `docs/decisions/crate-name-map.md` (§2, lines 67-130)
- Create: `docs/decisions/dsb-format-and-one-schema.md` (§3, lines 131-157)
- Create: `docs/decisions/figma-importer-deno-plus-dashc-wasm.md` (§4, lines 158-228)
- Create: `docs/decisions/unity-package-sited-in-this-repository.md` (§5, lines 229-252)
- Create: `docs/decisions/house-style.md` (§7, lines 317-414)
- Create: `docs/decisions/figma-corpus-self-authored-only.md` (§8 — the licensing
  ruling only)
- Create: `docs/decisions/figma-access-plan-and-pat-policy.md` (§11, lines 603-633)
- Create: `docs/decisions/annotator-plugin-contract-frozen.md` (§12, lines 688-726)
- Create: `docs/decisions/token-resolution-phase-split.md` (§13, lines 727-759)
- Create: `docs/decisions/no-authored-fill-weights.md` (from §19's fill-weights
  decline)
- Create: `corpus/figma-fixtures/README.md` (§8 — the fixture tables)
- Create: `docs/technotes/figma-plugin-api-findings.md` (§8 — the three
  plugin-API findings)
- Modify: `docs/decisions/staged-mutation-v01-scope.md` (fold in §9)
- Modify: `docs/decisions/README.md` (index all of the above)

**Line numbers are against `specs/SCOPE_DECISIONS.md` at `main` @ f71630c.**

- [ ] **Step 1: Write the ten straightforward records**

Each is one section of `SCOPE_DECISIONS.md`, rewritten as a decision record with
a `Status:`/`Date:` header. Keep the ruling; drop the "as of today" status prose
that has gone stale (§1's "GitHub access is blocked", §6's open-items list).

- [ ] **Step 2: Split §8 three ways**

§8 mixes a decision, a data description, and tool findings. It does not belong in
one file:

- The licensing ruling — "nothing enters `corpus/` that the project did not
  author" — is normative and binds all future fixture work →
  `docs/decisions/figma-corpus-self-authored-only.md`.
- The tier-1 fixture table, the tier-2 live-only targets, and the authoring status
  → `corpus/figma-fixtures/README.md`. It sits beside the data it describes,
  which is where a reader of `manifest.json` will look.
- The three Figma plugin-API findings (GRID frames read
  `gridColumnGap`/`gridRowGap` not `itemSpacing`; a WRAP frame needs
  `primaryAxisSizingMode = "FIXED"`; `GridTrackSize` exposes no track-level
  min/max) are informative and nothing depends on them →
  `docs/technotes/figma-plugin-api-findings.md`.

- [ ] **Step 3: Fold §9 into the existing record**

§9 (staged mutation lives in `dashscene-core`, not `dashcue`) is already covered
by `docs/decisions/staged-mutation-v01-scope.md`. Merge §9's reasoning into it
rather than creating a second record on the same subject.

- [ ] **Step 4: `docs/technotes/open-questions.md` — so `Q-N` citations resolve**

DESIGN §12 (lines 598-612) holds open questions `Q-1` through `Q-6`. They are
cited by identifier across the codebase, so deleting `specs/` without a home for
them would dangle every `Q-N` reference.

A technote is the right home: it is informative, nothing depends on it, and it
exists so identifiers stay resolvable. Write it as a **status index**, not a
restatement:

```text
# Technote — open questions

    status  index, 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §12

The seed document raised six open questions. This note exists so that a citation
of `Q-4` still resolves. It decides nothing.

| Question                                        | Status                                                |
| ----------------------------------------------- | ----------------------------------------------------- |
| Q-1 MSDF legibility below 14 px                 | **resolved** — `docs/decisions/q1-msdf-below-14px.md` |
| Q-2 …                                           | open — #NN                                            |
| Q-3 …                                           | open — #NN                                            |
| Q-4 layout fidelity: wrap, grid spans, baseline | open — #43                                            |
| Q-5 …                                           | open — #NN                                            |
| Q-6 …                                           | open — #NN                                            |
```

**Read DESIGN §12 and the issue list before filling this in.** Q-1 is resolved
(`q1-msdf-below-14px.md`). Q-4 maps to story #43 ("layout fidelity — wrap, grid
spans, baseline (Q-4)"), which names it in the title. Find the tracking issue for
each of the others; if one has no issue, file it — an open question with no issue
is an open question nobody is holding.

- [ ] **Step 4: Verify every "already gardened" claim before deleting**

These sections are deleted because a record already covers them. **Confirm each
against its named record before removing the section** — mechanically: open the
record, check it makes the section's claims.

| Section                 | Already covered by                                                                                                         |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| §14 Arabic atlas spike  | `technotes/msdf-arabic-atlas-spike.md` + `decisions/q1-msdf-below-14px.md`                                                 |
| §15 boundary B unified  | `decisions/boundary-b-unification.md`                                                                                      |
| §16 sectioned container | `decisions/dsb-sectioned-container.md`                                                                                     |
| §17 design session      | `decisions/asset-model-content-addressed-blobs.md`, `id-model-strings-compile-to-indices.md`, `remoting-two-transports.md` |
| §21 dashc wasm ABI      | `decisions/dashc-wasm-abi.md`                                                                                              |

§20 is **not** in this table. It is not "already gardened" — it is _overturned_,
by Branch 1's record. It gets no faithful record of its own, because writing one
would record a decision the project no longer holds.

- [ ] **Step 5: Index and commit**

```bash
dprint fmt docs/decisions/ docs/technotes/ corpus/figma-fixtures/README.md
markdownlint docs/decisions/ docs/technotes/ corpus/figma-fixtures/README.md
git add docs/decisions/ docs/technotes/ corpus/
git commit -m "docs(docs): garden the scope-decisions log into decision records"
```

### Task 8: `docs/roadmap.md` — the plan's shape

**Files:**

- Create: `docs/roadmap.md`

**Sources:** DESIGN §11 (slices v0.1-v0.9, plus the v1 and v2 outlines),
`SCOPE_DECISIONS.md` §18 (v0.1 retrospective), §19 (v0.2), §22 (v0.3), §23
(reactive bindings entering the plan).

- [ ] **Step 1: Write it as shape, not state**

The roadmap carries the plan's **shape**; GitHub keeps the plan's **state**. They
are different things, so nothing is duplicated and there is nothing to keep in
sync:

```text
shape (docs/roadmap.md)          state (GitHub)
-------------------------------  --------------------------------
which slices exist (v0.1-v0.9)   which stories exist
what each slice delivers         which are open, closed, assigned
inter-slice dependency edges     story-level dependency edges
which E-criteria a slice closes  debt triage and milestones
the epic issue number per slice  everything that churns weekly
the v1 and v2 outlines
```

The dividing line is churn. A slice-level dependency ("v0.6 needs v0.5's atlas")
changes at a phase-end plan revision — a handful of times across all of v0. A
story-level dependency ("#118 blocks #46") changes weekly and stays in the issue
body.

**Why the roadmap exists in the repo at all** (this reverses an earlier position,
so state it in the file): the promotion into public `driftsys/dashscene` may be a
fresh push, in which case the GitHub issues do not come with it and the plan is
the one engineering artifact that is lost. It is also not reviewable in a PR
alongside the code it plans, and not readable offline.

- [ ] **Step 2: Reflect the current slice state**

v0.1, v0.2, and v0.3 have closed. Their retrospectives are §18, §19, and §22. The
roadmap records the _revised_ slice map, not the original — and §23 adds reactive
bindings and the incremental commit to v0.4. **Read the epic issues before
writing this**; the design was written before v0.3 closed.

- [ ] **Step 3: Lint and commit**

```bash
dprint fmt docs/roadmap.md && markdownlint docs/roadmap.md
git add docs/roadmap.md
git commit -m "docs(docs): add the roadmap — slice shape in the repo, state in GitHub"
```

### Task 9: Technotes carry no decisions

**Files:**

- Create: 10 records in `docs/decisions/` (table below)
- Modify: `docs/technotes/producers-and-ir.md` (4 DECISION tags → links)
- Modify: `docs/technotes/rendering-and-painters.md` (2)
- Modify: `docs/technotes/runtime-content.md` (4)
- Modify: `docs/technotes/README.md` (state the rule)

**The rule being enforced (§4.6):** a **decision** is normative — it binds
downstream work. A **technote** is informative — nothing depends on it. A decision
filed as a technote is binding content in a home that advertises itself as
non-binding, so nothing knows it is bound.

**The two grades must survive the copy.** `DECISION` means settled.
`DECISION direction` means a leaning that has not been ratified. Flattening both
to "accepted" would silently promote four unratified directions into binding
decisions.

| Source technote             | §  | Record                                            | Status   |
| --------------------------- | -- | ------------------------------------------------- | -------- |
| `producers-and-ir.md`       | 1  | `dashc-lowers-figma-it-does-not-export.md`        | accepted |
| `producers-and-ir.md`       | 2  | `no-neutral-ir-above-dashscene.md`                | accepted |
| `producers-and-ir.md`       | 3  | `two-producer-entry-paths.md`                     | accepted |
| `producers-and-ir.md`       | 5  | `slint-reference-only-do-not-adopt.md`            | accepted |
| `rendering-and-painters.md` | 5  | `backend-tiering-unity-skia-lean.md`              | accepted |
| `rendering-and-painters.md` | 10 | `unity-painter-uses-brg.md`                       | proposed |
| `runtime-content.md`        | 2  | `downloaded-raster-needs-no-vector-engine.md`     | accepted |
| `runtime-content.md`        | 3  | `streamed-content-is-a-cross-process-producer.md` | proposed |
| `runtime-content.md`        | 4  | `lottie-bake-when-possible.md`                    | proposed |
| `runtime-content.md`        | 5  | `runtime-vector-via-thorvg-to-texture.md`         | accepted |

- [ ] **Step 1: Write the ten records**

Each carries a header:

```text
Status accepted (or: proposed — a direction, not yet ratified)
Date 2026-07-13 (the date it was decided, not today)
Source docs/technotes/<note>.md §N
```

`producers-and-ir.md` §1 **re-affirms** `SCOPE_DECISIONS.md` §4 rather than
deciding anything new. Its record is a cross-reference to the §4 record
(`figma-importer-deno-plus-dashc-wasm.md`, Task 7), not a duplicate ruling.

- [ ] **Step 2: Copy, do not move**

The technotes keep their prose — the reasoning, the alternatives, the `CANDIDATE`
and `OPEN` items. That reasoning is _why_ the decision reads as it does, and a
decision record is not the place for it. What changes in each note is the tag:

```text
DECISION (re-affirms SCOPE_DECISIONS §4; already reflected in the code).
```

becomes

```text
DECISION → [`docs/decisions/dashc-lowers-figma-it-does-not-export.md`](../decisions/dashc-lowers-figma-it-does-not-export.md)
```

The note stays readable end-to-end. It simply stops being the authority.

- [ ] **Step 3: State the rule in the technotes README**

```text
Technotes are **informative**. Nothing depends on a technote the way it depends
on a decision. If a note reaches a conclusion that binds downstream work, that
conclusion belongs in `docs/decisions/` and the note links to it.
```

- [ ] **Step 4: Verify no bare tags remain**

```bash
grep -rn "^DECISION" docs/technotes/ | grep -v "→" || echo "clean"
```

Expected: `clean`.

- [ ] **Step 5: Lint and commit**

```bash
dprint fmt docs/decisions/ docs/technotes/
markdownlint docs/decisions/ docs/technotes/
git add docs/decisions/ docs/technotes/
git commit -m "docs(docs): move the technotes' decisions into decision records"
```

### Task 10: Repoint every citation, then delete `specs/`

**Files:** 120 files, 265 live citations. By area:

```text
84  crates/            49  docs/decisions/
34  importers/         39  docs/design/
13  AGENTS.md          15  docs/technotes/
 9  goldens/            3  docs/book/
 3  corpus/             2  docs/specification/
```

**Not touched:** `docs/archive/` (91 hits — verbatim), `docs/wip/` (38 hits —
other sessions' work, §11).

**Interfaces:**

- Consumes: every record created in Tasks 5-9. A citation can only be repointed
  once its target exists, which is why this task is last.

- [ ] **Step 1: Build the citation → record map**

Every `DESIGN_1.md §N` and `SCOPE_DECISIONS.md §N` maps to exactly one new record.
The map is §3.1 and §3.2 of the design. Write it out once as a lookup table before
editing anything — the same citation appears many times, and deciding its target
per-occurrence is how inconsistencies get in.

- [ ] **Step 2: Repoint, area by area, committing per area**

Do **not** do this as one `sed` sweep. Each citation names a section, and sections
map to different files — `DESIGN_1.md §4` and `DESIGN_1.md §9` land in different
records. A blind substitution would point them at the same place.

Order: `crates/` → `importers/` → `goldens/` + `corpus/` → `docs/` → `AGENTS.md`
→ `docs/book/`. Commit after each.

Every edit is a one-line change in a comment, a description, or prose. **No source
logic is touched.**

- [ ] **Step 3: Reconcile the three shipped docs**

- **`AGENTS.md`** — "Read these two files before doing anything else in this repo"
  names both files; repoint at the new records. Its intro and its P5 both say
  **DSB** (PR #152 put it there) — both take Branch 1's ruling. Its layout section
  gains the requirement → case → proof chain.
- **`docs/book/overview.md`** — its "Where things live" section names both files,
  and its opening line still calls the project "dash".
- **`docs/specification/README.md`**, **`docs/design/README.md`**,
  **`docs/decisions/README.md`** — each promises this migration in the future
  tense. (Tasks 5-7 already rewrote these; confirm.)

- [ ] **Step 4: Delete `specs/`**

```bash
git rm -r specs/
```

- [ ] **Step 5: Verify — the full §8 checklist**

```bash
# 1. no dangling citations
grep -rIn "DESIGN_1\|SCOPE_DECISIONS" --exclude-dir=.git --exclude-dir=target \
  --exclude-dir=archive --exclude-dir=wip . || echo "clean"

# 2. SCD regression guard (PR #152 already achieved this)
grep -rIn "\bSCD\b\|scdc" --exclude-dir=.git --exclude-dir=target \
  --exclude-dir=archive . || echo "clean"

# 3. specs/ is gone
test ! -d specs && echo "specs/ deleted"

# 4. no bare DECISION tags in technotes
grep -rn "^DECISION" docs/technotes/ | grep -v "→" || echo "clean"

# 5. docs/wip/ is byte-identical to main except this plan and its design
git diff --stat origin/main -- docs/wip/
```

Expected on (5): **exactly two files** — the design and this plan. If any other
`docs/wip/` file appears, the branch has touched another session's work and the
§11 exception has been violated. Revert it.

`.scb` is expected to survive in exactly one place outside the archive: inside a
block quotation in `docs/decisions/staged-mutation-v01-scope.md`, which quotes
DESIGN §4's ".scb is one way to populate it" verbatim. A superseding record may
retire a name; it may not edit the words it quotes. The quote keeps `.scb` and
cites `docs/archive/`.

- [ ] **Step 6: Every relative link in `docs/` resolves**

```bash
grep -rhoE "\]\(([^)]+\.md)\)" docs/ --include=*.md | sed -E 's/.*\((.*)\)/\1/' \
  | sort -u | while read -r l; do
      case "$l" in http*) continue;; esac
      # resolve relative to each referring file; simplest check: does a file of
      # that basename exist anywhere under docs/?
      find docs -name "$(basename "$l")" -print -quit | grep -q . || echo "DANGLING: $l"
    done
```

Any `DANGLING:` output is a failure. (This basename check is a coarse net; if it
is noisy, use a proper link checker.)

- [ ] **Step 7: Full build, then commit**

```bash
just build
```

Expected: green.

```bash
git add -A
git commit -m "docs(repo): repoint every citation and delete specs/"
```

### Task 11: Land branch 2

- [ ] **Step 1: Squash and verify**

```bash
git rebase -i origin/main   # squash to one commit
just verify
```

- [ ] **Step 2: Open as draft; disclose the wip exception**

The PR body must state, explicitly, that `docs/wip/` is non-empty **on purpose**:

> `docs/wip/` is not empty on this branch. It carries this branch's design and
> plan, plus two live specs belonging to other sessions' in-flight v0.4 work
> (`2026-07-13-reactive-bindings-spec.md`,
> `2026-07-13-dirty-set-boundary-b-plan.md`).
>
> Those two files were already on `main` before this branch existed, so the
> `wip-gate` failure is a condition this branch **inherits, not one it
> introduces**. Gardening them here would archive another session's unfinished
> work and would make the gate go green for the wrong reason. Whoever finishes
> the reactive-bindings work gardens them.
>
> This is an accepted, disclosed exception to the `sdd-working-memory-lifecycle`
> rule, granted 2026-07-14.

Then run `/code-review` against the PR, capture every finding as a checklist, fix
criticals, and `gh pr ready`.

- [ ] **Step 3: Merge**

```bash
gh pr merge --merge
```

### Task 12: File the deferred work

Named in §9 as deliberate deferrals, so they do not read as oversights. File one
issue each, linked to the garden PR:

- [ ] **`debt(docs)`: adopt MarkSpec for `docs/specification/`** — typed entries,
      stamped ULIDs, `Satisfies:`/`Verified-by:` traces to tests. The requirement
      identifiers `G1`/`R1`/`P4`/`R-T2`/`E3` were preserved verbatim in this pass
      precisely so a MarkSpec adoption can key on them.
- [ ] **`debt(docs)`: make R1-R7 measurable.** Several are not independently
      verifiable as written — "perfect text quality" (R1), "high performance"
      (R1), "far less memory and CPU than a game engine" (R3). A tester cannot
      pass or fail them. This needs real target-hardware numbers, which is why it
      is not folded into a migration.
- [ ] **`debt(docs)`: ratify or drop the four `proposed` decisions** — Unity/BRG,
      streamed content, Lottie baking, and the Skia entry tier (Task 9). They are
      directions, not rulings, and they are now visible as such.
