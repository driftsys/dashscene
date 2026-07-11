> **update, 2026-07-11 (later same day):** the repo scaffold this file's
> §1/§6 flagged as mismatched/pending has been rebuilt from scratch against
> the crate map in §2, committed locally (`bbb4bfe`, 58 tracked files), and
> verified: `cargo check`/`clippy`/`fmt`/`test` all pass clean across all 13
> crates, the `justfile` parses and lists every required recipe, and all
> JSON/TOML config files validate. §7's open question — whether to dogfoods
> `git-std` from day one — is now confirmed **yes** (user decision); the
> scaffold's `justfile`, `.git-std.toml`, `bootstrap`, and CI `convco` job
> all wire it in for real, not as a stub. Two things could not be verified
> in-sandbox due to network egress limits: the `wasm32-unknown-unknown`
> rustup target (a real filename-collision bug between `dashc`'s `bin` and
> `lib` targets was found and fixed regardless — see `crates/dashc/Cargo.toml`)
> and `dprint`'s markdown plugin download. GitHub access remains blocked
> exactly as described below — nothing has been pushed anywhere; the local
> scaffold is sitting in this session's workspace ready to push the moment
> `driftsys/dashscene-staging` exists and this session can reach it. See §6
> and §7 for the updated status of each item.

# dash / dashscene — scope decisions addendum

    status   living addendum to specs/DESIGN_1.md — captures decisions made
             after the seed document that DESIGN_1.md itself doesn't (yet)
             reflect. Supersedes DESIGN_1.md wherever the two disagree.
    date     2026-07-11

DESIGN_1.md (see `specs/DESIGN_1.md`) is the seed architecture doc. This
file tracks what's been decided since, in the order it was decided, so a
future session (or teammate) doesn't have to reconstruct it from chat.

## 1. Repo: dashscene stays the public facade; dashscene-staging is where work happens

Superseded once, then revised again — recorded here so the reasoning
isn't lost. First decision was to repurpose `driftsys/dashscene` in
place as the working monorepo (it was created earlier purely to reserve
a family of crate names on crates.io, see §2; it has 3 commits, one open
dependabot PR, and its crates are doc-comment stubs only — no real
implementation to preserve). Then reconsidered: rather than flip
`dashscene` private during early messy development, **`dashscene` stays
public as-is**, reserved for its future role as the project's facade —
docs, book, marketing/landing site — and all actual development happens
in a new private repo, **`driftsys/dashscene-staging`**.

Rationale: crates.io's `repository =` field is just metadata on a crate
— it can point anywhere and be repointed at publish time, so there's no
technical requirement that development happen in the repo the reserved
names nominally point at. Keeping `dashscene` untouched avoids an early,
messy commit history landing in what's meant to become the public-facing
surface. The tradeoff accepted: when there's a real version running,
staging's content needs to be promoted into `dashscene` (fresh push or
history merge — decide then), rather than avoiding that step entirely.

Blocking: this session's GitHub connector has no access to _any_
`driftsys` repo yet — confirmed it's not just a missing individual grant
but that org-level listing/creation is unavailable entirely ("sessions
are bound to their configured repositories"). Re-confirmed in a later
session on the same day: no `gh` CLI installed, no GitHub MCP connector
available, and connector-suggestion search returns nothing (opted out in
user settings) — there is currently no mechanism in-session to create or
list `driftsys` repos at all. Action needed from the user: create
`driftsys/dashscene-staging` (private) on github.com, and grant this
session access to it (same mechanism needed for `dashscene` itself).
Nothing has been pushed anywhere yet. **A correctly-named local scaffold
now exists** (rebuilt against the crate-name mapping in §2 — see the
update note at the top of this file), committed locally, ready to push
the moment access opens.

## 2. Crate naming — reuse the 12 already-reserved crates.io names

All 12 were confirmed live on crates.io (published 2026-03-18, one
version each — placeholder reservations, not real releases):
`dashscene`, `dashscene-core`, `dashscene-engine`, `dashscene-compose`,
`dashscene-unity`, `dashscene-web`, `dashscore`, `dashlang`, `dashc`,
`dashcue`, `dashpaint`, `dashbuf`.

Mapping onto DESIGN_1.md's architecture:

    reserved name       DESIGN_1.md role
    -------------------  ------------------------------------------------
    dashscene            umbrella crate — facade / public API surface
    dashscene-core        arena, node tree, layout tables, paint tables —
                          the semantic model (DESIGN §5) — AND the staged
                          producer-mutation API (open/set_prop/
                          set_variant/commit), see §9
    dashscene-engine      Taffy solve, variants, FLIP, measure callback —
                          runtime that resolves the model (DESIGN §7.1)
    dashc                 compiler: Figma importer orchestration target,
                          lowering, diagnostics, .dsb emission (DESIGN §4,
                          §6.1) — now also built to wasm32 for the Deno
                          importer to call into directly (§4 below)
    dashbuf               the flatbuffer schema itself — document format,
                          sections, hashes (DESIGN §5); also names the
                          .dsb file extension (§3 below)
    dashpaint             paint table (fill/stroke/effect params, token
                          refs, material class) + the painter trait,
                          boundary B (DESIGN §8)
    dashcue               descriptive animation vocabulary + its runtime
                          scheduling — variant transitions, FLIP
                          triggers, springs, keyframes, loop tracks,
                          enter/exit (DESIGN §6.3); NOT the staged-
                          mutation API, which is dashscene-core's (§9)
    dashlang              Rust DSL skin (v0) and future typed skins over
                          the one producer surface (DESIGN §6.2)
    dashscene-unity        Rust-side FFI bindings for the Unity painter;
                          the actual Unity/C# work lives in a separate
                          repo (§5 below)
    dashscene-web          wasm/tiny-skia painter, parked (DESIGN §8.4)
    dashscore              parked — an authoring IDE, not in DESIGN_1.md's
                          scope at all
    dashscene-compose      parked — Android Jetpack Compose backend, not
                          a DESIGN_1.md target

Three crates DESIGN_1.md needs that have **no reserved name yet**:
typesetting (bidi/shaping/atlas, DESIGN §7.2), the Skia reference painter
(DESIGN §8.1 — the entire v0 painter), and the shared validator
(profiles/diagnostics/waivers, DESIGN §3 P4). Proposed names, confirmed
available on crates.io (checked, all 404/unreserved) but not yet
reserved: **`dashscene-typeset`**, `dashscene-skia`,
`dashscene-validator`. `dashscene-typeset` was chosen over
`dashscene-text` (too generic — DESIGN §7.2 itself is titled "one
typesetter") and over `dashscene-type` (collides with the Rust
ecosystem's `*-type`/`*-types` convention for shared type-definition
crates). Open action: reserve all three before v0.1 starts, so they
can't be squatted out from under the project.

`dashscore`, `dashlang`, and `dashscene-compose` carry the most
interpretive risk in this mapping — `dashscore`/`dashscene-compose` are
treated as unused/parked (no equivalent in DESIGN_1.md), and `dashlang`
is treated as "the DSL family" rather than a literal new declarative
language. Revisit if any of these prove wrong.

## 3. Document format: `.dsb`, one schema for file and wire

DESIGN_1.md's working name `.scb` is retired. New extension: **`.dsb`**
(dash scene buffer — matches the `dashbuf` crate that owns the schema).
Chosen over `.dsd` (dash scene document) because `.dsd` collides with a
live, actively-used format — Direct Stream Digital / SACD audio — plus
AutoCAD Drawing Set Description and DAZ Studio morph files; `.dsb`'s
collisions (Dell DataSafe Backup, an embroidery format, a DVD-slideshow
project format, a DAZ Studio script format) are all dormant or narrow.

Confirmed (not new — DESIGN_1.md already specifies this in §6.2 and
§11, this addendum just re-affirms it against the naming question):
**one flatbuffer schema serves both the on-disk file role and the wire/
remote-streaming role.** The same tables (node tree, layout, paint,
variant, text) describe intent whether loading a whole document or
applying one staged commit — a remote update is structurally a small
document, not a different data model. What differs is framing/transport
only: the file role uses the mmap section-packing discipline from
DESIGN §5 (hot sections at the head, cold sections page-aligned at the
tail, per-section hashes for the load gate); the wire role skips that
and uses plain length-prefixed flatbuffer message framing. FlatBuffers'
`file_identifier` mechanism (a 4-byte magic tag near the buffer root) is
shared across both roles, so a blob self-identifies as a dashbuf buffer
regardless of whether it arrived via mmap or a socket — this also
underpins the still-open admission-policy question for untrusted remote
producers (DESIGN Q-5).

## 4. Figma importer: Deno/TypeScript, calling `dashc.wasm`

New decision, not in DESIGN_1.md. The Figma importer (DESIGN §4 Stage 1,
§6.1) will be built in Deno/TypeScript rather than as a pure Rust binary,
split as follows:

    Deno owns (native TS strengths — HTTP, auth, JSON shaping):
      REST fetch against @figma/rest-api-spec's official TS types,
      personal-access-token rotation, root-frame declarations,
      reachability closure across files, variant-set closure, trim
      layers (root scoping, slot-child replacement, `_`-prefix sugar,
      sharedPluginData role reads), token phase 1/2 join (DESIGN §6.1).

    Also hosts the small Figma annotator plugin that writes
    sharedPluginData roles (placeholder|sample-content|redline|spec) —
    this has to run inside Figma's own plugin sandbox regardless of any
    other language choice, so it lives alongside the Deno importer as
    natural JS-side kin.

    dashc.wasm owns (same Rust code path as the native dashc binary,
    compiled to wasm32-unknown-unknown — not reimplemented):
      Figma≠CSS lowering (negative gap, stroke-align, canvas stacking,
      scale-to-inset), profile/vocabulary validation via
      dashscene-validator, .dsb emission. Deno hands it canonical
      post-closure JSON and gets back .dsb bytes or a diagnostics
      report — same R6 rule (error blocks emission, never a silent
      drop) whether invoked natively in CI or from Deno.

Why: keeps exactly one implementation of lowering and validation (no
drift between a Rust path and a hypothetical TS reimplementation);
R7 byte-reproducibility holds trivially since it's the same Rust code
either way (same argument DESIGN §8.4 already makes for wasm-Skia
goldens: wasm IEEE-754 determinism matches CPU goldens by construction).

Open: package name. Proposed `dashscene-figma`, published to JSR as
`@driftsys/dashscene-figma` — not yet reserved. JSR and deno.land/x
availability could not be confirmed from this session (jsr.io and
deno.land/x aren't reachable from the sandbox's network egress
allowlist, and web-fetch fallback attempts hit an unanswered provenance
prompt). Action for the user: check `https://jsr.io/@driftsys` directly
to confirm the scope/name is free. The scaffold's `importers/figma/deno.json`
uses this proposed name/version (`0.0.0`) as a placeholder pending that
confirmation.

**Repo: same repo as the Rust core, not split out.** Unlike Unity (§5),
where the only coupling to the core is a narrow versioned FFI wire
protocol and a repo split costs nothing, the Deno importer directly
imports `dashc.wasm` — the compiled output of the `dashc` crate sitting
in the same workspace. Splitting repos would mean publishing `dashc.wasm`
as a versioned artifact and consuming it with a version pin from a
second repo, coordinating two-PR landings every time the wasm interface
changes (JSON shape in, `.dsb`-or-diagnostics out) — real overhead for a
boundary that isn't architecturally distinct, since it's the same
compiler, just called from a different host process. A monorepo doesn't
require one toolchain: the Deno code lives in its own subdirectory with
its own `deno.json` and its own CI job (path-filtered so Rust-only
changes don't trigger it and vice versa); JSR publishing works fine from
a subdirectory, same as crates.io publishing works fine for individual
crates inside a Cargo workspace. Layout (now scaffolded, see update note
at top of file):

    dashscene-staging/
      crates/            13 dashscene-* / dashbuf / dashpaint / dashcue /
                          dashlang / dashc crates (§2)
      importers/figma/   Deno importer + sharedPluginData annotator plugin
        deno.json
        src/
        plugin/          Figma plugin manifest + sandboxed plugin code
      corpus/
      goldens/

## 5. Unity: separate repo, C#, not started yet

Unity work can't live in the Cargo workspace — different language and
toolchain entirely (C#, Unity project format or a UPM package). Two
distinct pieces, both C#, both living together in one Unity repo/package:

    producer front end (DESIGN §6.2, "C# decl. (v1)") — a C# declarative
      DSL running in-engine, builds a describe buffer, ONE commit across
      the FFI seam (no per-prop FFI; struct/Span, pooled, GC-free; typed
      keys via codegen)

    painter back end (DESIGN §8.2) — the renderer: rect table + glyph
      runs consumed over FFI, projected onto pre-instantiated
      GameObjects, paint entries resolved to SDF-shader-library
      materials (lit-opaque / lit-cutout / unlit-overlay)

Per DESIGN §11's own plan, Unity work doesn't start until v1 (after v0
exit criteria E1–E6, which are Rust+Skia only). Decision: **do not
create the Unity repo yet.** Default is to defer until v0 actually
exits; revisit if the name needs reserving earlier to prevent squatting.
The Rust-side `dashscene-unity` crate (already reserved, see §2) becomes
the thin FFI-bindings crate the C# side links against, not the Unity
project itself.

## 6. Open items / blocked

- **`driftsys/dashscene-staging` doesn't exist yet**: needs to be
  created (private) by the user — this session cannot create repos or
  list org repos (confirmed twice now, in two separate sessions:
  "sessions are bound to their configured repositories", no org-level
  API available at all, no `gh` CLI installed, no GitHub MCP connector
  available, and connector-suggestion search opted out in user
  settings). Not just a missing per-repo grant — no in-session mechanism
  exists at all right now.
- **GitHub access**: session's GitHub connector has no access to any
  `driftsys` repo yet, `dashscene` included (`403 — GitHub access to
  this repository is not enabled for this session`). Nothing has been
  pushed anywhere. Blocks everything downstream: pushing the scaffold,
  milestones/issues, CI actually running.
- **Local scaffold: rebuilt and verified, ready to push.** The mismatch
  flagged earlier (a scaffold built under the wrong `dash-*` naming) has
  been superseded: a fresh scaffold now exists in a later session's
  workspace, built directly against the crate map in §2 and the house
  style in §7, committed locally as `bbb4bfe`. Verified in that session:
  `cargo check`/`clippy -D warnings`/`fmt --check`/`test` all pass clean
  across all 13 crates; `just --list` parses the full justfile and shows
  every recipe from §7's spec plus `wasm`/`deno-check`/`deno-test`/
  `deno-fmt`; all JSON/TOML config files (`dprint.json`,
  `.markdownlint.json`, `deno.json`, plugin `manifest.json`,
  `.git-std.toml`, `book.toml`, `Cargo.toml`) parse; `markdownlint-cli`
  passes clean against every `.md` file. Not verified in-sandbox (network
  egress limits, not scaffold defects): `cargo build --target
  wasm32-unknown-unknown` (the rustup target itself couldn't download —
  static.rust-lang.org unreachable) and `dprint check` (plugins.dprint.dev
  unreachable). One real bug was found and fixed while checking the wasm
  path: `dashc`'s `bin` and `lib` targets both compiled to `dashc.wasm`
  on that target, a cargo filename collision — the lib target is now
  named `dashc_wasm` in `crates/dashc/Cargo.toml`. Still needed: push to
  `driftsys/dashscene-staging` once it exists and this session (or a
  future one) can reach it.
- **crates.io reservations**: resolved. `dashscene-typeset`,
  `dashscene-skia`, `dashscene-validator` are published as v0.1.0
  doc-comment-stub placeholders (matching the original 12's style:
  MIT, `repository` pointing at the public `driftsys/dashscene` facade
  per §1's rationale), squat-proofing the full 15-crate name set ahead
  of v0.1.
- **JSR reservation**: `@driftsys/dashscene-figma` confirmed available
  — the `@driftsys` scope already exists on JSR (owns `markspec`), and
  the package name itself returns 404 (unclaimed). Not yet published;
  that happens once `importers/figma/` has real code to ship.
- **Milestones/issues**: resolved — the full v0 plan now exists as
  GitHub issues on `driftsys/dashscene-staging` (see §10): one epic and
  one milestone per DESIGN §11 slice (v0.1–v0.9), broken into stories.
- **CI**: workflow file is scaffolded (`.github/workflows/ci.yml`) but
  not yet running anywhere (blocked on repo access/push).
- **Promotion path unset**: how `dashscene-staging` eventually becomes
  (or feeds) public `dashscene` — fresh push vs. history merge — is
  intentionally undecided until there's a real version running (§1).
- **`docs/` taxonomy scaffolded, not yet migrated into**: `docs/wip/`,
  `docs/archive/`, `docs/specification/`, `docs/design/`,
  `docs/decisions/`, `docs/technotes/` now exist per §7's update above,
  each with a README explaining its role. `specs/DESIGN_1.md` and
  `specs/SCOPE_DECISIONS.md` remain authoritative; folding their content
  into this taxonomy is deferred (user decision) to avoid breaking
  already-written `DESIGN_1.md §N` / `SCOPE_DECISIONS.md §N`
  cross-references. The online guide (`docs/book/`, mdBook source) was
  split out separately — overview + usage guide only, not a spec mirror.

## 7. House style — inspired by driftsys/git-std, driftsys/upskill, driftsys/markspec

Confirmed by reading those three repos directly. `dashscene-staging`
should follow the same conventions rather than invent new ones.

**Cargo workspace shape** (git-std): `resolver = "3"`;
`[workspace.package]` with `edition = "2024"` (not 2021), `license =
"MIT"`, shared `repository`; `[workspace.dependencies]` with
`path + version` for every internal crate; `[profile.release]` — `lto =
true`, `strip = true`, `codegen-units = 1`.

**`justfile`** (git-std's is the template): `assemble` (cargo build),
`test`, `lint` (`cargo clippy -- -D warnings` + `cargo fmt -- --check` +
`dprint check` + `markdownlint-cli`), `audit` (`cargo audit`), `check`
(test + lint + audit), `build` (assemble + check), `verify` (`git std
lint --range main..HEAD` + `just build` — run before opening a PR),
`fmt`, `doc` (`cargo doc --open`), `book` (`mdbook serve`), `release`
(`git std bump`), `publish` (ordered `cargo publish` per crate,
dependency order — for dashscene this means dashbuf → dashscene-core →
dashscene-typeset → dashscene-engine → dashscene-validator → dashpaint →
dashscene-skia → dashcue → dashlang → dashc → dashscene-unity →
dashscene-web → dashscene), `install`, `clean`. Add two dashscene-
specific recipes: `wasm` (build `dashc` for `wasm32-unknown-unknown`,
needed by the Deno importer, §4) and `deno-check`/`deno-test`/`deno-fmt`
scoped to `importers/figma/`.

**`dprint.json`**: markdown only (`includes: ["**/*.md"]`, the
`dprint/markdown` plugin) — it does not replace `cargo fmt` or `deno
fmt`, both of which run as their own separate lint/fmt steps for their
respective languages.

**`.git-std.toml`**: `scheme = "semver"`, `strict = true`, `scopes` as
an explicit list — the 13 crate names plus repo-wide scopes (`repo`,
`docs`, `ci`, `hooks`, `deps`, `release`) — rather than `"auto"`, which
only discovers `crates/*` and leaves no valid scope for commits that
aren't crate-specific, `[versioning] tag_prefix = "v"`, one
`[[version_files]]` entry per crate pointing at its version string in
`Cargo.toml` (git-std dogfoods itself this way — every crate version
bump and changelog entry
goes through `git std bump`, not manual edits).

**CI** (`.github/workflows/ci.yml`, git-std's shape): separate jobs for
`fmt` (`cargo fmt -- --check`), `dprint` (`dprint/check@v2.3` action),
`clippy` (`cargo clippy -- -D warnings`, `Swatinem/rust-cache`), `test`
(`cargo test`, `Swatinem/rust-cache`), `convco` (PR-only conventional-
commit-message validation), aggregated by a final `ci` job that fails if
any of the above failed. For dashscene, add a `deno` job (check/lint/
test/fmt, scoped to `importers/figma/` via a `dorny/paths-filter` gate so
Rust-only changes don't trigger it) and a `wasm-build` job (`dashc` →
`wasm32-unknown-unknown`, verifies the Deno importer's dependency
actually builds). No cross-platform `build-release` matrix yet — that's
git-std's own binary-distribution concern, not relevant until dashscene
ships a distributable binary of its own.

**`bootstrap` script**: ensures `git-std` itself is installed (detects
platform, downloads the matching release, verifies the sha256, installs
to `~/.local/bin`), then `exec git-std bootstrap` — git-std's own
subcommand handles the repo-specific setup (git hooks, etc.) from there.
Run after cloning or creating a worktree.

**Deno side** (markspec's `deno.json` is the template, applies to
`importers/figma/`): a `workspace` array pointing at the package
directory (Deno's native workspace feature, same idea as the Cargo
workspace); imports preferring JSR (`jsr:@std/...`) over npm where a JSR
package exists, `npm:` specifier otherwise (e.g. `@figma/rest-api-spec`
is npm-only); `tasks` for `check` (`deno check` on entry points), `test`
(`deno test` with the narrowest `--allow-*` set that works), `lint`
(`deno lint`), `fmt` (`deno fmt`); `fmt.include` scoped to
`ts/tsx/js/jsx/mts/cts/mjs/cjs`; `test.exclude`/`lint.exclude` covering
`editors/`, `.worktrees/`, `.claude/worktrees/` (markspec explicitly
excludes Claude Code's worktree directory from lint/test scanning —
worth copying).

**Governance/docs files**, present in all three repos and expected here
too: `LICENSE` (MIT), `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`,
`SECURITY.md`, `.editorconfig`, `.markdownlint.json`, `book.toml` +
`docs/book/` (mdBook source — an overview and a usage guide, the online
guide's actual content; served at `driftsys.github.io/dashscene-staging/`
or wherever it lands post-promotion, once published).

**`docs/` also follows the `sdd-working-memory-lifecycle` rule's
taxonomy** (separate from `docs/book/`, the online guide): `docs/wip/`
(Superpowers spec+plan working memory, transient, tracked), `docs/archive/`
(raw wip content once gardened), `docs/specification/` (requirements),
`docs/design/` (architecture), `docs/decisions/` (decision records),
`docs/technotes/` (explanatory notes). All five are currently empty
scaffolding — `specs/DESIGN_1.md` and `specs/SCOPE_DECISIONS.md` remain
the authoritative requirements/architecture/decision records for now;
migrating their content into this taxonomy is deferred to a future
gardening pass, to avoid breaking the many `DESIGN_1.md §N` /
`SCOPE_DECISIONS.md §N` references already written into the codebase.

**Resolved**: `dashscene-staging` **does** dogfood `git-std` from day
one (user-confirmed). The scaffold's `justfile` (`release`/`verify`
recipes), `.git-std.toml`, `bootstrap` script, and CI `convco` job all
wire it in for real rather than as stubs/placeholders.

## 8. Figma fixture corpus — self-authored committed fixtures, live-only public targets

Refines DESIGN §6.1's fixture plan ("record-and-replay… no public
fixture corpus is recent enough"). That claim was re-verified before
deciding: Grid shipped at Config 2025 (GA ~May 7, 2025) as its own
auto-layout section — not a fourth direction on the existing panel —
and the same event introduced the Figma Draw effects (noise/texture,
progressive blur, variable-width strokes) that sit on DESIGN §10.1's
REJECT list. Any corpus assembled before mid-2025 is therefore
structurally missing all three coverage targets (grid, boundVariables
at scale, 2025 effects). Confirmed decision input: the project has no
proprietary production Figma files to draw from — public sources only.

**Licensing finding (drives the whole shape).** Figma's Community Free
Resource License grants rights "solely in connection with your
authorized use of the Figma Platform," prohibits derivative works, and
has no carve-out for API-based extraction. Capturing a third-party
Community file's REST JSON and committing it to a repo as a standing
test fixture is at best ambiguous under that license — and this appears
to apply even to files published by Figma's own official Community
account (same license framework, no platform-owner exemption found).
Ruling: **nothing enters `corpus/` that wasn't authored by the project.**
Not a legal opinion — an ambiguity being routed around rather than
resolved; revisit (ask Figma / a specific creator) only if a real need
appears.

**Tier 1 — committed static corpus (`corpus/figma/`), entirely
self-authored.** Small, focused files rather than one mega-file — same
bisect-by-construction argument DESIGN §8 makes for painters: a failure
should implicate one construct, not "the fixture." Files are authored
in the project's own Figma account; their captured GET /file JSON is
what gets committed (record-and-replay per DESIGN §6.1).

    fixture            covers
    -----------------  ------------------------------------------------
    grid-basic         GRID mode: row/column spans, fixed/hug/fill
                       track sizing, min/max tracks. Real-JSON mirror
                       of the DSL stress corpus's synthetic grid cases.
    variables-bound    boundVariables on color + number props, bound
                       across ≥2 modes (light/dark). Dual purpose: the
                       coverage target now, AND the designated input
                       for the token-resolution phase 1→2 work later
                       (sidecar IDs → name join) — author it with that
                       reuse in mind.
    effects-2025       one instance each of noise/texture, progressive
                       blur, variable-width stroke. DIAGNOSTIC fixture,
                       not a rendering fixture: everything in it is
                       REJECT-list (§10.1), so under R6 it can never
                       emit a .dsb — its test asserts the diagnostic
                       report names each construct as an error. This is
                       why it MUST be its own file: error-severity
                       constructs block emission for the whole
                       document, so folding them into grid-basic or
                       variables-bound would destroy those files as
                       emission/rendering fixtures.
    lowering-*         hand-authored Figma mirrors of the Figma≠CSS
                       lowering edge cases (DESIGN §5): wrap,
                       hug-in-fill, negative gap, baseline row, variant
                       topology change. The DSL-generated versions
                       exercise Taffy but NOT the importer's lowering
                       path — these do. RTL Arabic needs no separate
                       file structurally: it's text content, so it
                       rides as a locale variant of a couple of these.

**Tier 2 — live-only validation, never captured or committed.** Folds
into the nightly live smoke test DESIGN §6.1 already plans, pointed at
three named public targets instead of left generic; the importer runs
against them live and the diagnostic report is reviewed — no JSON is
stored:

    Grid Playground (official Figma Community account) — grid mode
      against Figma's real emitted shapes (spans, hug, fractional
      tracks), not the project's approximation of them
    Config 2025 feature-update playground — same, for the 2025 Draw
      effects (progressive blur, noise/texture, pattern fills)
    one design-system kit, TBD (Material 3 / Polaris / Untitled UI) —
      boundVariables + nested auto-layout + variant sets at realistic
      production scale/messiness. None of these kits shows adoption of
      grid or 2025 effects yet, so this target buys scale, not
      construct coverage.

Open actions from this section: author the four tier-1 fixture groups
(needs the paid-seat PAT from the capture-tooling work, next topic);
pick the tier-2 design-system kit; wire the three tier-2 targets into
the nightly smoke test config when it exists.

## 9. Staged-mutation API lives in dashscene-core, not dashcue — resolved

Resolves the contradiction AGENTS.md flagged: DESIGN §4/§6.2 describe
the staged-mutation contract (`open`/`set_prop`/`set_variant`/`commit`)
as a property of the in-memory arena, while this file's §2 crate map had
assigned it to `dashcue`. **The arena wins: the API is
`dashscene-core`'s.** The §2 map above has been corrected to match.

Rationale:

- DESIGN §4 says it verbatim: "the in-memory arena + its staged
  mutation API (open/set_prop/set_variant/commit) is the real contract;
  .scb is one way to populate it." The API is defined as a property of
  the arena, and the arena is `dashscene-core`. The §2 assignment to
  `dashcue` was a mapping error, not a design decision.
- `commit` is mechanically an arena operation — it swaps the double
  buffer, bumps the generation stamp, updates the dirty set, all state
  `dashscene-core` owns. Housing the API elsewhere means either another
  crate reaching into core's internals or core exposing a lower-level
  mutation API anyway that the other crate merely wraps.
- Dependency graph: the v0.1 walking skeleton needs
  `open`/`set_prop`/`commit` but zero animation. With the API in core,
  v0.1 is `dashlang → dashscene-core → dashbuf` and `dashcue` doesn't
  exist until its slice (v0.4). The other way round, every producer
  drags in the animation crate to set a property.

What `dashcue` is, precisely: the DESIGN §6.3 descriptive animation
vocabulary — transition specs (tween/spring/keyframes), stagger,
per-prop smoothing, loop tracks, keyframe tracks, enter/exit specs —
plus their runtime scheduling/interpolation. The seam: `set_variant`
(the structural switch) is core's; the transition spec describing how
that switch animates is `dashcue` data referenced by the commit.
`dashcue` lands with slice v0.4 (variants + staged mutation + minimal
FLIP), not before.

## 10. The v0 plan is tracked as GitHub epics/stories, revised at each phase end

The whole v0 plan (DESIGN §11 slices v0.1–v0.9) now lives as GitHub
issues on `driftsys/dashscene-staging`, closing §6's "milestones/issues
not yet created" item:

- One milestone and one `epic`-labeled issue per slice (epic #1 =
  v0.1 … epic #47 = v0.9), each epic carrying a dependency graph and a
  story checklist.
- `story`-labeled issues (#2–#6, #8–#11, #13–#18, #20–#23, #25–#30,
  #32–#35, #37–#41, #43–#46, #48–#49) split so that independent
  stories can run in parallel, each in its own git worktree on the
  branch named in the story issue; every story body lists what it
  depends on and what it blocks. Cross-epic early starts are marked
  (e.g. the v0.3 paint/importer track does not depend on v0.2's layout
  work; the v0.5 atlas pipeline and the Arabic-atlas spike depend only
  on v0.1).
- Labels: `epic`, `story`, `debt` (deferred minor review findings);
  the stock `bug` label covers defects found later.
- Definition of done for every story (also in AGENTS.md "Plan
  tracking"): `just build` green; `/code-review` on the story diff
  before the PR; every finding captured as a PR checklist; critical
  findings fixed before merge; one `debt` issue per minor finding;
  merge only when CI is green and the review pass is complete.
- **Plan revision at phase end**: story breakdowns for future slices
  are provisional by design. When a slice's epic closes, the remaining
  epics/stories are revised against what was learned before the next
  slice starts; scope-level changes land here.
