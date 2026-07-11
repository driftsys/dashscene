# Security Policy

## Reporting a vulnerability

Please do **not** open a public GitHub issue for security vulnerabilities.

Instead, report it privately via GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability)
on this repository, or email the maintainers directly if that channel is
unavailable to you. Include:

- A description of the vulnerability and its potential impact
- Steps to reproduce, or a minimal repro case
- Any relevant `.dsb` fixtures, corpus files, or logs (redacted of anything
  sensitive)

We will acknowledge receipt within 5 business days and aim to provide a
remediation timeline within 14 days of confirming the report.

## Scope notes specific to dashscene

- The `.dsb` format loader (boundary A, see `specs/DESIGN_1.md` §5) is a
  trust boundary: it parses untrusted input (documents produced outside this
  repo's own compiler, or received over the wire per the v2 remote-streaming
  plan). Parsing bugs here — out-of-bounds reads against mmap'd sections,
  hash-check bypass, section-size confusion — are treated as security issues
  even before v2 ships.
- The Figma importer (`importers/figma/`) handles personal access tokens.
  Token handling or scope-escalation issues there are in scope.
- `dashc.wasm`'s boundary with the Deno importer (JSON in, `.dsb`/diagnostics
  out) is in scope for the same reason as boundary A above.

## Supported versions

This project has not yet reached a tagged v0.1 release. Until then, only
`main` is supported; report against the latest commit.
