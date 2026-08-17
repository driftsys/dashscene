# Driver prompt — lane E: dashc, the PR #1010 review inflow

Run this with **Opus**. Everything below marked "Verified" was checked against
the tree on 2026-08-15 with `origin/main` at `557179b`. Everything marked "the
issue claims" was not — check it yourself before acting on it.

## Setup

    git worktree add <worktrees>/wt-lane-e-dashc -b debt/v020-dashc-prototype-inflow origin/main
    cd <worktrees>/wt-lane-e-dashc
    ./bootstrap

## What you own

Four issues, all filed by the `/code-review` fan-out on PR #1010:

    #1016  a dangling CHANGE_TO withholds the document for an instance of a set the file does not carry
    #1017  read_action classifies a CHANGE_TO from destinationId's presence, not from whether it resolves
    #1018  shown counts INSTANCE nodes nested inside component definitions, which paint nothing
    #1019  five of relativeTransform's six components are read by nothing

Read each with `gh issue view <n>` before editing.

**#1016 and #1017 are two ends of one seam.** #1017 says the classification
happens at the wrong layer — the reader decides from a field's presence, the
resolver knows the answer, and nothing carries it back. #1016 is what the
resolver's new severity then costs when the destination legitimately does not
resolve. Decide the seam once and both follow; fixing either alone will
probably move the defect rather than close it. Say in the PR body which of the
two you consider load-bearing.

## Verified facts — do not re-derive

- `read_action(action, trigger_lowers, found)` is at
  `crates/dashc/src/figma/prototype.rs:115` — a free function taking
  `&mut Interactions`. This is #1017's subject.
- **`shown` is not a function.** It is a local `BTreeSet<&str>` bound inside
  `variants::apply` at `crates/dashc/src/figma/variants.rs:130`, built from
  `paths(file)` filtered to `node.kind == "INSTANCE"` and mapped through
  `node.component_id`. It is passed as a `&BTreeSet<&str>` parameter, also named
  `shown`, into `Plan::diagnostics`. Do not go looking for `fn shown`.
- **The comment directly above that binding already states the intent #1018
  says is not achieved**: "A definition paints nothing, so a reaction on a
  master no instance shows costs the picture nothing and is named nowhere — the
  same reasoning `Walk::visit` uses when it fires no finding at all inside a
  definition." So #1018 is a gap between a stated intent and the code under it,
  which is this repository's most common defect. Quote that comment in the PR
  body and say whether your change makes it true or whether you corrected it.
- `is_definition(node)` at `crates/dashc/src/figma/variants.rs:243` is exactly
  `node.kind == "COMPONENT" || node.kind == "COMPONENT_SET"` — it matches the
  definition node itself and **not its descendants**. That is the whole of
  #1018's "neighbouring question"; it is verified, not conjecture.
- `paths(file)` is at `crates/dashc/src/figma/variants.rs:297`.
- `interaction_diagnostics` is at `crates/dashc/src/figma/variants.rs:258`.
- The two diagnostic names are constants in the same file:
  `UNSUPPORTED_INTERACTION = "figma.prototype.unsupported-interaction"` at
  line 94 and `UNSUPPORTED_MOTION = "figma.prototype.unsupported-motion"` at
  line 101. Grep the constant, not the string, when you sweep for uses.
- `EmitPolicy` is in `crates/dashc/src/lib.rs`, not in the `figma` module.
- `rest::Node::relative_transform` is `Option<[[f32; 3]; 2]>` at
  `crates/dashc/src/figma/rest.rs:282`; `rest::Node::turn` at line 374 is its
  only reader. Verified — #1019's central claim holds.
- `differs_beyond_overrides` is at `crates/dashc/src/figma/variants.rs:1020`,
  and already carries a comment about why `relative_transform` is not compared.
  Read that comment before deciding #1019 is a defect rather than a recorded
  trade.
- The governing record for #1019 is
  `docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`, which
  rules scale and skew out of the vocabulary deliberately. **If your answer
  changes what that record says, edit the record in the same PR.** On PR #954
  a change was made that an accepted decision record had already named as its
  deferred option, and the record was not read first.

## The file that is NOT banned

`crates/dashc/tests/prototype_lowering.rs` **is yours to edit.** An earlier
driver prompt carried a standing ban on touching it, copied forward from a
"blocked on PR #982" note that had already expired — and the tests that needed
fixing lived in that file, so the ban forbade the fix. The block is gone: #878
and #976 are closed, PR #1010 merged as `2fce40a`.

General rule this produced: **before honouring an ordering constraint you did
not verify yourself, check `git merge-base --is-ancestor`.** A "blocked on an
open PR" note becomes false silently.

## Existing test files, so you add to one rather than inventing another

`crates/dashc/tests/` holds `prototype_lowering.rs`, `figma_lowering.rs`,
`component_lowering.rs`, `bindings_lowering.rs`, `text_lowering.rs`,
`flex_lowering.rs`, `round_trip.rs`, `abi.rs`, `asset_table.rs`,
`image_id_gate.rs`, `vector_field_weld.rs`, and a shared
`crates/dashc/tests/common/mod.rs`.

## What to measure rather than argue

- #1016's population is a **cross-file instance of a published-library
  component set**. Before changing severity, build the fixture: `componentId`
  naming no local node, reactions echoed onto the instance. If the corpus has
  no such capture, say so and write a synthetic one rather than reasoning about
  what Figma would send.
- #1018's population is an `INSTANCE` nested inside a `COMPONENT` master that
  is itself never instantiated. Two levels, not one. A test with a single
  definition proves nothing.
- #1019: two members of a set whose matrices differ **in scale only** must
  currently compare equal. Show that they do before changing anything — if they
  already differ for some other reason, the issue's premise is wrong.

## Definition of done

1. `just test` between edits. `just build` green before pushing — quote its
   Summary line, do not paraphrase it.
2. **`just wasm`** — `dashc` builds to `wasm32-unknown-unknown` for the Deno
   importer, and that is the only gate that sees the wasm half.
3. If your diff reaches `importers/figma/`, run `just deno-check` and
   `just deno-test` too.
4. Push. **`just verify` may fail on the secrets gate for reasons that are not
   yours** — worktrees share one object store, so the scan sees every unpushed
   commit on this machine. Issue #987 is about exactly this gate.
5. Open the PR **as an ordinary PR, never a draft**.
6. Run `/code-review` on the PR **while CI runs, not after**. Capture every
   finding as a checklist in the PR description. Never drop one silently.
7. Fix all critical findings. File each minor one as its own `debt`-labeled
   issue linked to this work, **on the v0.20 milestone**.
8. **When a critical finding changes the implementation, review the fix too.**
9. In prose and commit messages write **`Refs #<n>`**. A closing keyword fires
   from commit messages that land on `main`, matches mid-sentence, takes only
   the first number, and a negated sentence matches just as well as a positive
   one. Put the one intended closing line on its own line at the end.
10. Before merging: `gh issue list --milestone "v0.20 — hardening: the critical
    findings and the Android recovery path" --state open` and read it.
11. Rebase onto the latest `main`, squash to one conventional commit,
    force-push, wait for `ci` green **on the commit you are merging**, then
    `gh pr merge --merge`. Merging is strictly serial: `main` requires an
    up-to-date branch and auto-merge is disabled.
12. After merging, `gh issue view <n> --json state` for every issue your commits
    named, not only those in the PR body.
13. **Before merging, read your own PR's file list** — `gh pr view <n> --json files`.
    Any path outside `crates/dashc/` is a stray, and a stray is how a merge
    reverts another lane.
14. **After merging, check the previous lane's work is still on `main`:**

        git diff --stat <previous-merge-sha> origin/main -- <that PR's files>

    An empty diff is the pass. **This has failed twice.** PR #1037 reverted
    PR #1038 across seven files it never edited, and `main` was missing work four
    issues read as `CLOSED` for 90 minutes (restored by PR #1063). Earlier,
    PR #978 dropped PR #961's `justfile` recipe the same way (#991). CI is green
    through this: the older content still compiles and still passes its own
    tests, because the reverted lane's tests went with it.

## Do not

- Do not touch any file outside `crates/dashc/` and the decision records you are
  correcting.
- Do not change a diagnostic's severity without checking what
  `docs/specification/` says the policy is. P4 is the rule: every out-of-profile
  construct is a named diagnostic, never a silent drop — but "named" and
  "withholds the whole document under Strict" are different answers, and #1016
  is precisely the argument about which one this case earns.
- Do not merge on a green `just verify` alone. It runs no test tier.
