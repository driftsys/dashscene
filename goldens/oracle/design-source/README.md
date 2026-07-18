# goldens/oracle/design-source — the committed Figma REST image exports

This directory holds the design-source images the render oracle diffs the
reference painter against: one Figma REST `GET /images` export per captured
corpus frame, named `<frame>.png` to match the `frame` in `../manifest.json`.

Committed today: the two layout frames — `v08-wrap.png` (420x184) and
`v08-grid-spans.png` (720x480). The oracle imports each frame's committed Figma
fixture, renders it with the reference painter, and diffs that render against
the matching export here (`../README.md` describes the model). The remaining
manifest frames have no export yet: their `designSource` is `null`, their status
is `pending-265`, and they are a disclosed follow-on — the shadow frames need a
renderable fixture and the text/baseline frames need the glyph-run render path.

Each export is downloaded, never drawn: set a frame's `figmaFileKey` and
`figmaNodeId` in `../manifest.json` and run `deno task oracle-capture`
(`importers/figma`). Do not add a fabricated, hand-drawn, or render-derived
stand-in here. A design source that is not the real Figma export defeats the
whole point of the oracle (guardrail G-11: fidelity must be measured against a
real source, and a renderer that is its own oracle cannot see its own drift).
