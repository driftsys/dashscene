# v0.16 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    revised  2026-08-05, at the v0.16 open
    empties  when epic #594 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment
             its work lands, and records nothing a design record should hold.

Drive v0.16 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the
merge method and the five principles, and it is authoritative over anything
below. This prompt adds only what is not in it.

## What this slice is

R5 made falsifiable. `docs/specification/01-goals-and-requirements.md` states
it as "cold-start cost proportional to what is shown, not to file size (mmap +
section discipline)", and `docs/specification/05-qualification.md` makes the
startup-scaling benchmark the **first v1 exit criterion** under guardrail G-20.
Nothing has ever tracked it. Epic #594 is the work item that record's own
argument says a named criterion must have.

**The criterion is a ratio, not an absolute**, which is the whole reason this
runs now rather than waiting for target hardware the way epic #476 and #462 do.
A small-root document against a many-frame corpus document is measurable on any
machine.

## v0.15 is still open, and one of its stories is in flight

Epic #569 is open with story #586, the layer-4 perceptual band, in the
`dashscene-586` worktree. So `main` will move under this slice from a track
that is not this slice.

**Take that state from `gh issue list --milestone "v0.15 — the lean painter"
--state open` and `git worktree list`, not from this paragraph.** It has
already been wrong once: an earlier revision named issue #746 as in flight
beside #586, and #746 closed through PR #750 **twenty seconds before this
branch's own head commit** — a sharper demonstration of the point than the
paragraph was making. That is what a parallel track does to any state written
into a file.

Three consequences, all of them things v0.15 paid for already:

- Run `git worktree list` before assuming anything is unstarted.
- Read `git config --get remote.origin.url` before any fetch, reset or push.
- **Re-run `just build` after every rebase.** A clean rebase is not evidence
  that the two sides still compile together.

**The phase-end revision has not happened**, because the epic has not closed.
`AGENTS.md` puts it at the epic close: revise the remaining epics and stories
against what the slice taught before the next one starts. This prompt carries
the part of that revision that could be verified today — the stale claims in
the story bodies, below — but the ritual itself is still owed, and issue #741
(does `dashscene-web` become the web integration crate) and issue #727 (the
backend implementation guide) are both waiting on it.

## CI IS DOWN — READ THIS BEFORE ANYTHING ELSE

The account's GitHub Actions billing is unsettled and **no job can be
scheduled**. Confirmed again on 2026-08-05 against run 30964570312 on `main`:
all fifteen jobs report **zero steps**, `fmt`, `changes`, `dprint` and `ci`
fail and every other job is skipped behind them. The logs 404. The reason lives
on one endpoint and nowhere else:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

It returns, verbatim: _"The job was not started because recent account payments
have failed or your spending limit needs to be increased."_ **Query annotations
before diagnosing anything else** — the UI's "this check has no steps" is the
symptom, not a config fault, and the workflow file is valid.

**This is worse than it was during v0.15**, where the early jobs still ran and
only the late aggregates died. Now the first job in the graph dies, so nothing
downstream is even attempted.

**The owner authorised merging on local evidence while this lasts** — `just
build`, plus `just calibrate` when the diff touches the `packer` filter. Record
the exception on each PR rather than merging silently. When billing is settled,
re-run a workflow on `main` and confirm `exit-gate` and `ci` go green; the
standing rule then returns to force.

## The load path today — one full read and four copies

Epic #594 and story #596 name one copy. Measured on `36aba72`, per asset
payload, and keeping reads and copies apart because the most expensive step is
not a copy:

| where                                                     | what                                          |
| --------------------------------------------------------- | --------------------------------------------- |
| `dashbuf::open` → `blob_by_hash` → `verify_section`       | **reads** every byte of every blob (BLAKE3)   |
| `dashscene-core/src/load.rs:172` `payload.bytes.to_vec()` | **copies** into `ImageAsset`                  |
| `dashpaint/src/lib.rs:646` `blobs.extend_from_slice`      | **copies** into `ImageTable`'s pool           |
| `dashscene-skia/src/lib.rs:301` `images.clone()`          | **copies** the whole pool, inside the painter |
| `dashscene-skia/src/lib.rs:1763` `Data::new_copy`         | **copies** into Skia, inside the painter      |

Before that, the host holds the whole file: `dashbuf::open` takes `file: &[u8]`
and there is no mapping crate in the workspace. `demo/src/document.rs:48` does
not even read a file — it `include_bytes!`es `goldens/dsb/v03-paint.dsb` into
the binary, so the native host **has no file to map yet and story #595 has to
give it one**. `demo-web/src/document.rs:96` is the one host that already
fetches lazily, through `Envelope::read`.

**The first row is the finding that matters, and it is in no issue.**
`dashbuf::open` resolves every asset entry through `blob_by_hash`, which calls
`verify_section`, which BLAKE3-hashes the whole payload. So opening a document
reads every byte of every asset, which faults every page of the file in —
exactly what mapping it is supposed to avoid. `Container::parse` was written to
prevent this: its module doc says payload hashes are "checked on demand … so
that a caller verifying only the hot sections never faults a cold page", and
`Container::verify_hot` exists and "touches no blob payload". `open` does not
call it, while its own doc comment claims "Nothing is copied … so a memory
mapping of it works unchanged" — true of the copies, false of the faults.

**So story #595's mapping changes nothing on its own**, because the first
`open` faults the whole mapping in anyway. And **story #597 is not additive**:
before it can add prefetch, it has to move an existing eager verification off
the open path, which is the trust-chain change its own body describes ("a
painter must never receive bytes that have not been hashed") approached from
the other side. The record for where verification happens is story #597's to
write. Both issues carry the finding as a comment.

**Both issues cite `load.rs:99` and the code has moved** — the copy is at line
172 now, inside `load_document_bound` (declared at `load.rs:124`), over the
`BoundPayload` type that issue #640 introduced after the stories were written.

**The last two rows are inside a painter** rather than between the mapping and
one, so they fall outside epic #594's definition of done. The fourth is worth
knowing about anyway: `dashscene-skia`'s frame cache compares the incoming
table byte for byte every frame, which its own doc comment measures at 200 873 B
for `surfaces` and calls "linear in the table's encoded size". The many-frame
document will make that visible immediately. Filed as debt #752.

## Four claims in the story bodies are already stale

Verified on 2026-08-05. The bodies were written at the v0.13 close, before
v0.15 landed six stories that touch this seam.

- **Story #595's "one obstacle, and it is one line" is settled, and the answer
  is already in the tree.** Story #587 built `crates/dashbuf/src/prefix.rs`:
  `Envelope::read(prefix, file_len)`, `hot_len`, `blob_by_hash`, `plan`,
  `Plan::wanted` and `Plan::bind`. `Container::parse` stays strict and needs no
  change at all. The record is
  `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`, and
  `demo-web` is a live consumer. **Option 2 was taken; do not re-open option 1
  and do not build a second reader.** Story #595 is smaller than its body: map
  the file on the native side, and adopt what exists.
- **Story #596's "one option is ruled out by issue #600" does not hold, and
  this is the trap to learn from rather than the correction.** The story
  predicted that `Cow<'a, [u8]>` would become a compiler error once the FFI
  gate landed. The gate landed, and it does not catch this: it is stated over
  `ImageEntry`, the stored row. `crates/dashscene-unity/src/lib.rs` says so in
  as many words — `ImageAsset` "stays as the owning producer type, which no
  `extern "C"` signature names" — and neither `ImageAsset` nor `ImageTable`
  appears in any `extern "C"` signature, so a lifetime on either compiles fine.
  **The loud failure story #600 was built to give this decision does not
  exist**, which is exactly why the shape is settled in a record instead. The
  first draft of this prompt asserted the compiler error from the issue's own
  prediction without reading the gate; do not repeat that with the other four.
- **Story #596's claim that `ImageFormat` is `{ Png, Jpeg, Gif }` with no
  compressed variant is false since issue #640.** It has **seventeen** variants
  — the three encoded ones, twelve ASTC rungs and two `Rgba8` — and
  `ImageFormat::is_encoded()` is the predicate that separates them. The
  upload-without-decode path **is** representable at the seam today; story #581
  is built on it. Do not surface it as a gap.
- **Story #596 says `dashscene-wgpu`. The crate is `dashscene-gpu`.**

## Story #598 has no harness and no corpus document

This is the story the epic orders **first**, written to fail, and neither half
of what it measures exists yet.

- **Nothing in this workspace benchmarks anything.** No `criterion`, no
  `divan`, no `[[bench]]` target, no `just bench` recipe.
- **The largest committed `.dsb` is 4,345 bytes.** There are twelve, and every
  one is under 4.4 KB — the whole set is smaller than one corpus photograph. A
  "many-frame corpus document" does not exist, and a ratio measured over 4 KB
  files measures nothing but noise.
- **`dashlang`'s stress-corpus generator is a doc comment**, at
  `crates/dashlang/src/lib.rs:2`, not code.

**Both halves are settled, and the record is
`docs/decisions/startup-scaling-is-measured-by-a-counter.md`.** Read its D1–D7
before writing anything; the short form:

- **Cost is a count of bytes, not an elapsed time.** Asset payload bytes the
  load path reads, whether to hash them or to copy them — both happen to every
  payload today, and a counter seeing only one cannot falsify the other. So
  **no benchmark framework is added**, and the criterion-or-divan question does
  not arise.
- **The boundary is the load path, not the frame.** From opening the file to a
  committed arena with the shown root's assets resident, and no further, so the
  number is a property of loading rather than of whichever painter is selected.
- **Both documents show the same root and the assertion is equality**, not a
  ratio under a threshold. The ratio is reported, derived from the two counts.
- **The many-frame document is generated when the benchmark runs**, from a
  `dashc::Document` built in code — it is a plain struct with public fields —
  and `dashc::compile`. R7 makes emission byte-reproducible, so it is
  deterministic without being committed, and its payloads come from
  `corpus/photo`.
- **Wall-clock and the machine are recorded and asserted on nothing.** An
  absolute millisecond figure on a developer machine is not the criterion and
  must not be presented as one.

**Write it so it fails first, and demonstrate that rather than asserting it.**
The epic's definition of done requires running it at the base commit and
recording the number. A benchmark that has only ever been seen passing is the
`t2-check-has-no-teeth` shape that v0.13 spent an entire tier removing.

## Two more things worth knowing before the first story

**No mapping crate exists, and the one `mmap` mention in the workspace is
unrelated.** `Cargo.toml:113` turns `blake3`'s `mmap` feature **off** so the
portable implementation builds for `wasm32-unknown-unknown`, which `dashc`
requires. Adding a mapping crate is a new workspace dependency; it does not
touch that line, and turning that feature on is not what this slice means by
mmap.

**A mapping is native-only, and the web answer is already built.** wasm has no
`mmap`, which is exactly why story #587's prefix reader exists. The asymmetry
is not a problem to solve — but the ownership shape story #596 chooses has to
work on both sides, because `demo-web` is a real consumer and not a
hypothetical one.

## Order from the epic

Epic #594 states it: **story #598 first, written to fail** against the
pre-slice load path, so the criterion exists before the change that satisfies
it. Then story #595, then stories #596 and #597 in parallel, then story #598
re-run for the final ratio, and story #599 last.

Take the open and closed counts and `main`'s commit from `gh issue list
--milestone` and `git log`, never from this file — the pair went wrong three
times in v0.15, most recently within an hour of being corrected. The milestone
is `v0.16 — loading performance`, which is the exact string `--milestone`
wants.

## Definition of done, from the epic

- The startup-scaling benchmark exists, and **fails against the pre-slice load
  path** — demonstrated by running it at the base commit, not asserted.
- It passes at the end, with the ratio recorded and the machine named beside
  it.
- No asset payload is copied between the mapping and the painter.
- **Zero goldens moved.** Nothing here should change rendered output. Story
  #596's ownership change is the one that could, and it must be checked per
  file with `git hash-object`.

## The loop, per story

1. Read the story issue and every comment on it.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. **Check the scope against the code before writing any of it.** Four claims
   above were stale; assume there are more. `git log -S<symbol>` settles most
   of them in one command, and it has paid off on three of the last four
   stories.
4. Implement.
5. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — **read the filter, do not
   recall it**. `Cargo.toml` and `Cargo.lock` are in it; a crate manifest is
   not, because that entry is root-anchored. This slice adds a workspace
   dependency, so the filter will match.
6. Open the PR **ready, never a draft**. Name the tiers you actually ran, and
   record the CI exception.
7. **Run `/code-review` and mean it — see below.**
8. Capture **every** finding as a checklist in the PR description. Fix
   criticals inline; file one `debt` issue per minor finding.
9. Merge, delete the branch with `gh api -X DELETE`, remove the worktree,
   comment the outcome on the story, update memory.

## Reviewing your own diff does not work

Story #581 is the evidence. An inline author-only pass found three real defects
and reported the work as reviewed-with-a-caveat. The five-agent `/code-review`
fan-out then found **nine more**, three at 100 confidence, including one that
defeated the entire purpose of the story: a resident PNG fully re-decoded on
every frame, which is the exact cost the story was opened to remove.

**If the session forbids subagents, ask.** The owner enabled them on request.
Do not settle for an inline pass and a caveat. **Give each review agent its own
worktree or make it read-only**, emphatically and by name — five agents once
destroyed each other's edits in one worktree.

**Across five v0.15 stories the fan-out found almost no arithmetic and a great
deal of wrong prose.** Issue #715: two findings, both prose, zero code defects.
Story #583: six, none in the rendering path. Story #584: seven, all prose, and
two of them recurrences of the previous story's findings about the same two
files. So the two instruments do not overlap: **mutation finds what the code
does wrong, the fan-out finds what the prose claims wrongly.** Run both.

**The cheap defence is a grep, before the review.** After changing a field
name, a byte count, a line number or what a type owns, grep the tree for the
old _token_ — not the concept — and read every hit. This slice will falsify
prose in `docs/design/dsb-container-format.md`, `docs/design/dashpaint.md` and
several decision records, and the sweep should start from the previous story's
finding list.

## What has actually cost time here

Carried forward from v0.15 and earlier, filtered to what a loading slice can
hit.

- **A green summary is not a green build.** `just build` is four gates and only
  the first prints a summary; it once printed a green test summary and exited
  101 on clippy. Capture `cmd > file 2>&1; echo "REAL EXIT: $?"`, and **never
  pipe a build through `tail`** — the exit code becomes tail's.
- **Commit before mutation testing.** Story #584's script reverted each
  mutation with `git checkout --` against an uncommitted tree, and the third
  revert **destroyed three source files** while leaving a tree that still
  compiled. Every run after the first was worthless too.
- **A mutation that does not apply looks exactly like a survivor.** Verify by
  the **absence of the original**, and compare the **whole block** — `grep -F`
  matches a multi-line pattern line by line.
- **Fixing a finding and mutation-testing the fix are two separate steps.**
  After `Renderer::allocations` was corrected, the mutation that removed the
  corrected term still passed everything.
- **One row cannot falsify a stride, and one asset cannot falsify an offset.**
  Anything addressed as `base + row * stride` reads correctly for row 0 at
  every stride, because row 0 sits at the base. This slice is entirely about
  offsets and ranges into a mapping: **two entries are the minimum**, and the
  second must differ from the first in every field. This trap has caught this
  repository four times, most recently on `ImageEntry`'s own range lookup,
  where the only stroke sat at offset 0.
- **The uniform-fixture trap has four levels** — uniform data, uniform
  _arguments_, uniform _symmetry_, uniform _environment_. Before writing a
  fixture, list what the code reads and vary each axis.
- **A test name is a claim.** Four in story #581 could not fail on what they
  claimed, and only mutation found it.
- **Assert the drawn output, not the document.** Two tests once passed while
  the feature rendered nothing, because they asserted the authored intent
  rather than what `committed()` produced. Asserting the near side of a
  transformation cannot see a broken transformation.
- **Derive a fixture's bound from the constant, never restate it.**
- **A cost with no visible symptom needs a counter, not a golden.** The whole
  of this slice is invisible to every pixel test by construction. `Residency`
  and `Renderer::allocations` are the existing shape for this.
- **Estimate refactor churn from the read sites, not the construction sites.**
  Flattening `ImageTable` cost 5 files, not the 42 estimated.
- **A test that compares two independent arenas must resolve every index
  first** — a row index means nothing outside the table that assigned it
  (`docs/decisions/cross-arena-comparison-resolves-indices.md`).
- **`cd` does not persist between commands.** Use `git -C <abs-path>`, and do
  not remove a worktree while a shell's cwd is inside it, or while a process is
  still running from it.
- **markdownlint reads a line-initial `#123` as a heading**, and dprint reflows
  the paragraph, so the fix is to reword rather than to move the number.
- **An indented block in a Rust doc comment is a doctest and will fail the
  build.** Fence it as `` ```text ``. `just build` catches it at the doc-test
  gate, which is the last of the four.
- **Shape the branch before merging.** Rebase, squash to one conventional
  commit, force-push, then `gh pr merge --merge`. Check the tree hash across
  the squash — it must not change.

## The two decisions that were taken before the first story

Both were raised with the owner at the slice open, before any code, and both
have records. Do not re-open them in a story; read them and build against them.

**`docs/decisions/assets-borrow-from-the-mapping.md`** — the boundary-B
ownership shape for story #596. The table's pool becomes either the `Vec<u8>`
it allocated or a reference-counted handle to a region it does not own, never
both. `ImageEntry` keeps its twenty-byte shape, **the `Painter` trait does not
change, and no boundary-B type gains a lifetime** — which is the whole reason
the handle was chosen over a borrow, since a lifetime reaches every painter and
is inexpressible in a C header. D1–D8 also fix the 4 GiB cap that a `u32`
offset implies, why a mixed table is refused, and why `PartialEq` is debt #752
rather than part of the story.

**`docs/decisions/startup-scaling-is-measured-by-a-counter.md`** — the
benchmark for story #598, summarised above.

## Stop and ask, rather than deciding alone

- **A story needs something its prerequisite did not deliver.** This went three
  for three in v0.15 — issues #640/#716, story #584's word on `Instance`, and
  story #733's whole read-the-destination route. Every one was cheaper because
  it was raised before code was written. Story #596 reaching into
  `ImageTable`'s pool is the candidate here.
- **A golden moves.** That is a real regression until proven otherwise —
  never `UPDATE_GOLDENS=1` to make a test pass. This slice's definition of done
  says zero goldens move, so any movement stops the story.
- A story's scope turns out to be wrong, or already done. Story #595 is already
  smaller than its body says; check whether it is smaller still.

## When the slice is done

Close epic #594 with a summary of what landed, revise the remaining epics and
stories against what v0.16 taught, and record scope-level changes as
`docs/decisions/` records — the phase-end ritual in `AGENTS.md`. Archive this
file verbatim to `docs/archive/` and remove its row from `docs/wip/README.md`.
