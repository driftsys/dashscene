# Technote — producers & the intermediate representation

    status   design note, 2026-07-13. Captures conclusions from a design
             discussion; extends docs/archive/2026-07-14-design-1-seed.md
             and docs/archive/2026-07-14-scope-decisions.md without
             superseding them. Items marked DECISION are settled; items
             marked CANDIDATE / OPEN are not.
    scope    where producer-specific knowledge lives, what owns the neutral
             representation, and which external design tools we take as sources.

This note answers a cluster of related questions: should `dashc` handle Figma
export directly; do we need a driftsys-owned intermediate format; can we support
open-source design tools (Penpot) for layouting; and how we compare to Slint.
The through-line is one question — _where does producer-specific knowledge live,
and what owns the neutral form_ — so the answers are recorded together.

## 1. The Figma export boundary — `dashc` lowers, it does not export

DECISION → [`dashc-lowers-figma-it-does-not-export.md`](../decisions/dashc-lowers-figma-it-does-not-export.md)

"Figma export" is two different jobs and they live on opposite sides of a seam:

- **Getting data out of Figma** — REST fetch, PAT rotation, rate-limit backoff,
  the reachability closure across files, variant-set closure, trim rules, the
  token phase-1 sidecar. This is I/O, auth, and JSON shaping. It lives in the
  Deno importer (`importers/figma/`). `fetch.ts` and `capture.ts` are the
  implemented parts; `closure.ts` / `trim.ts` / `tokens.ts` are the v0.7 stubs.
- **Turning that into a `.dsb`** — the Figma≠CSS lowering, profile/vocabulary
  validation, deterministic emission. This is `dashc`, compiled to wasm and
  called from Deno via `compileViaWasm(canonicalJson) → { document | diagnostics }`.

`dashc` must own only the second. Three reasons, none cosmetic:

- **One implementation of lowering + validation.** Compiling `dashc` to wasm
  _and_ running it native exists precisely so there is no second, drifting copy
  of the rules in TypeScript. A seam at "canonical post-closure JSON" keeps the
  Rust surface a pure function.
- **R7 lives on the pure side.** `compile(&Scd) → Result<Vec<u8>, Report>` is
  deterministic because it touches nothing but its input. Put the fetch inside
  `dashc` and byte-reproducibility now depends on Figma's server, the clock, and
  token state.
- **Auth/rate-limits are host concerns.** PAT rotation, `Retry-After`, seat tiers
  — plumbing Deno does natively and a wasm compiler should not know exists.

Still open (lives in the stubs, not the decided part): exactly how much
canonicalisation Deno does before handoff versus how much raw Figma shape
`dashc.wasm` ingests. Today `dashc`'s lowering is Figma-_aware_; keep that
lowering **named as Figma-specific** so it never blurs into "the lowering" and
box out a future producer (see §4).

## 2. No neutral IR above dashscene — the two driftsys-owned formats

DECISION → [`no-neutral-ir-above-dashscene.md`](../decisions/no-neutral-ir-above-dashscene.md)

We already own two producer-neutral representations, and they are the right two:

- **`dashbuf` / `.dsb`** — the dashscene schema and its in-memory semantic model. Its
  reason to exist (P5) is that it is producer-neutral: Figma is one client, the
  DSLs are others.
- **The `dashscene-core` arena + staged-mutation API** (`open`/`set_prop`/
  `set_variant`/`commit`) — `docs/design/architecture.md` calls it "the real
  contract; `.dsb` is one way to populate it."

The tempting third format — a neutral "design interchange" layer _above_ dashscene that
Figma and Penpot both translate into — must **not** be built. It would carry the
same design intent dashscene's layout/variant/paint tables already carry, i.e. ~90%
schema overlap with dashscene, two schemas to evolve in lockstep, two validators, a
translation at every seam, and the classic interchange-format failure (lossy or
bloated). It would also dilute the "one schema, file and wire" discipline
(`docs/decisions/dsb-format-and-one-schema.md`). The tell that a
neutral-IR-above-dashscene is redundant is that it would
look almost exactly like dashscene.

What _is_ worth formalising is smaller: the **seam contract** — name and version
the "canonical post-closure JSON" handoff as a driftsys-owned _input schema for
the lowering step_, not a second IR. Thin, not a tar pit.

## 3. Two entry paths — producers choose compile-path or arena-path

DECISION → [`two-producer-entry-paths.md`](../decisions/two-producer-entry-paths.md)

There are two ways into dashscene, and picking the right one per producer is what makes
"no new format" workable:

- **Offline compile path** — external JSON → `dashc` lowering → dashscene → `.dsb`.
  Figma uses this because Figma is far from CSS and needs real lowering.
- **In-memory arena path** — producer code → `dashscene-core` API → dashscene, with no
  serialised intermediate. The Rust DSL uses this ("direct arena calls, no
  serialization", `docs/archive/2026-07-14-design-1-seed.md` §6.2).

The question for any new producer is which path it uses — never "do we need a new
format."

## 4. Penpot — candidate CSS-native second producer (arena-path, deferred)

CANDIDATE, deferred to post-v0.

Penpot is an open-source, self-hostable design tool with a layout engine and
programmatic access. Its decisive property: **its layout _is_ literal
CSS Flexbox and CSS Grid** — direction/wrap/justify/align/gap/padding, fit-fill-fix
sizing, grid tracks in fr/auto/px, spans, named areas — which is exactly Taffy's
model and dashscene's layout table. Where Figma needs the whole Figma≠CSS lowering bag,
Penpot maps near-identity onto dashscene.

That reframes "should we support it": because Penpot is CSS-native, a Penpot
producer should likely enter via the **arena path, like a DSL** — building dashscene
directly with a thin mapping — rather than through `dashc`'s JSON-lowering path
where it would only ever be a passthrough. So it adds no new format and no new
schema; it picks the entry path that matches how neutral the source already is.

Why it is genuinely interesting beyond "OSS":

- **It is the cleanest proof of P5.** Feeding the IR a second source whose layout
  vocabulary differs from Figma's but lands in the same tables either proves dashscene
  is producer-neutral or exposes where it is secretly Figma-shaped.
- **It sidesteps documented pains.** Self-hostable and open-format → no
  90-day-PAT treadmill (`docs/decisions/figma-access-plan-and-pat-policy.md`),
  no seat-gated rate limits, and no Enterprise wall in front of variables (the
  reason the token phase-1/phase-2 split exists,
  `docs/decisions/token-resolution-phase-split.md`). Penpot's tokens/variables
  are open.
- **License-clean fixtures.** Penpot is MPL-2.0 and self-hostable
  (<https://github.com/penpot/penpot/blob/develop/LICENSE>, retrieved
  2026-08-10), so a self-hosted instance's own files have none of the Figma
  Community licensing ambiguity that shapes
  `docs/decisions/figma-corpus-self-authored-only.md` — potentially a cleaner
  fixture source for pure layout-mechanics cases. An earlier revision of this
  note said AGPL; that was wrong, and MPL-2.0 is file-level copyleft rather
  than network copyleft, so the condition is materially weaker than the one
  this paragraph was reasoning about.

Not now: v0/v1 requirements (Arabic text, full Figma auto-layout, 2025 Draw-effect
triage) are Figma-shaped, and adding Penpot before the Figma path and the DSL both
exit v0 validates an abstraction not yet paid for. The cheap, forward-looking move
is §1's: keep the Figma≠CSS lowering named as Figma-specific so a future Penpot
path is "add a near-identity lowering," not "untangle Figma assumptions."

Before committing: spike a real Penpot file fetch and confirm the plugin API
exposes grid spans and areas as cleanly as the UI implies — that is the exact
corner (Taffy baseline + grid spans) already flagged as least-exercised (Q-4).

## 5. Build-vs-adopt — why the pipeline is written rather than taken

DECISION → [`no-gui-toolkit-dependency.md`](../decisions/no-gui-toolkit-dependency.md)

The reasonable alternative to writing this was building on an existing Rust GUI
toolkit. Slint is the nearest candidate — DESIGN already credits the
Taffy/Servo/Bevy/Slint/Zed lineage — so it is the one evaluated here.

**The requirements are about rendering somewhere else.** Slint describes itself as
"an open-source declarative GUI toolkit to build native user interfaces for Rust,
C++, JavaScript, or Python apps": it draws its own output, through its own
backends. dashscene has to hand one scene to renderers this project does not own
and have them agree to the pixel. Three requirements follow from that, and each is
a statement about dashscene rather than about any toolkit:

- **Unity as product renderer (G2).** The scene has to reach a game engine's lit,
  world-space SRP. The painters-only-colour split (P1/P2) exists so that one
  document renders identically across Unity, Skia and the lean native painter.
- **Identical Arabic on every backend (R1).** Shaping happens once in Rust and
  reaches every painter as atlas quads, so the Skia oracle, the lean painter and
  Unity cannot disagree about a glyph position. Nothing about that requirement
  depends on what another project supports.
- **Design-as-reproducible-source (P5/R7).** The Figma file stays the source of
  truth and recompiles through a validator, rather than being converted once into
  code that is then maintained by hand.

**The licence decides it independently of any of that.** Slint's framework is
triple-licensed, and a user may choose any one of the three
(<https://github.com/slint-ui/slint/blob/master/LICENSE.md>, retrieved 2026-08-10):
a royalty-free licence for proprietary desktop, mobile and web applications at no
cost, which excludes embedded systems; GPL-3.0-only at no cost for open-source
software on any platform including embedded; and a commercial licence for
proprietary use including embedded. dashscene targets embedded hardware for
proprietary products, so of those three only the commercial licence applies.

Separately, this repository is Apache-2.0
(`docs/decisions/apache-2-0-for-the-patent-grant.md`), and GPL-3.0-only code cannot
be incorporated into an Apache-2.0 project — so copying source is ruled out on its
own terms. Ideas may be read and reimplemented clean-room; source is not copied.

That is also why the dependency stack is permissive throughout — Taffy, rustybuzz,
ttf-parser, unicode-bidi, msdf-atlas-gen, skia-safe, wgpu, all MIT/Apache/BSD-family.
A GPL-3.0-only dependency anywhere would make this repository's own licence
unusable. The "if Unity softens, fall back to Slint" escape hatch is therefore not
free: it would mean taking the commercial licence or changing this repository's
licence.

## 6. Layout & placement — Taffy stands; radial/safety placement is absolute box + transform

The automotive HMI world sells full _toolchains_ (Kanzi, EB GUIDE, Altia, CGI
Studio, Crank Storyboard (now The Qt Company), Qt Automotive, Embedded Wizard),
plus Flutter and Android Automotive/Compose — not reusable layout _engines_. Their
layout systems are baked in and not extractable, and none offers a better box model
than CSS flex/grid; what they sell is HMI authoring + ASIL/ISO-26262-certified
renderers + AUTOSAR/QNX integration + 3D composition, which is orthogonal to the
solver choice. Among embeddable engines (Taffy, Yoga, Slint's, Flutter's, Qt's),
Taffy remains correct: the only pure-Rust engine covering all four CSS modes with
no runtime baggage. **No automotive engine is adopted.**

DECISION → [`radial-is-not-a-layout-mode.md`](../decisions/radial-is-not-a-layout-mode.md)

**Radial / curved / path-anchored placement, and safety-regulated fixed regions.**
Circular gauges, arced menus, and telltales at regulator-mandated positions recur in
automotive clusters, and CSS flex/grid, Figma auto-layout, and Penpot all lack a
radial mode. Resolved on purpose: radial is _not_ a dashscene layout mode and will
not become one. Placement stays an absolute box plus a transform (or normal flex);
the radial part is a transform or paint computed from a bound scalar; and the gauge
vocabulary is first-class bound-prop data in the animation vocabulary's per-prop
smoothing row, so the runtime owns time and the gauge is reproducible in tests (P3,
R4). Safety-regulated fixed regions stay absolute boxes checked by a `fixed-region`
validator attribute (resolved rect equals authored rect) — a check, not a layout
mode. Tick marks, arced label rings, and curved menus are producer-side repeater
math into absolute boxes at authoring time: no runtime radial solver, no new IR
concept.

## 7. Open items

- Seam contract (§1): name/version the canonical post-closure JSON handoff.
- Penpot (§4): spike a real fetch; confirm plugin-API grid-span/area fidelity.
- Radial/safety placement (§6): resolved as absolute box + transform
  ([`radial-is-not-a-layout-mode.md`](../decisions/radial-is-not-a-layout-mode.md));
  two follow-ups remain — fold the gauge parameter set into the animation spec when
  the per-prop smoothing vocabulary is written, and add the `fixed-region` attribute
  to the validator spec. Both are post-v0: a v1 full-feature-set candidate, not v2.
- Keep Figma≠CSS lowering named as Figma-specific in `dashc` (§1/§4).
