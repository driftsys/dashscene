# Figma access: Professional plan, PAT rotation, granular scopes, rate-limit handling

    status   accepted
    date     2026-07-12
    scope    the Figma REST access the fixture and capture work needs

## Context

The fixture and capture work (`docs/decisions/figma-corpus-self-authored-only.md`)
needs paid Figma access. The REST file endpoints are plan- and seat-gated,
and Starter's Tier-1 allowance is roughly 6 requests per month —
unusable — so a paid plan is a hard requirement, not a convenience.

## Decision

**Plan: Figma Professional with a Full seat.**

**PAT lifecycle.** Figma personal access tokens expire after 90 days — a
hard cap, with no non-expiring option. Rotation policy: rotate at about
75 days. The token is stored as a GitHub Actions secret, never in the
repo. The nightly live smoke test (`docs/design/dashc.md`) doubles as the
token canary: when the PAT expires or loses a scope, the smoke test is
what fails first. Auth failures surface as a named 401/403 diagnostic
that states the likely causes (expired PAT, missing scope), never as a
bare HTTP error.

**Scopes** (granular): `file_content:read` — this also covers
sharedPluginData, returned via `?plugin_data=shared`;
`file_metadata:read`; `library_content:read`. `file_variables:read` is
Enterprise-only and therefore unavailable on Professional — see
`docs/decisions/token-resolution-phase-split.md` for the consequence.

**Rate limits.** `GET /file` is Tier 1 = 10 requests/minute on
Professional. Importer and capture behavior derived from this:

1. Metadata-version-check first — hit the cheap metadata endpoint,
   compare the file version against the previous capture, and skip the
   full `GET /file` when unchanged.
2. A serialized limiter — at most one Figma request in flight at a time.
3. Honor `Retry-After` on 429 responses.

## Why

Starter's request allowance cannot support iterative fixture authoring
or a nightly smoke test; Professional's Tier 1 (10 requests/minute) can,
with the metadata-precheck and serialized-limiter discipline above
keeping usage well inside it. `file_variables:read` being
Enterprise-only is a plan fact, not a choice, and it is the reason token
resolution needs a Plugin-API-sourced join table rather than the REST
Variables endpoint.
