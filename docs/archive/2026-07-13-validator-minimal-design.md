# Spec — story #15: dashscene-validator, named diagnostics + minimal profile checks

Working memory. Gardened into `docs/decisions/` + `docs/design/` before the PR.

## Problem

P4: "Vocabulary is validated, never discovered. Every out-of-profile
construct is a named diagnostic, never a silent drop." The validator must
run from day one (DESIGN §5) even though the permissive Skia painter can
draw the whole v0.3 vocabulary — otherwise design files accumulate
vocabulary the lean painter will never support, and a painter swap becomes
design-file remediation instead of a re-golden.

Today `dashscene-validator` is a 3-line stub. Nothing validates anything.

## What the surfaces actually carry

Three findings from reading the code and DESIGN, each of which changes the
obvious design:

1. **The whole v0.3 schema vocabulary is in-profile.** DESIGN §10.1's
   triage table puts all four gradient kinds ("angular = gauges"), image
   fills + scale modes, and axis-aligned + rounded clip in the **NOW**
   band. The out-of-profile constructs — layer blur, backdrop blur,
   advanced blend modes, corner smoothing, luminance masks, noise, animated
   boolean ops — are **LATER (warn)** and **REJECT (error)**, and none of
   them exist in the `.dsb` schema at all. So "out-of-profile construct"
   cannot be detected by inspecting a `.dsb` document: by the time a
   construct is in the document, it is in the vocabulary.

   The triage therefore happens at the **import** boundary, on the
   producer's source vocabulary — which for Figma means the Figma REST
   JSON. But P5 says "Figma compatibility is a property of one producer":
   the validator must not know about Figma JSON. Resolution: the validator
   owns the **verdict table** keyed by a source-agnostic `Construct` enum;
   the producer (dashc, story #16) maps its source vocabulary onto
   `Construct` and asks for the verdict.

2. **Index referential integrity only exists on the document.** Boundary B
   has no dangling indices to find — every rect resolves to a pool entry
   (`boundary-b-unification.md`). The three bare-`u32` index fields debt
   #63 names (`Node.parent`, `Node.paint_entry`, `ImageFill.image`) are a
   `.dsb`-only failure mode.

3. **Geometry budgets only exist post-solve.** Debt #100's over-wide inside
   stroke needs the node's resolved box. P1 says the document carries
   intent, never results — a `Hug`/`Fill` node has no box in the document.
   So this rule is only checkable on the committed scene.

Those are three different questions against three different inputs. One
`validate()` cannot answer them.

## Design — three gates

    triage(construct, profile)          import gate    producer vocabulary → verdict
    validate_document(&Document, ..)    load gate      .dsb well-formedness
    validate_scene(rects, paints, ..)   paint gate     post-solve budgets

### Diagnostic shape

DESIGN §5: every out-of-profile construct → `{rule id, node path,
severity, workaround hint}`. Workaround hints and waivers are v0.7 (#41),
so v0.3 ships `{rule, severity, node, message}`.

Severity semantics, from DESIGN §5: **Error blocks emission**; **Warning**
is deferred vocabulary with a declared degrade (release builds run strict:
zero warnings, or an explicit waiver — waivers are #41).

Rule ids are stable, greppable strings (`paint.gradient.no-stops`), not
opaque numbers: a diagnostic a designer sees must be searchable.

### Profiles

`profile:full` (Unity-class) vs `profile:core` (lean/native painters).
At v0.3 the profiles differ on exactly the constructs DESIGN §10.1 annotates
`(profile:full)` — backdrop blur and advanced blend modes: **Error** under
`Core` (never available), **Warning** under `Full` (deferred, declared
degrade). Every other triaged construct has the same verdict in both.

This is the honest v0.3 answer, and it is what makes the acceptance
criterion testable. It is _not_ a claim that the profile split is finished:
it widens at v0.8 when effects enter the schema, and the Q-6 group-opacity
RT budget gets a number fixed in `profile:core`.

### Rule set (v0.1–v0.3 vocabulary)

Load gate — `validate_document`:

| rule id                            | severity | source                                                                                                          |
| ---------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------- |
| `node.parent-out-of-range`         | Error    | #63                                                                                                             |
| `node.parent-not-before-child`     | Error    | schema: "array order is DFS order"                                                                              |
| `paint.entry-out-of-range`         | Error    | #63                                                                                                             |
| `paint.conflicting-representation` | Error    | #63 (legacy `paint` + `paint_entry` both set)                                                                   |
| `text.string-out-of-range`         | Error    | same bug class as #63                                                                                           |
| `text.style-out-of-range`          | Error    | same bug class as #63                                                                                           |
| `vocabulary.unknown-enum`          | Error    | schema: append-only enums, "the load gate must range-check and emit a named diagnostic, never default silently" |

Paint-vocabulary rules — run by **both** `validate_document` and
`validate_scene` (a scene can be built without a document: the DSL and the
painter tests do exactly that):

| rule id                              | severity | source                                    |
| ------------------------------------ | -------- | ----------------------------------------- |
| `paint.gradient.no-stops`            | Error    | #100 (painter's `.first().expect()`)      |
| `paint.gradient.stop-budget`         | Error    | #100 (painter's `MAX_GRADIENT_STOPS = 8`) |
| `paint.gradient.stop-offset-invalid` | Error    | non-finite, or outside `0..=1`            |
| `paint.stroke.invalid-width`         | Error    | negative or non-finite                    |
| `paint.image-out-of-range`           | Error    | #63                                       |

Paint gate only — `validate_scene`:

| rule id                    | severity | source                                                                                   |
| -------------------------- | -------- | ---------------------------------------------------------------------------------------- |
| `paint.stroke.exceeds-box` | Error    | #100: `Inside` align with `width > min(w, h)` inverts the inset and the stroke collapses |

Threshold check against the painter: `draw_stroke` does
`rrect.with_inset((half, half))` with `half = width / 2`, so the inset box
is `w - width` by `h - height`. It inverts strictly above `min(w, h)`;
exactly at `min(w, h)` the stroke fully covers the box, which renders
correctly. Hence `>`, not `>=`.

### Why the validator depends on dashbuf + dashpaint, and not dashscene-core

Publish order is `dashbuf → dashpaint → dashscene-core → … → validator`.
Core cannot call the validator (it is published earlier), and it does not
need to: `CommittedScene`'s accessors already hand out `dashpaint` types
(`rects() -> &[RectEntry]`, `paints() -> &PaintTable`). So `validate_scene`
takes boundary-B types and the validator needs no core dependency at all.
The producer calls it, not the arena.

## Known gap, deliberately not closed here

The story text assumes two live producer entries, "DSL commit and dashc
compile". Neither can currently emit an out-of-profile construct:

- `dashc` is still a stub (that is story #16).
- **core's `Prop` enum only has `Fill(Color)`.** The staged-mutation
  producer API cannot express a gradient, stroke, corner radius, image, or
  clip _at all_. dashlang, which is a skin over `Prop`, inherits that.

So the v0.3 paint vocabulary can only enter a scene through a
`dashbuf::Document` or by a test constructing `dashpaint::PaintEntry`
directly (which is what the #14/#18 painter tests do). Wiring the validator
into `dashlang::Scene::build` today would validate scenes that can only
ever hold solid fills — a check that cannot fail.

That gap is real and worth an issue, but it is producer-vocabulary work,
not validator work: closing it means widening `Prop`, which belongs with
the document→arena loader in #16 (whose acceptance criteria already require
".dsb → loads in dashscene-core → renders"). File it; do not do it here.

## Alternatives considered

- **One `validate(&Document)` entry.** Rejected: cannot express #100's
  geometry budgets (P1 — no resolved boxes in a document), and cannot see
  the constructs the triage table is about (they never reach the schema).
- **Geometry rules on the document using the authored `FixedSizeLayout`.**
  Rejected: only correct for `Fixed`/`Fixed` sizing; silently wrong under
  `Hug`/`Fill`, which is worse than not checking.
- **A `Figma` module inside the validator that parses REST JSON.** Rejected
  by P5: Figma compatibility is a property of one producer. The validator
  owns the verdict; dashc owns the mapping.
- **Rule ids as an enum with numeric codes.** Rejected: a designer-visible
  diagnostic must be greppable. Stable strings, with the enum kept only as
  the internal constructor.

## Acceptance

- A construct in the LATER/REJECT bands produces the named diagnostic, with
  the profile-dependent severity; nothing is silently dropped.
- Every rule above has a test that fails without the rule.
- The two painter panics debt #100 names (`no-stops`, over-wide inside
  stroke) are each caught by a validator diagnostic before the painter is
  reached.
- `just build` green.
