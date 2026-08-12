# dashcue's Scheduler keeps `Vec` track storage

    status   accepted (debt #488, 2026-07-29)
    scope    crates/dashcue/src/scheduler.rs; binds docs/design/dashcue.md
             and any future change to Scheduler's track storage

## Context

`Scheduler` holds live animation tracks in `tracks: Vec<Track>`. `start` scans
it linearly to find a live track to retarget, removes that entry (`Vec::remove`,
an O(n) shift) on a hit, and pushes the new or retargeted track to the back;
`start_transition` calls `start` once per declared track; `sample` scans
linearly for one key. The struct's own doc comment carried a note scheduling a
storage revisit at v0.8, "with the stress corpus" — v0.8 closed, and so did v0.9
through v0.12, without the revisit happening (debt #488).

Since #77 closed, `samples()`'s iteration order is pinned as a real guarantee,
not an artifact of whatever storage happens to back it: live tracks must come
back in start order, with a retarget re-entering at the back. `dashlang`'s
reactive drive and the engine's FLIP frame output both emit in that order, and
the E5/E6 goldens depend on it being deterministic. Any storage change must
preserve that order deliberately, not incidentally.

Debt #488 is explicit that this is not a cost argument: nothing has measured the
linear scan as a problem, and issue #69 (duplicate-`PropKey` detection) and
issue #77 (the pinned order) are what made the stale note load-bearing enough to
need an answer rather than another slice of silence.

## Options

1. **Revisit the storage.** Replace `Vec<Track>` with a structure that gives the
   live-track lookup and the retarget path better-than-linear cost, while still
   honoring the #77 order guarantee.
2. **Keep `Vec`, and record why.** Close the stale note by writing down that the
   linear scan is the deliberate choice, not a deferred TODO, with the ordering
   guarantee as one of the reasons.

## Choice

Option 2.

## Why

- **No measurement exists.** A `VariantTransition` declares a handful of tracks
  in practice, and nothing in the stress corpus, a golden run, or any profile
  has driven `Scheduler` to a track count where an O(n) scan is visible.
  `docs/decisions/pre-v1-hardening-slice.md`'s rule for exactly this shape of
  debt — "resolvable is not the same as measurable" — sends unmeasured perf debt
  to the v1 performance epic (#476), not into a v0.13 storage rewrite. #488 says
  as much itself: argued on cost alone, this belongs with #476's other
  unmeasured perf debt, not here.
- **The realistic replacements don't clear the ordering bar for free.** A plain
  `HashMap<PropKey, Track>` gives O(1) lookup but has no defined iteration order
  at all — swapping to it would silently break the #77 guarantee the first time
  a hash happens to order two keys differently from their start order, which is
  exactly the failure #77 exists to prevent. An order-preserving keyed structure
  (an insertion-ordered map, or a hash map paired with a linked list) keeps
  `samples()` correct, but "retarget re-enters at the back" still means removing
  an arbitrary entry and reinserting it at the end — the same O(n) shift
  `Vec::remove` already pays today, just spent inside a structure that costs
  more code to bring in and more code to audit. The retarget path is the one
  place storage choice matters here, and none of the realistic replacements make
  it more than incrementally cheaper.
- **`Vec`'s ordinary push/remove is what gives `samples()` its start-order
  guarantee for free.** That property is exactly what #77 later pinned as
  semantics. Keeping the storage keeps the property automatic instead of
  something a replacement has to re-derive and re-test from scratch.

Reopen if the stress corpus or a real document is ever measured driving
`Scheduler` to a track count where the linear scan shows up in a profile. At
that point the question has a number behind it, which is what #476's entry
condition asks for.
