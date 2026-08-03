# The shader library is one WGSL file, and layer 2 checks its arithmetic by compute

    status   accepted (2026-08-03)
    scope    dashscene-gpu's shaders/sdf.wgsl and its layer-2 conformance
             suite; the shadow's quadrature; the software device CI installs

## Context

R-T5 (`docs/specification/03-target-hardware-rules.md`) asks for the SDF shader
math to be "single-sourced (common include) into both painters' shading
languages", so that two product painters drawing the same picture is a property
of one implementation rather than a promise two make separately.

Epic #569's layer 2 is what turns that from a review promise into something
executable: the functions evaluated at sample points and checked against
expectation, on a runner with no GPU.

## Decision

**D1 — one file, included textually.** `crates/dashscene-gpu/src/shaders/sdf.wgsl`
holds the math and no entry point. `dashscene_gpu::SDF_WGSL` exposes it as a
`&str`; the render pipelines (story #580) and the conformance suite concatenate
it with their own entry points. No `naga_oil`, no build script — WGSL's
inclusion mechanism is textual and the seam has one consumer today.

**D2 — the library samples nothing and reads no derivative.** Every function
takes its arguments and returns a number. In particular the MSDF resolve takes
its screen-pixel range as a parameter rather than from `fwidth`, which is what
Chlumsky recommends for 2D and what avoids the documented NaN in the compact
derivative form.

**D3 — layer 2 evaluates by compute shader, and the reference is independent.**
Each function is dispatched over a buffer of probes and compared against an
implementation derived separately: the rounded-box distance against brute-force
sampling of the outline, the median against a sort, the gradients against their
own definitions, the error function against its own definition integrated in
double precision, the blurred box against a 512-step quadrature.

**D4 — the shadow integrates twelve rows, and that number is measured.** The x
integral is exact; only the y integral is approximated. Twelve is the first
count that holds the budget below.

This is Wallace's construction with three times his samples, not Levien's
closed form. Story #579 asked for the closed form — "with no sampling loop" —
and named Wallace's four-sample integral as the fallback. What is here keeps
Levien's fitted error function and loops twelve times, so it is the fallback,
widened until it holds a stated budget. The story's "validate against a real
multi-pass blur" was also read as "validate against a fine quadrature of the
same integral", which is a stronger reference than a multi-pass blur and not
the same thing. Both substitutions are deliberate and neither was what the
story specified.

**D5 — the budget is one code point of 255.** A shadow within one code point of
the truth cannot be told from it in an eight-bit output, which is what the
goldens compare.

**D6 — CI installs `mesa-vulkan-drivers` in the existing `test` job**, rather
than giving layer 2 a job of its own.

## Why

**D3, on why compute is trustworthy on a software adapter.** Evaluating the math
in a compute shader removes the rasteriser, the antialiasing resolve, the blend
stage and the sampler. What is left is float arithmetic, which does not shift
with a Mesa version the way coverage and blending do. That is the same reason
layer 4 is _not_ trusted to a software adapter and is measured on real hardware.

**D3, on why the reference is not the shader.** A suite whose expectation
restates the implementation checks the shader compiler and not the math. The
independence earned its keep immediately: the first run disagreed on a probe,
and the defect was in the reference — its inside test inferred each corner's
outward direction from the sign of that corner's centre coordinate, which reads
the wrong side when a radius equals its half-extent and the centre lands exactly
on an axis. The shader was right.

**D4, and what was measured.** Story #579 says to validate the closed form
before trusting it, because its constants are empirically fitted. Measured
against the reference quadrature, over 884 probes — four cases sharing one
60x40 half-extent and differing in their radii and sigma:

    samples   4      6      8      12     16
    worst     5.09   2.50   1.57   0.83   0.53   code points of 255
    mean      0.84   0.40   0.25   0.13   0.08

Evan Wallace's four samples — the construction story #579 names as the one with
production mileage — come out at 5.10 code points at the worst probe, which is
visible. The error roughly halves as the count doubles. Twelve is the first that
fits inside D5's budget. The worst probe throughout is a corner whose radius is
most of the box's half-height, where the cross-section varies fastest.

The fitted error function was measured separately: worst absolute error
2.35e-4 over x in [-4, 4], against its own definition integrated by Simpson's
rule. That is well inside the 1e-3 it is trusted within, and two orders below a
code point.

The reference's own step counts are set by what the comparison needs. At 4000
y-steps and 2000 Simpson intervals the suite took 159 seconds, which no tier
should carry; at 512 and 256 it takes 2.8. The whole table above is measured
against the shipped 512-step reference. An earlier draft mixed the two — four
rows from the 4000-step reference and the adopted row from the 512-step one —
and claimed they agreed to three significant figures, which is false: 0.00619
becomes 0.00617 and 0.00213 becomes 0.00209. The conclusion is unchanged, and
the reference still integrates forty times more finely in y than the shader it
judges, but a table has to come from one instrument.

**D4, on the tail correction.** Clipping the integration window to three sigma
and treating everything outside it as empty is wrong wherever the box extends
past the window: those rows are still inside the shape and still contribute. It
made a sample at the centre of a tall box read 0.997 instead of 1. Each tail is
added back as the Gaussian mass out there times the row at the window's edge.

**D6, on one job rather than two.** The conformance suite is an ordinary
workspace test that happens to need a device. A separate job would mean
excluding it from the tiers (`docs/decisions/test-tiers.md`), and it would then
stop running on a developer's machine, where it works against whatever adapter
is present — which is where it was written and first run.

## Verified where, and where not

The suite was developed and run on an **Apple M3 via Metal**, and the adapter is
printed by a test so that a number always has the device beside it. Every
measurement above is from that adapter.

**The lavapipe half is not yet verified.** The `mesa-vulkan-drivers` install,
the `VK_ICD_FILENAMES` path and `WGPU_BACKEND=vulkan` are written from the
documented setup and have not been executed, because the account's Actions
billing was unsettled while this story was built and no CI job could be
scheduled. The first green `test` job after billing is settled is what confirms
it; if the ICD path is wrong the suite fails by name — "layer-2 conformance
needs a wgpu adapter and found none" — rather than passing vacuously, which is
the failure mode that matters.

## Consequences

- Story #580 includes `SDF_WGSL` in its render pipelines and adds no second copy
  of any function here.
- Story #582's MSDF path uses `msdf_coverage` with the uniform range D2
  requires, and supplies that range from the atlas scaling.
- Story #584 owns the shadow's cost and may revisit D4's sample count against a
  frame budget. The table above is what that decision needs; the budget in D5
  is what it must not cross.
- A second painter porting `sdf.wgsl` ports this suite with it, which is what
  makes R-T5's "single-sourced" claim checkable rather than asserted.
