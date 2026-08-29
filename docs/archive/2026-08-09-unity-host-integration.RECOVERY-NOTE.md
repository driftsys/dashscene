# Recovery note — 2026-08-09-unity-host-integration.md

Reconstructed on 2026-08-29 from Claude Code transcripts after the
`dashscene-unity-capture` and `dashscene-staging` checkouts were deleted. The
file was never committed to git.

The reconstruction replays the single `Write` and the four `Edit` calls
recorded for the file. Two of those four edits cannot be anchored, because the
tool result for the last one records that the file had been changed on disk
outside the tool stream. Those changes are not in any transcript and are not
recoverable. The two unanchored edits are reproduced below so their content
survives even though their exact placement does not.

## Unanchored edit recorded at 2026-08-09T09:53:38.212Z

Replacement text:

```markdown
   (`RENDER_TARGET_BUDGET`, `crates/dashscene-validator/src/lib.rs`). One measurement retires
   both. The per-root solve and committed-table cost of a multi-surface
   document belongs in the same pass, for the reason §4 gives: until the
   paint follows the shown root, a host pays for every surface every frame,
   and that cost is unmeasured too.
```

## Unanchored edit recorded at 2026-08-09T10:59:02.596Z

Replacement text:

```markdown
## 5. The reserved-node and overload pair

**Most of this already exists, and an earlier revision of this section
proposed it as new.** `docs/design/architecture.md` carries "Placeholders and
node replacement" as a reserved schema surface: `Node` already holds
`contribution_id`, `fragment_ref`, `declared_size` and `interim_fill`, added
append-only, and the record states that "node replacement is an
engine-painter-only concept, so it binds to the Unity painter row above as
well" — naming this exact case. The runtime contract is designed at
`docs/technotes/runtime-content.md` §7: a declared-size box that never hugs,
an `interim_fill` shown while content resolves, and a `contribution_id` a
runtime producer binds against. Four decision records already build on it.

So the vocabulary question is settled and this capture should use its terms —
placeholder, `contribution_id`, `declared_size`, `interim_fill` — rather than
the ones an earlier revision invented ("reserved", "fulfil"). What follows is
what that surface does **not** yet cover.

The split the existing contract already makes, restated so the rest reads:

- **The placeholder itself** is producer-agnostic document vocabulary. It
  says nothing here is authored to be final, and any host can read it.
- **Which object fills it** is host-specific and binds through
  `contribution_id`, outside the IR. Putting a host's object identity in the
  schema would make one producer's integration story part of the format,
  against P5.

`interim_fill` is also the answer to the "does the placeholder paint?"
question this capture asked as though it were open: the distinction between
_anchor_ (the box positions an object and the node still draws) and
_replaced_ (the host draws in its place) is `interim_fill` present or absent,
not a new pair of modes.

**What is genuinely not covered** is the diagnostic — nothing today reports a
placeholder no host filled, or a host covering a node that is not a
placeholder. That is the part worth a decision record, and it extends the
placeholder contract rather than standing beside it.
```
