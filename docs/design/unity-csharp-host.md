# The C# host: P/Invoke over the C ABI, lifetime and the tick

    status  as-built at story #1121 (2026-08-21)
    source  story #1121, epic #1106. The requirements are
            [../specification/07-embedding-and-distribution.md](../specification/07-embedding-and-distribution.md)
            R-E1, R-E2, R-E10, R-E13, R-E16 and R-E17, settled by story #1125.
    why     [../decisions/the-native-library-ships-inside-the-unity-package.md](../decisions/the-native-library-ships-inside-the-unity-package.md)
            rules the library name and where it sits;
            [../decisions/host-integration-in-three-layers.md](../decisions/host-integration-in-three-layers.md)
            puts a Unity host at layer 0 in its host-draws form;
            [c-abi.md](c-abi.md) carries the ABI itself.

The managed half a Unity host sits on: version negotiation, runtime lifetime,
document load, the tick, and the committed frame under a lease. **It draws
nothing.** The `BatchRendererGroup` painter that consumes these tables is story
#1122, and the glyph atlases its runs sample do not cross the ABI at all until
story #1123.

    Samples~/FrameLoop --+--> CommitPacer      (when to commit)
                         |
                         +--> DashsceneRuntime --+--> Native (14 DllImports)
                              FrameLease --------+
                                                 |
                                           dashscene_ffi (cdylib)

## Compile territory, and why the frame loop is a sample

`Runtime/` is **engine-independent**, and that is a constraint rather than a
preference. R-E10 requires every C# type under it to compile against
`netstandard.dll` 2.1.0 and names `unity/package-compat` as the check; that
project carries no Unity reference assemblies and CI runs no editor, so a
`MonoBehaviour` under `Runtime/` fails R-E10's own check.

So the parts worth testing — the declarations, the lifetime, the error channel,
the lease and the stride comparison — are in `Runtime/`, where two gates compile
them and a third executes them. What is left needs an editor to mean anything:
`Time.deltaTime`, a component lifecycle, and a place for the painter to hang.
That is `Samples~/FrameLoop/`, hidden from Unity's importer by the `~` suffix —
one of the four shapes R-E2 enumerates — so it needs no `.meta` and is outside
the compile gate.

**This is a known collision with story #1122.** The BRG painter must reference
`UnityEngine`, and it belongs under `Runtime/` rather than in a sample. R-E10's
check cannot compile it as constituted. Issue #1286 carries that.

## Decisions in the binding, each of which is a defect if reversed

**Every `bool` on the surface binds as `byte`.** C's `bool` is one byte and
.NET's default marshalling for `bool` is the four-byte Win32 `BOOL`, so an
out-parameter left to the default writes three bytes past its target. Binding
them as `byte` also keeps every type blittable, so nothing on the surface needs
a `[MarshalAs]` and `DsFrame` crosses with no marshalling at all.

**`ds_runtime_release_frame`'s `drawn` binds as `int`, not `bool`.** The header
spends a paragraph on it: a `bool` crossing _into_ the library has two valid bit
patterns and any other is undefined behaviour where the arguments bind, before
anything in the library can turn it into a status. The `bool`s above are ones
the library writes through an out-pointer, which is the opposite direction and
not the same hazard.

**`DashsceneRuntime` is not a `SafeHandle` and has no finalizer.** A
`SafeHandle` releases on the GC's finalizer thread, and the runtime is
thread-affine — that `ds_runtime_free` answers `DS_WRONG_THREAD` and the runtime
leaks with nothing reported. A type that cannot be collected correctly should
not carry the machinery that claims it can, so `Dispose` is explicit and
documented as owning-thread-only.

**The handle is cleared only when the free succeeded.** Every other status means
the runtime was not freed — `WrongThread` from a foreign thread, `FrameLeased`
from a release that did not complete, `BadHandle` from a double dispose — and
zeroing it anyway would make the retry the thrown message prescribes hit the
`_handle == 0` guard and do nothing, turning a reported failure into an
unrecoverable leak. All of them are reported; an earlier form of this method
reported only `WrongThread` and discarded the rest. The lease release runs in a
`try`/`finally` so the free happens even when releasing throws, which is
reachable on `DsStatus.Panic` — whose own documented remedy is to free the
runtime.

**The stride table is derived from `frame_of`, not from the member names.** Two
of the nineteen arrays do not hold the type their name suggests: `extra_fills`
holds `PaintKind` and `shapes` holds `VectorField`. The other seventeen are what
they look like — `strokes` holds `Stroke`, `shadows` holds `Shadow`, `blurs`
holds `Blur`. The seven `*Range` types are rows of no array here at all: five
are index ranges inside `PaintEntry`, and `StopRange` and `GlyphRange` sit
inside `Gradient` and `GlyphRun`. A table written from the names would have
compared two arrays against the wrong size and reported a mismatch on a correct
build.

**A stride mismatch releases the lease before it throws.** The acquire has
already succeeded at that point, so throwing straight out would leave the lease
held and refuse every later tick for the life of the runtime — turning a
diagnosable version mismatch into a runtime that never advances again.

## What the three gates see, and what none of them does

| gate                   | question                                        | recipe           |
| ---------------------- | ----------------------------------------------- | ---------------- |
| `unity/abi-check`      | do boundary B's C# types match the Rust ones?   | `just unity-abi` |
| `unity/package-compat` | would Unity compile this package at all?        | `just unity-abi` |
| `unity/ffi-check`      | do the P/Invoke declarations match the library? | `just unity-ffi` |

`unity/ffi-check` is the one story #1121 added. Before it, nothing compiled a C#
P/Invoke against `crates/dashscene-ffi/include/dashscene.h` — issue #1266 item
2. It is not, however, the only gate that executes: `abi-check` declares sixty
`[DllImport]`s and round-trips structs by value through `dashpaint-abi`.
`package-compat` is the one that only compiles.

**Seventeen checks**, and the grouping below accounts for all of them: the
declared entry points against the ABI's named set (1), the version handshake in
both directions (3), five statuses from real calls — `NoDocument`, `Open`,
`Map`, `BadHandle`, `NullArgument` (4) — the frame under a lease, including a
mutated row size and `FrameLeased` from three different callers (5), the
lifetime pair create/free and dispose-under-lease (2), and the commit pacer's
cadence and divisor advice (2).

**Two of them perform the mutation their requirement's own _Check_ asks for.**
R-E16 says "build a host against a mismatched value and assert it refuses" and
R-E17 says "mutate a row type's size and assert the host refuses rather than
drawing" — so the gate does exactly that, rather than a developer doing it once
by hand and the record claiming the gate does it. `CompareAbiVersion` exists as
the seam for the first, because `Native.AbiVersion` is a `const` the compiler
inlines and no reflection can move it; the second mutates `FrameLease.RowSizes`
in place, since the field is readonly and the array's contents are not.

**Every entry point is declared, including the four a Unity host never calls.**
`ds_runtime_attach_surface`, `ds_runtime_detach_surface`, `ds_runtime_resize`
and `ds_runtime_draw` belong to a host that hands dashscene a surface. They are
declared because an unbound symbol is an ungated one — but **.NET binds a
`DllImport` lazily, at the first call**, so declaring them gates nothing on its
own. The symbol-resolution check is what makes it real: it looks every
declaration up in the loaded library, so a rename fails now rather than in the
story that first calls one. A lookup proves the name and not the signature; the
behavioural checks prove the signatures of what they exercise.

**None of the three reads a shipped binary.** All build both halves from one
tree, so they observe only a disagreement this repository already contains. A
stale committed library is what `DsSlice::stride` catches at run time, which is
why R-E17 makes that check mandatory in the host rather than advisory.

## The `.meta` files, and how they were made

R-E2 requires a committed `.meta` beside every path Unity imports, because a
Git-URL package lands in `Library/PackageCache` immutable, where Unity ignores
an asset with no `.meta` rather than generating one. The package ships twelve.

**They were generated by an editor**, on `6000.3.22f1`, by importing the package
into a throwaway project as a `file:` dependency — a local package is mutable,
so Unity writes the `.meta` beside each asset. Hand-writing them would have
meant guessing an importer class per extension, and the guid is the load-bearing
part: it is what an asset reference resolves through, and nothing can mint one
later inside an immutable package.

**A script's `.meta` is two lines, and that is canonical rather than a truncated
write.** Unity emits `fileFormatVersion` and `guid` with no `MonoImporter` block
for a script in a package; two independent batchmode passes produced
byte-identical files, and 1119 of the 4805 `*.cs.meta` files in the editor's own
`BuiltInPackages` carry exactly those two keys and nothing else.

## Known gaps, named

- **No painter and no text.** Story #1122 and story #1123.
- **No native library and no release.** R-E3, R-E18 and R-E21 stay unmet, and
  they are about shipping rather than about this directory: `just host-lib`
  builds the cdylib and nothing places it into the package. Committing it was
  considered and deferred — it is about 9.6 MB of undeltifiable binary in a
  public repository's permanent history for a package that cannot yet draw.
- **`ds_runtime_load_document_with_text` is declared and not wrapped.** The
  managed surface exposes the two loaders that need no font cascade; the third
  takes `DsFontFace` arrays whose atlases story #1123 owns.
- **Three fixed behaviours are pinned by nothing.** `ReleaseLease` clears its
  managed handle only after the library has released, `Dispose` reports a failed
  free, and `FrameLease` refuses use after release — but provoking the first two
  needs `ds_runtime_release_frame` or `ds_runtime_free` to fail on the owning
  thread with a live handle, which only `DsStatus.Panic` reaches and the gate
  cannot induce. Mutating the release ordering back leaves all seventeen checks
  green, measured rather than assumed.
- **No gate compiles `Samples~/FrameLoop/`.** The `~` that hides it from Unity's
  importer hides it from `package-compat` and `ffi-check` too, and no CI job
  runs an editor. That is why `CommitPacer` sits in `Runtime/` rather than in
  the sample: the pacing arithmetic carries a numeric claim, so it lives where a
  gate can reach it. What is left in the sample is Unity glue.
- **The thread-affinity question is narrowed, not closed.** Story #1125 measured
  `OnPerformCulling` on the main thread under `6000.3.22f1` with URP on macOS
  and Metal, so a host can bracket its job dispatch — but the target is Android,
  where no reading has been taken. Issue #1267 question 2, whether
  `DS_WRONG_THREAD` should distinguish a dead thread from a foreign one, is
  untouched and remains an owner's ruling.
