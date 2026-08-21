# com.driftsys.dashscene

The C# side of dashscene's painter boundary.

**This package declares types. It does not draw.** There is no painter here, no
host, and no native library — the `BatchRendererGroup` painter is story #1122
and the host that loads a native library is #1121. What is here is
`Runtime/BoundaryB.cs`: the value types that cross boundary B, declared so that
a painter written against them agrees with the Rust side byte for byte.

## What checks the declarations

`unity/abi-check` compiles **this package's own `BoundaryB.cs`**, builds
`crates/dashpaint-abi` as a dynamic library, and compares every type against
what the Rust build reports — member by member, matched by name. It runs on
every pull request and needs no Unity editor. `just unity-abi` runs it locally.

It catches anything wrong with **this file**: a member added, removed, renamed,
moved or widened, including two same-width members exchanged and an enum that
lost its `: byte`.

Two things it does not catch, both measured rather than reasoned about:

- A member whose C# type has the right size and the wrong meaning — `uint`
  declared as `float`.
- A member added to the **Rust** type that fits inside padding already there, so
  no size and no offset moves. `abi_surface!`'s member lists are hand-written,
  which is what leaves that open; issue #1252 carries it.

`unity/abi-check/Program.cs` states both.

## What a consumer must satisfy

Story #1125's spike settled the questions a consumer will ask — the distribution
form, the minimum Unity version, the render pipeline, the scripting backend, the
API compatibility level, and where the native library sits per platform. They
are requirements now, in
[`../../docs/specification/07-embedding-and-distribution.md`](../../docs/specification/07-embedding-and-distribution.md),
with the reasoning in the three records
[`../../docs/decisions/README.md`](../../docs/decisions/README.md) lists under
story #1125.

**Several are unmet by this package as it stands**, and story #1121 is where
they close. Read the requirements file for the current set rather than this
list, which is a pointer and not a census:

- **R-E2 — this package ships no `.meta` files, and so delivers nothing.** A
  Git-URL package is immutable, and Unity does not generate a missing `.meta`
  there; it ignores the asset. The `.asmdef` is never imported and
  `BoundaryB.cs` never compiles. See
  [`../../docs/decisions/unity-package-distribution-is-a-git-url-and-meta-files-are-committed.md`](../../docs/decisions/unity-package-distribution-is-a-git-url-and-meta-files-are-committed.md).
- **R-E1 — `package.json` carries no `unity` field.** It should declare
  `"6000.3"`. Omitting it claims compatibility with every editor ever released.
- **R-E16 and R-E17 — nothing here calls the native library at all**, so the
  mandatory `ds_abi_version` handshake and the per-array `stride` check are both
  absent. `BoundaryB.cs` carries no `DllImport`. Shipping the `.meta` files and
  the `unity` field does not close story #1121 without these.

## Licence

Apache-2.0. `LICENSE` and `NOTICE` travel with the package because §4 of that
licence requires both to.

**`LICENSE`, not `LICENSE.md`**, which is what UPM conventionally expects. The
repository formats every tracked Markdown file with `prim`, and on the `.md`
name it rewrites 325 lines of the licence text. The extensionless name is what
the Rust crates already carry for the same reason.
