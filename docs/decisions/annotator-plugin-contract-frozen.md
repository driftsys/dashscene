# Annotator plugin: deferred to v1, its data contract frozen now

    status   accepted
    date     2026-07-12
    scope    the sharedPluginData annotator plugin (importers/figma/plugins/annotator/)
    binds    the Deno importer and any capture written before the plugin
             exists

## Context

The sharedPluginData annotator plugin (`docs/design/dashc.md`'s trim-layer
"machine truth" writer) is placed alongside the Deno importer
(`docs/decisions/figma-importer-deno-plus-dashc-wasm.md`). Building it is not
needed yet, but captures and importer code written before it exists must stay
compatible with what it will write.

## Decision

**Defer the plugin itself to v1.** Freeze its data contract now:

- Namespace: sharedPluginData namespace `"dashscene"`.
- Keys: `role` = `placeholder | sample-content | redline | spec`, and `v` =
  `"1"` (contract version stamp).
- Reserved keys, defined now and written later: `contribution-id` (placeholder
  nodes only) and `material-class` = `lit-opaque | lit-cutout | unlit-overlay`
  (consumed by the Unity painter).

**Deferral trigger** is event-based, not version-based, with two triggering
events: (a) the first externally authored Figma file entering the pipeline —
self-authored fixtures do not need roles, so this is what the role-writing
machinery waits for; or (b) the start of phase-2 token-resolution work, which
needs the id → name/collection/mode export command
(`docs/decisions/token-resolution-phase-split.md`). Event (b) may fire first and
may require only the token-export command, not the role-writing machinery.

**Annotation is a three-channel inventory, cheapest channel first:** (1) native
Figma structure (names, the `_` prefix, hidden flags) where it already encodes
the intent; (2) the repo-side export manifest for per-file declarations; (3)
sharedPluginData last, only for what genuinely must travel on the node inside
Figma.

**Distribution:** the plugin stays in-repo and unpublished. Professional cannot
publish org-private plugins, so publishing would mean public Community
publication; distribution is therefore "import plugin from manifest" from a
checkout — the same mechanism the fixture-author plugin uses.

## Why

Freezing the contract now, ahead of the plugin's own implementation, lets
captures and importer code reference stable keys (`role`, `v`,
`contribution-id`, `material-class`) without waiting on v1. Deferring the plugin
itself avoids building role-writing machinery before any externally authored
file needs it.

## Consequences

- The `docs/decisions/figma-corpus-self-authored-only.md` fixture-author plugin
  is a **different** plugin: it only creates nodes and never writes roles; this
  contract does not apply to it.
- `docs/decisions/token-resolution-phase-split.md`'s phase-2 join table is the
  annotator plugin's first mandatory job, ahead of the rest of the plugin, per
  trigger (b) above.

## As-built (#39)

Trigger (b) fired: story #39 built the plugin
(`importers/figma/plugins/annotator/`). It honors this contract unchanged — the
`dashscene` namespace, the `role` =
`placeholder | sample-content | redline | spec` values, and the `v = "1"` stamp
are exactly what the annotate commands write, and the importer's trim pass
(`importers/figma/src/trim.ts`) reads. Nothing pinned here is amended.

The plugin delivers the token-export command (trigger (b)'s minimum) plus the
role-writing commands. The reserved keys (`contribution-id`, `material-class`)
are still defined-but-unwritten, as this contract states. The vartable shape
token export produces is recorded in
`docs/decisions/token-resolution-phase-split.md`, not here, since it is the
token-resolution decision's concern.

## As-built (#302)

Story #39 placed the plugin directly under `importers/figma/plugin/`, one level
up from the fixture-author dev plugin nested at
`importers/figma/plugin/fixture-author/` — the two plugins sat at different
depths despite being independent siblings. Issue #302 moved them to sibling
directories: `importers/figma/plugins/annotator/` and
`importers/figma/plugins/fixture-author/`. Config, comments, and docs were
updated to match; no compiled or runtime code depended on the old paths.
