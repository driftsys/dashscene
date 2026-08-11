# The animation reference set is the union of two producers, not SMIL

    status   accepted 2026-08-11, at the v0.18 close (epic #769). Gardened
             from docs/wip/2026-08-09-svg-as-a-second-producer.md, whose
             reference-set half this is; that capture stays for its
             profile half, which waits on the SVG importer
    affects  dashcue, dashbuf, dashscene-core, dashc, the future SVG importer
    related  docs/decisions/a-loop-is-ambient-paint-anchored-at-load.md
             docs/decisions/motion-is-document-data-keyed-on-the-destination.md
             docs/decisions/a-step-is-a-pair-of-keyframes.md
             docs/decisions/dashcue-keyframe-values-are-progress-fractions.md
             docs/decisions/two-producer-entry-paths.md
             docs/technotes/runtime-content.md

SMIL is SVG 1.1's declarative in-file animation vocabulary: `<animate>`,
`<animateTransform>`, `<animateMotion>`, `<set>`, the deprecated
`<animateColor>`, with `attributeName`, `from`/`to`/`values`, `keyTimes`,
`keySplines`, `dur`, `begin` and `repeatCount`. It is the only animation
vocabulary this project has a large official corpus for, which makes it
tempting as the checklist that defines what `dashcue` must eventually
express.

`docs/wip/2026-08-07-animated-content-import.md` observes that SMIL "maps
onto `dashcue` better than Lottie does", and that remains true of the
mapping. **It does not follow that SMIL should define the feature set**, and
this record rules that it does not.

## Three structural reasons

**It is a timeline model; `dashcue` is a state-transition model.**
`dashcue::VariantTransition` carries `tracks` and `stagger`, and
defines motion as how a prop travels from its old to its new resolved value
when a variant switch commits. There is no timeline in the crate at all —
the word appears zero times across its three modules. Adopting SMIL as the
reference means adopting one, which is a second scheduling regime rather
than a vocabulary addition. The two do not merge: `advance(dt)`
forward-integrates and a spring carries velocity, so a spring track cannot
be seeked.

**Its value model is what P1 forbids.** SMIL's `from`, `to` and `values` are
literal attribute values. `dashcue::Keyframe` is deliberately
the opposite — its `value` is a progress fraction of the bound `from → to`
span, because a document never carries resolved values
(`docs/decisions/dashcue-keyframe-values-are-progress-fractions.md`). About
a third of real SMIL usage animates resolved geometry. An importer must bind
those to solver-produced endpoints or refuse them by name; it can never
carry them.

**Most of what SMIL adds is timing, not motion.** `begin="rect.click+2s"`,
`begin="a.end"`, `restart`, `min` and `max`, `additive="sum"`,
`accumulate="sum"`, `fill="freeze"`, `repeatCount="indefinite"`, `<mpath>`,
`calcMode`. That is an interval-timing dependency graph, and under P4 every
unimplemented piece of it is a named diagnostic rather than a silent drop.
It is the same shape as the embedded-wasm binding proposal rejected in
`docs/decisions/binding-expressions-are-not-embedded-wasm.md`: a general
computation model offered where a description was wanted.

## The two producers are complementary rather than overlapping

| capability                    | SMIL                           | Figma             | in the stack today                           |
| ----------------------------- | ------------------------------ | ----------------- | -------------------------------------------- |
| state to state transition     | no                             | yes               | `VariantTransition`                          |
| spring                        | no — `keySplines` beziers only | yes, presets      | `TransitionSpec::Spring`                     |
| stagger                       | manual `begin` offsets         | limited           | the `stagger` field                          |
| endpoints from layout (FLIP)  | no — authored literals         | yes               | binds at commit                              |
| ambient loop, shimmer, pulse  | yes                            | no                | yes — story #772, built 2026-08-09           |
| motion path                   | yes                            | no                | no                                           |
| draw-on (`stroke-dashoffset`) | yes                            | no                | no such prop                                 |
| rotation                      | yes                            | yes               | yes — story #770, built 2026-08-09           |
| discrete visibility switching | yes                            | yes, via variants | `Prop::Visible`, and a step is two keyframes |
| event and sync-base timing    | yes                            | trigger only      | no                                           |
| animating resolved geometry   | yes                            | n/a               | forbidden by P1                              |

Neither column is a superset of the other. So the reference feature set is
the **union of the two producers, expressed in `dashcue`'s own terms**,
which is P5 restated: no producer's limitations define the format. SMIL is
the checklist for the ambient half — the only half with a measurable
official corpus — and Figma's `reactions` payload is the checklist for the
reactive half.

## The census, which is the ambient half's work-list

Animated attributes across the 525 official SVG tests, measured when the
source capture was written:

    transform 147 · fill 128 · fill-opacity 82 · x 65 · stroke-width 56
    stroke 54 · visibility 46 · display 28 · height 23 · width 17
    color 11 · xlink:href 10 · fill-rule 10 · y 8 · stroke-dashoffset 8

    animateTransform type=  translate 106 · rotate 21 · scale 18
                            skewX 1 · skewY 1

`rotate` being the second most common transform type is independent
evidence for the ordering v0.18 already took, and story #770 acted on it.

Read as a work-list: `fill`, `fill-opacity` and `opacity` are props that
already exist and needed only the loop track that story #772 built. `x`,
`width`, `height` and `d` are P1 refusals. `stroke-dashoffset` would be a
genuinely new channel, and nothing schedules it.

The method for re-deriving these counts is held in
`docs/wip/2026-08-09-svg-as-a-second-producer.md` rather than repeated here,
so the numbers stay checkable against the corpus rather than trusted from
this record.
