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
dependency order — for dashscene this means dashbuf → dashpaint →
dashscene-core → dashscene-typeset → dashscene-engine →
dashscene-validator → dashscene-skia → dashcue → dashlang → dashc →
dashscene-unity → dashscene-web → dashscene, since story #4 made
dashscene-core depend on dashpaint, see §15), `install`, `clean`. Add two dashscene-
specific recipes: `wasm` (build `dashc` for `wasm32-unknown-unknown`,
needed by the Deno importer, §4) and `deno-check`/`deno-test`/`deno-fmt`
scoped to `importers/figma/`.

**`dprint.json`**: markdown only (`includes: ["**/*.md"]`, the
`dprint/markdown` plugin) — it does not replace `cargo fmt` or `deno
fmt`, both of which run as their own separate lint/fmt steps for their
respective languages.

**`.git-std.toml`**: `scheme = "semver"`, `strict = true`, `scopes` as
an explicit list rather than `"auto"`, which only discovers `crates/*`
and leaves no valid scope for commits that aren't crate-specific. The
list is the 13 crate names, plus a scope for each non-crate component
that has its own artifacts and tooling — `goldens` (the golden images
and their diff tooling), `corpus` (the fixture corpus itself: captured
Figma JSON, fonts, generated stress scenes — **data only**, since the
capture tool is code and lives under `importers/`), `importers` (the
Deno/TypeScript Figma importer and its capture tool, which have their
own toolchain and their own CI job) — plus the repo-wide scopes `repo`,
`docs`, `ci`, `hooks`, `deps`, `release`.

The list is deliberate, not exhaustive: a scope earns its place by
making the changelog or a `git log --grep` more useful, and one scope per
top-level directory would dilute them. **`specs/` therefore has no scope
of its own — it is documentation, so it takes `docs`, the same as
`docs/`.** (`corpus` and `importers` were added on 2026-07-13, after
`repo` had absorbed both for want of anywhere better; `specs/` commits
had landed under `repo`, `docs`, and `dashbuf` alike, and `docs` is the
ruling.)

Also `[versioning] tag_prefix = "v"`, and one `[[version_files]]` entry
per crate pointing at its version string in `Cargo.toml` (git-std
dogfoods itself this way — every crate version bump and changelog entry
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

**Status update (2026-07-12): all 8 tier-1 fixtures are authored**, in
the `dashscene-corpus` Figma project; the fixture-name → file-key map
is committed at `corpus/figma-fixtures/manifest.json` (the committed
corpus directory landed as `corpus/figma-fixtures/`, not the
`corpus/figma/` named above). The fixtures were built programmatically
by a development-only Figma plugin,
`importers/figma/plugin/fixture-author/` — one menu command per
fixture; re-running a command rebuilds its frame, so fixtures are
regenerable rather than hand-built. This plugin is NOT the §12
annotator plugin: it only creates nodes, it never writes
sharedPluginData roles. `effects-2025` currently carries 3 of its 4
REJECT constructs: texture was written via the plugin API; noise and
progressive blur were applied manually in the Effects panel because
both plugin-API writes failed in review; variable-width stroke has no
plugin API at all and is still pending as a manual step (draw a line,
apply a variable-width profile with the Draw tools). The review found
the two failures have different causes: the progressive-blur write
used a nonexistent effect type (`PROGRESSIVE_BLUR`), when the correct
shape is a `LAYER_BLUR` or `BACKGROUND_BLUR` effect with `blurType`
`PROGRESSIVE` — so progressive blur IS plugin-authorable, and that
write is corrected in the fixture-author plugin; the noise write's
shape was verified field-by-field against the pinned plugin typings
and is correct, so its failure is the beta-availability case the
plugin's checklist mechanism exists for, and re-runs now preserve the
manually applied effect (the plugin harvests the old frame's effects
before rebuilding). Manual application remains the recorded state of
the current Figma files until they are regenerated. Three
plugin-API findings worth recording for future fixture authoring: a
GRID frame reads its gaps from `gridColumnGap`/`gridRowGap`, not
`itemSpacing`; a WRAP frame must set `primaryAxisSizingMode = "FIXED"`
after `layoutMode`, otherwise it hugs its children into a single row
and nothing wraps; and `GridTrackSize` exposes only a track `type`
(`FIXED`/`FLEX`/`HUG`) plus a `value`, with no track-level min/max, so
the grid-basic row's "min/max tracks" is covered instead as child
constraints (`minWidth`/`maxWidth` on a grid child), and hug track
sizing is covered by a `HUG`-typed track.

Remaining open actions: apply `effects-2025`'s variable-width stroke
manually; capture the 8 files' GET /file JSON into
`corpus/figma-fixtures/` with the capture tooling under the §11 access
rules; pick the tier-2 design-system kit; wire the three tier-2
targets into the nightly smoke test config when it exists.

**Status update (2026-07-13): the corpus is captured, and the
`effects-2025` entry above states the wrong reason.** One of the four
open actions listed above is discharged, and one claim in the fixture
table is refuted by the capture itself — recorded here rather than by
rewriting the table, so the correction is traceable.

- **The capture is done** (#139's blocker, PR #142). All **nine**
  tier-1 fixtures — the eight above plus `v03-paint`, added with the
  paint slice (PR #136) — are committed as `GET /file` JSON under
  `corpus/figma-fixtures/`. That discharges the second open action
  ("capture the 8 files"), and only that one. **Three remain open:**
  applying `effects-2025`'s variable-width stroke manually (every node
  in the captured file is `strokeType: "BASIC"`, so it is genuinely
  still absent), picking the tier-2 design-system kit, and wiring the
  three tier-2 targets into the nightly smoke test.
- **`effects-2025` can never emit a `.dsb`, but not for the reason the
  table gives.** The table says everything in it is REJECT-list, so R6
  blocks it. As built, the compile stops earlier and for a different
  reason: the file's root frame carries `layoutMode: HORIZONTAL`, and
  `dashc` refuses auto-layout outright (`CompileError::Unsupported`)
  **before** the triage gate ever runs. The fixture therefore does not
  reach the diagnostic path at all on its own, and its acceptance test
  strips the `layoutMode` key — and only that key — to reach the three
  effects it was authored to carry.
- **Auto-layout is refused on two grounds, and the second is the
  load-bearing one.** The in-memory document model has no flex
  vocabulary (#140), _and_ Figma's `absoluteBoundingBox` for a node
  inside an auto-layout frame is the **solver's output**. Lowering it
  as a fixed box would write a result into a document that P1 says may
  carry only intent. This is why the refusal is correct rather than a
  temporary gap: it would still be correct even if the box happened to
  be right.
- Consequence for future fixtures: **a diagnostic fixture must not be
  authored inside an auto-layout frame**, or the auto-layout refusal
  masks the constructs it exists to exercise. `layoutMode: NONE` is a
  precondition of any fixture whose purpose is to reach the triage
  gate.

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

## 11. Figma access: plan and seat, PAT lifecycle, scopes, rate limits

Decisions for the paid Figma access the fixture and capture work needs
(§8's "paid-seat PAT" item). Plan: **Figma Professional with a Full
seat**. The REST file endpoints are plan- and seat-gated, and
Starter's Tier-1 allowance is roughly 6 requests per MONTH — unusable
— so a paid plan is a hard requirement, not a convenience.

**PAT lifecycle.** Figma personal access tokens expire after 90 days —
a hard cap, with no non-expiring option. Rotation policy: rotate at
about 75 days. The token is stored as a GitHub Actions secret, never
in the repo. The nightly live smoke test (DESIGN §6.1) doubles as the
token canary: when the PAT expires or loses a scope, the smoke test is
what fails first. Auth failures surface as a named 401/403 diagnostic
that states the likely causes (expired PAT, missing scope), never as a
bare HTTP error.

**Scopes** (granular): `file_content:read` — this also covers
sharedPluginData, returned via `?plugin_data=shared`;
`file_metadata:read`; `library_content:read`. `file_variables:read` is
Enterprise-only and therefore unavailable on Professional — see §13
for the consequence.

**Rate limits.** `GET /file` is Tier 1 = 10 requests/minute on
Professional. Importer and capture behavior derived from this:
(1) metadata-version-check first — hit the cheap metadata endpoint,
compare the file version against the previous capture, and skip the
full `GET /file` when unchanged; (2) a serialized limiter — at most
one Figma request in flight at a time; (3) honor `Retry-After` on 429
responses.

## 12. Annotator plugin: deferred to v1, contract frozen now

The sharedPluginData annotator plugin (DESIGN §6.1's trim-layer
"machine truth" writer; §4 places it alongside the Deno importer) is
**deferred to v1**, but its data contract is frozen now so captures
and importer code written before the plugin exists stay compatible:

- Namespace: sharedPluginData namespace `"dashscene"`.
- Keys: `role` = `placeholder | sample-content | redline | spec`, and
  `v` = `"1"` (contract version stamp).
- Reserved keys, defined now and written later: `contribution-id`
  (placeholder nodes only) and `material-class` =
  `lit-opaque | lit-cutout | unlit-overlay` (consumed by the Unity
  painter, DESIGN §8.2).

The deferral trigger is **event-based, not version-based**, with two
triggering events: (a) the first externally-authored Figma file
entering the pipeline — self-authored fixtures do not need roles, so
this is what the role-writing machinery waits for; or (b) the start of
phase-2 token-resolution work, which needs the
id → name/collection/mode export command (§13). Event (b) may fire
first and may require only the token-export command, not the
role-writing machinery.

Annotation is a three-channel inventory, cheapest channel first:
(1) native Figma structure (names, the `_` prefix, hidden flags) where
it already encodes the intent; (2) the repo-side export manifest for
per-file declarations; (3) sharedPluginData last, only for what
genuinely must travel on the node inside Figma.

Distribution: the plugin stays in-repo and unpublished. Professional
cannot publish org-private plugins, so publishing would mean public
Community publication; distribution is therefore "import plugin from
manifest" from a checkout (the same mechanism the fixture-author
plugin uses).

The §8 fixture-author plugin is a DIFFERENT plugin: it only creates
nodes and never writes roles; this contract does not apply to it.

## 13. Token resolution: phase split; the join table must come from the Plugin API

Refines DESIGN §6.1's two-phase token plan with what the Professional
plan actually allows.

**Phase 1 — resolved literals plus a sidecar receipt.** The importer
emits resolved literal values into the `.dsb` and writes a
`<out>.vars.json` sidecar preserving the `boundVariables` IDs. The
sidecar is fully derivable from the captured `GET /file` JSON — an
R7-safe receipt (re-deriving it is byte-reproducible), not a second
source of truth. Phase-1 documents are single-theme by construction:
one resolved mode, no runtime theme switching.

**Phase 2 — id → name/collection/mode join**, switching the `.dsb` to
token refs.

**Key finding (supersedes DESIGN §6.1's "or naming convention"
parenthetical):** on Professional there is no naming-convention
fallback. Variable names, collections, and modes are exposed only by
the Enterprise-gated Variables REST endpoint, so on this plan the join
table MUST come from the Figma Plugin API. Concretely: one more
command on the §12 annotator plugin exports the
id → name/collection/mode table, which makes token export the
annotator plugin's first mandatory job — and, per §12, the one command
that phase-2 work can require ahead of the rest of the plugin.

The table format is source-agnostic: if Enterprise REST access ever
becomes available, it is a drop-in replacement producer for the same
table. Staleness is guarded by stamping the table with the Figma file
version it was exported from; a version mismatch against the capture
is a diagnostic. For fixtures, the table is committed as
`corpus/figma-fixtures/<file>.vartable.json`.

## 14. Arabic atlas spike (#25): msdf-atlas-gen confirmed; Q-1 resolved for v0

The v0.5 spike (issue #25, worked ahead of epic #1 per
`docs/decisions/text-track-early-start.md`) validated the pinned
text-stack tooling against Arabic. Full methodology and evidence:
`docs/technotes/msdf-arabic-atlas-spike.md`.

- **msdf-atlas-gen holds for Arabic.** Version 1.4.0 accepts glyph-id
  input (`-glyphset`), loads GSUB-only glyphs (no cmap entry) without
  issue, and keys its JSON layout by glyph index. A 211-string corpus
  (every letter in all four joining contexts, harakat, lam-alef
  variants, digits, tatweel) shaped through HarfBuzz produced 113
  distinct glyph ids for Noto Sans Arabic (28 GSUB-only) and 248 for
  Amiri (176 GSUB-only); every one is present and geometrically
  correct in the generated atlas (Noto: zero deviations beyond 1 px
  tolerance at 64 px/em; Amiri: hairline quantization only).
- **Q-1 is resolved for v0** (`docs/decisions/q1-msdf-below-14px.md`):
  MSDF-only, no per-size bitmap atlases. The visual check puts the
  MSDF legibility floor at 14 px/em (12 px/em acceptable, below that
  degraded); raising atlas resolution does not move the floor, so the
  fallback design stays parked until v1 target-hardware data. Text
  below 14 px/em becomes a warning-severity validator diagnostic once
  text validation exists.
- **Reproducibility input for #27 (R7):** with a pinned generator
  version and seed, two multi-threaded runs were byte-identical on
  one machine; cross-machine identity still needs a CI check.
- **Carried into later stories:** Noto Sans Arabic ships no Latin and
  no solidus, so mixed-script strings need font fallback and per-font
  charset unions (#34, #28); glyph runs must carry per-glyph x/y
  offsets, not just advances (#26, #28); bidi run splitting must
  precede shaping (as DESIGN §7.2 already requires).

## 15. Boundary B unified in dashpaint: core depends on it; publish order updated

Story #4 (dashscene-skia, the first `Painter` implementation) resolved
the boundary-B reconciliation that stories #2 and #3 each deferred to
it (their crates were built in parallel against a pinned contract, each
holding its own copy of the shared shapes):

- **`dashpaint` owns the boundary-B types** (`Color`, `RectEntry`,
  `PaintIndex`, `PaintEntry`, `PaintKind`, `PaintTable`);
  `dashscene-core` depends on `dashpaint`, deletes its mirror types,
  and re-exports what it consumes. Dependency direction follows the
  DESIGN §4 pipeline (producers → runtime → painters): the reverse
  would make every painter build the runtime (and, from v0.2, Taffy).
  §7's publish order above moved `dashpaint` before `dashscene-core`
  accordingly (also updated in the workspace `Cargo.toml` comment and
  the `justfile` publish recipe).
- **Every committed rect resolves.** The `NO_PAINT = u32::MAX`
  sentinel is gone from the committed output: an unfilled node interns
  the shared draws-nothing entry (`PaintEntry::default()`). Painters
  have no sentinel special case; `PaintTable::resolve` stays the single
  failure mechanism. `dashbuf`'s `Node.paint_entry` keeps its
  `uint32::MAX` sentinel — that is the document format's "references no
  pool entry", a different level.
- **Paint indices are typed**: `RectEntry.paint` is
  `PaintIndex(u32)`, `#[repr(transparent)]` (layout unchanged,
  entries stay blittable) — closes the cross-index-confusion debt
  (#54).

Details and alternatives: `docs/decisions/boundary-b-unification.md`.

## 16. Section-ordering spike (#56): .dsb becomes a sectioned container at v1

The v0.1 spike (issue #56) measured how much physical-layout control
the FlatBuffers toolchain gives, against the pinned versions (`flatc`
25.12.19, Rust crate 25.12.19). Decision record:
`docs/decisions/dsb-sectioned-container.md`; full evidence on the
issue.

- **A single flatbuffer cannot express DESIGN §5's section
  discipline on the Rust producer path.** Page alignment has no
  supported route: `flatc` caps struct `force_align` at 32, vector
  `force_align` above 32 is honored only by C++ codegen (Rust codegen
  silently ignores it), and the Rust `FlatBufferBuilder` has no direct
  alignment method. Vtable deduplication in the Rust builder is
  unconditional (C++ has `DedupVtables(bool)`; Rust has no
  equivalent), so hot tables can reference vtable bytes physically
  located among cold data. The verifier's supported entry points have
  no subtree scoping, so a hot-only load gate would have to be
  hand-composed from lower-level verifier primitives. And sections
  have no stable byte ranges to hash.
- **Direction: `.dsb` becomes a thin container** — a fixed envelope
  (magic, version, section table with per-section id, flags, offset,
  length, hash) framing one complete flatbuffer per section, each with
  its own `root_type` and `file_identifier`. Cold section offsets are
  page-aligned by the writer; per-section hashes cover contiguous byte
  ranges. §3's "one schema for file and wire" survives: both roles
  frame the same per-section flatbuffers, the wire with length
  prefixes, the file role with the envelope.
- **Timing:** v0 keeps single-flatbuffer `.dsb` files; the envelope
  lands with the v1 loading-performance work (R5 mmap measurements).
- **Carried into the schema stories (#8, #13, #20, #26):**
  cross-table references are integer indices — never flatbuffer
  offsets reaching into another future section — and every
  section-destined table (node tree, layout, paint, variant,
  text/strings, heavy decor) stays one offset away from `Document`, so
  lifting it to a section root later is mechanical. The v0.1 schema's
  inline `Node.paint` is the one deviation; story #13 lifts it into a
  `Document`-level paint table plus index.

## 17. Design session (2026-07-12): asset model, id model, remoting transports

A design session following spike #56 elaborated the `.dsb` container
into a concrete format direction and settled three connected models.
Each is a decision record in `docs/decisions/`; this entry is the
scope-level summary.

- **Container refinements** (`dsb-sectioned-container.md`,
  "Refinements" section): the envelope is a hand-specified fixed
  layout (`#[repr(C)]` header + fixed-stride 64-byte section-table
  entries), deliberately not flatbuffers — it is validated before any
  parser is trusted and evolves by version bump. Section kinds split
  into structured (a flatbuffer, verifier-checked) and blob (raw
  bytes, hash-checked). Page alignment is required exactly once, at
  the hot/cold boundary; per-blob alignment is writer policy (64-byte
  quantum; page-align large blobs for per-range prefetch/evict). The
  format is little-endian with explicit-LE accessors; big-endian
  targets are deferred, correctness kept by construction. Loading is
  one whole-file `mmap`; the envelope is read through the mapping.
- **Asset model** (`asset-model-content-addressed-blobs.md`): the ui
  document carries asset identity and metadata, never bytes — a hot
  `AssetTable` of content-hash entries with intrinsic metadata;
  payloads are raw well-known-format bytes (KTX2/PNG/...) with no
  dashscene framing, living in blob sections or fetched by hash.
  Layout never blocks on payloads; the signed root transitively
  authenticates lazily fetched blobs; the client cache is a
  content-addressed store. Supersedes v0.3's inline
  `Document.images`, which stays until the asset work lands (the
  migration is named in the record).
- **Id model** (`id-model-strings-compile-to-indices.md`): source
  strings compile to dense integer indices; content hashes identify
  assets; session-scoped producer handles (distinct from doc indices)
  address nodes across structural commits. Strings survive only as
  debug names and an opt-in, validator-checked exports table.
  Allocation is deterministic (canonical DFS first-visit order) per
  R7.
- **Remoting** (`remoting-two-transports.md`): two transports —
  ordered snapshots + commit deltas (snapshots speak indices, deltas
  speak handles; DFS-contiguous slices; keyframe resync), and a pull
  channel fetching assets by content hash. The file role is the two
  transports materialized; the envelope never crosses the wire.
  Implementation is v1+, but three rules bind v0 work now: handles ≠
  indices in the producer API (as-built `NodeId` already conforms),
  pools append-stable before deltas exist (the committed paint pool's
  per-commit re-intern is a named migration), and subtree-shaped
  operations reusing the document vocabulary.

## 18. v0.1 retrospective: plan revision at the v0.1 epic close

Epic #1 (v0.1 — walking skeleton) closed 2026-07-12 with all six
stories merged (#2, #3, #4, #5, #6, #56). Per §10, the remaining epics
and stories were revised against what v0.1 taught. This section records
the scope-level outcomes; story-level changes were applied to the
issues directly.

- **Boundary-B unification landed early.** Story #4 moved the shared
  paint types into `dashpaint` and made `dashscene-core` depend on it,
  ahead of the ownership revisit the original breakdown deferred to #4
  (`docs/decisions/boundary-b-unification.md`, §15). The `NO_PAINT`
  sentinel is gone; downstream stories build on the unified model.
- **Two v0.2 debt items are folded into story #9 (Taffy solve)**
  instead of standing alone: #58 (the redundant absolute-position
  scratch vec) and #59 (the `node_ids`/`rect_index` inverse-permutation
  pair and its `u32::MAX` placeholder). Both live in the commit walk #9
  rewrites, and #59's `Some(u32::MAX)` → panic path becomes reachable
  once a solver can leave a node unsolved. Debt #55 (paint-less node
  representation) is anchored to #9: v0.2 flex containers are the first
  layout-only, paint-less nodes.
- **v0.3 keeps inline `Document.images`.** The content-addressed asset
  model (`docs/decisions/asset-model-content-addressed-blobs.md`)
  supersedes the inline field, but v0.3 (#13, #16, #17) shipped inline
  images to keep that slice small. The migration is deferred to v0.7 as
  a new story (#107) under epic #36 (importer catch-up); #16 and #17
  are noted not to target the new model.
- **The schema-design decisions gardened during v0.1 now bind future
  stories:** sectioned-container (§16) binds #20 and #26;
  `id-model-strings-compile-to-indices.md` binds #26; the asset model
  binds #107. These bindings are stamped on the respective issue
  bodies.
- **AGENTS.md "Where to start" was reconciled to as-built** (debt #83):
  the section now records the v0.1 skeleton as complete and points at
  `docs/design/` and `docs/decisions/`, rather than framing the
  skeleton as the next work.

## 19. v0.2 retrospective: plan revision at the v0.2 epic close

Epic #7 (v0.2 — flex core) closed 2026-07-13 with all four stories
merged (#8, #9, #10, #11). Per §10, the remaining epics and stories
were revised against what v0.2 taught. This section records the
scope-level outcomes; story-level changes were applied to the issues
directly.

- **`dashlang` cannot author a flex scene, and that is now a scheduled
  dependency.** Its builder exposes `at`/`size`/`fill`/`child` only,
  and `Scene::build` commits through the fixed solver, which ignores
  flex. `docs/decisions/negative-gap-lowering.md` D3 deferred both the
  flex vocabulary and the question of how a `dashlang` scene reaches
  the engine solver; story #11 confirmed the deferral rather than
  resolving it, and authored its goldens against `dashscene-core`'s
  `Txn` directly. Filed as #118, which **blocks #46** (the DSL-generated
  stress corpus) — a dependency the original breakdown did not record.
- **DSB does not get authored fill weights** (#117, closed at this
  revision). Epic #7's scope list named them, but core's
  `AxisSizing::{Fixed, Hug, Fill}` carries no weight and
  `dashscene-engine` maps every `Fill` to `flex_grow = 1.0`, so `Fill`
  siblings always split free space equally — and Figma auto-layout has
  no flex weight either, so an authored weight would be a CSS-flexbox
  construct with no Figma counterpart and no producer emitting it.
  Story #11 goldened the equal split rather than inventing one, and the
  construct is now declined outright: P4 says vocabulary is validated,
  never discovered, so a weight would have to be carried by the schema,
  core's `Prop` set, the engine's mapping, and every validator profile,
  permanently, for something nothing produces. P5 ("no producer's
  limitations define the format") is the argument on the other side and
  is a real one — the code-DSL path could plausibly want a 2:1 split —
  but P5 says Figma's limits must not _bound_ the format, not that the
  format should grow constructs nobody has asked for. Reopen when a
  real consumer appears (the C# declarative DSL, or a stress-corpus
  case needing an unequal split); it is then a schema change with a
  stated requirement behind it. "Fill weights" is dropped from the v0.2
  scope wording.
- **Flex goldens are exact-match by construction.** Their scenes are
  dimensioned so every solved rect lands on an integer, so the fills
  carry no anti-aliased edges and the goldens compare with zero
  tolerance — unlike the v0.3 paint goldens, which need 1–2 % for
  cross-machine anti-aliasing jitter. This binds future flex goldens: a
  construct that cannot be made integral changes the scene's
  dimensions, not the comparison function
  (`docs/decisions/v02-flex-goldens-per-construct.md`, extending
  `golden-comparison-space.md`).
- **The lowering-suite revisit trigger is now due, at #16.** Negative-gap
  is the first of the four Figma≠CSS lowerings and landed as a single
  `dashscene-core` `Txn` method rather than a lowering module, because
  abstracting a suite around one member would have been premature. Its
  decision record says to revisit when the second lowering lands — and
  the remaining three (canvas stacking, strokes-in-layout,
  scale-to-inset) are #16's scope in v0.3. Stamped on the issue so the
  choice is made deliberately.
- **§18's fold-in of #58 and #59 into story #9 did not happen.** Both
  remained open through the v0.2 close. Correcting the record here
  rather than leaving §18 to imply otherwise: they are re-anchored to
  v0.4 alongside the other core and `dashbuf` cleanups (#61, #65, #114,
  #115).
- **Debt is now anchored to the slice that owns it.** Every previously
  unanchored debt and bug issue carries a milestone. Two were moved
  deliberately rather than mechanically: #64 (the `dashbuf`
  schema-evolution guard) was pulled **forward** to v0.3, because v0.3
  gives the `.dsb` format its first real producer in `dashc` (#16) and
  a silent schema break gets expensive once the format has an external
  producer; and #97 (resolving `clipsContent` into painter-consumable
  clips) was anchored to v0.3, because the reference painter panics on
  any node with `entry.clip` and story #18 had to defer its clips
  golden for that reason.

## 20. The IR is named DSB; SCD is retired

`DESIGN_1.md` §0 named the intermediate representation **SCD** ("scene
document") and the compiler **scdc**, and said in the same breath that
both were working names — "rename freely, the architecture doesn't
care". This records that the invitation was taken up.

**Decision (2026-07-13): the IR is DSB, and SCD is retired.** The name
follows the artifact that actually shipped: the format is `.dsb`, its
schema lives in `dashbuf`, and the compiler is `dashc`. Nothing was
ever published under the name SCD, so nothing external breaks.

Two names for one thing is the cost this removes. Before the rename the
document was an `Scd` in memory, serialized as a `.dsb` on disk, and
described as "SCD" in prose — three spellings of one concept, and a
reader had to learn that they were the same. They are now one.

Scope of the change:

- Rust: `Scd` → `Dsb`, `ScdNode` → `DsbNode`, `crates/dashc/src/scd.rs`
  → `dsb.rs`, and `crates/dashc/Cargo.toml`'s `description` (which is
  published metadata, so the retired name would have reached crates.io).
  No public API outside `dashc` was affected, and no behavior changed —
  the rename is mechanical and the whole test suite passed unaltered
  before and after.
- Prose: `DESIGN_1.md`, this document, `AGENTS.md`, and the `docs/`
  records.
- **The rest of the SCD vocabulary went with it.** Renaming `SCD` alone
  would have left `DESIGN_1.md` half-converted, so the seed document's
  body also drops `scdc` (the compiler is `dashc`), and its §13
  "suggested workspace layout" — which listed a `scd-*` crate family
  that was never adopted — now shows the layout that exists, per §2's
  crate map.
- **`.scb` is the one thing deliberately left alone**, and it is worth
  saying why, because the reflex is to rename it too. §3 already retired
  the extension in favour of `.dsb`, and §9 quotes DESIGN §4 **verbatim**
  — including its `.scb`. Rewriting the extension in the seed document
  would turn a quotation that claims to be verbatim into one that is not.
  A superseding record may retire a name; it may not edit the words it
  quotes. So `DESIGN_1.md`'s body keeps `.scb`, its naming note says so,
  and §3 remains the ruling.
- **`docs/archive/` is deliberately untouched.** Archived specs and
  plans are a historical record of what was decided at the time, and
  they said SCD. Rewriting them would falsify that record. They keep
  the old name, and this section explains why a reader will find it
  there.

## 21. The dashc wasm ABI is hand-written and pinned (story #17)

`dashc` builds to `wasm32-unknown-unknown` so the Deno importer can call
the same Rust code path the native library call runs (§4). Until story #17
that was aspiration, not fact: the crate was a bare `cdylib` with no
`#[unsafe(no_mangle)]` exports and no bindgen, so `just wasm` produced a
module that exported nothing callable. This section records the boundary
that fixed it, because #37 and the whole v0.7 importer build on it.

- **The ABI is hand-written, not wasm-bindgen.** Core WebAssembly has
  four value types and one linear memory — no string, array, or object
  type — so _every_ Rust-to-JS boundary reduces to "the guest reserves
  bytes, the host copies data in, the host passes an offset and a length".
  wasm-bindgen generates that; it does not avoid it. And it would not
  save the expensive half: `dashscene-validator` and `dashpaint` carry no
  `serde` by design, so `dashc` must own serializable mirrors of `Report`,
  `Diagnostic`, `Location`, and `CompileError` under any option. What
  wasm-bindgen buys is ~150 lines of framing; what it charges is a
  `wasm-bindgen-cli` pinned to the exact crate version in `bootstrap` and
  in CI, plus a post-`cargo` step in `just wasm`. For a two-function
  boundary consumed by one caller this repo also writes, that is a bad
  trade. Full reasoning, including the rejected flatbuffers-envelope
  option: `docs/decisions/dashc-wasm-abi.md`.

- **Five exports, wire version 1.** `dashc_abi_version`, `dashc_alloc`,
  `dashc_free`, `dashc_compile_figma`, `dashc_figma_image_refs`. The
  request framing and the response envelope are little-endian and
  length-prefixed (`crates/dashc/src/abi/wire.rs`, mirrored in
  `importers/figma/src/wasm.ts`). The Deno side checks
  `dashc_abi_version` at load, so a stale `.wasm` fails with a sentence
  rather than a misdecode. A version bump is how the contract evolves.

- **The module is `dashc_wasm.wasm`, not `dashc.wasm`.** The `[lib]`
  target is named `dashc_wasm` to avoid colliding with the `dashc` bin,
  which compiles to `dashc.wasm` — the CLI, which reads files and reads
  the environment and exports none of the ABI. `just wasm` therefore
  builds `--lib`, so that decoy is not produced at all.

- **#17 owns `imageRef` resolution, and asks rather than scans.** Figma
  serializes an image fill as a bare `imageRef` with no bytes anywhere in
  the file JSON, and `dashc` does no I/O. So the Deno side resolves refs
  (`GET /v1/files/:key/images`, then the presigned download) and passes
  the bytes in. _Which_ refs is `dashc`'s answer — `dashc_figma_image_refs`
  — not a walk written in TypeScript: a second copy of "where an imageRef
  lives in Figma's shape" is free to drift from the lowering that consumes
  it (P5). The capture tool commits the image **bytes**, never the
  presigned URL, which is regenerated per fetch (issue #141).

- **The `deno` CI job now runs on Rust changes.** It is what checks the
  ABI: it loads `dashc_wasm.wasm` and pins its output against
  `goldens/dsb/v03-paint.dsb`. Before #17 the job was path-filtered to
  `importers/figma/**`, so a `dashc` change that broke the ABI with no
  importer edit would skip it and merge green against a boundary nothing
  checked. The filter now includes `crates/**`, `Cargo.toml`,
  `Cargo.lock`, and `goldens/dsb/**`; the module is built once, in
  `wasm-build`, and handed over as an artifact so no Rust toolchain enters
  the deno job.

- **Byte-identity is checked through a shared golden.** The story's
  acceptance criterion — "fixture → `.dsb` byte-identical to dashc-native
  output" — is checked in two CI jobs that never meet:
  `crates/dashc/tests/figma_lowering.rs` asserts the native library call
  emits `goldens/dsb/v03-paint.dsb`, and `importers/figma/src/wasm_test.ts`
  asserts the wasm ABI emits the same bytes. Each half runs in the job
  that already exists for its toolchain, and identity is transitive.

## 22. v0.3 retrospective: plan revision at the v0.3 epic close

Epic #12 closed on 2026-07-14 with all eight stories merged. AGENTS.md requires
the remaining epics and stories to be revised against what was learned before
the next slice starts. This is that revision.

- **The v0.7 breakdown was plumbing around a compiler that cannot import a real
  file.** Its stories — closure (#37), cross-file resolution (#38), trim (#39),
  deterministic emission (#40), validator (#41), content-addressed blobs (#107)
  — all build _around_ the import. Not one of them widened what the lowering can
  express. `dashc` produces exactly one kind of document (fixed-layout,
  paint-only, text-less) and **refuses an auto-layout frame outright**, and most
  real Figma frames are auto-layout. Building #37 to #40 first would have stacked
  five stories on a compiler that cannot import a normal frame.

- **Seven of the nine tier-1 fixtures were captured for work no story
  scheduled.** `lowering-wrap`, `lowering-hug-in-fill`, `lowering-negative-gap`,
  `lowering-baseline`, `grid-basic` (layout), `variables-bound` (tokens), and the
  text in `lowering-baseline` all had a designated input and no owner. The
  corpus was ahead of the plan.

- **Three stories added to v0.7, and the slice re-ordered.** #140 is promoted
  from debt to a story — widening the lowering to auto-layout and grid — and it
  gates the rest of the slice. #159 (token resolution, phase 1 sidecar and phase
  2 join, §13) and #160 (text lowering, Figma `TEXT` into `Dsb`) are new: both
  had a stub, a captured fixture, and a written design, and neither had a story.

- **The wasm ABI is settled, so v0.7 does not have to settle it.** Story #17
  designed and pinned it (§21, `docs/decisions/dashc-wasm-abi.md`). It widens by
  carrying more, not by changing the contract — a v0.7 story that needs to send
  something new across the boundary extends the framing at wire version 1, or
  bumps the version deliberately.

- **v0.4 is unaffected.** Variants, staged mutation, and FLIP are `dashscene-core`
  and `dashcue` work; they touch neither the importer nor the ABI. Nothing blocks
  starting the slice.

- **Debt was routed to the slice that does the work**, rather than left in a
  closed milestone: the capture tool and lowering-robustness items to v0.7, the
  vocabulary gaps whose constructs v0.8 is named after (masks, group opacity,
  shadows, stacked paint) to v0.8, and the release-tooling item to v0.9. This
  follows the v0.2 precedent, whose debt moved forward at its epic close rather
  than being stranded.

- **The Figma PAT is an unmonitored dependency.** It expired unnoticed and only
  surfaced mid-story, because nothing in the repo exercises the credential — no
  test touches the network. v0.7 depends on captures far more heavily than v0.3
  did. §11 already says "rotate at ~75 days"; it is now worth a check that makes
  the state visible rather than a rule that relies on someone remembering.
