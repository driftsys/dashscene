# Figma vocabulary profile

    status  as-built, gardened from the seed document 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §10.1

This is a profile specification: it defines what the validator must accept,
warn on, and reject. It is what makes P4 ("vocabulary is validated, never
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

Deferred items are a negotiation surface with design, not a
compatibility debt: every LATER item has a designer-visible
workaround today, and the validator says so at import time.

## Paint and text edge cases

The entries below close gaps found while enumerating the Figma paint and
text property space against R6: every property maps to supported, lowered,
or diagnosed — there is no fourth bucket. Each names its tier and the
disposition a designer sees at import time.

| Construct                            | Tier                  | Disposition and designer-visible workaround                                                                                                                                                                                                                                                                                     |
| ------------------------------------ | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| stroke-on-text alignment (in/out)    | LATER (warn)          | Centered stroke on text is supported first; inside/outside collapses to centered and the diagnostic names the collapse.                                                                                                                                                                                                         |
| per-side stroke widths               | LATER (warn) → REJECT | One uniform stroke width is supported; per-side is warned pending a triage decision that may reject it. Workaround: four edge rects.                                                                                                                                                                                            |
| dashed strokes                       | LATER (warn)          | Solid strokes are supported. Workaround: a baked dash pattern.                                                                                                                                                                                                                                                                  |
| single-stop gradients                | NOW (lowered)         | Lowered to the equivalent solid fill as an explicit lowering, with an info diagnostic recording it — never an undocumented degrade.                                                                                                                                                                                             |
| mask scoping / bounds                | NOW (semantics)       | A mask's clip bounds are the tight intersection of the mask and its maskee, not the parent box; pinned by `crates/dashscene-core/tests/arena.rs` and by the `v013-mask-effect-bleed` golden, the scene where the two readings differ in pixels (refines [masks-and-group-opacity.md](../decisions/masks-and-group-opacity.md)). |
| text letter-case (upper/lower/title) | NOW (supported)       | Applied in the typesetter, pre-shaping (P2).                                                                                                                                                                                                                                                                                    |

The per-side-stroke tier is provisional: it stays a warning until the triage
discussion decides support (four edge rects lowered) or rejection with the
four-edge-rects workaround. The single-stop-gradient and mask-scoping rows are
semantics rulings, not open vocabulary — they bind the lowering and the
validator the same way the tier block above does.
