# The package and its native library are one versioned artifact, and `ds_abi_version` is the refusal

    status   accepted (2026-08-21, story #1125's spike). Settles question 4
             of issue #1125 — version negotiation — and restates it, because
             the question as filed assumed two repositories.
    date     2026-08-21
    scope    every version number a Unity consumer can see, and what happens
             when two of them disagree
    related  docs/design/c-abi.md (the DS_ABI_VERSION rule, and its gap)
             docs/decisions/publishable-and-the-first-version.md (0.2.0)
             docs/specification/07-embedding-and-distribution.md (R-E3 and
             R-E18, which D2 is the reasoning for; R-E16 and R-E17, which D3
             and D4 are)

## Context, and the premise that moved

Issue #1125 asks this as "version negotiation across two repositories". It was
filed 2026-08-16; on 2026-08-17 the owner's ruling sited the C# package in this
repository. **There are no longer two repositories**, so the coordination the
question anticipated does not exist. What does exist is several independent
version lines inside one repository, and a customer holding two artifacts that
reached their machine by different routes.

## The lines

| line                    | declared at                                                                                | what moves it                                                                                                                                     | who reads it               |
| ----------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| `DS_ABI_VERSION`        | `crates/dashscene-ffi/include/dashscene.h`, and again in `crates/dashscene-ffi/src/lib.rs` | a changed signature, a renumbered `DsStatus`, or a condition re-routed onto a different discriminant (D5); **not** a new symbol or a tail variant | a host, once, at start-up  |
| package version         | `unity/com.driftsys.dashscene/package.json`                                                | `git std bump`, via `.git-std.toml`                                                                                                               | UPM                        |
| workspace version       | the root `Cargo.toml`                                                                      | `git std bump`                                                                                                                                    | crates.io                  |
| `.dsb` `format_version` | `crates/dashbuf/src/container.rs`                                                          | a deliberate bump; a reader that does not implement it refuses the file whole                                                                     | the loader                 |
| `DsSlice::stride`       | reported per array, per call                                                               | any row-type layout change                                                                                                                        | a host, per acquired frame |

`DS_ABI_VERSION` is **2**. It has moved once, at story #1226, when ten entry
points changed signature for the generational handle. `DsStatus` has grown from
nine variants to twenty-one without moving it, because every addition went on
the tail.

## Decision

**D1 — the package version tracks the Cargo workspace, and is not its own line
and not the ABI's.** This ratifies what is already mechanised: `.git-std.toml`
carries `unity/com.driftsys.dashscene/package.json` as its one non-`Cargo.toml`
version file, and the package's CHANGELOG already states it. The first published
version is **0.2.0**, with the workspace.

The ABI version cannot serve: it is a single integer with no minor or patch, and
UPM requires `major.minor.patch`. It would also read as `2.0.0` today and
collide with the workspace's own major when it arrives.

**D2 — the package and the native library it carries are one artifact, selected
by one git tag.** They are built from one commit and shipped together, and no
other combination is supported. This is what makes the negotiation tractable
rather than a matrix: under a Git URL it is **the tag, not `package.json`'s
version, that a customer selects** — Unity's manual says a Git dependency "uses
a Git URL reference instead of a version" and gives no guarantee that the
version in its manifest respects semver — so if the binary rides the tag, mixing
takes deliberate effort instead of being the default.

**D3 — the handshake is mandatory, lives in the package, and refuses.**

The host calls `ds_abi_version` once before any other `ds_runtime_*` call,
compares it against the constant its C# was built with, and refuses with both
numbers named. Story #1121 does this: `DashsceneRuntime.EnsureAbiCompatible`
runs before `ds_runtime_new` and throws `DashsceneAbiMismatchException`, which
carries both numbers as fields. Until then `BoundaryB.cs` contained no
`DllImport` at all, and the one Java host that reads the value **logs it and
does not compare**. Advice has been in the header since story #840 and has
produced no check in any host; a package that ships a host is where it stops
being advice.

A _missing_ library already fails adequately — .NET raises
`DllNotFoundException`, which story #1230 measured. A _version-mismatched_ one
is silent, which is the case this decision closes.

**D4 — `stride` is checked per array, per frame, and is not redundant with
`unity/abi-check`.**

`abi-check` compares the package's C# against the Rust source at one commit. The
customer at run time holds a C# assembly and a native library, and `stride` is
the only thing on their machine that can observe those two having come from
different commits. It is redundant only if D2 is enforced perfectly, and D2 is a
policy rather than a mechanism.

**D5 — a re-routed condition moves `DS_ABI_VERSION`, which the rule did not
previously say.**

`docs/design/c-abi.md` records the gap and names the case: `SurfaceLost` did not
only appear at the tail, it **re-routed an existing condition** — a lost
swapchain that used to arrive as `DsStatus::Surface`. A host built against an
earlier header meets a value it does not recognise and stops, losing a recovery
it had. The version did not move, so the handshake passed and gave the wrong
answer.

This record closes the rule: **adding a variant is free; moving an existing
condition onto one is a version bump**, exactly as a changed signature is. The
contrast case stays free — `FrameLeased` re-routes nothing, because nothing
could reach it before leases existed.

## Consequences

- **`c-abi.md`'s versioning section is amended** to carry D5, since it is that
  section's rule that was incomplete. This record is the reasoning; the header
  and that design doc are where a reader meets it.
- **The technote's `ds_abi_version=1` reading is stale.** Story #1230 measured
  the mechanism before story #1226 moved the number to 2. The mechanism stands;
  the value does not, and the technote says so now.
- R-E16 and R-E17 were unmet on `main` and were satisfied by story #1121.

## Alternatives considered

**Let the host application do the check.** This is the current state, and the
reason no host in this repository performs one. A declarations-only package can
defer it; a package that ships a host cannot.

**Log the mismatch and continue**, which is what the Java harness does. Rejected
on the header's own reasoning: the alternative to refusing is discovering the
mismatch as a corrupted argument.

**Version the library separately from the package**, so a customer can mix. It
is the shape the issue's "two repositories" framing implied. Rejected: it
creates a compatibility matrix that nothing tests, to serve a case — a customer
deliberately pairing mismatched halves — that D2 exists to prevent.
