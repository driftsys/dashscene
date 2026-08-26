# AGENTS.md — dashscene

dashscene turns UI designed in Figma — or authored programmatically in code —
into pixels on screen, through one intermediate representation (the dashscene
document; `.dsb` is its file extension), one shared layout+text runtime, and
interchangeable paint backends (Skia reference, Unity product, a lean native
painter later).

The target is embedded display hardware, defined by its constraints rather than
by one market: a tiling GPU, a fixed frame budget, and layout that must resolve
identically on every backend. In-vehicle screens are where it is measured;
industrial and medical panels, kiosks, avionics and handhelds impose the same
constraints. Keep prose naming the constraint, and name automotive as an
instance of it rather than as the boundary.

**Read these before doing anything else in this repo:**

- `docs/specification/` — goals, requirements, principles, target-hardware
  rules, and the Figma vocabulary profile.
- `docs/design/architecture.md` — the stack, the pipeline, its two boundaries,
  and the crate-to-purpose map; links to each crate's own as-built design record
  under `docs/design/`.
- `docs/decisions/` — everything decided since, each traced to what it affects:
  repo strategy, the full crate-name map, the `.dsb` format decision, the
  Deno/wasm Figma importer split, the Unity C# package sited under `unity/` in
  this repository (2026-08-17, reversing its deferred separate repo), and the
  driftsys house-style conventions this repo follows
  (`docs/decisions/house-style.md`).
- `docs/roadmap.md` — the v0/v1/v2 plan.

These are living records. A decision that changes one is recorded there directly
— don't silently diverge from it.

## Repo status

`driftsys/dashscene`, public. It was `dashscene-staging` until the rename, which
kept its history, issues and milestones — which is why every `#N` in these
records still resolves. The archived `driftsys/dashscene-name-reservations`
records how the crates.io names were reserved; it is not what the published
stubs point at.

**Counts, dates and licence state are not restated here.** They have been wrong
in this file repeatedly, and a number nobody trusts is drift surface with no
information in it. Derive or read them:

- crates.io reservations, which names are parked, the full role map —
  `docs/decisions/crate-name-map.md`
- licence and first real version —
  `docs/decisions/apache-2-0-for-the-patent-grant.md`,
  `docs/decisions/publishable-and-the-first-version.md`
- `main` carries a ruleset and a merge queue — see the **shipping-a-change**
  skill, and `docs/decisions/review-before-ready-not-before-open.md`

## Crates

One Cargo workspace (`resolver = "3"`, `edition = "2024"`,
`license = "Apache-2.0"`). Role-by-role mapping and every naming ruling:
`docs/decisions/crate-name-map.md`. As-built status:
`docs/design/architecture.md`, which links each crate's own design record.

    dashscene              umbrella / facade
    dashscene-core         semantic model — arena, node tree, layout+paint
                           tables — plus the staged-mutation producer API
                           (open/set_prop/set_variant/commit)
    dashscene-engine       Taffy solve, variants, FLIP, measure callback
    dashscene-typeset      bidi, shaping, glyph atlas pipeline
    dashscene-validator    profiles, diagnostics, waivers
    dashpaint              paint table + painter trait (boundary B)
    dashpaint-abi          the C representation of boundary B — the
                           improper_ctypes_definitions gate over dashpaint's
                           value types. Not bindings, not Unity-specific
    dashscene-skia         Skia reference painter
    dashscene-gpu          the lean painter — instanced quads and analytic SDF
                           over wgpu, native and web from one codebase
    dashcue                descriptive animation vocabulary + its runtime
                           scheduling (transitions, springs, keyframes, FLIP)
    dashlang               Rust DSL skin + stress-corpus generator
    dashbuf                flatbuffer schema — the .dsb document format
    dashc                  compiler CLI; also builds to wasm32-unknown-unknown
                           for the Deno importer
    dashpack               asset packer — canonical payloads to per-profile
                           derivations (RAW/HiFi/LoFi), cold-bank assembly,
                           derivation manifest
    dashpack-astcenc-sys   raw bindings to the vendored astcenc C++ sources;
                           no external CLI
    dashscene-ffi          the C ABI every platform host sits on — runtime
                           lifecycle, document load, the tick, the surface
                           handoff, and the committed frame handed out under a
                           lease for a host that draws it itself
    dashscene-desktop      desktop integration surface — window-to-surface
                           handoff, winit frame loop, the published `Present`
                           seam, a mapped `.dsb` load bounded by the shown root
    dashscene-web          web integration surface — canvas-to-surface handoff,
                           requestAnimationFrame loop, byte-range `.dsb` load
    dashscene-android      Android integration surface — the
                           android.view.Surface to ANativeWindow handoff, the
                           AChoreographer loop on its own thread, and the
                           surfaceDestroyed handshake

Non-crate directories: `importers/figma/` (Deno/TypeScript Figma REST importer
and `sharedPluginData` annotator; calls `dashc.wasm` rather than reimplementing
lowering — `docs/decisions/figma-importer-deno-plus-dashc-wasm.md`), `corpus/`
(stress corpus + Figma fixture captures), `goldens/` (CI golden images + diff
tooling), `conformance/` (layer 2's expectations as data, so a painter in
another shading language can check R-T5's single-sourcing rather than take it on
trust), `demo/`, `demo-web/`, `demo-android/` (the showcase hosts), `measure/`
(artifacts built to be weighed, not run), and `unity/` (the UPM package and the
gates over it — enumerated in the **project-gates** skill).

**Do not hand-maintain member counts in prose.** Derive them:

    cargo metadata --no-deps --format-version 1 | jq '.packages | length'
    cargo metadata --no-deps --format-version 1 \
      | jq '[.packages[] | select(.publish != [])] | length'   # publishable
    cargo metadata --no-deps --format-version 1 \
      | jq -r '.packages[] | select(.publish == []) | .name'   # never published

## Commands

The ones used on almost every turn:

    just test        sanity tier, seconds. Between edits and before every commit
    just build       assemble + full check — the thorough local gate, and what
                     CI runs. Run before pushing and before opening a PR
    just lint        clippy -D warnings, fmt --check, doc-links, prim, deno fmt
    just fmt         reformat everything in place
    just check       regression tier + lint + audit + secrets + wasm + c-abi
    just install     ./bootstrap — hooks, git-std, cargo-nextest, jq, prim

`just --list` enumerates the rest. **What each gate actually covers, which tier
to run when, what a gate cannot catch, and which recipes need a device, a Unity
editor, an NDK or an SDK that bootstrap does not install** — all of that is in
the **project-gates** skill. Read it before claiming a gate passed or that the
suite ran; a green `ci` means nothing red ran, not that everything ran.

## The development process

Every change — a feature or a bug fix — goes through the same eight stages, in
this order. The **guard rail** under each stage is what most often goes wrong
there. The pointer is the single artifact that owns the detail; nothing about a
stage is duplicated here, because a rule stated in two places drifts in one of
them.

1. **Specify.** Agree what the change must do before designing how.
   `superpowers:brainstorming`. The spec is written to `docs/wip/`. _Guard
   rail:_ implementation does not begin while the requirement is only a sentence
   in a conversation.

2. **Plan.** Turn the spec into ordered steps, each independently verifiable.
   `superpowers:writing-plans`. The plan is written to `docs/wip/`. _Guard
   rail:_ every step states how it will be verified, not only what it changes.

3. **Isolate.** Create the worktree, check out the story branch, run
   `./bootstrap`. `superpowers:using-git-worktrees`. _Guard rail:_ never a
   branch in the primary checkout. The shell working directory resets between
   commands, so use absolute paths and `git -C`.

4. **Implement, test first.** One behaviour at a time: write the failing test,
   confirm it fails for the stated reason, then make it pass.
   `superpowers:test-driven-development`, and the **implementing-a-change**
   skill for the loop as run here. _Guard rail:_ a test that has never failed
   pins nothing. This applies to bug fixes as strictly as to features —
   reproduce the bug in a test first.

5. **Review each behaviour change as it lands**, not once at the end of the
   branch. A light pass over that one change while it is still one edit old.
   **implementing-a-change**. _Guard rail:_ a behaviour change that reaches the
   code and not the prose describing it costs one edit now and a full review
   round later.

6. **Garden `docs/wip/`** into the durable records **before** opening the pull
   request, so the records sit inside the reviewed diff. `sdd-gardening`. _Guard
   rail:_ a record written while the raw original stays in `docs/wip/` is a
   copy, not a gardened record. Removing the file and updating the ledger is one
   commit.

7. **Ship.** `just build` green; open the pull request ready, never draft; run
   the full review; record a disposition against every finding; merge through
   the queue. **shipping-a-change**. _Guard rail:_ the review is a multi-seat
   pass — requirements and rules, tests, prose-against-code — plus an
   independent bug sweep, then one refutation per finding, consolidated into a
   single ledger. Fix critical findings and any major finding that is not heavy;
   file the rest as `debt` with a milestone. Never drop a finding silently.

8. **Clean up.** Remove the worktree once the branch has landed.
   **shipping-a-change**. _Guard rail:_ re-verify a worktree at the moment you
   delete it, not when you listed it, and remember a removed worktree can leave
   a stale shared target directory that makes unrelated tests fail.

**Test strategy** — which tier to run when, what the corpus and the golden
images are for, when the calibration tables must be re-derived, and when a
mutation is required rather than optional: the **project-gates** skill.

**Planning a slice** — epic and story shape, milestone placement, and the
phase-end revision: the **slice-planning** skill.

## Where to start

v0 is built one slice at a time, v0.1 onward — the count grows as phase-end
revisions open new ones, so read the roadmap for the range rather than a number
here. **`docs/roadmap.md` holds the slice map** — which slices are done and
which remain, what each delivers, and how they depend on each other — and marks
each slice closed or open. The current slice is the first one still open; the
epics under "Plan tracking" track the live work inside it. The roadmap is
revised at each phase-end epic close, so read it for slice status rather than
trusting a slice named in prose here, which goes stale the moment an epic
closes.

For the parts already on `main`: as-built component status is in `docs/design/`
(start at `docs/design/architecture.md`), the decisions behind it are in
`docs/decisions/`, and the requirements they satisfy are in
`docs/specification/`.

A crate is out of scope until the slice that reaches it — the roadmap says which
slice that is. Do not build ahead of the plan.

**Resolved (`docs/decisions/staged-mutation-v01-scope.md`):** the
staged-mutation contract (`open`/`set_prop`/`set_variant`/`commit`) lives on the
arena in `dashscene-core` — `docs/design/architecture.md` defines it as a
property of the arena, and `commit` mechanically operates on state core owns
(double buffer, generation stamp, dirty set). `dashcue` is the descriptive
animation vocabulary and its scheduling only; the transition spec describing how
a `set_variant` animates is `dashcue` data referenced by the commit, while the
switch itself is core's. `dashlang` builds directly on `dashscene-core`;
`dashcue` doesn't enter the graph until v0.4.

## Principles (`docs/specification/02-principles.md` — don't violate these)

- **P1** — the document carries intent, never results. No resolved x/y/w/h, no
  rasterized pixels, no glyph positions.
- **P2** — one solver, one typesetter; painters only color. A painter never
  measures, wraps, kerns, or moves anything.
- **P3** — producers mutate, the runtime owns time. Nothing producer-side
  executes inside the frame loop.
- **P4** — vocabulary is validated, never discovered. Every out-of-profile
  construct is a named diagnostic, never a silent drop.
- **P5** — Figma compatibility is a property of one producer. The dashscene
  document is a schema-first IR with its own spec; no producer's limitations
  define the format.

<!-- git-std:bootstrap -->

## Post-clone setup

Run `./bootstrap` after `git clone` or `git worktree add`.
