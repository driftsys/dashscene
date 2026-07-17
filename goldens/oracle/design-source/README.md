# goldens/oracle/design-source — the committed Figma REST image exports

This directory holds the design-source images the render oracle diffs the
reference painter against: one Figma REST `GET /images` export per corpus frame,
named `<frame>.png` to match the `frame` in `../manifest.json`.

**It is empty of images today.** The exports are authored manually and tracked
by the parked manual-Figma-authoring issue **#265**. Until they land, every
manifest frame's `designSource` is `null` (status `pending-265`) and the render
oracle measures nothing — see `../README.md` for the gate and the drop-in
procedure.

Do not add a fabricated, hand-drawn, or render-derived stand-in here. A design
source that is not the real Figma export defeats the whole point of the oracle
(guardrail G-11: fidelity must be measured against a real source, and a renderer
that is its own oracle cannot see its own drift).
