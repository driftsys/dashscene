# conformance

Layer-2 conformance data: dashscene's SDF shader math stated as inputs, expected
values and tolerances, so a painter in **any** shading language can check it.

    layer2-probes.json   the probe table — one case per probed function of
                         the shader library, and the probes of each

The math itself is one file,
[`../crates/dashscene-gpu/src/shaders/sdf.wgsl`](../crates/dashscene-gpu/src/shaders/sdf.wgsl).
R-T5 (`../docs/specification/03-target-hardware-rules.md`) asks for it to be
single-sourced into both product painters' shading languages, so that two
painters drawing the same picture is a property of one implementation rather
than a promise two make separately. Getting that file into another shading
language is the cost a second painter takes on — by hand, or by generating it
with a translator. **This directory is what says the result is right**, either
way, and it exists because until it did, R-T5's promise rested on one
implementation testing itself (issue #828).

## What is here and what is not

`layer2-probes.json` carries **the claims that are one value at one input**.
Everything else layer 2 asserts is a property, which ports as a property and
cannot be loaded: saturation out where no row reaches, a stroke band whose hard
edges tie at its endpoints, a relation between two probes, a guard saying the
fixture could fail at all.

The rule for which is which — and the list of properties, each named for the
claim it makes — is stated once, in
[`../docs/design/dashscene-gpu.md`](../docs/design/dashscene-gpu.md) under **The
probe table, and what stays a property**. Read it before deciding a painter is
conformant because the file loads.

## Running it

A consumer needs three things: a way to evaluate each function the file names
over the arguments it records, a JSON parser, and a comparison.

    for each function in functions:
        for each probe in function.probes:
            got  = evaluate(function.name, probe.args)
            want = probe.expected
            assert |got - want| <= function.tolerance,  componentwise

`crates/dashscene-gpu/tests/layer2_conformance.rs` is the first consumer and the
worked example: `the_shader_matches_the_committed_probe_table` is exactly the
loop above, dispatching each function as a compute shader over
`tests/shaders/conformance.wgsl`. **A second painter ports that test, not the
expectations.**

Three things a consumer must do besides comparing. The first two share a reason
— a suite reading its expectations from a file passes quietly over work that
stopped happening:

- **Check the probe count against something outside the file.** Comparing what
  you evaluated against what the file declares is a tautology — both sides come
  from the same parse, which is what an assertion in this repository's own
  consumer turned out to be before it was removed. Pin the counts you expect in
  your own harness, so that a file that arrives with a case truncated fails
  rather than running a shorter loop. And fail on a function name you cannot
  dispatch rather than skipping it.
- **Fail on a NaN.** Write the comparison as `|got - want| <= tolerance` and
  fail when that is false, **not** as `> tolerance`. Both are false for a NaN,
  so the second form accepts every one of them silently. This repository's own
  consumer had that defect and a review found it: a probed function returning a
  quiet NaN passed the whole suite.
- **Do not regenerate the file.** A consumer that recomputes the expectations
  from its own implementation tests that implementation against itself and
  proves nothing.

### The envelope

The top-level object carries five fields before `functions`, and one of them is
load-bearing:

    {
      "format": 1,               <- the version handshake. REFUSE what you do
                                    not know; this repository's own consumer
                                    does, and a silent misread of a format 2 is
                                    the failure this field exists to prevent
      "about":       "...",      <- prose, for a human who opened the file
      "shader":      "crates/dashscene-gpu/src/shaders/sdf.wgsl",
      "properties":  "...",      <- where the rule for what is NOT here lives
      "recorded_by": "...",      <- what wrote it
      "functions":   [ ... ]
    }

`format` changes when the shape below it changes incompatibly. Check it before
you read anything else.

### The shape of a probe

    {
      "name": "coverage",
      "signature": "coverage(d: f32, width: f32) -> f32",
      "arguments": ["d", "width"],
      "result": "f32",
      "tolerance": 1e-6,
      "reference": "transliterated — the ramp is its own definition, …",
      "probes": [
        { "args": [-4.0, 0.0], "expected": 1.0 }
      ]
    }

**Every expectation is the function's value at the `f32` of its arguments**, not
at the decimal as written, because `f32` is what the shader receives. A consumer
that compares in double precision without rounding the arguments first is adding
its own error: for `erf_approx` that reaches 2.4e-8, against a tolerance of
1e-3, so it changes no verdict — but it is your harness's error and not the
table's, and it is worth not attributing to the mathematics.

`args` is positional and `arguments` names the positions. A scalar is a number,
a `vec2f`/`vec3f`/`vec4f` is an array of that many numbers, and an argument that
is itself an array is an array of those — `gradient_ramp` takes eight offsets
and eight colours. `result` is `f32` or `vec4f` and says how many components
`expected` carries; every component is compared against the same `tolerance`.

`reference` says whether the expectation was derived **independently** of the
shader — a sort for the median, brute-force outline sampling for the distance,
Simpson integration for the error function, a 512-row quadrature for the blurred
box — or **transliterated** from the same expression, which `coverage` and the
four gradient parameterizations are because those functions _are_ their
definitions and there is no second derivation to write. A transliterated row
still catches an error on the shader side; it checks the transliteration and not
the mathematics, and a reader should not think otherwise.

`stroke_coverage` is a third case and its `reference` field says so: the band's
overlap with the pixel footprint is a different derivation from the shader's
difference of two edge ramps, and the two are algebraically equal for a linear
ramp. So it checks the _arrangement_ rather than adding a second derivation —
which is the defect that was actually there, a single ramp of a folded distance
that painted a 0.25-unit stroke at 0.625 coverage. Read the field rather than
assuming the binary.

**One trap worth naming before you start, because it has now caught two
independent implementations.** `blurred_rounded_box` and `rounded_box_sdf` both
**clamp the corner radii before doing anything else** — no edge may be
over-subscribed by the two radii that meet it, and every radius scales by the
worst ratio. It is easy to miss in the blurred case, where the signature says
`radii` and the body integrates: the reference in this repository's own suite
omitted it, and so did a Python implementation written afterwards from this
file. Neither was caught until a blurred pill was recorded, because every other
recorded row leaves the radii untouched and the clamp is the identity on them.
The table now carries that row.

Two details of the recorded fixtures are load-bearing rather than arbitrary, and
a consumer that "tidies" them silently weakens the file:

- **`gradient_ramp`'s slots past `count` hold poison** — an offset of `-100` and
  a colour of nines. A slot past the count is one the function must not read;
  the offset compares true against every `t`, so an over-running walk mixes the
  nines in and leaves the tolerance by three orders of magnitude. Zeroes there
  would be indistinguishable from a black stop.
- **The gradient frames carry two properties on purpose: handles of unequal
  length, and an oblique angle between them.** With equal-length perpendicular
  handles, dropping the secondary handle from a radial passes. With
  perpendicular handles of any lengths, projecting onto the primary axis alone
  agrees with the full frame in `x`, so a linear gradient passes too. Both
  properties have to be present somewhere in the recorded frames, and
  `the_gradient_frames_in_the_file_can_fail_a_wrong_painter` is what asserts
  they are.

### Your symbols may not be these names

`name` is the function's identifier **in `sdf.wgsl`**, and that is what a
consumer keys on. It is not a promise about what the function is called in your
language: a translator renames whatever its target reserves, and a hand port
renames whatever its author prefers. Map the table's names to your own symbols
rather than assuming they match.

That is not hypothetical. `naga`'s HLSL backend — wgpu's own translator, and a
dependency of this repository — puts every identifier through `Namer::sanitize`
and then `Namer::call` (`naga-30.0.0`, `src/proc/namer.rs`), and **two different
rules rename things in this table**:

- **A trailing underscore**, for a first-use name that ends with a digit or
  collides with an HLSL keyword or type name. Two names here take it, for
  different reasons — `median3` ends with a digit, `sample` is a keyword. The
  digit case is not a collision: `call` uniquifies by appending `_<n>`, so a
  name already ending in a digit takes the separator to keep that suffix
  unambiguous. A **third** name in the library takes it and is not in the table:
  `blurred_rounded_box`'s local `step` is an HLSL intrinsic, so it translates as
  `step_`. This condition has a fourth clause, a set of builtin identifiers, and
  it is **empty for HLSL** (`back/hlsl/writer.rs`'s `reset` passes
  `proc::KeywordSet::empty()`).
- **A `_<n>` suffix on every repeat of a name**, which reaches the **argument**
  names extensively. `Namer` keeps **one `unique` map for the whole module** —
  `namespace()` exists to scope it and the HLSL writer never calls it — and
  `Namer::reset` names every function's arguments against that one map. So an
  argument name used by more than one function is renamed from its second
  occurrence on:

      float rounded_box_sdf(float2 p, float2 half_size_1, float4 radii_in)
      float gradient_linear_t(float2 p_2, float2 origin_1, float2 primary_1,
                              float2 secondary_1)

  Counted over a generated translation of this library: **46** such identifiers,
  including `half_size_1..4`, `p_1..6`, `origin_1..4`, `primary_1..4`,
  `secondary_1..4`, `d_1`, `d_2`, `t_1`, `width_1`, `width_2`, `radii_1`.

**This paragraph said the opposite until a review caught it.** It claimed one
rule accounted for every rename and that no two names here collide — true of the
**function** names, which is all that had been checked, and false of the
argument names, which repeat across functions by design. The check that settles
it is not reading `namer.rs`: it is translating `sdf.wgsl` and grepping the
output for `_[0-9]`.

Two more rules exist and reach nothing here, worth naming because their output
does not look like a renamed function at all:

- **`sanitize` rewrites a name _starting_ with a reserved prefix** to
  `gen_<name>` — a rewrite of the front. There are four prefixes and one is a
  bare **`naga_`**, so any `naga_*` function comes out as `gen_naga_*`.
- **`sanitize` filters characters**, dropping leading digits, collapsing runs of
  `_` and replacing anything outside `[A-Za-z0-9_]`. A mid-name `__` is legal
  WGSL and _is_ collapsed. Checked: no name in this table carries one.

What this means for a consumer: **key on the table's `name`, which is the WGSL
function name, and bind arguments by position.** Positional order is what
survives every rule above. Argument _names_ are documentation here, not a
binding surface, which is why the renames do not reach this file's contract.

    median3   ->  median3_    the only function name a suffix rule touches
    sample    ->  sample_     the only argument name HLSL reserves
                              (`msdf_coverage`'s first parameter)

Re-derive all of it against whatever version you translate with, from the
translated output rather than from the translator's source.

### Numbers, and the precision your parser needs

**Every expectation is the function's value at the `f32` of its arguments**, not
at the decimal as written, because `f32` is what the shader receives. A consumer
comparing in double precision without rounding the arguments first is adding its
own error — for `erf_approx` that reaches 2.4e-8 against a tolerance of 1e-3, so
it changes no verdict, but it is the harness's error and not the table's.

**Use a correctly-rounded JSON float parser.** This is not pedantry:
`serde_json` with default features writes `0.49000000953674316` and reads that
same text back as `0.4900000095367432`, a different double. Both narrow to the
same `f32`, so again no verdict moves — but a consumer that compares recorded
arguments against its own fixtures in `f64` will see spurious mismatches. Rust
consumers want serde_json's `float_roundtrip` feature, which is what this
repository's own consumer enables.

## Re-recording it

**The file is committed truth, and it is generated once rather than on every
run** — the same rule `goldens/README.md` states for a golden image. Do not hand
edit it. Change the fixtures or the references in
`crates/dashscene-gpu/tests/layer2_conformance.rs` and re-record:

    cargo test -p dashscene-gpu --test layer2_conformance -- \
        --ignored record_the_probe_table
    prim fmt --no-primignore conformance/layer2-probes.json
    just test

The recorder is `#[ignore]`d so that no test tier runs it. All three steps
matter:

- **The `prim fmt`** is what lays the file out. The recorder emits one line;
  prim keeps an array on one line where it fits and breaks it where it does not,
  and it _preserves_ a break the author already made — so a pretty-printed
  emission would commit one number per line, measured at about four times the
  lines and twice the bytes. `--no-primignore` is needed only inside a
  `.claude/worktrees/` worktree, where `.primignore`'s `.claude/` entry matches
  the absolute path and every prim pass skips every file, `just prim` included
  (issue #1284).
- **The `just test`** is not optional. After a re-record, the shader is the only
  independent word on what was written: the references that produced the file
  are also what `the_recorded_*` checks it with, so those tests agree with a
  laundered file by construction and
  `the_shader_matches_the_committed_probe_table` is what does not.
- **Read the diff before committing.** A re-record that moves a number is either
  a fixture change you meant or a reference regression you did not.
