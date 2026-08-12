# Startup scaling is measured by a byte counter, not by a stopwatch

    status   accepted (2026-08-05); **AS-BUILT 2026-08-07 (v0.16, story #598,
             PRs #759 and #786)** — the counter, both documents and the
             criterion shipped as written. Measured 9.81x against the pre-slice
             load path and **1.00x** against the one v0.16 left; the
             qualification criterion is marked measured against that. The
             as-built section at the end records what changed under D2.
    scope    the startup-scaling benchmark that makes R5 falsifiable under
             guardrail G-20, and the two documents it measures.

## Context

R5 states "cold-start cost proportional to what is shown, not to file size
(mmap + section discipline)", and
[`../specification/05-qualification.md`](../specification/05-qualification.md)
makes it the first v1 exit criterion: "A scaling benchmark with a small-root
document and a many-frame corpus document asserts that cold-start cost tracks
the shown root, not the document size."

Nothing has ever measured it, and neither half of the measurement exists:

- **No benchmark harness of any kind.** No `criterion`, no `divan`, no
  `[[bench]]` target, no `just bench` recipe anywhere in the workspace.
- **No document large enough to measure.** The largest committed `.dsb` is 4 345
  bytes and there are twelve, every one under 4.4 KB — the whole set is smaller
  than one corpus photograph. `dashlang`'s stress-corpus generator is a doc
  comment at `crates/dashlang/src/lib.rs:2`, not code.

So the benchmark's shape is not constrained by anything that exists, and both
questions are open at once: what is measured, and what it is measured over.

## Decision

**D1 — cost is a count of bytes, not an elapsed time.** A byte count is exact,
identical on every machine, and either right or wrong with no tolerance to argue
about. A timing ratio needs a threshold that will drift, and cannot run on the
two-core CI runners without flaking. This also follows the rule this repository
already applies to invisible costs: a cost with no visible symptom needs a
counter, not a stopwatch — the same instrument as `Residency::decodes` and
`Renderer::allocations`.

**D2 — the counter counts asset payload bytes the load path reads**, whether
they are read to hash them or read to copy them. Both currently happen to every
payload and each alone is enough to make cold start scale with file size, so a
counter that sees only one of them cannot falsify the other.

**D3 — the boundary measured is the load path, not the frame.** It runs from
opening the file to a committed arena with the shown root's assets resident, and
stops there. It does **not** include what a painter does with the bytes
afterwards, so the number is a property of loading rather than of whichever
painter is selected — `dashscene-skia` copies twice internally and
`dashscene-gpu` does not, and that difference must not appear in this criterion.

**D4 — both documents show the same root, and the assertion is equality.** Not a
ratio under a threshold. If cold start tracks the shown root, then showing the
same root out of a small document and out of a many-frame one reads the same
number of payload bytes, and any drift is a defect rather than noise. The ratio
the qualification record asks for is reported, derived from the two counts.

**D5 — the many-frame document is generated when the benchmark runs.**
`dashc::Document` is an ordinary struct with public fields, so the benchmark
builds one directly and calls `dashc::compile`; R7 makes emission
byte-reproducible for a given input, so the generated file is identical every
run without being committed. Its payloads come from `corpus/photo`, which is
already in the tree. Nothing multi-megabyte enters git.

**D6 — wall-clock and the machine are recorded, and asserted on nothing.** An
absolute millisecond figure on a developer machine is not the criterion and must
not be presented as one. It is reported beside the counts, with the machine
named, as band and golden measurements already are.

**D7 — it is demonstrated failing at the base commit, not asserted to fail.**
Epic #594's definition of done requires running it before stories #595 and #596
land and recording the number. A benchmark that has only ever been seen passing
is the `t2-check-has-no-teeth` shape v0.13 spent an entire tier removing.

## Consequences

- **No benchmark framework is added**, and the "criterion or divan" question
  does not arise. The benchmark is a test with counters.
- It runs anywhere the suite runs, including the two-core CI runners and
  `wasm32`, because nothing in it is a timing.
- It fails at the base commit by construction: every asset payload is read once
  to hash it and copied once by the loader, so the many-frame document's count
  is a multiple of the small one's.
- The counter has to be observable across two crates — `dashbuf`'s verification
  and `dashscene-core`'s copy — which is a constraint on where it lives, not a
  choice this record makes.

## Alternatives considered

**`mincore(2)` over the mapping.** The most direct reading of "what did cold
start touch": map the file, load, then count resident pages. Refused on a detail
that cannot be worked around cheaply — the benchmark writes the document itself,
so the page cache is warm and `mincore` reports the whole file resident
regardless of what the process touched. Defeating that needs root or
`F_NOCACHE`, and it is POSIX-only, so the web path would need a second
instrument.

**A wall-clock ratio through `criterion` or `divan`.** Closest to the literal
words "cold-start cost". Refused as machine-dependent, needing a tolerance that
drifts, and unable to run on the CI runners without flaking — the same argument
that made the test tiers a membership list rather than a timing budget.

**A committed multi-megabyte fixture under `goldens/dsb/`.** Matches how every
other `.dsb` fixture is held, and freezes the benchmark's input. Refused for
repository weight: the 70 committed binary artifacts today are all small, and R7
already gives determinism without the file.

**A synthesised Figma REST JSON compiled through `compile_figma`.** Exercises
the real producer path. Refused because it adds a large JSON fixture to git and
couples a loading benchmark to the importer's vocabulary, so a change to Figma
lowering could move a number that is supposed to be about loading.

## As built (story #598, PRs #759 and #786)

The counter, the two documents and the criterion shipped as written. What the
measurement says, on macos aarch64:

| load path                       | small root | many-frame  | ratio     |
| ------------------------------- | ---------- | ----------- | --------- |
| pre-slice (PR #759, 2026-08-05) | 394 774 B  | 3 871 854 B | **9.81x** |
| as of v0.16's close (PR #786)   | 197 387 B  | 197 387 B   | **1.00x** |

The many-frame document carries 1 935 927 B of asset payloads against the small
one's 197 387 B, and costs the same to show the same root. D7 asked for the
failure to be demonstrated rather than asserted, and it was: the first row is a
run, not a prediction.

**D2's two recording sites are not the two this record named.** It named
`dashbuf::open_with_cost` for the read and
`dashscene_core::load_document_bound_with_cost` for the copy. The read site is
`dashbuf::residency::BlobResidency::touch_with_cost` — story #597 moved the hash
to the touch that makes a payload resident
(`verification-moves-from-open-to-touch.md` D8), and the eager reader's
instrumented sibling was deleted once nothing measured through it. The copy site
is unchanged. D2's _reasoning_ is untouched: each read alone makes cold start
scale with file size, so a counter seeing only one cannot falsify the other, and
`each_recording_site_counts_its_own_read_and_no_other` pins them apart because
the criterion cannot.

**D3's boundary held, and the benchmark had to move to stay inside it.** It
measured `open` plus the owning loader over bytes in memory, which is a path no
host takes; the owning path also cannot be bounded by what is shown, since
`load_document` copies every payload into an owned `ImageAsset`. The re-run
writes each document to a file, maps it, and runs the native host's sequence.
That is D9 of the verification record, and it is the difference between
measuring a host and measuring a benchmark.

**What the criterion is held out of, and why it is no longer a profile.** It ran
in `[profile.scaling]` while it was knowingly red, because a red test cannot sit
in a gate. It is an ordinary `regression` test now
(`docs/decisions/test-tiers.md`). One capability had to be replaced rather than
deleted with the profile: the `just scaling` recipe carried
`--success-output=immediate`, and without it a passing run prints nothing —
which would have deleted the record D6 asks for. CI re-runs the binary with
`--nocapture`, beside the render oracle and the calibrated budgets.
