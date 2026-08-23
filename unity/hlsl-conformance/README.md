# unity/hlsl-conformance

The committed layer-2 probe table, evaluated through the **generated HLSL**.

    SdfConformance.compute        one kernel per probed function, over the
                                  package's own Sdf.hlsl
    DashsceneHlslConformance.cs   the harness: reads the table, packs the
                                  probes, dispatches, compares
    ProbeJson.cs                  a JSON reader, and a check that its float
                                  parse is correctly rounded

Run it with `just unity-conformance`. It needs a Unity editor, so it runs on no
CI runner here — the same prerequisite `just unity-editor` has, for the reason
[`the-native-library-ships-inside-the-unity-package.md`](../../docs/decisions/the-native-library-ships-inside-the-unity-package.md)
D4 records.

## Why it exists

`../../conformance/layer2-probes.json` is the SDF shader library's arithmetic
stated as inputs, expected values and tolerances, so that a painter in any
shading language can check it. Issue #828 produced that file and exactly one
consumer — `crates/dashscene-gpu/tests/layer2_conformance.rs`, which dispatches
the **WGSL**.

`unity/package-gate`'s `the_committed_hlsl_is_what_the_wgsl_compiles_to`
re-derives `Runtime/Shaders/Sdf.hlsl` from the WGSL and compares the file as
**text**. That says the generator ran. It does not say the generated arithmetic
evaluates to the values the table records, and a translation can be textually
stable and numerically wrong on the target: `mad`-contraction reassociating a
difference the source wrote deliberately, an intrinsic that differs on
negatives, half precision on a mobile compiler. Issue #1195 is a measured
instance from the other side — Metal folded `(o + b) - (o + a)` to `b - a` and
erased a cancellation the shader depended on.

This directory closes issue #1312. **It does not on its own close epic #1106**,
whose definition of done is "The BRG painter draws a `.dsb` through the C# host,
and #828's conformance suite is what says it drew the right thing" — two halves,
and this is the second. Nothing here draws a `.dsb`, constructs `BrgPainter` or
runs the C# host; issue #1298 records that nothing in this repository constructs
the painter at all.

## What a pass licenses, and what it does not

**The backend is read back, not assumed.** Unity translates the HLSL for
whatever graphics device the editor obtained. On macOS that is Metal, and the
harness prints `SystemInfo.graphicsDeviceType` in its OK line for that reason. A
pass is a statement about that backend's translation of this arithmetic. **It is
not a pass on the fleet**, which runs GLES 3.2 and Vulkan on Android.

**An editor is not a player.** Issue #1313 is the measured instance: the
package's shaders are stripped out of a player build and `Shader.Find` returns
null, while every gate in this repository passes. This harness resolves its
compute shader by asset path through `AssetDatabase`, which exists only in an
editor — so it says nothing about stripping, and nothing about how a shipped
build resolves an asset. What it measures is arithmetic.

**It is a compute compile, not the fragment compile the painter uses.** That is
layer 2's design rather than a shortcut — a compute dispatch removes the
rasteriser, the antialiasing resolve, the blend stage and the sampler, leaving
float arithmetic, which is what makes the layer meaningful on whatever adapter a
machine offers. `docs/design/dashscene-gpu.md` states it for the WGSL side and
it is equally true here: what is measured is the shader library's arithmetic,
not the three material shaders that call it.

**A run can write into the working tree.** The recipe imports the package as a
`file:` dependency, which makes it a mutable package: Unity writes a `.meta`
beside any asset that has none. Every asset in the package has one today, so a
run adds nothing — but that is a property of the tree rather than of the recipe,
and `just unity-editor` carries the same warning. Check `git status` after a run
that added a file.

**The compute shader is a fixture, not part of the package.** It sits in
`Assets/` of the throwaway project the recipe writes under `target/`, so a
consumer's build never carries a conformance harness. It reaches the arithmetic
through `#include "Packages/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl"` —
the package's own include path, so what is evaluated is the installed file
rather than a copy of it.

## What one run measured

Metal on an Apple M3, Unity `6000.3.22f1`, 2026-08-23. Every function inside its
tolerance; the worst error per function, beside the same figure from the WGSL
consumer on the same machine:

    function              tolerance    HLSL/Unity   WGSL/wgpu
    clamp_radii             1.0e-5        0            0
    rounded_box_sdf         2.0e-2        7.629e-6     7.629e-6
    coverage                1.0e-6        0            0
    median3                 1.0e-7        0            0
    msdf_coverage           1.0e-6        0            0
    gradient_linear_t       1.0e-5        1.192e-7     1.192e-7
    gradient_radial_t       1.0e-5        5.960e-8     5.960e-8
    gradient_angular_t      1.0e-5        1.848e-6     1.192e-7
    gradient_diamond_t      1.0e-5        5.960e-8     5.960e-8
    gradient_ramp           1.0e-6        2.980e-8     5.960e-8
    stroke_coverage         1.0e-5        0            0
    erf_approx              1.0e-3        2.347e-4     2.347e-4
    blurred_rounded_box     3.922e-3      3.501e-3     3.501e-3

`ProbeJson.SelfCheck` passed on that editor's runtime, so its `double.Parse`
lands all six adversarial literals on the correctly-rounded double.

Two readings are worth keeping, and neither was predicted:

- **The worst error is the same figure on eleven of the thirteen.** The two that
  move do so for different reasons, and only one of them is understood.
  `gradient_angular_t`'s generated body is the WGSL operation for operation —
  `frac(atan2(y, x) / 6.2831855 + 1.0)` against `fract(atan2(…) / tau + 1.0)` —
  so what differs is the platform's own `atan2` and `frac`, and it is about 15
  times worse through the HLSL: 1.8e-6 against a tolerance of 1e-5, five times
  inside it. `gradient_ramp` calls no transcendental at all and moves the other
  way, halving from one float ULP at that magnitude to the next; its generated
  body differs in three visible ways — naga writes WGSL's `mix` as `lerp`,
  clamps every array index with `min(…, 7u)`, and wraps the walk in a
  `loop_bound` guard. **Which of the three moves the bit was not measured**, and
  none of this isolates the cause of a one-ULP difference. Identical worst
  errors are also not identical values, and the run shows it:
  `blurred_rounded_box`'s worst is 3.501e-3 in both languages and at **different
  probes** — `p = [-20, -30]` through the HLSL against `p = [-20, 30]` through
  the WGSL. What was measured is the statistic, not a claim about every probe.
- **`blurred_rounded_box` sits at 89 % of its tolerance in both.** That figure
  is the **shader's** own twelve-row quadrature error against the 512-row
  reference, which
  [`shader-library-and-layer-2.md`](../../docs/decisions/shader-library-and-layer-2.md)
  records at 0.89 code points of D5's budget of one — not a backend artefact,
  and not the reference's own slack, which the same record measures at 3.0e-8
  between two independently written quadratures. Both languages meeting it at
  the same figure is what says neither translation added to it.

## The three obligations, and where each one is

`../../conformance/README.md` puts three obligations on a consumer beyond
comparing, and each had already been a real defect in this repository's own
consumer. They are met here rather than rediscovered:

- **The counts are pinned outside the file.** `DashsceneHlslConformance.Pinned`
  states thirteen functions, their probe counts, their component counts, their
  positional argument names **and their tolerances**; `PinnedProbes` and
  `PinnedValues` state the two totals separately, so a typo in one statement is
  caught by the other. The set comparison runs both ways — **a function in the
  file this harness cannot dispatch is a failure, not a skip** — the case
  _count_ is compared too, because a file carrying one function twice satisfies
  both set comparisons while the second copy is evaluated by nothing, and the
  tolerance is pinned because it is the one column a count pin cannot cover: the
  comparison reads it out of the file, so a widened one cannot fail and cannot
  be noticed.
- **The comparison fails on a NaN.** `Outside` is written as the negation of
  `|got - want| <= tolerance`, never as `> tolerance`: both are false for a NaN,
  so the second form accepts every one of them silently. Two checks hold it at
  two layers, each pinning its own: `SelfCheckComparison` drives `Outside` with
  a NaN and two infinities, and `SelfCheckFailureList` drives `Compare` — the
  call the evaluation loop makes — with a NaN, an exact match and a finite
  disagreement, the last so that comparing the shader's answers against
  themselves is caught too.
- **Nothing here regenerates the expectations.** There is no reference
  implementation in this directory and there must not be one; the only
  arithmetic is `|got - want|`.

**What is ported is the table test and not layer 2's properties.**
`docs/decisions/shader-library-and-layer-2.md` says a second painter ports both;
this ports one, which is issue #1324. The two fixture-validity guards among the
properties are about the data rather than the implementation, so the Rust suite
running them covers this consumer too. What is unevaluated in HLSL is the three
regime properties, the stroke alignment sweep, and
`a_sample_on_the_edge_is_half_covered`, which
[`dashscene-gpu.md`](../../docs/design/dashscene-gpu.md) names as a property
over widths the table does not carry.

The envelope is honoured too: `format` is a version handshake and an unknown one
is refused before anything below it is read. `SelfCheckEnvelope` drives that
refusal with `2`, `0` and `1.5` — the committed file is format 1 either way, so
rewriting the comparison is otherwise caught by nothing. `SelfCheckCasePins`
does the same for the four pinned columns, one synthetic case each; the
tolerance in particular has no other driver, since the negative control corrupts
by 1.0 and would still report exactly its two failures against a table whose
tolerances had all been widened.

**What none of these self-checks reaches is a rewrite that deletes the call.**
`Evaluate` could drop `CheckFormat`, `CheckCase` or `Compare` and every
self-check would still pass. That is a property of a harness nothing else runs
rather than of any one check, and it is the other half of what issue #1323
closes.

Three more checks are the harness's own word about itself. The first runs before
the table is opened; the other two read the file and the compute shader, so they
run after the parse, beside the pins:

- **`ProbeJson.SelfCheck`** parses six literals whose correctly-rounded doubles
  were computed outside the harness — `1e23`, the largest subnormal, a halfway
  case, and the example `conformance/README.md` gives — and compares bit
  patterns. `conformance/README.md` asks a consumer for a correctly-rounded
  parser; this measures the runtime rather than trusting the framework's
  documentation.
- **`CheckSymbols`** asserts each function is defined in the generated HLSL
  under the symbol the compute shader calls. Twelve of thirteen are the WGSL
  name; `median3` is `median3_`, because naga appends an underscore to a name
  ending in a digit. Without this, a naga version that renamed something else
  would fail as an undeclared-identifier compile error with nothing saying that
  the translator's namer is where to look.
- **`CheckComputeStruct`** reads `SdfConformance.compute`'s own `Probe` and
  requires its member list to be exactly the four this harness packs. The
  `Marshal.SizeOf<Probe>()` check beside it compares the C# struct against a C#
  literal and so cannot see a member added to the HLSL side alone — which would
  leave `48 == 48` true and surface as thousands of value mismatches with
  nothing naming the stride.

## The gate has been watched failing

`just unity-conformance-negative` copies the table, moves **two** recorded
expectations by one unit — `erf_approx`'s probe 7, a scalar against a tolerance
of 1e-3, and component 2 of `gradient_ramp`'s probe 3, a `vec4f` row against
1e-6 — and requires three things of the run:

1. a non-zero exit, which is the **weakest** of the three: a missing editor, a
   bad path and a shader that did not compile all exit non-zero;
2. **both** corrupted values named, which is what pins the arithmetic mapping a
   flat value index back to a probe and a component. Neither corruption is at
   index zero, and that is the point of the pair: a scalar corruption cannot pin
   it at all because `at / 1` equals `at`, and at `at == 0` every wrong mapping
   agrees with the right one. `gradient_ramp`'s probe 3 component 2 is flat
   index 14, which a dropped divisor reports as probe 14 and a swapped mapping
   as `probe 2[3]`;
3. **exactly two of 2555 values differing.** Without it, any mutation that makes
   the gate reject _everything_ — a zeroed tolerance, a broken readback — passes
   the control, because the corrupted probes are among the failures too.

The mutation itself is confirmed by reading the two values back from both files,
not by comparing the files: `jq` re-serialises the document, so `cmp` reports a
difference whether or not the filter matched, and a guard built on it can never
fire.

It writes the corrupted copy under `target/` and never touches
`conformance/layer2-probes.json`, which is committed truth.

## Not shipped, and not format-checked

**Nothing holds the pins either.** The counts in `Pinned`, `PinnedProbes` and
`PinnedValues` exist so that a truncated table fails rather than running a
shorter loop — and since no tier reads this directory, they can themselves drift
against the table with everything green. Issue #1323 carries the check that
would close it, in `unity/package-gate` once issue #1307's lane has left that
crate.

These files are copied into a throwaway project by the recipe and live outside
`unity/com.driftsys.dashscene/`, so `unity/package-compat`'s glob never sees
them — the same siting as `unity/editor-compat/`, and for the same reason. Like
that directory, they carry no `.csproj`, so the `dotnet format` pass CI runs
over `unity/abi-check` and `unity/ffi-check` does not reach them.
