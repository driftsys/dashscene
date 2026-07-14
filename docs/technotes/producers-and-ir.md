# Technote — producers & the intermediate representation

    status   design note, 2026-07-13. Captures conclusions from a design
             discussion; extends specs/DESIGN_1.md and specs/SCOPE_DECISIONS.md
             without superseding them. Items marked DECISION are settled; items
             marked CANDIDATE / OPEN are not.
    scope    where producer-specific knowledge lives, what owns the neutral
             representation, and which external design tools we take as sources.

This note answers a cluster of related questions: should `dashc` handle Figma
export directly; do we need a driftsys-owned intermediate format; can we support
open-source design tools (Penpot) for layouting; and how we compare to Slint.
The through-line is one question — _where does producer-specific knowledge live,
and what owns the neutral form_ — so the answers are recorded together.

## 1. The Figma export boundary — `dashc` lowers, it does not export

DECISION (re-affirms SCOPE_DECISIONS §4; already reflected in the code).

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

DECISION.

We already own two producer-neutral representations, and they are the right two:

- **`dashbuf` / `.dsb`** — the dashscene schema and its in-memory semantic model. Its
  reason to exist (P5) is that it is producer-neutral: Figma is one client, the
  DSLs are others.
- **The `dashscene-core` arena + staged-mutation API** (`open`/`set_prop`/
  `set_variant`/`commit`) — DESIGN §4 calls it "the real contract; `.dsb` is one
  way to populate it."

The tempting third format — a neutral "design interchange" layer _above_ dashscene that
Figma and Penpot both translate into — must **not** be built. It would carry the
same design intent dashscene's layout/variant/paint tables already carry, i.e. ~90%
schema overlap with dashscene, two schemas to evolve in lockstep, two validators, a
translation at every seam, and the classic interchange-format failure (lossy or
bloated). It would also dilute the "one schema, file and wire" discipline
(SCOPE §3). The tell that a neutral-IR-above-dashscene is redundant is that it would
look almost exactly like dashscene.

What _is_ worth formalising is smaller: the **seam contract** — name and version
the "canonical post-closure JSON" handoff as a driftsys-owned _input schema for
the lowering step_, not a second IR. Thin, not a tar pit.

## 3. Two entry paths — producers choose compile-path or arena-path

DECISION (frames how every future producer is added).

There are two ways into dashscene, and picking the right one per producer is what makes
"no new format" workable:

- **Offline compile path** — external JSON → `dashc` lowering → dashscene → `.dsb`.
  Figma uses this because Figma is far from CSS and needs real lowering.
- **In-memory arena path** — producer code → `dashscene-core` API → dashscene, with no
  serialised intermediate. The Rust DSL uses this ("direct arena calls, no
  serialization", DESIGN §6.2).

The question for any new producer is which path it uses — never "do we need a new
format."

## 4. Penpot — candidate CSS-native second producer (arena-path, deferred)

CANDIDATE, deferred to post-v0.

Penpot is the only serious open-source design tool with a first-class layout
engine and programmatic access. Its decisive property: **its layout _is_ literal
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
  90-day-PAT treadmill (SCOPE §10), no seat-gated rate limits, and no Enterprise
  wall in front of variables (the reason the token phase-1/phase-2 split exists,
  SCOPE §12). Penpot's tokens/variables are open.
- **License-clean fixtures.** Penpot is AGPL/self-hostable, so self-authored
  Penpot files have none of the Figma Community licensing ambiguity that shapes
  SCOPE §8 — potentially a cleaner fixture source for pure layout-mechanics cases.

Not now: v0/v1 requirements (Arabic text, full Figma auto-layout, 2025 Draw-effect
triage) are Figma-shaped, and adding Penpot before the Figma path and the DSL both
exit v0 validates an abstraction not yet paid for. The cheap, forward-looking move
is §1's: keep the Figma≠CSS lowering named as Figma-specific so a future Penpot
path is "add a near-identity lowering," not "untangle Figma assumptions."

Before committing: spike a real Penpot file fetch and confirm the plugin API
exposes grid spans and areas as cleanly as the UI implies — that is the exact
corner (Taffy baseline + grid spans) already flagged as least-exercised (Q-4).

## 5. Slint — build-vs-adopt: reference for ideas only

DECISION: do not adopt or borrow code from Slint.

Slint is the closest thing in the Rust world to "the stack you'd reach for instead
of building this," and DESIGN already credits the Taffy/Servo/Bevy/Slint/Zed
lineage. But it solves a different problem and its licensing forecloses adoption.

Different problem: Slint is a declarative GUI _toolkit that renders itself_; dash
is a design-source-to-pixels _pipeline that renders the same scene across foreign
engines it does not own_. Three of our hard requirements are exactly what Slint's
architecture cannot give, no matter how mature it gets:

- **Unity as product renderer (G2).** Slint owns rendering top to bottom (its own
  software / femtovg / Skia / Qt backends); it has no path into a game engine's
  lit, world-space SRP. Our painters-only-colour split (P1/P2) exists precisely to
  render one scene identically across Unity / Skia / native.
- **Perfect Arabic identical on every backend (R1).** Slint's text is its own
  integrated, renderer-bound path with historically limited complex-script support;
  "identical Arabic in Unity _and_ the native painter _and_ the Skia oracle" is not
  something it targets. Our shape-once-in-Rust + atlas-quads approach gives that by
  construction.
- **Design-as-reproducible-source (P5/R7).** Slint's Figma integration is a
  one-shot "Figma to Slint" code generator; after it runs you own the `.slint`
  code and the design file stops being the source of truth. Ours is a reproducible
  pipeline with a validator.

Licensing is the decisive, separate blocker (this is the headline, the capability
gaps are supporting detail). Slint is tri-licensed: royalty-free only for
proprietary **desktop/mobile/web**, GPLv3 for open source, and a **paid commercial**
licence for proprietary **embedded**. Our target is embedded/automotive, so the
royalty-free tier does not apply; the doors are GPLv3 (a non-starter for a
proprietary automotive product) or a commercial contract with SixtyFPS (recurring
cost + single-vendor dependency on the critical path). And because our repo is MIT
(SCOPE §7), GPLv3 code cannot be lifted into it — so Slint is not even a _code_
borrow source; the Figma-to-Slint plugin is under the same terms. **Reference for
ideas only** (its software-renderer design, its MCU/GLES work), clean-room, never
source.

This strengthens the build decision: our permissive pure-Rust stack (Taffy,
rustybuzz, ttf-parser, unicode-bidi, msdf-atlas-gen, skia-safe — all MIT/Apache/
BSD-family) is what keeps dash MIT and promotable into the public `dashscene`
facade. A GPLv3 dependency anywhere would poison that. The "if Unity softens, fall
back to Slint" escape hatch is therefore not free — it is GPLv3 (incompatible) or
commercial (cost + lock-in).

## 6. Layout & placement — Taffy stands; radial/safety placement is the open gap

The automotive HMI world sells full _toolchains_ (Kanzi, EB GUIDE, Altia, CGI
Studio, Crank Storyboard (now The Qt Company), Qt Automotive, Embedded Wizard),
plus Flutter and Android Automotive/Compose — not reusable layout _engines_. Their
layout systems are baked in and not extractable, and none offers a better box model
than CSS flex/grid; what they sell is HMI authoring + ASIL/ISO-26262-certified
renderers + AUTOSAR/QNX integration + 3D composition, which is orthogonal to the
solver choice. Among embeddable engines (Taffy, Yoga, Slint's, Flutter's, Qt's),
Taffy remains correct: the only pure-Rust engine covering all four CSS modes with
no runtime baggage. **No automotive engine is adopted.**

OPEN (worth an explicit decision rather than an accidental one): **radial / curved /
path-anchored placement, and safety-regulated fixed regions.** Circular gauges,
arced menus, telltales at regulator-mandated positions — CSS flex/grid, Figma
auto-layout, and Penpot all lack a radial mode. Today the design absorbs gauges as
angular gradients + absolute positioning + rotation, i.e. radial is _not_ a layout
concept, it is manual absolute placement plus a transform. Decide on purpose:
is radial/anchored placement ever a first-class dashscene layout mode, or forever
"absolute box + transform, producer computes the angle"? For automotive clusters
this recurs, so it should be written down.

## 7. Open items

- Seam contract (§1): name/version the canonical post-closure JSON handoff.
- Penpot (§4): spike a real fetch; confirm plugin-API grid-span/area fidelity.
- Radial/safety placement (§6): first-class layout mode vs absolute+transform.
- Keep Figma≠CSS lowering named as Figma-specific in `dashc` (§1/§4).
