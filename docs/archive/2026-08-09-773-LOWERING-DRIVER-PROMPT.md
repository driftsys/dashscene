# Story #773's lowering half — the variant table, from Figma

    status   live; hand this to a session as its first message. It is narrower
             than `2026-08-09-v018-DRIVER-PROMPT.md`, which is still the guide
             for the slice as a whole — that prompt says story #773 is what
             remains, and this is what remains of it.
    written  2026-08-09, after #882 landed the capture, the three rulings were
             taken, and one experiment was built and reverted. Everything
             specific below was checked against `main` at 91ca605 and the
             branch at f193b99. Stale the moment a story lands.
    empties  when #773 closes. Archive it verbatim to `docs/archive/` rather
             than gardening it — a driver prompt is spent the moment its work
             lands. Removing it from `docs/wip/` and editing
             `docs/wip/README.md` are one commit, not two.

Read `AGENTS.md` first. It holds the story workflow, the test tiers, the merge
method and the five principles, and it is authoritative over anything here.

## Where to work

This prompt and the decision-record amendment it refers to are **on `main`**.
Start a fresh worktree and branch the way `AGENTS.md` asks — `git worktree add`
before the first edit, then `./bootstrap`.

The worktree at `<worktrees>/wt-773-prototype`
was where this was written; it holds nothing the branch does not, so reuse it
or remove it as you prefer.

## What is already done, and what remains

**#773 splits in two, and the first half has landed.** Pull request #882
(merge `91ca605`) captured the fixtures and pinned the REST shapes: scope
bullets 1 and 3. What remains is bullet 2, the lowering.

Two committed fixtures are the whole input:

- `corpus/figma-fixtures/prototype-smart-animate.json` — the case that must
  lower. Ten non-empty `interactions` arrays, ten `SMART_ANIMATE` actions.
- `corpus/figma-fixtures/prototype-refused.json` — fifteen `refused-*` nodes,
  one per construct that must be named rather than dropped (P4).

`docs/technotes/figma-rest-shapes.md` §"The
prototype-interaction shapes" is the specification. Read it before the code;
it is more precise than this file and it was written from the payload.

## What is already ruled — do not re-derive these

Three rulings, taken 2026-08-09 and recorded in full in a comment on
issue #773. Take them as given.

- **The variant-set emitter folds into this story.** The Figma path emits no
  variant sets at all: `grep -rn "variant_sets\|VariantSet\|VariantMember"
  crates/dashc/src/figma/*.rs` returns nothing, and the walk skips
  `COMPONENT`/`COMPONENT_SET` whole. A `VariantTransition` nests on a
  `VariantMember`, so without the emitter a lowered interaction has nowhere to
  land. `docs/decisions/figma-component-lowering.md` deferred exactly this and
  called it "its own story with its own overridable-prop mapping" — this is
  that story, so **amend that record in place** rather than contradicting it
  silently.
- **Spring presets are refused by name.** `GENTLE`, `QUICK`, `BOUNCY` and
  `SLOW` arrive as a bare `{"type": "GENTLE"}` with no `easingFunctionSpring`,
  so mapping one onto `dashcue`'s `Spring { stiffness, damping_ratio }` would
  put numbers in the document that no producer supplied. Liftable when the four
  presets' parameters are measured and recorded. It costs two of the captured
  arms, plus `refused-bouncy`-shaped cases.
- **Paint channels stay out.** The widening was ruled, built, and **reverted**
  — see the next section. Lower rect diffs only; a fill diff is a named
  refusal.

## The one thing that was tried and reverted

**A variant transition cannot animate a paint prop, and the reason is
architectural.** `Arena::commit` resolves a node's paint from the variant
overlay _before_ the node's own staged value. A paint transition animates
between two members while the destination member is active, so every sample
the runtime stages is masked by the override it is travelling towards.
Measured: a `FillR` track over a half-second linear tween commits `0.0` on the
tick where an eased sample would be `0.75`.

A rect track has no such problem because commit takes geometry from an
**injected `LayoutSolver`** — a seam a runtime can write over, which is what
`FlipOverlay` does. Paint has no equivalent seam.

That is **issue #891**, which holds the three mechanisms considered and why
none is a bolt-on, and the reverted runtime sketch is saved with it. The
constraint is written into
`docs/decisions/motion-is-document-data-keyed-on-the-destination.md` by this
branch's one commit. **Do not re-attempt the widening inside #773.**

## The design, confirmed against the payload

Checked node by node against `prototype-smart-animate.json`, not inferred.

- The `COMPONENT_SET` holds two member `COMPONENT`s, `state=rest` and
  `state=active`, each with three children. They differ in exactly three
  nodes: `bar` in Width (64 → 288), `dot` in X (16 → 280), and `panel` in Y
  (96 → 88) and Height (32 → 76). That fan-out is deliberate — the technote
  says so — and it is what exercises one Figma spec becoming four tracks.
- Each `INSTANCE` carries `componentId` naming the member it currently shows,
  and its children carry the **same names** as the component's. So member
  children map onto instance children **by name**.
- Therefore: **one `VariantSet` per instance**, its `active_member` taken from
  `componentId`, its member overrides expressed against that instance's own
  child node indices. Components keep not painting, so story #242's rule
  survives untouched.
- The transition is **keyed on the destination member**, which is already
  #771's shape. `state=rest`'s interaction targets the active member with
  `EASE_OUT` / 0.3 s; `state=active`'s targets rest with `EASE_IN` / 0.2 s;
  each easing-arm instance carries its own.
- `VariantTransition.stagger` lowers to **0 always** from this producer.
  Figma has no stagger.

## Four traps the payload sets

Each is pinned in the technote and each would produce a wrong picture silently.

- **The duration is in seconds, nested; the flat field is milliseconds.**
  `interactions[].actions[].transition.duration` returns `0.30000001192092896`
  for a 0.3 s write. `@figma/rest-api-spec` documents the nested field as
  milliseconds, **which is wrong**. A lowering that trusted the comment would
  divide by 1000 and animate everything in under a millisecond, and both
  fields are `number`, so nothing would object.
- **Never read the flat triple.** `transitionNodeID` / `transitionDuration` /
  `transitionEasing` are lossy and partly fabricated: they cannot express the
  trigger, the navigation, the transition type or a second action, and where
  the interaction says there is no transition the triple invents one
  (`refused-on-key-down` carries `"transition": null` inside and
  `transitionDuration: 300` outside).
- **An instance echoes an inherited interaction in full**, so read reactions
  off the node being walked and never resolve back through the component set.
- **`Reaction.action` never appears.** REST emits `actions` only; no fallback
  for the deprecated singular is needed.

## What the work is

1. **Lower `COMPONENT_SET` into `VariantSet`** — the design above. This is the
   bulk, and it is what `figma-component-lowering.md` deferred.
2. **Diff the members into per-prop tracks**, rect channels only.
3. **Read `interactions`** for the destination member, the duration and the
   easing, and attach a `VariantTransition` keyed on the destination.
4. **Refuse fifteen constructs by name**, one per `refused-*` node, plus the
   spring presets. `crates/dashc/src/figma/triage.rs` is the existing shape for
   a named refusal.
5. **Amend `docs/decisions/figma-component-lowering.md`** — this is the story
   its alternatives section anticipated.

## Check the scope against the code before writing any of it

Every story in v0.16 was smaller or differently shaped than its body said, and
all four in this slice were too: #770 was half-built already, #832's central
trade-off did not exist, #852's stated cost was not real, and #771 turned out
to need an emitter that did not exist. **This story has already done it once**
— its own "read the reactions the importer already fetches" turned out to
require a variant-set emitter first.

## CI is down for billing, and will not be fixed

`changes`, `dprint` and `fmt` fail with **zero steps** and every other job
skips behind them. The reason lives on one endpoint and nowhere else:

    gh api /repos/{owner}/{repo}/check-runs/<job-id>/annotations \
      --jq '.[] | "\(.annotation_level): \(.message)"'

Every `failure` in this state says nothing about the code. Merge on local
evidence — `just build`, plus `just calibrate` when the diff touches any path
in the `packer` filter — and record the exception on the pull request.

`cargo audit` also fails, on a corrupted upstream advisory database
(`duplicate advisory ID: RUSTSEC-2026-0244`). Untouched `main` fails
identically. The pre-push hook runs it, so pushing needs `--no-verify` after
the regression tier and lint are green. Say so on the pull request.

## The loop

1. Read issue #773 and **every comment on it** — the three rulings live only
   there.
2. Work in the existing worktree; do not start a new one.
3. Check the scope against the code before writing any of it.
4. Implement.
5. `just build`. `just lint` also gates intra-doc links, which clippy does not
   resolve.
6. Open the pull request **ready, never a draft**. Name the tiers actually run
   and record both CI exceptions.
7. **Run `/code-review` and mean it** — the fan-out, not an author pass. It
   returned thirteen findings on #772's pull request and the author pass had
   found none of them.
8. Capture every finding as a checklist in the description. Fix criticals
   inline; file one `debt` issue per minor finding.
9. Before merging, re-read the milestone's open issues. Then
   `gh pr merge --merge`, delete the branch, remove the worktree, comment the
   outcome on #773, and archive this prompt.

## A concurrent session works this repository

It landed #882 — this story's own first half — between the moment #773 was
described as blocked and the moment the lowering started, and it filed #872
the same evening. **Re-read the milestone before pressing merge, not only at
the start**, and check `git config --get remote.origin.url` before any fetch,
reset or push.

## What closing this closes

Epic #769's last story. All three of its gaps are built — a rotation channel
(#770, #832), motion rows (#771, #617) and loop tracks (#772) — so #773 is the
only thing between v0.18 and its close, with debt #845 and #891 carried
forward. The phase-end revision follows, and it re-checks `docs/features.md`
against the code rather than against the records.
