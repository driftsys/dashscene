# Apache-2.0, not MIT and not a dual licence, for the patent grant

    status   accepted
    date     2026-08-09
    scope    the whole workspace; docs/decisions/house-style.md (which
             recorded MIT), LICENSE, NOTICE, Cargo.toml's
             `[workspace.package]`; **and the published crates.io
             placeholders, added 2026-08-18** — see "What this record did
             not reach"
    amended  2026-08-18: the reserved names on crates.io were all MIT,
             including three reserved after this decision. Every one now
             carries Apache-2.0

## Context

The workspace was MIT from its first commit, inherited from the driftsys house
style. MIT grants copyright permissions and says nothing about patents.
Apache-2.0 §3 grants an explicit patent licence from every contributor and
terminates that licence for anyone who brings a patent action alleging the work
infringes. Publishing the repository makes the difference material: MIT would
leave contributors free to contribute code and later assert a patent over it.

Relicensing needs no third party's agreement and no history rewrite — a licence
is granted at distribution, not stamped on each commit.

**On authorship.** `git shortlog -sne` shows two author identities —
`Sebastien Tasson <sebastien.tasson@gmail.com>` and
`stasson <stasson@users.noreply.github.com>` — which are two git configurations
belonging to the repository owner, not two copyright holders.
`stasson@users.noreply.github.com` is a GitHub-issued no-reply address for that
same account. The `Co-Authored-By:` trailers name assistant models used during
development; those hold no copyright in the output.

This is recorded because an auditor reading the public history sees two names on
the commits and should not have to infer that they are one person.

## Options

1. Stay MIT.
2. `MIT OR Apache-2.0`, the common Rust ecosystem convention.
3. Apache-2.0 alone.

## Choice

Option 3, Apache-2.0 alone.

## Why

- **A dual licence gives away the clause that motivated the change.** Under
  `MIT OR Apache-2.0` the licensee chooses, so anyone unwilling to be bound by
  §3's defensive termination takes the MIT option instead. The people most
  likely to exercise that choice are the ones the clause exists to deter.
- **The cost of dropping MIT is narrow.** Apache-2.0 cannot be incorporated into
  GPLv2-only code, which matters only for a consumer wanting to link this
  runtime into a GPLv2-only program — an unlikely shape for a userspace UI
  runtime. Every other permissive and copyleft target accepts it.
- **No dependency blocks it.** A sweep of the full dependency graph on
  2026-08-09 found no GPL, AGPL, SSPL or MPL crate. Every licence present is
  permissive or dual-licensed with a permissive option; the only conditional is
  `r-efi` at `MIT OR Apache-2.0 OR LGPL-2.1-or-later`, which offers Apache-2.0.
- **The vendored tree stops being an exception.** `dashpack-astcenc-sys` was
  `MIT AND Apache-2.0` because Arm's astcenc is Apache-2.0 under an MIT wrapper.
  Both halves are now one licence, so the compound expression is gone and
  `NOTICE` carries the attribution that Apache-2.0 §4(d) gives force to.

## What this record did not reach, and now does (2026-08-18)

**The scope line said "the whole workspace" and the registry is not the
workspace.** Every reserved name on crates.io was published `0.1.0` under MIT
and stayed there — 21 of them. **Two were reserved on the day this record was
accepted**: `dashscene-ffi` and `dashscene-android`, both 2026-08-09. Everything
else predates it. So the ruling was not ignored so much as never pointed at the
registry: it named no artifact outside the workspace, and the reservation
procedure in [`crate-name-map.md`](crate-name-map.md) mentioned no licence at
all.

**Dates in these records are local, and crates.io reports UTC.** The three
2026-08-08 reservations were made at 23:27 UTC on 2026-08-07 — the same moment,
one day apart in the two conventions. A reader comparing a record against the
registry finds that offset on every name reserved late in the evening, and it is
not drift.

**A published version cannot be edited**, so the correction is additive: each of
the 21 gained a `0.1.1` carrying `license = "Apache-2.0"`, with `LICENSE` and
`NOTICE` inside the package as §4 requires, and each MIT `0.1.0` was yanked. The
metadata is otherwise preserved verbatim — only the licence and the version
moved. `dashpaint-abi`, reserved the same day, was published Apache-2.0 from the
start, and so has no MIT version to yank and no 0.1.1.

**The first real version is unaffected**, and that is worth saying because it
was nearly recorded wrongly.
[`publishable-and-the-first-version.md`](publishable-and-the-first-version.md)
rules it **0.2.0**, on the reasoning that 0.2.0 clears the whole 0.1.x band. A
0.1.1 placeholder sits inside that band, so it changes nothing — and it makes
that record's own argument stronger rather than weaker: "0.1.1 would clear the
floor and read as a patch on a 0.1.0 release that never existed" was
hypothetical when written, and 0.1.1 now exists as a placeholder on 21 names.

**What does follow is a rule for the next reservation**: it is published under
this record's licence. The yanked MIT versions stay downloadable for anything
that already resolved them, which is nothing — every name held one placeholder
and no dependants.

## Consequences

- Every crate inherits `license = "Apache-2.0"` from `[workspace.package]`.
  `dashpack-astcenc-sys` no longer overrides it.
- **The 0.1.0 name reservations on crates.io stay MIT permanently — accepted
  here, and superseded by the owner's ruling of 2026-08-18.** The reasoning
  holds and is why this was defensible for nine days: a published version's
  licence cannot be changed, those MIT grants are irrevocable, and each 0.1.0 is
  an empty stub, so the grant covers no working code. What the ruling changed is
  that "harmless" is not the same as "what the project says it is licensed
  under", and a reader querying crates.io saw MIT on every name. Each of the 21
  now carries a 0.1.1 under Apache-2.0 with the MIT 0.1.0 yanked; the
  irrevocable grant on the yanked versions is untouched, because nothing can
  touch it. See "What this record did not reach" above. The first real release
  is **0.2.0** either way (issue #795).
- `docs/decisions/house-style.md` recorded MIT as the house default. This record
  supersedes it for this repository; the house style itself is driftsys-wide and
  is not changed here.
- Contributions arrive under Apache-2.0. A contributor licence agreement is not
  required for that: §5 already places inbound contributions under the same
  terms.
- **A DCO sign-off is intended and is not yet implemented.** Nothing in
  `CONTRIBUTING.md` asks for `Signed-off-by`, and `git std lint` checks
  conventional-commit shape rather than trailers, so today an outside
  contribution would arrive without one and nothing would notice. This is a gap
  to close before the repository accepts outside contributions, not a property
  it currently has.
