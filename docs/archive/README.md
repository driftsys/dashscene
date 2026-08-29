# archive

Raw specs and plans from `docs/wip/`, kept verbatim once their content has
been gardened into `docs/specification/`, `docs/design/`, `docs/decisions/`,
or `docs/technotes/`. Never deleted — this is the historical record of what
was actually planned, for comparison against what was actually built.

See the `sdd-working-memory-lifecycle` rule and the `sdd-gardening` skill.

## Local paths are replaced by `<worktrees>/` everywhere here

This is the one systematic edit made to the files in this directory. Every
absolute path into a developer's home directory has been replaced by
`<worktrees>/`, so a `git worktree add` line reads `<worktrees>/wt-lane-h-gpu`
rather than naming one machine. Nineteen files were affected, three of them
predating v0.20.

It is a departure from verbatim, recorded here because that is the contract.
The substitution is mechanical.

**A second departure, 2026-08-18.** One sentence in
`2026-08-15-v020-wave2-lane-G-android-ffi.md` read `does not obviously fix #960`,
where a closing keyword directly governs an issue number that is still **open**.
A pull-request body quoting that sentence closes #960 — which is not
hypothetical: #885 was closed exactly that way, from a body quoting a record. The
sentence now reads `is not obviously the answer to #960`; its meaning is
unchanged and nothing else in the file was touched. The v0.20 phase-end revision
made the change and `docs/decisions/slices-are-planned-against-their-inflow.md`
records the sweep that found it.

**No file tracked in this repository names a developer's home directory.** The
check is one command:

```sh
git grep -nE '/Users/|/home/[a-z]' -- . ':!.github/workflows/ci.yml'
```

What it should find is the CI runner's own path and nothing else:
`/home/runner/` quoted inside `2026-07-12-atlas-pipeline-plan.md`, matching the
copy in `ci.yml` that the exclusion skips — plus this section, which contains
the pattern it is describing. Anything beyond those is a developer's machine
leaking into a public repository, where it is noise to every other reader.

**Nothing enforces this**; it is prose. The three pre-v0.20 files that carried
such a path got there because nobody was checking, and prose will not stop the
fourth.

## The v0.20 lane prompts did not come from `docs/wip/`

Most of this directory came from `docs/wip/`, as the opening paragraph says —
though not all of it: a handful were written straight into `docs/archive/`,
among them `2026-07-14-scope-decisions.md` and
`2026-08-13-text-across-the-c-abi.md`. The `2026-08-15-v020-wave2-*` and
`2026-08-16-v020-wave3-*` files are a third case again. v0.20 ran its build
waves as parallel lanes, and those lane prompts were written and kept **outside
this repository** while the slice ran.

They are archived here anyway, for the reason this directory exists: they are
what was actually planned, against which what was actually built can be
compared. They are also the only record of how the slice was organised — the
territory rules that kept concurrent lanes out of each other's files, the
ordering constraints between them, and the post-merge check that most of them
carry, added after a merge silently reverted another lane's files twice.

**This paragraph also said the slice ran under no single driver prompt, and that
"nothing was ever held in `docs/wip/` to archive", and both are now contradicted
by an artifact.** `2026-08-13-v020-HARDENING-DRIVER-PROMPT.md` in this directory
is a v0.20 driver prompt, written the day epic #951 was filed and covering the
nine items that carried no hold. It was never committed, which is why the claim
held for as long as it did: it was true of git and false of the working tree. It
was recovered on 2026-08-29 — see "Four files here were recovered from
transcripts" below — and its own status line says it was held here and due for
archival at the slice's close. So epic #951's "this slice's driver prompt
archived" did have an artifact to name, and this is it.

**The set is partial and its shape is uneven.** Wave 2 archives four lanes
(D, E, F, G; lane G has three successive prompts). Lanes A to C ran earlier in
the slice and no prompt for them was kept. Wave 3's `phase-0-doc-link-gate` is
not a lane at all — it is the serialised gate that had to land before the wave
started, and it says so.

Read them as working memory, not as records. Several state facts that were true
when written and were corrected within the day. **Where one disagrees with
`AGENTS.md` or with a record under `docs/decisions/`, those are right** — that
matters most for the merge procedure, which every prompt here states in the
order that was current when it was written and which `AGENTS.md` has since
reversed. Prompts predating v0.20 carry the same instruction — how many depends
on how loosely you read "rebase then squash", which is why no count is given
here.

## Four files here were recovered from transcripts, and are not verbatim

The `2026-08-13-v020-*` and `2026-08-09-unity-*` files were reconstructed on
2026-08-29 from Claude Code session transcripts, after the checkout holding them
was deleted. **None of the four had ever been committed**, so nothing could be
recovered from git. They are the one departure from verbatim in this directory
that is not mechanical, and this section is that departure recorded, as the
local-path substitution above is recorded.

**What "not verbatim" means, file by file.** Three of the four replay cleanly:
the reconstruction replays the tool calls that wrote them and the result matches
what the transcript recorded. `2026-08-09-unity-host-integration.md` does not.
Two of its four recorded edits could not be placed, because the file had been
changed on disk outside the tool stream before the last of them, and those
changes are in no transcript. `2026-08-09-unity-host-integration.RECOVERY-NOTE.md`
beside it names both and reproduces their text.

**One consequence is worth stating, because it inverts a section.** The
unplaceable edits rewrote §5. The text held here is the version the author then
corrected: it proposes "reserved" and "fulfil" as new document vocabulary, and
the correction says most of it already exists and should use the placeholder
contract's terms — `contribution_id`, `declared_size`, `interim_fill`. So §5
reads as a proposal for something already built. The recovery note carries the
corrected text; `docs/design/architecture.md` ("Placeholders and node
replacement") and `docs/technotes/runtime-content.md` §7 carry the contract
itself.

**Why they are archived rather than gardened.** Their content is already carried
elsewhere, so there was nothing left to garden into a record:

- The **enablers plan** plans a data plane in `crates/dashscene-unity`. That
  crate was renamed to `dashpaint-abi` at story #1239 and kept only its
  boundary-B gate; the data plane landed in `dashscene-ffi` at story #859, under
  a structurally different contract — an enforced lease and a generational
  handle, where the plan assumed a documented lifetime and a raw pointer.
  `2026-08-20-ffi-data-plane.md` in this directory is that story's own plan and
  supersedes this one throughout.
- The **capture's** §6 open questions are all resolved or tracked: Q-6's cost is
  measured (#1128) with the count-to-budget conversion open at #549 and #1270;
  packaging settled at #1124 and #1125; the placeholder surface and its
  diagnostic built at #1126 and #1127; the Figma lowering open at #1264.
- Its one finding that had **not** been acted on is now #1383 —
  `unity-package-sited-in-this-repository.md` still describes the painter as
  projecting onto pre-instantiated GameObjects, which
  `unity-painter-uses-brg.md` reversed on 2026-08-18.

**The capture was never meant to land here.** PR #850, "capture the Unity
host-integration design as working memory", is closed and was never merged. What
is archived is therefore a document the repository had already declined once —
kept because it is what was actually thought on 2026-08-09, which is what this
directory is for, and not because it was ever adopted.

## A reference here may name a file that has since been renamed

These documents are verbatim, so a path they name is the path that existed when
they were written. `docs/technotes/` was renamed in 2026-08 — six notes lost a
date prefix from their filenames and three lost a process word — and the
references in this directory were deliberately **not** rewritten to match.

Rewriting them would make the archive read as though it had always named the
current file, which is the one thing this directory exists not to do. If a
reference does not resolve, the file was renamed; `git log --follow` on the
current name finds it.
