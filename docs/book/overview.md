# Overview

dash turns UI designed in Figma — or authored programmatically in code —
into pixels on screen, through one intermediate representation, one shared
layout+text runtime, and interchangeable paint backends.

Primary targets are embedded/automotive-class devices rendered by a game
engine (Unity) or a lean native renderer, with a Skia backend serving as
the reference implementation, test oracle, and 2D path.

## Status

This project is in early development. No end-user functionality exists
yet; work is proceeding through the `v0.1` "walking skeleton" milestone
(schema, minimal DSL, fixed rects, solid fills, `.dsb` round-trip, painter
trait, golden harness), with the `v0.3` paint-vocabulary schema (gradients,
images, strokes, corners, clip) landed early.

## Where things live

- The full architecture — goals, requirements, stack, document format,
  producers, painters, target-hardware rules, and the release plan — is in
  `specs/DESIGN_1.md`.
- Everything decided since, in the order it was decided, is in
  `specs/SCOPE_DECISIONS.md`, which supersedes `DESIGN_1.md` wherever the
  two disagree.
- Contributor-facing entry point (crate map, commands, current start
  order): `AGENTS.md` at the repository root.

See the [usage guide](./usage-guide.md) for building the project from
source.
