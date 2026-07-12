# dashscene fixture author

Development-only Figma plugin that generates the tier-1 corpus fixtures
(SCOPE_DECISIONS §8) programmatically, so fixtures are **regenerable**
rather than hand-built. Never published; the repo is the distribution
channel (§12).

## One-time setup

1. Open the **Figma desktop app** (dev plugins don't load in the browser).
2. Menu → Plugins → Development → **Import plugin from manifest…**
3. Pick `importers/figma/plugin/fixture-author/manifest.json` from your
   checkout.

## Authoring the fixtures

One Figma file per fixture (§8: failures must bisect to one construct).
For each name below: create a blank design file with that exact name,
then run Plugins → Development → dashscene fixture author → _(name)_.

    grid-basic                   3x3 GRID, fixed+flex+hug tracks, spans,
                                 hug/fill/fixed/minmax children
    variables-bound              fixture-tokens collection (light/dark),
                                 color + number bindings, one subtree
                                 pinned to dark mode
    effects-2025                 REJECT-list diagnostics fixture — see
                                 manual steps below
    lowering-wrap                wrapping row, fixed-width chips
    lowering-hug-in-fill         HUG child inside FILL container
    lowering-negative-gap        itemSpacing -16 overlap row
    lowering-baseline            mixed-size baseline row + Arabic RTL run
    lowering-variant-topology    variant set with differing child counts
                                 + one instance

Re-running a command deletes and rebuilds its frame — safe to iterate.

## Manual steps (the plugin will remind you)

- **effects-2025**: noise, texture, and progressive blur are written by
  the plugin with the shapes the Plugin API documents. These 2025 effect
  types are still _beta_, so if a write is rejected the plugin lists that
  cell in a `_manual-checklist` text node; apply the missing effect via
  the UI effects panel. **Variable-width stroke has no plugin API at
  all**: draw a short line and give it a variable-width profile with the
  Draw tools, always manually. Re-running is safe to iterate: effects
  applied through the panel are re-applied to any cell whose fresh write
  fails, and any node you drew by hand (for example the variable-width
  line) is moved into the rebuilt frame — a construct stays on the
  checklist only while it is actually missing.
- **lowering-baseline**: if `Noto Sans Arabic` isn't available the
  Arabic run is skipped — add any Arabic text node manually (keep the
  Arabic-Indic numerals, e.g. `السرعة ١٢٠ كم/س`).

## After authoring

Capture each file's `GET /file` JSON (with `?plugin_data=shared`, §12)
into `corpus/figma-fixtures/` with `deno task capture`, run from
`importers/figma/`. It needs `FIGMA_TOKEN` set to a personal access
token with the `file_content:read`, `file_metadata:read`, and
`library_content:read` scopes. PAT setup and rate-limit rules:
SCOPE_DECISIONS §11.
