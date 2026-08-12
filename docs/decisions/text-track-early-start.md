# Decision: start the v0.5 text/atlas track before v0.1 completes

    status   accepted
    date     2026-07-12
    scope    plan sequencing (issues #25, #27; later #26, #28)
    session  parallel session C (text/atlas track)

## Context

The v0 plan (`docs/roadmap.md`) nominally orders slices v0.1 → v0.9, and the
v0.5 story bodies carry a "depends on: epic #1" line. Three sessions execute the
plan in parallel; session C owns the text/atlas track.

## Options

1. Wait for epic #1 (v0.1 walking skeleton) to close before starting any v0.5
   story.
2. Pull the text-track stories forward now, in the order #25 → #27 → #26 → #28,
   holding #26 until #2 (dashscene-core arena) is merged.

## Choice

Option 2 — start the text track immediately.

## Why

- Text (R1: Arabic shaping, ligatures, bidi, identical quality on every backend)
  is the project's highest-risk requirement. `docs/roadmap.md` itself schedules
  the Arabic-atlas spike "at the start of v0.5 at the latest" and the epic list
  marks the v0.5 atlas work as a cross-epic early start that depends only on the
  v0.1 crate scaffold.
- The crate scaffold (13 crates, workspace, CI wiring) is already on `main`. #25
  is investigation only, and #27 (atlas pipeline) is build-time tooling: font
  file in, atlas + metrics blob out. Neither consumes the arena, the painter
  trait, or the golden harness that epic #1 builds.
- The one true dependency is honored explicitly: #26 (text schema + core tables)
  modifies `dashscene-core`, so it waits until #2 is merged to `main`.
- If session C is ever blocked on other sessions, #21 (dashcue vocabulary) is
  the designated filler story.
