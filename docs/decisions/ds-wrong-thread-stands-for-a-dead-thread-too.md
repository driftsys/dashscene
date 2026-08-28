# DS_WRONG_THREAD stands for a minting thread that has exited as well as a live foreign one

    status   accepted (2026-08-23, owner's ruling on issue #1267 question 2).
             The ruling's whole obligation is a sentence in the C header, and
             that sentence landed with this record
    scope    crates/dashscene-ffi (the DsStatus contract and include/dashscene.h),
             and every host that branches on DsStatus. crates/dashscene-android
             is the host whose thread lifecycle makes the case reachable,
             though its own teardown does not reach it — see reason 3
    related  docs/decisions/the-c-abi-runtime-handle-is-generational.md (the
             handle layout that makes the two cases indistinguishable, and
             the additive-widening rule this leans on)
             docs/decisions/the-frame-crosses-under-a-lease.md (issue #1267's
             question 1, ruled 2026-08-19 and built by story #859)
             docs/design/c-abi.md (the threading rule)

## What was asked

Issue #1267 raised two questions while story #1226 was implementing the
generational handle. **This record answers the second one only.**

The first — whether a Unity host survives thread affinity when a
`BatchRendererGroup` culling callback reads the frame — is not open and is not
here. It was ruled on 2026-08-19 (borrowed views under a lease, never an owned
snapshot) and recorded in
[`the-frame-crosses-under-a-lease.md`](the-frame-crosses-under-a-lease.md); its
one residual, which thread Unity invokes `OnPerformCulling` on, was then
**measured** rather than ruled. Story #1125 read it on 2026-08-21 on
`6000.3.22f1` with URP and a real Metal device in a windowed player: the main
thread, `IsThreadPoolThread` and `IsBackground` both false. The reading, its
configuration and its limits are in
[`../technotes/unity-toolchain.md`](../technotes/unity-toolchain.md).

So the premise in issue #1267's own title — "Unity's off-thread culling
callback" — is refuted by measurement. The issue was not renamed, because the
title is also how it is found.

## The question this record answers

Under the layout story #1226 shipped —
`thread(20) | index(12) |
generation(32)`, thread numbers never recycled — two
situations are indistinguishable from inside the library:

- the minting thread is alive and this simply is not it;
- the minting thread has **exited**, so the handle can resolve on no thread ever
  again.

Both produce `DS_WRONG_THREAD`. The question is whether they should.

## Decision

**`DS_WRONG_THREAD` stands for both. The C ABI does not distinguish them.**

The obligation this creates is a documentation change rather than a code one:
**the header must say so explicitly** — that the status means "this call is not
on the owning thread", and that it does **not** report whether the owning thread
is still alive, so a host cannot infer recoverability from it.

That sentence is now in `crates/dashscene-ffi/include/dashscene.h` on
`DS_WRONG_THREAD` itself, which is what a host branches on. `DsRuntime`'s own
block already carried the mechanism and the reason, and is unchanged; the status
carries the consequence, so neither restates the other.

**What the header said before was worse than silent.** It read "The remedy is to
call from the thread that created it", unconditionally. Where the minting thread
has exited there is no such thread, so the one remedy the header offered is
exactly the inference the ruling forbids.

## The options, and what each costs

**1. One status, and say so in the header — chosen.**

- _Cost:_ a host that receives `DS_WRONG_THREAD` cannot tell a retryable
  situation from a permanent one, and may retry a handle that can never resolve.
  The header sentence is the whole mitigation.
- _Cost of being wrong later:_ none that is structural. Widening is additive and
  narrowing after a host relies on the narrow form is not
  ([`the-c-abi-runtime-handle-is-generational.md`](the-c-abi-runtime-handle-is-generational.md)),
  so choosing one status today leaves both options available.

**2. A distinct status for a dead minting thread.**

- _Cost:_ a process-wide registry of live threads — precisely the shared state a
  thread-affine table exists to remove — maintained on the path the frame loop
  uses. A recurring per-frame cost paid to serve an error path.
- Also not free at the ABI: a new status is additive in value, but a host that
  branches exhaustively meets a case it did not have.

**3. A query call a host makes only after receiving `DS_WRONG_THREAD`.**

- _Cost:_ the same registry, but the lookup lands on the error path rather than
  the frame path. Cheaper than option 2 in the steady state and identical in
  what it must maintain.
- Not built now, and it is the shape option 1 leaves available.

## Why option 1

1. **It forecloses nothing.** See the additive-widening rule above.
2. **The alternative costs the property the design exists to protect.** Both
   other options need the process-wide registry, and option 2 puts its
   maintenance on the frame path.
3. **Every host that meets this case today already knows the answer.** On
   Android a render thread ends per surface lifecycle, and it is the host that
   ends it. It does not need this library to report that a thread the host
   itself ended has ended.

   **And this repository's own Android host does not reach the case at all**,
   which is worth stating because it is the host a reader will check.
   `<DocumentFrames as Frames>::detach` (`crates/dashscene-android/src/host.rs`)
   calls `ds_runtime_detach_surface` and then `ds_runtime_free` **on the render
   thread itself** — its own comment says "this is the only thread that has ever
   touched it". It is reached from `LoopState::shut_down`
   (`crates/dashscene-android/src/machine.rs`), and the free function
   `loop_::destroy` (`crates/dashscene-android/src/loop_.rs`) joins that thread
   only afterwards, making neither call itself. So the handle is retired before
   the thread ends and never outlives it. That strengthens this reason rather
   than weakening it: the one host here that could meet the case has been
   written so it does not.

## What reopens this

**One concrete trigger: the first host that holds a handle across a lifecycle
event it does not itself drive.** At that point the host cannot know whether the
owning thread is alive, and the third reason above stops holding.

The fix then is additive — option 2 or option 3 — so nothing here has to be
designed twice, and option 3 is the one that keeps the registry off the frame
path.

## What this record does not claim

**It does not claim the two cases are indistinguishable in principle**, only
that this ABI does not distinguish them and that distinguishing them costs the
shared state thread affinity removes.

**It does not claim a handle outliving its thread is ordinary on Android
today.** An earlier draft of this record, the header, and
[`the-c-abi-runtime-handle-is-generational.md`](the-c-abi-runtime-handle-is-generational.md)
all said so, and this repository's own Android host refutes it — it frees on the
render thread before the join, as reason 3 records. All three are corrected;
`grep -rn "ordinary there" docs/ crates/` is the check that found the third
after the first two had been fixed, which is the class this instance belonged
to. What is true is narrower: a render thread ends per surface lifecycle, so a
host that keeps a handle past one meets the case as a matter of course. Whether
any host does is a property of that host, not a reading taken on a device.

**It does not touch question 1**, which was ruled and built elsewhere.

## How it is enforced

`the_header_says_ds_wrong_thread_does_not_report_whether_the_thread_lives`, in
`crates/dashscene-ffi/src/lib.rs`. The ruling's whole obligation is a sentence,
and nothing compiles a comment, so the test reads the committed header and
requires three clauses inside the block that documents `DS_WRONG_THREAD` —
bounded on both sides, so a clause that migrated elsewhere in the header does
not satisfy it. It then requires that **no sentence in that block offers a
remedy without the condition on it**, which is the other half: a header can
carry all three clauses and an unconditional remedy at once, and that is the
state the ruling replaced. The same three clauses are required of
`DsStatus::WrongThread`'s own Rust doc, because
[`../design/c-abi.md`](../design/c-abi.md) states the ruling once per audience
and a `cargo doc` reader never sees the C header. Its clause list is three of
its own: two are the same strings as the header's and the third differs, each
written for its own audience — so editing one audience to match the other's
wording breaks that one's assertion. The no-unconditional-remedy rule runs over
both.

**Confirmed by mutation**, not by passing. Deleting the "does NOT say whether
that thread is still alive" clause takes it red; so does moving every clause out
of that block into the file's top comment while leaving them in the file; so
does dropping the qualifier. Re-adding the removed sentence takes it red, and so
does a reworded unconditional remedy that no blocklist would have matched —
which is why the rule is over every sentence rather than over a list of
spellings. A pure re-wrap of the comment does **not** take it red, which was run
as well.
