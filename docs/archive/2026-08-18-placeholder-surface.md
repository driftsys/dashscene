# The placeholder surface — story #1126

    status  gardened (story #1126, 2026-08-18)
    scope   crates/dashbuf schema, crates/dashc lowering,
            crates/dashscene-core load

Working memory for story #1126, kept as the raw original of what the session
decided. The durable record gardened from it is
`docs/decisions/a-placeholder-is-a-table-and-declares-its-measure-size.md`,
which supersedes this file wherever the two differ — three points did change
while building: `interim_fill` became an inline `Fill` union rather than a
paint-pool index, the core surface became `Prop::Placeholder` on the
`Prop::Text` precedent rather than a variant-set-style call, and the frozen
fixture is not extended because `UPDATE_DSB_FIXTURE=1` no longer regenerates.

## The problem

`docs/design/architecture.md` describes node replacement binding to four `Node`
fields — `contribution_id`, `fragment_ref`, `declared_size`, `interim_fill`.
None exists. `table Node` ends at `rotation_anchor_y` and nothing is reserved.
The names appear only in prose: that record, `docs/technotes/runtime-content.md`
§7, `docs/decisions/downloaded-raster-needs-no-vector-engine.md`, and the seed.

Story #1127's diagnostic cannot check anything until the surface exists, and an
embedded Unity host cannot draw dashscene content *with* engine content — only
beside it — which is the product epic #1106 describes.

## What bounds the design

- **The specification puts activation in v1.** `docs/specification/05-qualification.md`:
  "only placeholder activation stays in v1". The *surface* is v0.21 work; what
  fills a placeholder at runtime is not. This story carries the surface and
  stops there.
- **R7 is byte-identity, not merely append-only.**
  `docs/specification/01-goals-and-requirements.md` R7 is "Reproducible builds:
  same input → byte-identical document". The append-only discipline is what
  preserves it, and `docs/decisions/dsb-frozen-fixture-r7-guard.md` is the
  guard. The testable obligation: a document with no placeholder emits the
  bytes it emitted before this vocabulary.
- **The seed intended exactly this.** "Reserved surface (schema now, feature
  later): placeholder node kind with contribution_id / fragment_ref /
  declared_size / interim_fill. Flatbuffer fields are optional and ids
  append-only, so this costs nothing and keeps old loaders reading new
  documents."

## Decision 1 — a nested table, not four loose fields

`Node` gains one field, `placeholder: Placeholder`, holding all four.

**Presence of the table is the predicate.** A node carrying a `Placeholder` is
a declared placeholder; a node without one is not. Story #1127's four-state
diagnostic reads exactly this, and rows 2 and 3 of its table — "unfilled" and
"undeclared overload" — need a predicate that cannot be ambiguous.

Four loose fields would force the predicate to be a convention ("`contribution_id`
is present"), which #1127 would then depend on, and would spend four `Node`
field ids instead of one.

Nesting is also what `Node` already does for a grouped concern: `layout`,
`flex`, `constraints` and `paint` are all nested tables.

## Decision 2 — `declared_size` is the measure size, not a second box

`Node` already states its box (`layout.width`/`height`) and its sizing mode
(`constraints.sizing_h`/`sizing_v` ∈ {Fixed, Hug, Fill}). A `declared_size`
duplicating the box would be a second source of truth that can disagree with
the first.

It is not that. `declared_size` is **what the engine's measure callback reports
while no contribution is bound** — an intrinsic size for a node whose content
has not arrived.

This is what makes §7's contract hold *by construction*: "a declared-size box
(never hug — lazy content must not reflow the scene)". Because the measured
size is declared rather than derived from content, a contribution arriving at a
different size does not change what was measured, so nothing reflows. The
alternative reading — pin the node to `Fixed` and ban `Hug` — enforces the same
rule by restriction, and a Hug parent could then not size around a placeholder
at all.

Nothing reads `declared_size` in this story. That is the point: the surface is
carried, activation is v1.

## The schema

Appended at the tail of `crates/dashbuf/schema/dashbuf.fbs`:

    table Placeholder {
      contribution_id: uint32 = 4294967295;  // index into Document.strings
      fragment_ref:    uint32 = 4294967295;  // index into Document.strings
      declared_size:   Vec2;                 // absent = undeclared
      interim_fill:    uint32 = 4294967295;  // index into Document.paints
    }

and one field on `Node`, after `rotation_anchor_y`:

    placeholder: Placeholder;

The `uint32::MAX` sentinel and the string/paint-pool indices are the conventions
`parent`, `text`, `text_style` and `paint_entry` already use.

## The work

| file | change |
| --- | --- |
| `crates/dashbuf/schema/dashbuf.fbs` | `table Placeholder`; `Node.placeholder` |
| `crates/dashc/src/document.rs` | IR `Placeholder`; `Node.placeholder: Option<Placeholder>` |
| `crates/dashc/src/emit.rs` | intern the strings and the paint; build the table |
| `crates/dashscene-core/src/load.rs` | read it into the arena |
| `crates/dashc/tests/round_trip.rs` | a placeholder survives emit → load |
| `crates/dashbuf/tests/fixtures/v0_5_document.dsb` | regenerated with a placeholder |
| `crates/dashbuf/tests/schema_evolution.rs` | assert its values back |

## What proves it

- **Round-trip** — all four values survive emit → load.
- **Byte-identity (R7)** — a document with no placeholder emits bytes identical
  to before this vocabulary. The `Placeholder` table is absent on an ordinary
  node, so flatc writes nothing for it.
- **The frozen fixture** — `v0_5_document.dsb` gains a placeholder with
  deliberately **non-default** values, regenerated under `UPDATE_DSB_FIXTURE=1`
  in the same commit that adds the fields, asserted by value. The guard record
  requires all three properties: not generated at test time, values not
  defaults, extended in the commit that adds the fields.

**What is not proved.** Story #1126's "an older loader still reads a document
that has one" is not directly testable — there is no old binary to run against.
The frozen fixture proves no field id shifted. That an old reader ignores an
appended field is a flatbuffers vtable property, true by construction given the
append. The records say it that way rather than claiming a test that does not
exist.

## Out of scope

- The measure callback consuming `declared_size` — activation, v1.
- The unfilled / undeclared-overload diagnostic — story #1127, which needs this.
- A Figma annotation path declaring a placeholder — untracked; file if wanted.
- The host binding that fills one — needs #859's data plane.

## Records to update

- **New** `docs/decisions/` record carrying decisions 1 and 2.
- `docs/design/architecture.md` — flip "a schema surface **still to be added**
  … none of the four exists" back to carrying them. This is the *opposite* edit
  from the one story #1126's body describes: the record was corrected under
  issue #876 after the story was filed, so the story's "Done when" ("becomes
  true") is stale.
- Check `docs/technotes/runtime-content.md` §7 and
  `docs/decisions/downloaded-raster-needs-no-vector-engine.md` for prose that
  now describes something real.

`Refs #876` only — no closing keyword. Its state gets checked in both directions
after the merge.
