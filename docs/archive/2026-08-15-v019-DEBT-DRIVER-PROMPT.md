# v0.19 driver prompt — the remaining debt, and two stories inside it

    status  written 2026-08-15, after #947 landed and #931 and #946 were
            taken. Every count and issue state below was re-derived at the
            moment of writing, not recalled — the previous handoff that was
            written from memory came back with fifteen defects, and its
            reader could not check it. Archived to `docs/archive/` when its
            work lands. **A driver prompt has no row in
            `docs/wip/README.md`** — captures have the table, prompts have
            that file's prose — so the commit adding this one updates those
            paragraphs and the tracked count with it.
    scope   the v0.19 debt that is pickable without hardware. **#925 and
            #946 have since landed** and are struck from this list below;
            what remains is **#944**, a story wearing a `debt` label, and
            **#922** and **#930**. (**#945** landed too — struck below.)
    epic    #833

## Re-derive before trusting any of this

    gh issue list --milestone "v0.19 — Android, the C ABI, and layer 0" \
      --state open --json number,title,labels

At 2026-08-15 that returned **sixteen rows**: two `story` (#842, #843, both
waiting on hardware), twelve `debt`, one `epic` (#833), and #767 unlabelled.
`main` was at `3a7c0d3`.

**The list decays under v0.20's work, which is running in parallel.** This is
not hypothetical: #968 moved #950's premise, and #988 edited
`goldens/tooling/tests/common/`, which is #929's and #932's territory. Check
each issue's premise against the code before starting it, not after.

## What is already done, so it is not redone

- **#947** — the C ABI receives fonts. `ds_runtime_load_document_with_text`
  takes an array of `DsFontFace`, each pairing a face with the committed sheet
  its glyphs sample; `TextResources::from_faces` assembles them.
  `DS_ABI_VERSION` stayed 1. Merged as pull request #978.
- **#931** — `docs/decisions/test-tiers.md` no longer carries test counts. Do
  not put them back. A reader who wants one runs the tier and reads its
  `Summary` line.
- **#946, items 2 and 4** — the scaling prologue's anchor, and
  `prefetch::roots`' doc.

Both of the last two are in pull request **#1011**, open at the time of writing.
Check whether it merged before touching those files.

## #946 cannot be closed as written, and this is the first thing to do

**DONE.** The issue was amended to record item 3 as investigated and rejected,
and closed on items 2 and 4. The section below is what said so, kept because the
argument is the record.

Its third finding — that every rebuild issues two `document_replaced` calls and
one is redundant — **is wrong**, and #1011 records why at the call site.

`CommittedScene::renumbered` is `arena.shown_root != previous_shown_root` and
nothing else (`crates/dashscene-core/src/arena.rs`). A rebuild whose scene never
names a shown root — every showcase scene built in code — never reports one, so
the eager call in `Host::rebuild` is the only notification the presenter gets.
Removing it leaves the lean painter describing the outgoing arena on the path
`demo` takes by default.

**Amend the issue** to record that item 3 was investigated and rejected. Items 2
and 4 are the ones #1011 fixes, so the issue closes on those two plus that
amendment — not silently as "fixed".

## The two that are stories wearing a `debt` label

Both are worth their own spec and plan rather than a sweep commit.

- **#925 — LANDED.** `ds_runtime_load_document_mapped` takes a path and a
  required `ShownRoot` ordinal and reads only the assets that root's subtree
  draws. `first_derived_payload` and `show_appended_root` moved to
  `dashscene-core` with it, so the recipe is stated once instead of three times.
  Do not re-do it. The paragraph below is what the prompt said before, kept
  because the reasoning it gives for the shape is what was built:

  > The natural successor to #947 and the piece that makes R5 true there:
  > today's load is whole-file and owning, so `dashscene_core::load_document`
  > copies every payload whether or not anything draws it. The ABI's own module
  > documentation now argues for it, and the versioning rule prices it — a new
  > symbol is free where a parameter on the shipped one is not. Take the
  > `ShownRoot` with it; the module docs explain why doing them together is what
  > makes the bound real rather than nominal.
- **#944 — the commit's per-node scratch vectors scale with the document.** Its
  own body says it: "That is a story, not a fix inside another story." Eight
  vectors sized at `arena.nodes.len()` and a carry-forward loop over every node,
  to produce a one-row table on the band's own fixture. Re-keying them off
  `NodeId` slots is a change to the commit's whole interior. **The band cannot
  see this** — it measures Taffy computations and committed rect rows, and
  neither moves — so whichever way it is fixed, it needs a third term that can.

## The three smaller ones, each with a question to settle first

- **#945 — LANDED.** The gate is `LiveScene::take_renumbering`, which stamps as
  it answers; both hosts dropped their copy and `dashscene-ffi` gained the
  report it never had. Do not re-do it. What the prompt said before, kept
  because the correction below is the useful part:

  > **The sequencing this prompt asserted was wrong, and #925 landing proved
  > it.** It said the stale-upload defect becomes real on Android the moment
  > #925 lands. It did not: the mapped load names the shown root once, at load,
  > and adds no symbol to change it afterwards, so `renumbered` can fire only on
  > the load's own commit — which `load_into` and `load_mapped_into` both report
  > `document_replaced` after. The issue itself is amended with this. It is a
  > de-duplication, orderable freely. The shape #945 suggests is a
  > `LiveScene::renumbered_since_shown()`, so the rule has one statement rather
  > than a copy in each host — the same move story #810 made for the frame
  > clamp.
- **#930 — the many-root document is rebuilt on every call.** Read the saving
  before spending on it: the issue states there is **none under nextest**, which
  runs one process per test, so a `LazyLock` is never shared. The regression
  tier and `just build` both run under nextest. The saving is real only for the
  two CI steps that use `cargo test`. Measure before and after on those, or
  decide it is not worth the memoisation.
- **#922 — the flatc install has no integrity check.** Needs a decision, and one
  input to it is **unchecked**: whether flatbuffers publishes a release
  signature. The issue's own option list says so. A hardcoded sha256 is not
  available — pull request #919 made the action derive its version from the
  workspace manifest, and a per-version checksum would put the version back in
  two places, which is the failure that PR removed.

## Traps this session hit, which cost real time

- **A restored file can re-run the stale binary.** `sed -i.bak` then
  `mv f.rs.bak f.rs` restores the original **mtime**, so cargo judges the
  artifact current and re-runs the mutated build. The revert reports a failure
  with the source visibly correct. The tell is the panic's line number
  disagreeing with the file. Revert with `git checkout --`, which writes a fresh
  mtime.
- **A background command's reported exit code is the last command's.**
  `just
  build > log; echo $? > status` notifies success whatever `just build`
  did. Write the status to a file and read that file.
- **A clean rebase is not a correct one.** Rebasing #947 applied nineteen
  commits with no conflict and then failed to build: `Atlas::new` had become
  fallible on `main`, and a changed signature is not a textual overlap.
- **`main` moves under you.** It moved four times during this session's work.
  Re-check before pushing and before merging, not only at the start.
- **CodeQL's `rust/access-invalid-pointer` fires on any FFI handle round-trip**
  in a test, and no rewrite that still dereferences the handle clears it —
  relocating the dereference under an `assert!` only moves the alert. It is
  dismissed on #978 and tracked as #979; do not spend a CI cycle rediscovering
  that.

## Environment

- `just verify`, the pre-push hook, runs **no test tier**. `just build` is the
  thorough local gate; quote its `Summary` line rather than a claim.
- `git push` hangs behind `git-credential-manager`. Use
  `git -c credential.helper='!gh auth git-credential' push`.
- Commit scopes are pinned in `.git-std.toml`. It is `docs(docs)`, never
  `docs(decisions)`.
- **Several sessions work this repository at once.** The stash stack is shared
  across worktrees, so never `git stash`; and `just secrets` scans every
  unpushed object across all local refs, so another branch's finding blocks your
  push. That happened this session and is why `.secrets-history-baseline` gained
  three entries in pull request #977.
