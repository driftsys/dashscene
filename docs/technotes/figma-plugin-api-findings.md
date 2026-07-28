# Figma plugin-API findings from authoring the tier-1 fixtures

    status   informative
    date     2026-07-12
    source   docs/archive/2026-07-14-scope-decisions.md §8;
             importers/figma/plugins/fixture-author/
    informs  any future fixture authored programmatically through the Figma
             Plugin API

This note records three Figma Plugin API shapes found while building the
`fixture-author` development plugin
(`importers/figma/plugins/fixture-author/`), which programmatically
authors the tier-1 corpus fixtures
(`corpus/figma-fixtures/README.md`). Each cost real debugging time
because the field name or the sizing-mode default is not what the
adjacent Figma REST/plugin naming would suggest.

## A GRID frame reads its gaps from `gridColumnGap`/`gridRowGap`, not `itemSpacing`

`itemSpacing` is the auto-layout row/column gap property. A `GRID`-mode
frame ignores it; its gaps are separate properties,
`gridColumnGap`/`gridRowGap`.

## A WRAP frame needs `primaryAxisSizingMode = "FIXED"`

Setting `layoutWrap = "WRAP"` alone is not enough. Without also setting
`primaryAxisSizingMode = "FIXED"` after `layoutMode`, the frame hugs its
children into a single row and nothing wraps.

## `GridTrackSize` exposes no track-level min/max

A grid track's `GridTrackSize` carries only a `type`
(`FIXED`/`FLEX`/`HUG`) and a `value` — there is no track-level min/max
field. A fixture whose intent is "min/max tracks" has to express that
constraint as a **child** constraint instead (`minWidth`/`maxWidth` on a
grid child), and hug track sizing is covered by a `HUG`-typed track
rather than a track-level bound.
