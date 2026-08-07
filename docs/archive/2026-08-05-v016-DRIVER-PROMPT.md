# v0.16 driver prompt — drive the slice to completion, one story at a time

    status   live; hand this to a session as its first message
    revised  2026-08-07, mid-slice. Two stories have landed and the third is
             recorded but not built. The 2026-08-05 revision described a slice
             that had not started; almost everything specific in it is now
             either done or wrong, so this is a rewrite rather than an edit.
    empties  when epic #594 closes. Archive it verbatim to docs/archive/
             rather than gardening it — a driver prompt is spent the moment
             its work lands, and records nothing a design record should hold.

Drive v0.16 to completion, one story at a time, in a loop.

Read `AGENTS.md` first — it holds the story workflow, the test tiers, the
merge method and the five principles, and it is authoritative over anything
below. This prompt adds only what is not in it.

## Where the slice stands

**Take the open and closed counts and `main`'s commit from `gh issue list
--milestone "v0.16 — loading performance"` and `git log`, never from this
file.** The pair went wrong three times in v0.15 and this file has already been
rewritten once for the same reason. What is below was true at
`76a802a` on 2026-08-07.

| story              | state                                                                  |
| ------------------ | ---------------------------------------------------------------------- |
| #598 first half    | merged, PR #759. The criterion exists and was **demonstrated failing** |
| #595 map the file  | **closed**, PR #760                                                    |
| #596 assets borrow | **closed**, PR #762                                                    |
| #597 residency     | **recorded, not built.** PRs #763, #764, #766                          |
| #598 re-run        | open                                                                   |
| #599 the records   | open                                                                   |

**The number has not moved, and that is correct.** `just scaling` still reports:

    small-root  (1 frame)    394 774 B
    many-frame  (65 frames)  3 871 854 B
    ratio                    9.81x, against a criterion of 1.00x

Story #595 removed a read that was not the expensive one, and story #596 removed
two copies. **Story #597 is the one that moves it**, and until it lands the
criterion being red is the slice working as designed, not a regression to
chase.

## The load path today

Measured at `76a802a`. Two of the five rows the 2026-08-05 revision listed are
gone; what is left is verification, in two places.

| where                                               | what                                   |
| --------------------------------------------------- | -------------------------------------- |
| `dashbuf::open` → `blob_by_hash` → `verify_section` | **reads** every payload an entry names |
| `prefix::Plan::bind`                                | **reads** every payload it is handed   |
| `dashscene-skia/src/lib.rs:301` `images.clone()`    | copies the pool, inside the painter    |
| `dashscene-skia/src/lib.rs:1763` `Data::new_copy`   | copies into Skia, inside the painter   |

The last two are painter-internal and outside epic #594's definition of done.
The fourth is worth knowing about: `dashscene-skia`'s frame cache compares the
incoming table every frame, and story #596 made that comparison cheaper for a
mapped table without removing it. Debt #752.

**Which of the first two runs depends on the host, and this is the thing the
last revision got wrong.** Since #596 the native mapped host reads its envelope
with `dashbuf::prefix`, plans, and calls `bind`; `dashbuf::open` is left holding
`demo`'s embedded golden, a `&'static [u8]` with no pages to fault. So **on the
only path where a mapping exists, `bind` is the one doing the damage.** Changing
`open` alone would leave the criterion at 9.81x and make story #597 look like it
had failed.

## The four records this slice is built on

Read them before writing anything. Three were decided before any code and one
corrects a mistake in another; none of them is optional, and none of them should
be re-opened in a story.

- **`docs/decisions/startup-scaling-is-measured-by-a-counter.md`** — cost is a
  count of asset payload bytes, not an elapsed time, so no benchmark framework
  exists. Both documents show the same root and the assertion is equality.
- **`docs/decisions/assets-borrow-from-the-mapping.md`** — the image table's
  pool is owned or mapped and never both; `ImageEntry`, the `Painter` trait and
  every boundary-B type are unchanged. As built at #596.
- **`docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`** —
  `Container::parse` stays strict; the prefix reader is the second reader.
  **Do not build a third.**
- **`docs/decisions/verification-moves-from-open-to-touch.md`** — story #597's
  contract, D1–D9. It was corrected once already; read the status block, which
  says what changed and why.

## Story #597 — what is left to build

Its own body and epic #594 both list `madvise`. **That is out** (owner's ruling,
2026-08-07, D5, filed as #767 against v1). Remaining scope: **prefetch, hash on
touch, mark ready.** The story title still says `madvise`; the comment thread is
where it was given up.

The work, in the order that keeps the tree green:

1. **Rename the eager reader to `open_verified`.** 54 call sites over 26 files,
   nearly all tests that want the eager behaviour and should keep it. A
   twenty-seventh file, `crates/dashscene-core/src/load.rs`, names
   `dashbuf::open` only in the worked example in its module doc — that has to
   change too, because it states the read contract and after this there are two.
   **Re-derive both numbers before quoting either**; the pair is what goes
   wrong.
2. **`open` verifies the hot half and returns `Vec<Range<u64>>`.** The return
   type is the guarantee, not a convenience: a caller cannot hand a range to a
   painter.
3. **`dashbuf::Residency`** — touch + hash + mark ready, per blob, `Send + Sync`.
   The only thing that turns a range into readable bytes.
4. **Both readers give up their hashing to it.** `open` and `Plan::bind`. Moving
   one leaves the other, and the one that matters is `bind`.
5. **Prefetch the shown root's assets and nothing else**, with the set computed
   from the hot document. Nothing is touched to decide what to touch.
6. **`LoadCost::record_hashed` moves to the touch**, out of both readers.

## Story #598's re-run — the trap that is already known

**The benchmark measures the owned path.** `goldens/tooling/tests/startup_scaling.rs`
calls `open_with_cost` plus `load_document_bound_with_cost`, builds its two
documents **in memory**, and maps nothing. Left alone it will keep reporting the
old path's number no matter what #597 does.

So the re-run has to write each generated document to a temporary file, map it,
and load it the way the native host does. That is also what makes the criterion
a measurement of what a host really does rather than of a path only the
benchmark takes.

Then: record the ratio and the machine, move the test out of `[profile.scaling]`
into `regression`, delete the profile and the `just scaling` recipe, and update
the section `docs/decisions/test-tiers.md` added for it — it says outright that
the holding is temporary and the profile goes away.

## Story #599 — what the records already owe

Three corrections are known and waiting; do not rediscover them.

- **Guardrail G-19** in `docs/technotes/engineering-guardrails.md` is recorded
  as failing, with the measured numbers and story #597 named. Settle it against
  a measurement.
- **`docs/decisions/asset-model-content-addressed-blobs.md` overclaims.** It
  says "the signed root covers the hot sections … transitively authenticated by
  the same signature". **Nothing is signed**: `Header::signature_offset` and
  `signature_length` are reserved and required to be zero in version 1, and
  `root_hash` deliberately does not cover the header. What the hashes buy today
  is corruption and transport detection, not tamper resistance. Say so.
- **The as-built results** for the three records above, against the records
  rather than replacing them.

Then close epic #594, revise the remaining epics and stories against what v0.16
taught, archive this file verbatim to `docs/archive/`, and remove its row from
`docs/wip/README.md`. **The v0.15 phase-end revision is still owed** — it was
never done, and issues #741 and #727 are waiting on it.

## CI IS DOWN — READ THIS BEFORE ANYTHING ELSE

Still down. Confirmed on 2026-08-07 against run 31149355870 on `main`: `changes`,
`dprint` and `fmt` fail with **zero steps** and every other job is skipped behind
them. The logs 404. The reason lives on one endpoint and nowhere else:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

It returns, verbatim: _"The job was not started because recent account payments
have failed or your spending limit needs to be increased."_ **Query annotations
before diagnosing anything else** — "this check has no steps" is the UI's
wording for a job that was never scheduled, not a config fault, and the workflow
file is valid.

**The owner authorised merging on local evidence while this lasts** — `just
build`, plus `just calibrate` when the diff touches the `packer` filter. Record
the exception on each PR rather than merging silently. When billing is settled,
re-run a workflow on `main` and confirm `exit-gate` and `ci` go green; the
standing rule then returns to force.

## The loop, per story

1. Read the story issue and **every comment on it**. Three of the five stories
   in this slice had a comment that changed their scope.
2. `git worktree add` **before the first edit**, then `./bootstrap`.
3. **Check the scope against the code before writing any of it.** Every story
   in this slice has been smaller or differently shaped than its body said.
4. Implement.
5. `just build`. Run `just calibrate` when the diff touches any path in the
   `packer` filter in `.github/workflows/ci.yml` — **read the filter, do not
   recall it**. `crates/dashbuf/**`, `Cargo.toml`, `Cargo.lock` and `.config/**`
   are in it; `crates/dashpaint/**` and `crates/dashscene-core/**` are not.
6. Open the PR **ready, never a draft**. Name the tiers you actually ran, and
   record the CI exception.
7. **Run `/code-review` and mean it** — the fan-out, not an author pass.
8. Capture **every** finding as a checklist in the PR description. Fix
   criticals inline; file one `debt` issue per minor finding.
9. Merge, delete the branch with `gh api -X DELETE`, remove the worktree,
   comment the outcome on the story, update memory.

## What this slice has cost, in its own words

Everything here happened in this slice. The general traps are in `AGENTS.md`
and in the v0.15 prompt in `docs/archive/`.

- **A claim can be true of the function it names and still be the wrong
  function.** The #597 record said the eager verification is "one call in one
  function" and named `dashbuf::open`. Every word was true of `open`. Story
  #596, four hours earlier in the same session, had moved the mapped host onto
  `prefix::Plan::bind`. **A review agent told to verify every claim passed them
  all**, because each claim was true of the thing it named; nobody asked whether
  the named function was still on the path. `grep -n "blake3::hash"` over the
  crate answers it in one command. Ask "how many places do this?" before writing
  "one".
- **Assert the address, not the bytes.** A copy has equal bytes. Both #595 and
  #596 turn on a pointer test — a section, and then an image, must be at the
  offset its table declares. In each case mutating the code to return a leaked
  copy killed the pointer test **while the byte-equality test beside it passed**.
- **The mapped path turns loud failures silent.** It reads no header by design,
  so the owning path's header parse — issue #640's guard — is gone. Two
  replacements were needed and the fan-out found the second: the demo refuses a
  file that binds through a derivation manifest, and `push_mapped` asserts a
  baked row's length. **My own test had staged a PNG's byte range tagged
  `Astc4x4Unorm` and passed.**
- **When a change makes a type's ownership conditional, grep for "owns".** #596
  falsified three doc comments it did not edit — `Arena::images`,
  `CommittedScene::images`, and `demo/src/document.rs`'s module doc, which
  stated one read contract while the same PR added a second path in the same
  file.
- **Re-derive a count and its denominator separately.** "54 call sites over 27
  files" did not reconcile: 55 raw matches, one a comment, so 54 sites over 26
  files.
- **Quote records by copy, never by memory.** Two quotations in the #597 record
  were not verbatim — an em dash for a parenthesis, an em dash for "because".
- **The anti-uniform-fixture guard fires on your own assumptions too.** #595's
  asserted that every fixture has at least two sections; eight of the ten have
  exactly one. The real guard is "the walk saw at least two distinct non-zero
  offsets".
- **A knowingly-red test cannot sit in a gate and must not be silently
  skipped.** `[profile.scaling]` and `just scaling` are how that was held, and
  `just test-all` is red on purpose until the slice closes.
- **markdownlint reads a line-initial `#462` as a heading** and dprint reflows
  the paragraph, so parenthesise the number or reword — do not move it.

## Reviewing your own diff does not work, and the two instruments do not overlap

Three fan-outs this slice, 14 findings: **13 prose, 1 code.** Eleven mutations,
every one killed by a test the fan-out never mentioned. Mutation finds what the
code does wrong; the fan-out finds what the prose claims wrongly. **Run both.**

**Give each review agent its own worktree or make it read-only**, emphatically
and by name — five agents once destroyed each other's edits in one worktree.
Read-only agent types are the cheap way.

**The cheap defence before the review is a grep.** After changing a field name,
a byte count, a line number or what a type owns, grep the tree for the old
_token_ — not the concept — and read every hit.

## Stop and ask, rather than deciding alone

This went three for three in v0.15 and three for three again here: the prefix
route, the verification contract, and `madvise`. All three were cheaper for
being raised before code.

- **A story needs something its prerequisite did not deliver.** #596 needed
  per-entry ranges and neither `open` nor a slice could give them.
- **A story's scope turns out to be wrong, or unmeasurable.** `madvise` was in
  #597 by name and came out because nothing in the slice can see it.
- **A golden moves.** That is a real regression until proven otherwise — never
  `UPDATE_GOLDENS=1` to make a test pass. This slice's definition of done says
  zero goldens move, and none has.

## Definition of done, from the epic

- The startup-scaling benchmark exists, and **fails against the pre-slice load
  path** — **met**, demonstrated by running it at PR #759.
- It passes at the end, with the ratio recorded and the machine named beside
  it. **Not met**: 9.81x, waiting on #597.
- No asset payload is copied between the mapping and the painter. **Met** at
  #596, asserted by address.
- **Zero goldens moved.** Met so far, and checked per file with `git hash-object`
  if anything looks like moving.
