# Figma vocabulary profile

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §10.1

This is a profile specification: it defines what the validator must accept, warn
on, and reject. It is what makes P4 ("vocabulary is validated, never
discovered") checkable. The validator that implements this triage is
`crates/dashscene-validator`; its three-gate architecture is recorded in
[validator-three-gates.md](../decisions/validator-three-gates.md).

    NOW (v0/v1)    all four gradient types (angular = gauges),
                   image fills + scale modes, baked drop/inner
                   shadows, backdrop blur (v0.11 — every painter
                   honours it, see
                   decisions/backdrop-blur-is-core-vocabulary.md),
                   shape masks, group opacity (compiler
                   detects non-overlapping children → per-node
                   opacity free; overlapping → budgeted RT),
                   axis-aligned + rounded clip, full text stack,
                   static variable-font instances, full auto-layout
                   (R2). Renders ~95 % of real product design
                   files.
    LATER (warn)   layer blur (budgeted), advanced blend modes
                   (profile:full; spike
                   KHR_blend_equation_advanced first — it may make
                   multiply/screen nearly free), corner smoothing
                   (squircle), luminance masks, clip-on-rotated,
                   kashida justification.
    REJECT (error) noise/texture/progressive-blur effects, animated
                   boolean ops, animated variable-font axes,
                   variable-width strokes — each with a documented
                   workaround (bake it, slot it, design without it).

Deferred items are a negotiation surface with design, not a compatibility debt:
every LATER item has a designer-visible workaround today, and the validator says
so at import time.

**Two mechanisms carry these bands, and they behave differently.**
[06-dashc-figma-lowering.md](06-dashc-figma-lowering.md) requires the split —
the producer maps, the validator decides — and which one a construct travels
through decides its disposition:

- **The triage path.** `constructs_of` recognises the construct, the node **is
  lowered**, and `dashscene_validator::triage` assigns the severity from
  `Construct::verdict(profile)` — the _validator profile_, Core or Full, not the
  emit policy. A REJECT-band construct is an error under both emit policies. The
  Figma producer raises five constructs this way: advanced blend modes, corner
  smoothing, layer blur, progressive blur, and noise or texture effects.
- **The blocker path.** A gate in the walk finds something it cannot lower at
  all, the node is **not** lowered, and `figma.unsupported` names it. Here the
  severity is the emit policy's — an error under `EmitPolicy::Strict`, which
  withholds the document (R6), and a warning under `Partial`, which is what the
  production importer runs (`importers/figma/src/import.ts` defaults `strict` to
  false). A refusal also drops every layer below the refused one, while naming
  only the refused layer itself (issue #875).

**A construct can sit in a band above and still be refused.** A luminance mask
is LATER for the validator, whose `Construct::LuminanceMask` carries a warning,
but the Figma walk pushes a blocker before that construct is ever raised —
`constructs_of` never emits it — so the layer does not import. The band and the
table row below are both true, of different components. Read the band as the
validator's vocabulary and the table as what the Figma lowering does.

**A fourth disposition, for what is not read at all.** The three bands above
assume the construct is seen. `crates/dashc/src/figma/rest.rs` models the
properties the lowering was taught and does not set `deny_unknown_fields`, so a
property outside that model is dropped with nothing reported — not lowered, not
diagnosed, and describable by none of the three bands. Per-side stroke widths
are in this class; so are constraints, stroke cap and join, layout grids, export
settings and paragraph spacing. This profile calls it **NOT READ (silent)**, and
[`../figma-support.md`](../figma-support.md) enumerates the class against the
code.

Prototyping is **not** in it, though issue #802's own body put it there:
`rest.rs` models `interactions`, `figma/prototype.rs` reads them, and
`figma/variants.rs` reports what it cannot lower under
`figma.prototype.unsupported-interaction` and
`figma.prototype.unsupported-motion`.

The table's fourth label, **REFUSED (named)**, is the blocker path above: the
layer does not import, every layer below it is dropped too, and
`figma.unsupported` names the construct at a severity the emit policy chooses.

## Paint and text edge cases

The entries below close gaps found while enumerating the Figma paint and text
property space against R6. The tier column states **what the importer does
today**, not what is planned for it; where the two differ, the disposition
column names the intended tier as well.

| Construct                                                       | Tier                   | Disposition and designer-visible workaround                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------------------------------------------------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| stroke on text (any alignment)                                  | REFUSED (named)        | Any visible stroke on a TEXT node refuses the layer, centered included — the gate reads the `strokes` array, never the alignment. There is no collapse to centered and no stroke-on-text lowering of any kind. Intended tier LATER (warn). Workaround: bake the outline, or place a shape behind the text.                                                                                                        |
| per-side stroke widths                                          | NOT READ (silent)      | `rest.rs` has no field for them, so a design using them compiles with the single uniform `strokeWeight` and nothing is reported. Intended tier LATER (warn) → REJECT. Workaround: four edge rects.                                                                                                                                                                                                                |
| dashed strokes                                                  | REFUSED (named)        | A non-empty `strokeDashes` refuses the layer by name; the stroke is not repainted as a solid one. The gate sits below `single_visible_paint`, so a node whose `strokes` are empty or all invisible is **not** refused however its `strokeDashes` reads — Figma leaves that field set from an earlier edit. Intended tier LATER (warn). Workaround: a baked dash pattern.                                          |
| luminance masks                                                 | REFUSED (named)        | `maskType: LUMINANCE` refuses the layer by name, as does `ALPHA`. Only a geometry mask on a box-shaped node lowers, to a hard clip. Intended tier LATER (warn). Workaround: bake the masked result, or use a geometry mask.                                                                                                                                                                                       |
| single-stop gradients                                           | NOW (lowered verbatim) | Lowered as a gradient carrying its one stop — **not** converted to a solid fill, and no diagnostic records it. One stop is valid input: the validator refuses zero (`paint.gradient.no-stops`) and more than `MAX_GRADIENT_STOPS` (`paint.gradient.stop-budget`). A one-stop gradient is still subject to the offset rules — `paint.gradient.stop-offset-invalid` refuses an offset outside `0..=1`.              |
| mask scoping / bounds (not an importer disposition — see below) | NOW (semantics)        | A mask's clip bounds are the tight intersection of the mask and its maskee, not the parent box; pinned by `crates/dashscene-core/tests/arena.rs` and by the `v013-mask-effect-bleed` golden, the scene where the two readings differ in pixels (refines [masks-and-group-opacity.md](../decisions/masks-and-group-opacity.md)).                                                                                   |
| text letter-case (upper/lower/title)                            | REFUSED (named)        | A `textCase` other than `ORIGINAL` refuses the layer by name. There is no case-transform vocabulary anywhere in the workspace — not in `dashscene-typeset`, `dashbuf` or `dashscene-core` — so this is an unbuilt construct rather than drift from a working one. It was NOW (supported) here and is unscheduled now; guardrail G-8 binds whenever it is built. Workaround: author the text in the case you want. |

The per-side-stroke tier is provisional: it stays unread until the triage
discussion decides support (four edge rects lowered) or rejection with the
four-edge-rects workaround. Being unread is the worst of the three cases: a
designer using it sees no diagnostic at all, which is the one disposition P4
does not permit for a construct this profile has an opinion about.

The mask-scoping row is a semantics ruling, not open vocabulary — it binds the
lowering and the validator the same way the tier block above does.

This table was corrected against the lowering on 2026-08-14 (issue #802). It
held six rows and five of them said something the code does not do — every one
reading as **more** supported than it is, which is the direction that costs a
designer time. Only the mask-scoping row survived unchanged. The luminance-mask
row is new here, and the LATER band above **keeps** its entry: the band is the
validator's vocabulary and the row is the Figma lowering's behaviour, and for
this construct the two differ. Derive a row from `crates/dashc/src/figma/mod.rs`
and `crates/dashc/src/figma/rest.rs` before trusting it, not from this file's
history.
