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
  Deno/wasm Figma importer split, Unity's deferred separate repo, and the
  driftsys house-style conventions this repo follows
  (`docs/decisions/house-style.md`).
- `docs/roadmap.md` — the v0/v1/v2 plan.

These are living records. A decision that changes one is recorded there directly
— don't silently diverge from it.

## Repo status

This is `driftsys/dashscene`, and it is **public**. It was `dashscene-staging`
until 2026-08-11; the rename kept its history, its 501 issues and its 21
milestones, which is why every `#N` in these records still resolves.

There are **21** reserved crates.io names: the 12 taken on 2026-03-18, before
this repo's first commit, plus 9 reserved during development as the crates
needing them arrived. Nineteen are this workspace's crates — every one of them —
and `dashscore` and `dashscene-compose` stay parked. These counts were off by
one for a while, having not moved when `dashscene-ffi` was reserved, so
re-derive them from `docs/decisions/crate-name-map.md` rather than trusting the
number here. Beware `demo`: a crate of that name has existed on crates.io since
2018 and is not ours, so querying the 25 workspace member names against
crates.io returns 20 — the 19 crates plus that one. Neither 20 nor 25 is the
reservation count.

The repository that made those reservations is archived as
`driftsys/dashscene-name-reservations`, kept because every published stub points
its `repository` field there. **The visibility flip has happened** —
`gh repo view driftsys/dashscene --json visibility` returns `PUBLIC`, and that
was the last step this record was waiting on
(`docs/decisions/repo-staging-and-public-facade.md`). One consequence is already
load-bearing: branch protection and rulesets are free on a public repository,
and `main` now carries one
(`docs/decisions/review-before-ready-not-before-open.md`).

## Crates

19 crates in one Cargo workspace (`resolver = "3"`, `edition = "2024"`,
`license = "Apache-2.0"`). Full role-by-role mapping:
`docs/decisions/crate-name-map.md`.

    dashscene            umbrella / facade
    dashscene-core        semantic model — arena, node tree, layout+paint
                          tables — plus the staged-mutation producer API
                          (open/set_prop/set_variant/commit)
    dashscene-engine      Taffy solve, variants, FLIP, measure callback
    dashscene-typeset     bidi, shaping, glyph atlas pipeline
    dashscene-validator   profiles, diagnostics, waivers
    dashpaint              paint table + painter trait (boundary B)
    dashscene-skia        Skia reference painter (the whole v0 painter)
    dashcue                descriptive animation vocabulary + its runtime
                          scheduling (transitions, springs, keyframes,
                          FLIP specs) — lands at slice v0.4
    dashlang               Rust DSL skin + stress-corpus generator
    dashbuf                flatbuffer schema — the .dsb document format
    dashc                  compiler CLI; also builds to wasm32-unknown-unknown
                          for the Deno importer
    dashpack-astcenc-sys   raw bindings to the vendored astcenc C++ sources —
                          the ASTC encoder and its in-process reference
                          decoder; no external CLI
    dashpack               asset packer — canonical payloads to per-profile
                          derivations (RAW/HiFi/LoFi), cold-bank assembly,
                          derivation manifest; lands across slice v0.12
    dashscene-unity        Rust-side FFI bindings only — the Unity/C# project
                          itself is a separate, not-yet-created repo
    dashscene-desktop     the desktop integration surface — the window-to-surface
                          handoff, the winit frame loop, rebuilding on resize,
                          the published `Present` seam and the lean painter's
                          implementation of it, and a mapped `.dsb` load bounded
                          by the shown root. `demo` keeps the demonstration —
                          the scene list, input, the painter choice and the
                          Skia presenter — and consumes it. Added at slice
                          v0.17 (story #794)
    dashscene-web          the web integration surface — the canvas-to-surface
                          handoff, the requestAnimationFrame loop, rebuilding
                          on resize, and the byte-range .dsb load. The
                          wasm/tiny-skia painter the name once described is
                          retired; dashscene-gpu covers the browser. Became
                          this at slice v0.17 (story #741); demo-web keeps the
                          demonstration and consumes it
    dashscene-ffi          the C ABI every platform host sits on — runtime
                          lifecycle, document load, the tick and the surface
                          handoff. Kotlin reaches it through JNI, and the
                          iOS and Unity hosts that follow inherit it. Added at
                          slice v0.19
                          (story #840)
    dashscene-android     the Android integration surface — the
                          android.view.Surface to ANativeWindow handoff, the
                          AChoreographer frame loop on its own thread, and the
                          surfaceDestroyed handshake that blocks until the
                          surface is dropped. The first host to sit on
                          dashscene-ffi rather than beside it. Added at slice
                          v0.19 (story #841)
    dashscene-gpu          the lean painter — instanced quads and analytic
                          SDF over wgpu, native and web from one codebase;
                          lands across slice v0.15

Plus `importers/figma/` (Deno/TypeScript — the Figma REST importer and the
`sharedPluginData` annotator plugin; calls `dashc.wasm` directly rather than
reimplementing lowering/validation, see
`docs/decisions/figma-importer-deno-plus-dashc-wasm.md`), `corpus/` (stress
corpus + Figma fixture captures), `goldens/` (CI golden images + diff tooling),
`demo/` (the windowed showcase host — the window, the event loop and the frame
loop, landed at v0.14), and `demo-web/` (the same showcase in a browser — a
canvas, `requestAnimationFrame`, and a `.dsb` fetched by byte range, landed at
v0.15).

Six of those directories hold workspace members that are never published:
`demo/`, `demo-web/` (the browser host — a canvas, the lean painter, and a
`.dsb` fetched by byte range, landed at v0.15), `demo-android/` (the third host
— a SurfaceView, the native vsync loop and the showcase scenes, landed at
v0.19), `corpus/showcase/` (the scenes all three hosts draw), `goldens/tooling/`
(the golden-image harness) and `measure/web-minimal/` (the smallest browser
embedder that draws a `.dsb` — an artifact built to be weighed, not run, and
what the runtime payload budget is measured over; see
`docs/decisions/publishable-and-the-first-version.md`). Twenty-five members in
total, nineteen of them the crates above.

## Commands

    just build      assemble + full check (this is what CI runs)
    just test        sanity tier — ~5 s. Between edits, and before every
                      commit.
    just test-regression  regression tier — every test but the
                      calibration re-derivations. What `build` and the CI
                      `test` job run; the pre-push hook does not.
    just calibrate    calibration tier — 10 tests, ~54 s. Re-derives the
                      committed asset tables; see the schedule below.
    just test-all     every tier in one run.
    just lint         clippy -D warnings, cargo fmt --check, prim, deno fmt --check
    just prim         prim fmt --check + prim lint over the Markdown, JSON,
                      YAML and TOML. Its own recipe because CI's `prim` job
                      runs exactly it. Both verbs are needed: `prim lint`
                      reports no format drift for Markdown
    just fmt          reformat everything in place
    just check        regression tier + lint + audit + secrets + the two wasm
                      gates + c-abi, which compiles the committed header from C
                      and checks the two halves agree (needs a C toolchain)
    just secrets      gitleaks over HEAD and over history, plus a pattern-grep
                      backstop over every object git would push. Unreachable
                      objects are deliberately out of scope — see the recipe.
                      Needs gitleaks, which bootstrap does not install; it
                      reports its absence
    just licenses     copy LICENSE and NOTICE into every publishable crate;
                      Apache-2.0 §4 requires both to travel inside the package
    just figma-sharing  assert every corpus fixture is explicitly link-viewable.
                      Needs a Figma PAT and the network, so it is outside
                      `check`; run it before publication
    just verify       the pre-push hook: commit-message lint, then lint + audit
                      + a secret scan of the objects being pushed. Seconds, and
                      it runs NO test tier — `just build` is the thorough local
                      gate, and CI runs the tier on the pull request. `ci.yml`
                      fires on `pull_request` and on pushes to `main`, so a
                      push to a branch with no PR open runs nothing at all;
                      once one is open, every further push re-runs it
    just wasm         build dashc for wasm32-unknown-unknown
    just wasm-painter build dashscene-gpu for wasm32 — the gate that keeps a
                      blocking wait off the web path, where it would deadlock
    just wasm-host    build demo-web for wasm32 — its browser half compiles on
                      no other target
    just wasm-lint    clippy every crate with a wasm32 half, on that triple —
                      the part of `lint` a host pass cannot see. Its own recipe
                      because CI's `wasm-gates` job runs exactly it
    just android      cross-compile the four Android members for
                      aarch64-linux-android — dashscene-gpu, dashscene-ffi,
                      dashscene-android and demo-android. The second platform's
                      compile gate, and the only one the last two have: their
                      JNI halves compile on no other target. Needs an NDK,
                      which bootstrap does not install
    just android-probe  cross-compile the D3a probe, push it to an attached
                      device and run it: what the painter's own device request
                      reports on that adapter (docs/design/android-toolchain.md)
    just android-splitscreen  build, install and cold-launch the lifecycle
                      harness in split-screen, screenshot it to check the
                      painter drew at all, then assert from logcat that the
                      surface-destroy handshake completed — D4's third case
                      (issue #874). Needs a handheld or tablet emulator image,
                      not target hardware: the automotive image declares no
                      split-screen feature. Needs the SDK build-tools and
                      python3 as well, neither of which bootstrap installs, on
                      top of the NDK that android needs. No hand gesture is
                      needed — am start --windowingMode 6 is accepted — but it
                      takes effect only on a cold launch, so the recipe
                      force-stops first
    just web-build    assemble the browser host into target/web (needs
                      wasm-bindgen-cli, which bootstrap does not install)
    just web          serve target/web on 127.0.0.1, byte ranges honoured
    just deno-check   just deno-test   just deno-fmt   just deno-capture
                      — scoped to importers/figma/
    just book         serve the mdBook docs locally
    just install      ./bootstrap — installs git hooks, git-std, cargo-nextest,
                      jq and prim. It does **not** install gitleaks, which
                      `just check` needs: gitleaks publishes no single-file
                      binary bootstrap can verify by checksum, and a secret
                      scanner fetched over an unverified path is the wrong
                      thing to add. Bootstrap reports its absence instead

Full recipe set: `justfile`. Conventions behind all of it — publish order,
`.git-std.toml` versioning, CI job breakdown, what prim covers and why it is
pinned — are in `docs/decisions/house-style.md`, sourced from driftsys/git-std,
driftsys/upskill, driftsys/markspec.

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

## Plan tracking

The v0 plan lives as GitHub issues on this repo: one `epic`-labeled issue and
one milestone per `docs/roadmap.md` slice, broken into `story`-labeled issues. A
slice that is opened but not yet planned has its milestone and no epic, which is
where issues surfaced by the previous slice are placed. Stories are split so
that independent stories can run in parallel; each story is worked in its own
git worktree, on the branch named in the story issue, and its body lists what it
depends on and what it blocks.

**Running CI's expensive path on demand.** `gh workflow run ci --ref main`
forces every path-filtered gate — `calibration` and `deno` — on. Ordinary work
already schedules them, and the slowest shape measures about 5 min wall
(`calibration` 272 s inside a 304 s run, 2026-08-11), so this is for when
waiting for a diff that happens to trigger them is the problem: after editing
the filter lists themselves. `--ref <branch>` works too, so a filter change can
be measured before it merges (`docs/decisions/test-tiers.md`).

**When to run which test tier** (`docs/decisions/test-tiers.md`). The suite runs
as three tiers, so "tests pass" is no longer a claim about all of it:

- **While editing, and before every commit** — `just test`. Seven seconds, and
  everything except the four slower binaries the regression tier adds. There is
  no reason to skip it.
- **Before pushing, and before opening a PR** — `just build`, which runs the
  regression tier. **The `pre-push` hook no longer runs it.** `just verify`,
  which the hook runs, is bounded at seconds: commit-message lint, `lint`,
  `audit`, and a secret scan scoped to the objects being pushed. So **a green
  push is not a statement that any test ran** — run `just build` by hand when
  you want that before pushing, and read the CI `test` job otherwise. `lint`
  still type-checks the whole workspace and every package `wasm-lint` names
  (`clippy --all-targets` compiles what it lints), so a compile error still
  fails locally; a test failure is what now reaches CI unverified.
- **When the diff touches any path in the `packer` filter** — the filter is
  defined in the `changes` job of `.github/workflows/ci.yml`, and enumerated
  with a reason per entry in `docs/decisions/test-tiers.md`. Run
  `just calibrate` before merging. The path list is deliberately not repeated
  here: it has already drifted three times as a partial copy, most recently
  omitting `Cargo.lock`. CI runs the tier regardless, and merging with that job
  red is what `docs/decisions/ci-green-before-story-merge.md` exists to prevent.
- **At slice close** — `just calibrate`, whatever the slice touched. This is the
  one run not driven by a path, and it is the backstop against a table drifting
  through a change the filter did not predict.
- **Name the tier in the PR body.** Never report a tier as run that was not run.
- **A green `ci` job does not mean the suite ran.** It means nothing red ran.
  When the diff is documentation only — every changed file is Markdown under
  `docs/` or Markdown at the repository root — `test`, `clippy`, `demo-build`,
  `wasm-build`, `wasm-gates`, `android-build`, `atlas-repro`, `render-oracle`,
  `exit-gate-tests` and `exit-gate` all skip, and `deno` skips with them. Read
  the individual jobs to see which tiers executed
  (`docs/decisions/test-tiers.md`).

Story workflow — the definition of done for every story:

- **Garden what this branch added to `docs/wip/` first** — before the
  `just build` below, so that build covers the prose just written, and before
  the PR, so the durable records sit inside the reviewed diff. Prose asserting
  what the code does not do is this repo's most common defect, so a record
  gardened after the review would be exactly the wrong artifact to exempt.

  Three states are acceptable for a file the branch added, and
  `docs/decisions/review-before-ready-not-before-open.md` states them in full
  rather than this file restating them: **gardened** (durable record written,
  raw original moved to `docs/archive/`, one commit — a record written while the
  original stays put is a copy), **partly gardened** (the implemented half is a
  record, the file stays for the rest, its `status` line says which is which),
  or **held** with the condition that empties it recorded in
  `docs/wip/README.md` — a table row for a capture, that file's prose for a
  driver prompt. Anything else the branch added is ungardened debt.

  **Removing a file from `docs/wip/` and updating that ledger is one commit, not
  two.** It has gone stale both ways: through an archiving that never touched
  it, and through an edit that updated one of its two copies of the count and
  left the other. The same commit re-points the records that cited the file at
  its old path — nineteen records in `docs/decisions/` carry a `docs/wip/`
  citation, and one has pointed at nothing since 2026-07-29 (issue #914).

  All of it binds **what the branch adds**, not the directory — `docs/wip/` is a
  standing shelf and is not expected to be empty.
- `just build` green.
- Open the PR as an ordinary pull request — **never a draft**. Draft means "not
  ready for review", which is the opposite of why the PR was opened: reviewers
  are not requested, and `/code-review` stops without reviewing when the PR is a
  draft (`docs/decisions/review-before-ready-not-before-open.md`).
- Run `/code-review` on the PR (`--comment` posts the findings as inline PR
  comments) **while CI runs, not after it**. Neither answer depends on the
  other, so waiting for green before starting the review only adds the shorter
  of the two to the wall clock. The merge gate is unchanged: both must be
  complete. Capture every finding as a checklist in the PR description — never
  drop a finding silently.
- Fix all critical findings before merging. For minor findings, file one
  `debt`-labeled issue each (linked to the story) instead of fixing them inline,
  **and put it on a milestone** — the current slice, the next one, or `v1` for
  anything not scheduled to a slice. Debt with no milestone is invisible at
  every slice close, and it is the largest population of the four: 52 open
  `debt` issues carry none, against 42 in `v1`, measured 2026-08-12. Re-derive
  with `gh issue list --label debt --state open
  --limit 300 --json milestone`
  rather than trusting those two numbers.
- **When a critical finding changes the implementation, review the fix too** — a
  pass over what changed, not a second full pass. The fix is written under more
  time pressure than the original and lands after the pass that would have
  caught it.
- The findings checklist is what says the PR is not ready to merge: an absent or
  unticked checklist means the review is still running. **The review half is not
  enforced mechanically**: the direct mechanism is a required approving review,
  GitHub refuses self-approvals, and there is no second account to give one — so
  it is held by the checklist and by whoever presses merge. What _is_ enforced,
  since 2026-08-12, is the rest: a ruleset on `main` with an empty bypass list
  requires a pull request and a green `ci`, and refuses force-pushes and
  deletion. That also means **`main` takes no direct push at all**, so
  `just release` and any hotfix travel through a PR like everything else.
- **A closing keyword next to an issue number closes that issue — in PR prose
  and in any commit message that lands on `main`.** The keywords are `close`,
  `fix` and `resolve`, in any inflection, optionally followed by a colon. GitHub
  matches them anywhere, including mid-sentence, and **a negation is not a
  defence**: a sentence saying an issue was _not_ fixed matches exactly as well
  as one saying it was.

  Three incidents, all of them prose that meant the opposite:

  - Story #49 was closed by a docs PR discussing whoever would close it. The
    story was never built, and two shipped documents then described its
    deliverable as shipped.
  - On 2026-08-11 a commit recording that a debt had been filed **rather than**
    fixed closed it two seconds after its PR merged. The debt was silently gone
    from the milestone.
  - The same phrasing in another commit closed an issue nearly six hours before
    the work that settled it.

  Only the **first** number after the keyword is taken, so a sentence naming
  three issues closes one and leaves two — which makes the damage look arbitrary
  and easy to miss.

  **Write `Refs #N`.** It carries no keyword and cannot fire under any later
  edit, which a keyword separated from the number by a few words can. Reserve a
  closing keyword for the one issue the change actually completes, and put it on
  its own line at the end. When naming an issue mid-sentence, write "issue #N"
  or restructure.

  After merging, check `gh issue view <n> --json state` for **every** issue the
  branch's commits named, not only those in the PR body. An issue closed this
  way is reopened by hand, with a comment saying it was closed by accident and
  not fixed — otherwise the reopen reads as a reversal of someone's judgement
  rather than a correction.
- **Re-read the milestone's open issues before merging**, not only the story's
  own: `gh issue list --milestone "<slice>" --state open`. Debt filed against a
  slice in progress is often a warning about the story that is open right now.
  Issue #783 predicted that a `dashbuf::Residency` would collide with the
  existing `dashscene_gpu::Residency`, and it was filed **twelve minutes after
  story #597's PR was opened and twenty-six before it merged** — so checking at
  the start would have found nothing, and checking before the merge button would
  have saved the rename a whole extra PR cost. A slice's other sessions file
  against the work in flight, not against the work that is finished.
- Merge only when the review pass is complete, every critical finding is
  resolved, and CI is green on the commit being merged. A green run earlier is
  not a promise: a later push, or a rebase onto a moved `main`, can turn it red
  again, so check the commit you are about to merge.

Merging a PR — how the branch lands on `main`:

- Shape the branch before you merge it, not at the merge button. Rebase onto the
  latest `main`, squash the branch's commits into one conventional commit, and
  force-push. The PR then carries exactly one commit, and it applies to `main`
  without conflict.
- Keep separate commits only when they are separately meaningful — for example a
  preparatory refactor and the behavior change that builds on it, each
  independently reviewable and revertable.
- Land the PR with a merge commit ("Create a merge commit"). The branch is
  already squashed, so `main` still reads as one change per PR, and the merge
  commit records which PR the change came from.
- Avoid "Rebase and merge". It replays each branch commit onto the current
  `main`, so a conflict already resolved on the branch can come back during the
  replay (this is what blocked PR #108). A merge commit integrates the branch
  as-is and does not re-raise resolved conflicts.
- All three merge methods stay enabled, and GitHub has no default-merge-method
  setting: the merge button preselects whichever method that person used last.
  Never rely on the preselection — name the method explicitly,
  `gh pr merge --merge`.

Plan revision at the end of each phase: story breakdowns for future slices are
provisional by design. When a slice's epic closes (v0.1, v0.2, …), revise the
remaining epics and stories against what was learned before starting the next
slice — update, split, merge, or re-order the issues, and record scope-level
changes as new or updated records in `docs/decisions/`.

**Re-check `docs/features.md` in the same pass**, against the code rather than
against `docs/design/` or `docs/specification/`. It asserts, feature by feature,
what is built and what is not, and no test fails when one of those assertions
goes stale. Four review rounds on the pull request that introduced it found 35
factual errors, and the majority came from claims written out of this
repository's own design and specification records — four of which had themselves
drifted from the code (`04-figma-vocabulary-profile.md`'s letter-case row,
`typeset-latin.md`'s "deliberately absent" list, the v0.10 close's import-oracle
frame count, and the atlas record's byte-identity reading). The recurring
mistake is depth: confirming a capability exists without checking which branches
it does not cover, what the default path does, or whether any command reaches
it. That is what this re-check is for.

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
