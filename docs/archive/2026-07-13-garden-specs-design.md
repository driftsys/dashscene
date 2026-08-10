# Garden `specs/` into the `docs/` taxonomy — design

    status   design, approved 2026-07-13; re-planned 2026-07-14 against
             main @ f71630c after 24 commits landed under it
    branch   docs/garden-specs
    scope    dissolve specs/DESIGN_1.md and specs/SCOPE_DECISIONS.md into
             docs/specification, docs/design, docs/decisions, docs/technotes,
             and docs/archive; settle the IR's name; delete specs/

## 0. What the 2026-07-14 re-plan changed

The design was written against `main` on 2026-07-13 and 24 commits landed
before it was executed. Five of them change this plan. Each is folded into the
sections below; they are listed here so a reader knows what moved and why.

1. **The SCD rename already shipped** (PR #152), and it went the opposite way
   from §5 as originally written: it made `DSB` the name of the IR. That
   conflicts with `docs/technotes/glossary.md`, which landed the same day and
   calls the IR **dashscene**, with `.dsb` as the flatbuffer extension. Both are
   on `main`. §5 is rewritten to settle it: **dashscene is the IR; `.dsb` is the
   flatbuffer extension.** `SCOPE_DECISIONS.md` §20 is superseded, not gardened.
2. **`SCOPE_DECISIONS.md` grew to §23** (959 → 1268 lines). §3.2 gains
   dispositions for §20–§23.
3. **`DESIGN_1.md` §13 was repaired**, not left stale — it now shows the real
   workspace layout. Its disposition changes from "delete" to "feed
   `architecture.md`".
4. **`docs/specification/` is no longer empty** — `dashc-figma-lowering.md`
   landed in it.
5. **Four technotes landed** (~990 lines) carrying ten items tagged `DECISION`.
   Normative content in an informative home. §4.6 is new: the decisions are
   copied out into records, and the technotes keep only their reasoning.

Two scope rulings taken in the re-plan:

- **`docs/specification/` is numbered; nothing else is.** It is read
  front-to-back; `docs/design/` and `docs/decisions/` are reference sets entered
  by name. No `adr_NN_` prefix: the 37 existing records are cited by path from 22
  places in source code, and a global counter collides across parallel branches.
- **`docs/wip/` is left exactly as it is, this once** (user instruction,
  2026-07-14). It holds two live specs from other sessions, plus this design.
  The `sdd-working-memory-lifecycle` rule wants `docs/wip/` empty on a
  `main`-targeting branch; that step is deliberately skipped here rather than
  garden another session's in-flight work out from under it. §11 records the
  exception.

## 1. Why

`docs/specification/` holds no requirements. Its README says they are supposed
to live there and currently do not:

> Nothing lives here yet — the project's requirements currently live in
> `specs/DESIGN_1.md` (goals G1-G3, hard requirements R1-R7). They'll move
> here [...] as future work gardens them in.

(The README's "nothing lives here yet" is now literally out of date — story #16
landed `dashc-figma-lowering.md` beside it on 2026-07-14 — but its point stands:
the requirements are still in `specs/`, which is what this branch is for.)

`docs/design/README.md` and `docs/decisions/README.md` carry the same
promise for architecture and decisions. The migration was deferred twice —
`SCOPE_DECISIONS.md` §6 and §7 both record the reason: it would break the
many `DESIGN_1.md §N` / `SCOPE_DECISIONS.md §N` citations already written
across the codebase.

Three further problems have accumulated since:

- **Stale status.** `SCOPE_DECISIONS.md`'s opening update note and most of
  §6 state that `driftsys/dashscene-staging` does not exist, that GitHub
  access is blocked, and that a local scaffold is waiting to be pushed. All
  three were true on 2026-07-11 and are false now. A reader has no way to
  know that.
- **Superseded content presented as current.** `DESIGN_1.md`'s header still
  calls the project "dash" and names the repo `driftsys/dash`. (Its §13 was
  repaired by PR #152 and now shows the real workspace layout — this bullet
  originally called §13 stale, and that half of it no longer applies.)
- **Two names for the IR.** `SCD` itself is gone — PR #152 retired it on
  2026-07-13, after this design was written. But the rename left the project
  with two live names for the same thing: `SCOPE_DECISIONS.md` §20 and
  `crates/dashc` say the IR is **DSB**, while `docs/technotes/glossary.md`,
  landed the same day, says it is **dashscene** and that `.dsb` is merely the
  flatbuffer extension. Both are on `main`. §5 settles it.

The requirement set is also not traceable to the tests that verify it. The
v0 exit criteria E1-E6 read as acceptance criteria but name no verifier, so
a requirement with no proof is indistinguishable from one with a proof.

## 2. Decisions taken in the design session

1. **`specs/` is retired entirely.** There is no surviving `specs/`
   directory. Its content lands in the four homes named by the
   `sdd-working-memory-lifecycle` rule — `docs/specification/`,
   `docs/design/`, `docs/decisions/`, `docs/technotes/` — plus
   `docs/roadmap.md` (decision 4) and `docs/archive/` for the verbatim
   originals.
2. **The specification is plain markdown now, MarkSpec later.** Requirement
   identifiers (`G1`, `R1`, `P4`, `R-T2`, `E3`, `Q-4`) are preserved
   verbatim, because they are cited across the codebase and because a later
   MarkSpec adoption keys on them. An issue is filed to track that adoption.
3. **Every citation is repointed in this branch.** 265 live citations across 120
   files cite `DESIGN_1.md §N` or `SCOPE_DECISIONS.md §N` (§7 has the
   measurement and the method). Each is rewritten to name its new record. Nothing
   is left dangling and no citation is downgraded to a bare concept name.
4. **The plan gets an in-repo home: `docs/roadmap.md`, carrying shape only.**
   See §4.5. GitHub keeps the plan's state.
5. **`corpus/` and `goldens/` do not move.** They stay top-level. The
   specification traces into them rather than absorbing them.

Decision 4 reverses an earlier position in this session (that GitHub alone
was sufficient). The argument that changed it is in §4.5.

**Added in the 2026-07-14 re-plan:**

- **`docs/specification/` is numbered; no other folder is.** It is read in
  order; the others are entered by name. Numbers are assigned, never reshuffled.
  See §4.1.
- **No `adr_NN_` prefix on decision records.** The 37 existing records are cited
  by path from 22 places in source code and ~150 places in docs, so a rename
  doubles the diff for no semantic gain — and a monotonic counter collides across
  the parallel branches this repo actually runs. Chronology already lives in git;
  the filename is the citable handle, and `boundary-b-unification` is a better
  one than `ADR-14`.
- **Technotes hold recommendations only.** Every `DECISION`-tagged item in
  `docs/technotes/` is copied into a decision record, preserving the
  settled-versus-direction distinction as a `Status:` line. See §4.6.
- **dashscene is the IR; `.dsb` is the flatbuffer extension.**
  `SCOPE_DECISIONS.md` §20 is superseded. See §5.
- **`docs/wip/` is left exactly as it is, this once.** See §11.

## 3. Disposition of every section

Every section of both files has exactly one disposition. Nothing is dropped
without being named here.

### 3.1 `DESIGN_1.md`

| Section                             | Disposition                                              |
| ----------------------------------- | -------------------------------------------------------- |
| header                              | superseded (project is dashscene; crate names are real)  |
| §1 goals G1-G3, requirements R1-R7  | `docs/specification/01-goals-and-requirements.md`        |
| §2 stack                            | `docs/design/architecture.md`                            |
| §3 principles P1-P5                 | `docs/specification/02-principles.md`                    |
| §4 pipeline, boundaries A and B     | `docs/design/architecture.md`                            |
| §5 the document                     | `docs/design/architecture.md` → `docs/design/dashbuf.md` |
| §6 producers                        | `docs/design/architecture.md`                            |
| §7 common runtime                   | `docs/design/architecture.md`                            |
| §8 painters                         | `docs/design/architecture.md`                            |
| §9 target-hardware rules R-T1..R-T5 | `docs/specification/03-target-hardware-rules.md`         |
| §10.1 Figma vocabulary triage       | `docs/specification/04-figma-vocabulary-profile.md`      |
| §10.2 placeholders                  | `docs/design/architecture.md` (marked planned)           |
| §11 exit criteria E1-E6             | `docs/specification/05-qualification.md`                 |
| §11 slices v0.1-v0.9                | `docs/roadmap.md`                                        |
| §11 v1 and v2 outlines              | `docs/roadmap.md`                                        |
| §12 open questions Q-1..Q-6         | `docs/technotes/open-questions.md`                       |
| §13 workspace layout                | `docs/design/architecture.md` — **changed 2026-07-14**   |

**§13's disposition changed.** The original design deleted it as superseded,
because it listed an `scd-*` crate family that was never adopted. PR #152
repaired it: it now shows the layout that actually exists. It is therefore
current content, and it feeds `architecture.md` rather than being dropped.

### 3.2 `SCOPE_DECISIONS.md`

| Section                    | Disposition                                                                                                         |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| opening update note        | deleted — stale (repo exists, GitHub reachable, scaffold pushed)                                                    |
| §1 repo strategy           | `docs/decisions/repo-staging-and-public-facade.md`                                                                  |
| §2 crate-name map          | `docs/decisions/crate-name-map.md`                                                                                  |
| §3 `.dsb` format           | `docs/decisions/dsb-format-and-one-schema.md`                                                                       |
| §4 Deno importer           | `docs/decisions/figma-importer-deno-plus-dashc-wasm.md`                                                             |
| §5 Unity separate repo     | `docs/decisions/unity-separate-repo-deferred.md`                                                                    |
| §6 open items / blocked    | deleted — every item is resolved or is a GitHub issue                                                               |
| §7 house style             | `docs/decisions/house-style.md`                                                                                     |
| §8 Figma fixture corpus    | split three ways — see below                                                                                        |
| §9 staged mutation in core | folded into the existing `docs/decisions/staged-mutation-v01-scope.md`                                              |
| §10 plan tracked as issues | the process rules stay in AGENTS.md (already there); the phase-end revision ritual is restated in `docs/roadmap.md` |
| §11 Figma access           | `docs/decisions/figma-access-plan-and-pat-policy.md`                                                                |
| §12 annotator plugin       | `docs/decisions/annotator-plugin-contract-frozen.md`                                                                |
| §13 token resolution       | `docs/decisions/token-resolution-phase-split.md`                                                                    |
| §14 Arabic atlas spike     | deleted — already gardened (`technotes/msdf-arabic-atlas-spike.md`, `decisions/q1-msdf-below-14px.md`)              |
| §15 boundary B unified     | deleted — already gardened (`decisions/boundary-b-unification.md`)                                                  |
| §16 sectioned container    | deleted — already gardened (`decisions/dsb-sectioned-container.md`)                                                 |
| §17 design session         | deleted — already gardened (asset-model, id-model, remoting records)                                                |
| §18 v0.1 retrospective     | deleted — outcomes are on the issues; raw text survives in the archive                                              |
| §19 v0.2 retrospective     | deleted — same; the fill-weights decline gets `docs/decisions/no-authored-fill-weights.md`                          |
| §20 the IR is named DSB    | **superseded, not gardened** — see §5; `docs/decisions/dashscene-document-is-the-ir.md` overturns it                |
| §21 dashc wasm ABI         | deleted — already gardened (`docs/decisions/dashc-wasm-abi.md`, landed with story #17)                              |
| §22 v0.3 retrospective     | `docs/roadmap.md` — the v0.3 outcomes and the revised slice map                                                     |
| §23 reactive bindings      | `docs/roadmap.md` (the slice-level scope change) — the design itself is live working memory in `docs/wip/`          |

**§20 is the one section that is overturned rather than relocated.** Every other
row here either moves its content to a new home or deletes text whose ruling
still stands and is recorded elsewhere. §20's ruling does not stand: §5 reverses
it. It is not given a faithful decision record, because writing one would record
a decision the project no longer holds. The superseding record cites it by name
so the reversal is traceable, and §20's text survives in
`docs/archive/2026-07-14-scope-decisions.md` — under the superseded banner Branch
1 adds to it, and otherwise unedited (§3.3).

**§23 is deliberately split.** The slice-level fact — that reactive bindings and
the incremental commit enter the plan — belongs in the roadmap, because that is
the roadmap's job (slice shape). The design content behind it is still an active
session's working memory in `docs/wip/2026-07-13-reactive-bindings-spec.md`, and
this branch does not touch `docs/wip/` (§11).

Each "already gardened" deletion is verified against the named record before
the section is removed. The check is mechanical: the record exists and covers
the section's claims.

**§8 splits three ways.** It mixes a decision, a test-coverage description,
and a set of tool findings:

- The licensing ruling ("nothing enters `corpus/` that the project did not
  author") is normative and binds all future fixture work →
  `docs/decisions/figma-corpus-self-authored-only.md`.
- The tier-1 fixture table, the tier-2 live-only targets, and the current
  authoring status → a new `corpus/figma-fixtures/README.md`. That directory
  holds only `.gitkeep` and `manifest.json` today, so the README is created
  by this change. It sits beside the data it describes, which is where a
  reader of `manifest.json` will look.
- The three Figma plugin-API findings (GRID frames read
  `gridColumnGap`/`gridRowGap` not `itemSpacing`; a WRAP frame needs
  `primaryAxisSizingMode = "FIXED"`; `GridTrackSize` exposes no track-level
  min/max) are informative and nothing depends on them →
  `docs/technotes/figma-plugin-api-findings.md`.

### 3.3 Archive

Both originals are copied verbatim to `docs/archive/`:

    docs/archive/2026-07-14-design-1-seed.md
    docs/archive/2026-07-14-scope-decisions.md

They are never edited again. This is why their `SCD` / `scdc` / `.scb`
references are left in place: the archive is the historical record of what
was actually written, not a doc to keep current.

**"Verbatim" means as-of-retirement, not as-of-first-writing** (ruling,
2026-07-14). Branch 1 (§11) adds a superseded banner to `SCOPE_DECISIONS.md` §20
before this archive is taken, so the archived copy carries that banner. That is
deliberate, and Branch 2 must not strip it.

The banner is what stops `main` contradicting itself in the window between the
two branches merging: Branch 1 renames the type to `Document`, so without the
banner `main` would carry code that says `Document` and a §20 that says the IR is
"DSB", with nothing connecting them. And a reader of the archive is better served
by a §20 that admits it was overturned than by one that does not.

Nothing below the banner is edited.

## 4. New records

### 4.1 `docs/specification/`

**Numbered, and it is the only folder that is** (re-plan ruling, §0). The
specification is read front-to-back — goals, then the principles that bind them,
then the constraints, then the profile, then the proof. The number carries that
order. `docs/design/` and `docs/decisions/` are reference sets a reader enters by
name, so numbering them would impose an order that does not exist (there is no
sense in which `dashcue` precedes `goldens`).

    01-goals-and-requirements.md    G1-G3, R1-R7 — identifiers verbatim
    02-principles.md                P1-P5
    03-target-hardware-rules.md     R-T1..R-T5 + the texture policy
    04-figma-vocabulary-profile.md  the NOW / LATER / REJECT triage
    05-qualification.md             E1-E6 — the verification layer
    06-dashc-figma-lowering.md      already on main; renamed into the scheme

- `01` — G1-G3 and R1-R7, verbatim identifiers. They are cited across the
  codebase, and a later MarkSpec adoption keys on them.
- `02` — P1-P5. Normative and binding on all downstream work; AGENTS.md restates
  them and code comments cite them by identifier.
- `03` — R-T1..R-T5 plus the texture policy.
- `04` — the NOW / LATER / REJECT triage. This is a profile specification: it
  defines what the validator must accept, warn on, and reject, which is what
  makes P4 ("vocabulary is validated, never discovered") checkable.
- `05` — the verification layer. See below.
- `06` — **not new.** `docs/specification/dashc-figma-lowering.md` landed on
  `main` with story #16 while this branch was open. It is renamed into the
  numbering scheme and its inbound citations are repointed. Its content is not
  touched.

**Numbering does not renumber on insert.** A new topic takes the next free
number; nothing is shuffled to make room. The order the numbers encode is the
reading order they were introduced in, not a ranking, so a gap or an
out-of-sequence arrival costs nothing. This is the property that makes the
scheme safe in a repo where several branches add specification files in
parallel — the failure mode that rules out `adr_NN_` prefixes in
`docs/decisions/` does not arise here, because nothing depends on the numbers
being contiguous.

**Quality and performance requirements are not given their own file.** They
exist — R1 (text quality), R3 (memory and CPU), R5 (cold start) in `01`, and the
target-hardware rules in `03` — but they are not regrouped. Pulling them out
would mean renumbering requirement identifiers that are cited across the
codebase, and that is content surgery, not a move. It belongs to the
measurability slice (§9), where the identifiers can be reworked once, under a
rule that enforces measurability rather than merely asserting it.

**`qualification.md` is the file that makes the specification drive the
tests.** It holds the exit criteria E1-E6, and an exit criterion is not a
requirement — it is the _proof_ of a requirement. Each entry states which
requirement it verifies, which corpus case exercises it, and which test
executes it:

| Criterion                         | Verifies | Status                      |
| --------------------------------- | -------- | --------------------------- |
| E1 same screen authored both ways | G1       | open — v0.9                 |
| E2 Arabic golden-stable           | R1       | open — v0.6                 |
| E3 stress corpus green            | R2       | partial — 2 of 6 constructs |
| E4 dirty Figma file → report      | R6       | open — v0.7                 |
| E5 variant switch via FLIP        | R4       | open — v0.4                 |
| E6 byte-identical `.dsb`          | R7       | open — v0.7                 |

This closes the chain the specification exists to carry:

    requirement (R1)
      → criterion  (E2)                    docs/specification/05-qualification.md
        → case     (an RTL corpus scene)   corpus/
          → proof  (a golden test)         goldens/ or a crate test

Without the criterion, R1 reads "perfect text quality" and nothing can fail
it. With it, R1 is proven by E2, E2 is proven by a named golden, and a
missing golden is a visible gap rather than an absence nobody notices. That
is why criteria whose slice has not landed are listed as **open** rather than
omitted.

The file carries no version in its name. "v0 exit criteria" is a heading
inside it; v1's criteria will be a second heading, not a second file.

The same chain is restated in AGENTS.md's layout section, so a contributor
meets it without reading the specification first.

### 4.2 `docs/design/architecture.md`

The system-wide architecture: stack, the three-stage pipeline, boundaries A
and B, the document, the producers, the common runtime, the painters. It
links down into the ten per-component records that already exist
(`dashbuf.md`, `dashpaint.md`, `dashscene-core-arena.md`,
`dashscene-engine.md`, `dashscene-skia.md`, `dashlang.md`, `dashcue.md`,
`atlas-pipeline.md`, `typeset-latin.md`, `goldens.md`).

**It is written thin — changed 2026-07-14.** The original design had it carry
the producer, painter, and runtime material in full. Four technotes landed on
`main` since (~990 lines) that already cover exactly that ground:
`producers-and-ir.md`, `rendering-and-painters.md`, `runtime-content.md`, and
`glossary.md`. Restating them would create a second copy that drifts.

So `architecture.md` carries only what nothing else does — the stack, the
three-stage pipeline, boundaries A and B, and the component map — and links out
for the rest. The test applied to every paragraph: **does this exist anywhere
else?** If it does, link; do not restate.

**Deviation from the `sdd-working-memory-lifecycle` rule, taken
deliberately.** The rule says shipped docs describe the system as-built, and
that forward-looking concepts stay in `docs/wip/` until implemented. The
Unity painter, the lean native painter, the web painter, placeholders and
node replacement, and remote streaming are all unbuilt. They cannot go to
`docs/wip/` — that directory is transient working memory for an active
session, and the rule treats a non-empty `docs/wip/` on a `main`-targeting
branch as unfinished work, not a mergeable state.

They belong in `architecture.md` because they are the reason the built parts
have the shape they do: boundary B exists precisely so that painters are
interchangeable, and deleting the painters that do not exist yet would delete
the justification for the seam that does. Each unbuilt component is marked
**planned** and names the requirement or decision that binds it. The rule's
concern is that unbuilt things not be described as if built; an explicit
status marker satisfies that. The deviation is recorded in the record itself
so a later reader knows it was chosen rather than overlooked.

### 4.3 `docs/decisions/` — new records

Three groups. Twenty-two records in total, and only the first group requires new
judgment — the other two relocate rulings that already exist.

**From `SCOPE_DECISIONS.md`** — ten records, listed in §3.2: repo strategy, the
crate-name map, the `.dsb` format, the Deno importer, Unity's separate repo,
house style, Figma access, the annotator contract, token resolution, and the
self-authored-corpus rule. Plus `no-authored-fill-weights.md` from §19.

**From the technotes** — ten records, listed in §4.6, each carrying a `Status:`
of `accepted` or `proposed`.

**New in this branch** — one:

- `dashscene-document-is-the-ir.md` — dashscene is the IR; `.dsb` is the
  flatbuffer extension. Explicitly supersedes `SCOPE_DECISIONS.md` §20. See §5.
  This is the only record here that decides something the project had not
  already decided; it lands on **branch 1** (§11), ahead of the garden pass,
  because the prose the garden pass writes depends on it.

### 4.4 `docs/technotes/`

- `open-questions.md` — Q-1 through Q-6 as a status index, so that a citation
  of `Q-4` still resolves. Q-1 is resolved and points at
  `docs/decisions/q1-msdf-below-14px.md`. Q-2 through Q-6 each point at the
  GitHub issue tracking them. A technote is the right home: it is
  informative, nothing depends on it, and it exists so identifiers stay
  resolvable.
- `figma-plugin-api-findings.md` — the three plugin-API findings from
  `SCOPE_DECISIONS.md` §8.

### 4.5 `docs/roadmap.md` — the plan's shape

The plan needs an in-repo home, and it did not have one. Three reasons:

- **It does not survive the promotion.**
  `docs/decisions/repo-staging-and-public-facade.md` (from
  `SCOPE_DECISIONS.md` §1) records that this repo's content is eventually
  promoted into public `driftsys/dashscene`, and that the mechanism — fresh
  push or history merge — is intentionally undecided. If it is a fresh push,
  the GitHub issues do not come with it, and the plan is the one engineering
  artifact that is lost.
- **It is not reviewable.** A change to the plan cannot be proposed,
  discussed, and approved in a pull request alongside the code it plans.
- **It is not readable offline**, and it is not versioned with the code.

`docs/roadmap.md` therefore carries the plan's **shape**, and GitHub keeps
the plan's **state**. They are different things, so nothing is duplicated and
there is nothing to keep in sync.

    shape (docs/roadmap.md)          state (GitHub)
    -------------------------------  --------------------------------
    which slices exist (v0.1-v0.9)   which stories exist
    what each slice delivers         which are open, closed, assigned
    inter-slice dependency edges     story-level dependency edges
    which E-criteria a slice closes  debt triage and milestones
    the epic issue number per slice  everything that churns weekly
    the v1 and v2 outlines

The dividing line is churn. A slice-level dependency ("v0.6 needs v0.5's
atlas") changes at a phase-end plan revision, which
`SCOPE_DECISIONS.md` §10 already institutionalizes as a ritual — a handful of
times across the whole of v0. A story-level dependency ("#118 blocks #46")
changes weekly, and belongs in the issue body where it already lives.

`docs/roadmap.md` is not gardened working memory, so it is not one of the
four homes the `sdd-working-memory-lifecycle` rule defines, and it does not
need to be — the rule governs where spec-and-plan output lands, not every
document in `docs/`. `docs/book/` already sits outside the four on the same
basis. The roadmap is a curated living document, updated deliberately at each
phase-end revision.

### 4.6 Technotes carry no decisions — new, 2026-07-14

Four technotes landed on `main` on 2026-07-13. Three of them carry items tagged
`DECISION`, which is a taxonomy violation: the `sdd-working-memory-lifecycle`
rule makes a **decision** normative — it binds downstream work and is traced to
what it affects — and a **technote** informative, with nothing depending on it. A
decision filed as a technote is binding content in a home that advertises itself
as non-binding, so nothing knows it is bound.

**Ruling (user, 2026-07-14): technotes hold recommendations only. Every
`DECISION` item is copied into a decision record.**

The tags come in two grades and the distinction must survive the copy. `DECISION`
means settled. `DECISION direction` means a leaning that has not been ratified.
Flattening both to "accepted" would silently promote four unratified directions
into binding decisions. Each new record therefore carries a `Status:` line —
`accepted` or `proposed` — and the four directions land as `proposed`.

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

`producers-and-ir.md` §1 re-affirms `SCOPE_DECISIONS.md` §4 rather than deciding
anything new, so its record is a cross-reference to the §4 record
(`figma-importer-deno-plus-dashc-wasm.md`), not a duplicate ruling.

**Copied, not moved.** The technotes keep their prose — the reasoning, the
alternatives, the `CANDIDATE` and `OPEN` items — because that reasoning is why
the decision reads as it does, and a decision record is not the place for it.
What changes in each technote is the tag: `DECISION` becomes a link to the
record that now holds it. The note stays readable end-to-end; it simply stops
being the authority.

## 5. Settling the IR's name

**Rewritten 2026-07-14.** SCD is already gone — PR #152 retired it on
2026-07-13, before this branch ran. The open problem is no longer SCD; it is
that `main` now carries **three descriptions of the same name**, all landed the
same day, and no two of them agree:

| Source                                   | The IR is called | `.dsb` is                      | `DSB` expands to    |
| ---------------------------------------- | ---------------- | ------------------------------ | ------------------- |
| `SCOPE_DECISIONS.md` §20, `crates/dashc` | **DSB**          | the file extension             | —                   |
| `specs/DESIGN_1.md` naming note          | **DSB**          | the format that shipped        | "dash scene binary" |
| `docs/technotes/glossary.md`             | **dashscene**    | "dashscene buffer", the format | "dashscene buffer"  |

Three documents, one concept, and even the two that agree on the _name_ disagree
on what it _stands for_ — "dash scene binary" versus "dashscene buffer". That is
what an unsettled name looks like a day after it was settled, and it is why this
cannot be left to be tidied later.

**Ruling (user, 2026-07-14): dashscene is the IR. `.dsb` is the flatbuffer
extension.** The glossary is right and §20 is wrong.

The reason is the one §20 argued past. §20's case was that "two names for one
thing is a cost this removes" — but the IR and its serialization are not one
thing. `.dsb` is _one_ way to carry the document; the arena in `dashscene-core`
is another, and a producer can populate it without a `.dsb` ever existing.
Naming the IR after one of its encodings makes the other encoding sound
secondary, and it makes P5 ("DSB is a schema-first IR with its own spec and
validator") read as though the file format is what gets validated. The
document is what gets validated.

The substitutions:

    the IR, in prose   → the dashscene document  (or just "the document")
    Rust type          → Document                (was Dsb)
    Rust node type     → Node                    (was DsbNode)
    crates/dashc/src/  → document.rs             (was dsb.rs)
    .dsb               → unchanged — the flatbuffer extension, "dashscene buffer"
    scdc, SCD, .scb    → already retired by PR #152; nothing left to do

**Two name collisions, both confined to one file.** `dashbuf`'s generated
flatbuffer types include **both** `Document`/`DocumentArgs` and `Node`/`NodeArgs`,
and `crates/dashc/src/emit.rs` imports all four. So the rename collides twice —
on the document type and on the node type.

It is confined to `emit.rs` because that is the only file in `dashc` that imports
the flatbuffer types at all; `lib.rs` and `main.rs` touch `dashbuf` only through
`root_as_document`, a free function with nothing to collide with. That is not a
coincidence — the emitter is by definition the one place the in-memory document
meets its wire encoding, so it is the only place both vocabularies are in scope.

`emit.rs` therefore aliases the flatbuffer side, all four:

    use dashbuf::{
        Document as FbDocument, DocumentArgs as FbDocumentArgs,
        Node as FbNode, NodeArgs as FbNodeArgs,
        ...
    };

The `Fb` prefix marks the wire types, and the unprefixed names mean the in-memory
document — which is the distinction §5 exists to enforce, made visible in the one
file where it could be confused.

`docs/decisions/dashscene-document-is-the-ir.md` records the ruling and
explicitly supersedes `SCOPE_DECISIONS.md` §20, so the two are not
re-conflated later. §20 is therefore **superseded, not gardened**: it does not
get a faithful decision record of its own, because its ruling no longer holds.
Its text survives verbatim in the archive.

**This makes the branch touch source code**, which the original design promised
it would not (§7: "no source logic is touched"). That promise is kept by
splitting the work — see §11.

## 6. Shipped docs to reconcile

Three shipped documents point at `specs/` and must be updated in the same
change, or they break the moment `specs/` is deleted:

- **`AGENTS.md`** — "Read these two files before doing anything else in this
  repo" names both files. It is repointed at the new records. Its intro and its
  P5 text both say **DSB** today (PR #152 put it there), so both take §5's
  ruling: the IR is the dashscene document. Its layout section gains the
  requirement → case → proof chain.
- **`docs/book/overview.md`** — its "Where things live" section names both
  files, and its opening line still calls the project "dash". Both are fixed.
- **`docs/specification/README.md`, `docs/design/README.md`,
  `docs/decisions/README.md`** — each promises this migration in the future
  tense. Each is rewritten to describe what is now there.

## 7. Citation repointing

Measured against `main` @ f71630c on 2026-07-14:

    423  citation hits in the repo, total
    -29  in specs/       — the files being deleted; not repointing work
    -91  in docs/archive/ — verbatim history; never edited
    -38  in docs/wip/     — not touched by this branch (§11)
    ----
    265  live citations to repoint, across 120 files

By area:

    84  crates/            49  docs/decisions/
    34  importers/         39  docs/design/
    13  AGENTS.md          15  docs/technotes/
     9  goldens/            3  docs/book/
     3  corpus/             2  docs/specification/

They sit in crate doc comments, `Cargo.toml` descriptions,
`crates/dashbuf/schema/dashbuf.fbs`, every existing `docs/design/` and
`docs/decisions/` record, the four new technotes, the Deno importer sources,
`goldens/`, and `corpus/`. Each is rewritten to name its new record.

All edits are one-line changes in comments, descriptions, and prose. **No source
logic is touched** — the `Dsb` → `Document` rename that §5 requires is real code
churn, and it is precisely why it is split into its own branch (§11) rather than
folded in here. That split is what keeps this diff mechanical: the judgment lives
in the new records, and every other line is a citation swap.

## 8. Verification

Every grep below excludes `docs/archive/`, `target/`, and `.git/`. The archive is
verbatim history and is never a failure.

**Branch 1 — `fix/ir-naming`:**

1. `grep -rE "\bDsb\b|\bDsbNode\b"` returns zero matches. The IR type is
   `Document`; the only surviving `Dsb` spelling is the `.dsb` extension itself.
2. `cargo test -p dashc` passes **unchanged** — same tests, same assertions,
   before and after. A rename that needs a test edited is not a rename.
3. `docs/decisions/dashscene-document-is-the-ir.md` exists and names
   `SCOPE_DECISIONS.md` §20 as superseded.

**Branch 2 — `docs/garden-specs`:**

1. `grep -rE "DESIGN_1|SCOPE_DECISIONS"` returns zero matches.
2. `grep -rE "\bSCD\b|scdc"` returns zero matches. PR #152 already achieved
   this; the check is a regression guard, not new work.
3. `.scb` appears nowhere except inside a block quotation that cites the
   archived seed document by path. `SCOPE_DECISIONS.md` §9 quotes DESIGN §4's
   ".scb is one way to populate it" verbatim, and that quotation survives into
   `docs/decisions/staged-mutation-v01-scope.md`. A superseding record may retire
   a name; it may not edit the words it quotes. So the quote keeps `.scb` and
   points at `docs/archive/`, where the sentence it quotes actually lives.
4. `specs/` does not exist.
5. Every relative link in `docs/` resolves to a file that exists.
6. Every "already gardened" deletion in §3.2 is confirmed against its named
   record before the section is removed.
7. Every `DECISION` tag in `docs/technotes/` has become a link to the record
   that now holds it (§4.6). `grep -rn "^DECISION" docs/technotes/` returns zero
   bare tags.
8. **`docs/wip/` is byte-identical to `main`, except for this design.**
   `git diff --stat origin/main -- docs/wip/` shows exactly one file changed.
   This is the check that proves the §11 exception was an exception and not a
   licence — the branch must not have touched the other two sessions' specs.

**Both branches:** `just build` is green (it runs markdownlint and dprint over
every `.md`, so the new records pass the same lint as the rest of the repo), and
`just verify` is green before the PR is opened.

## 9. Out of scope

Named so that they are deliberate deferrals and not oversights:

- **Adopting MarkSpec.** Deferred by decision; an issue is filed.
- **Moving `corpus/` or `goldens/`.** Considered and declined: `tests/`
  collides with the Rust convention and would produce
  `tests/goldens/tooling/tests/`; `corpus/` holds importer inputs and font
  assets, not only tests; and it would mix a workspace-member refactor into a
  docs-only change. The mental model is delivered by the documented chain in
  §4.1 instead.
- **Rewriting requirements to be measurable.** Several of R1-R7 are not
  independently verifiable as written ("perfect text quality", "high
  performance", "far less memory and CPU"). They are gardened verbatim in
  this pass. Rewriting them is the MarkSpec adoption's job, where the
  measurability rule can be enforced rather than merely asserted. An issue is
  filed.
- **Editing `docs/archive/`.** The archive is verbatim history.

## 10. Alternatives considered

- **Keep `specs/` as frozen redirect stubs**, so the ~130 existing citations
  still resolve. Rejected: it leaves two files that will rot, and it defers
  the citation work rather than doing it.
- **Keep `specs/` as the acceptance layer**, with only architecture and
  decisions moving to `docs/`. Rejected: two specification homes, and a
  deliberate divergence from the `sdd-working-memory-lifecycle` rule bought
  nothing that `docs/specification/` does not already provide.
- **Leave the plan in GitHub alone, with no in-repo roadmap.** This was the
  session's initial position and it was reversed. Rejected because the plan
  would not survive the promotion into public `dashscene` if that promotion
  is a fresh push (the mechanism is undecided by design), because a plan
  change cannot be reviewed in a pull request, and because the plan is then
  the only engineering artifact not versioned with the code. See §4.5.
- **A full `docs/plan/` directory** rather than a single `docs/roadmap.md`.
  Rejected: per-slice plan records would drift from the epic issues, which is
  the two-sources-of-truth failure the shape/state split exists to avoid. One
  file is enough for a slice map that changes a handful of times across the
  whole of v0.
- **Fold the exit criteria into `goals-and-requirements.md`**, with each
  requirement carrying its own "verified by" line. Rejected: E1-E6 are
  cross-cutting (E1 spans two producers and a painter), so several would have
  to be duplicated across requirements or assigned arbitrarily to one.
- **Name the verification layer `acceptance-v0.md`.** Rejected on both
  counts: "acceptance" invites confusion with human sign-off when these are
  automated end-to-end qualification tests, and a version in the filename
  means v1 gets a near-duplicate file instead of a second heading.
- **Repoint citations to concepts instead of files** (`DESIGN_1.md §4` →
  "the stage-1/2/3 pipeline"). Rejected: it decouples source comments from
  doc paths, but it removes the ability to jump from a doc comment to the
  record, which is most of what those citations are for.
- **Group `corpus/` and `goldens/` under `tests/` or `qualification/`.**
  Rejected — see §9.

## 11. Landing the work — two branches

**Rewritten 2026-07-14.** §5's ruling makes the IR rename a source-code change,
and the original design's reviewability rested on this being docs-only. So the
work splits, and the order matters: the garden pass writes prose that names the
IR, so the rename has to be settled first or the prose is written twice.

**Branch 1 — `fix/ir-naming`** (small; touches code)

    docs/decisions/dashscene-document-is-the-ir.md   new; supersedes SCOPE §20
    crates/dashc/src/dsb.rs → document.rs            Dsb → Document, DsbNode → Node
    crates/dashc/src/emit.rs                         alias dashbuf::Document as FbDocument
    crates/dashc/{lib,main}.rs, tests/               follow the rename
    crates/dashc/Cargo.toml                          description — published metadata
    docs/technotes/glossary.md                       already correct; cite the new record
    specs/SCOPE_DECISIONS.md                         mark §20 superseded (specs/ still exists here)

Mechanical, behavior-preserving, and the test suite must pass unaltered before
and after — the same bar PR #152 set for the rename it is correcting.

**Branch 2 — `docs/garden-specs`** (large; docs-only)

Everything else in this design. It rebases onto branch 1 once that lands, so it
inherits the settled vocabulary and never writes the old name.

**`docs/wip/` is not gardened — deliberate exception, user instruction
2026-07-14.** The rule wants `docs/wip/` empty on a `main`-targeting branch, and
this branch leaves three files in it: this design, plus
`2026-07-13-reactive-bindings-spec.md` and `2026-07-13-dirty-set-boundary-b-plan.md`,
which are another session's live working memory for in-flight v0.4 work.

Two reasons, and the second is the real one:

- Gardening another session's in-progress spec would archive work that is not
  finished, from under the person still writing it.
- The two files were already non-empty on `main` before this branch existed. The
  `wip-gate` failure is a condition this branch **inherits**, not one it
  introduces — so fixing it here would mean this PR silently absorbs someone
  else's gardening debt, and the gate would go green for the wrong reason.

The PR says so explicitly, so the gate's red state reads as a known, accepted
exception rather than an oversight. Whoever finishes the reactive-bindings work
gardens those two files; this branch gardens `specs/`.

Each branch squashes to one commit and lands with a merge commit, per AGENTS.md.
