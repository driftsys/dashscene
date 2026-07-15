# Slint: reference for ideas only, never adopted or borrowed as code

    status   accepted
    date     2026-07-13
    source   docs/technotes/producers-and-ir.md §5
    scope    the whole rendering architecture;
             docs/decisions/house-style.md (MIT)

## Context

Slint is the closest thing in the Rust world to "the stack you'd reach
for instead of building this" — `docs/design/architecture.md` already
credits the Taffy/Servo/Bevy/Slint/Zed lineage — but it solves a different problem
and its licensing forecloses adoption.

## Choice

Do not adopt Slint and do not borrow code from it, including its
Figma-to-Slint plugin. Treat it as reference for ideas only (its
software-renderer design, its MCU/GLES work), clean-room, never source.

## Why

- **Different problem.** Slint is a declarative GUI toolkit that renders
  itself; dashscene is a design-source-to-pixels pipeline that renders the
  same scene across foreign engines it does not own. Three hard
  requirements are exactly what Slint's architecture cannot give: Unity as
  a lit, world-space product renderer (G2, no path into a game engine's
  SRP); identical Arabic text on every backend (R1, Slint's text is its
  own renderer-bound path with historically limited complex-script
  support); design-as-reproducible-source (P5/R7, Slint's Figma
  integration is a one-shot codegen after which the design file stops
  being the source of truth).
- **Licensing is the decisive, separate blocker.** Slint is tri-licensed:
  royalty-free only for proprietary desktop/mobile/web, GPLv3 for open
  source, and a paid commercial licence for proprietary embedded. The
  target here is embedded/automotive, so the royalty-free tier does not
  apply; GPLv3 is a non-starter for a proprietary automotive product, and
  a commercial contract is recurring cost plus single-vendor dependency
  on the critical path. Because this repo is MIT
  (`docs/decisions/house-style.md`), GPLv3 code cannot be lifted into it, so Slint is not a code-borrow
  source even for the plugin.

## Consequences

- The permissive pure-Rust stack (Taffy, rustybuzz, ttf-parser,
  unicode-bidi, msdf-atlas-gen, skia-safe — all MIT/Apache/BSD-family) is
  what keeps dashscene MIT and promotable into the public `dashscene`
  facade; a GPLv3 dependency anywhere would poison that.
- The "if Unity softens, fall back to Slint" escape hatch is not free —
  it is GPLv3 (incompatible) or commercial (cost + lock-in).
