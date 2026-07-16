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
                   shadows, shape masks, group opacity (compiler
                   detects non-overlapping children → per-node
                   opacity free; overlapping → budgeted RT),
                   axis-aligned + rounded clip, full text stack,
                   static variable-font instances, full auto-layout
                   (R2). Renders ~95 % of real product design
                   files.
    LATER (warn)   layer blur (budgeted), backdrop blur + advanced
                   blend modes (profile:full; spike
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
