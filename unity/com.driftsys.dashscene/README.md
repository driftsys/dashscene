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

## What this package does not settle

Issue #1125 owns the questions a consumer will ask, and none of them is answered
here: the distribution form, the minimum Unity version, the render pipeline, the
scripting backend, the API compatibility level, and where the native library
sits per platform. `package.json` deliberately carries no `unity` field for that
reason.

## Licence

Apache-2.0. `LICENSE` and `NOTICE` travel with the package because §4 of that
licence requires both to.

**`LICENSE`, not `LICENSE.md`**, which is what UPM conventionally expects. The
repository formats every tracked Markdown file with `prim`, and on the `.md`
name it rewrites 325 lines of the licence text. The extensionless name is what
the Rust crates already carry for the same reason.
