# Apache-2.0, not MIT and not a dual licence, for the patent grant

    status   accepted
    date     2026-08-09
    scope    the whole workspace; docs/decisions/house-style.md (which
             recorded MIT), LICENSE, NOTICE, Cargo.toml's
             `[workspace.package]`

## Context

The workspace was MIT from its first commit, inherited from the driftsys
house style. MIT grants copyright permissions and says nothing about
patents. Apache-2.0 §3 grants an explicit patent licence from every
contributor and terminates that licence for anyone who brings a patent
action alleging the work infringes. Publishing the repository makes the
difference material: MIT would leave contributors free to contribute code
and later assert a patent over it.

Relicensing needs no third party's agreement and no history rewrite — a
licence is granted at distribution, not stamped on each commit.

**On authorship.** `git shortlog -sne` shows two author identities —
`Sebastien Tasson <sebastien.tasson@gmail.com>` and
`stasson <stasson@users.noreply.github.com>` — which are two git
configurations belonging to the repository owner, not two copyright holders.
`stasson@users.noreply.github.com` is a GitHub-issued no-reply address for
that same account. The `Co-Authored-By:` trailers name assistant models used
during development; those hold no copyright in the output.

This is recorded because an auditor reading the public history sees two names
on the commits and should not have to infer that they are one person.

## Options

1. Stay MIT.
2. `MIT OR Apache-2.0`, the common Rust ecosystem convention.
3. Apache-2.0 alone.

## Choice

Option 3, Apache-2.0 alone.

## Why

- **A dual licence gives away the clause that motivated the change.** Under
  `MIT OR Apache-2.0` the licensee chooses, so anyone unwilling to be bound
  by §3's defensive termination takes the MIT option instead. The people
  most likely to exercise that choice are the ones the clause exists to
  deter.
- **The cost of dropping MIT is narrow.** Apache-2.0 cannot be incorporated
  into GPLv2-only code, which matters only for a consumer wanting to link
  this runtime into a GPLv2-only program — an unlikely shape for a userspace
  UI runtime. Every other permissive and copyleft target accepts it.
- **No dependency blocks it.** A sweep of the full dependency graph on
  2026-08-09 found no GPL, AGPL, SSPL or MPL crate. Every licence present is
  permissive or dual-licensed with a permissive option; the only conditional
  is `r-efi` at `MIT OR Apache-2.0 OR LGPL-2.1-or-later`, which offers
  Apache-2.0.
- **The vendored tree stops being an exception.** `dashpack-astcenc-sys` was
  `MIT AND Apache-2.0` because Arm's astcenc is Apache-2.0 under an MIT
  wrapper. Both halves are now one licence, so the compound expression is
  gone and `NOTICE` carries the attribution that Apache-2.0 §4(d) gives
  force to.

## Consequences

- Every crate inherits `license = "Apache-2.0"` from `[workspace.package]`.
  `dashpack-astcenc-sys` no longer overrides it.
- **The 0.1.0 name reservations on crates.io stay MIT permanently.** All 21
  reserved names were published at 0.1.0 under MIT — twelve on 2026-03-18
  and nine as the crates arrived, the most recent on 2026-08-09. A published
  version's licence cannot be changed, and those MIT grants are irrevocable.
  Each 0.1.0 is an empty stub — no implementation was distributed — so the
  grant covers no working code. The first Apache-2.0 release is **0.2.0**,
  which is already the planned first real version (issue #795).
- `docs/decisions/house-style.md` recorded MIT as the house default. This
  record supersedes it for this repository; the house style itself is
  driftsys-wide and is not changed here.
- Contributions arrive under Apache-2.0. A contributor licence agreement is
  not required for that: §5 already places inbound contributions under the
  same terms.
- **A DCO sign-off is intended and is not yet implemented.** Nothing in
  `CONTRIBUTING.md` asks for `Signed-off-by`, and `git std lint` checks
  conventional-commit shape rather than trailers, so today an outside
  contribution would arrive without one and nothing would notice. This is a
  gap to close before the repository accepts outside contributions, not a
  property it currently has.
