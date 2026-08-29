# The C ABI's runtime handle is a generational integer, in a thread-affine table

    status   accepted (2026-08-18, owner's ruling on issue #1226) — binds the
             C ABI's identity type and its threading model. Built by pull
             request #1268 and on `main`; decision 3 below carries the
             schedule, and "What the implementation answered" carries what
             the code says
    scope    crates/dashscene-ffi (the header, the entry points, DS_ABI_VERSION),
             crates/dashscene-android (the JNI consumer),
             docs/design/c-abi.md (the threading rule)
    related  docs/design/c-abi.md (the threading rule this changes)
             docs/decisions/host-integration-in-three-layers.md (the layering
             this ABI carries)

## What was asked

Issue #1226 asked for a ruling on three things, and this record answers those
three and **deliberately no more**. An earlier draft specified status names, a
bit split, null semantics and thread-exit behaviour; four review rounds found
real defects in each, because they are implementation design being written
against code that does not exist yet. They are handed to the implementing pull
request below, as questions it must answer rather than answers it must carry
out.

The line between the two is a **property** against a **mechanism**. Decision 2's
handle-uniqueness clause is a property, added after the cut that produced this
shape left it promised and unbacked; the bit split that delivers it stays a
question.

## Decision

**1. A runtime is identified by a generational integer, not by a pointer.**

    typedef uint64_t DsRuntime;    /* an opaque handle, not an address */

Rust keeps runtimes in a slot table; every entry point becomes a lookup rather
than a dereference. **The property this buys is the whole point of the ruling: a
handle that is no longer valid, or that is used wrongly, produces a `DsStatus`
the host can act on instead of undefined behaviour.** A forged or
arithmetic-derived value is detected for the same reason; a bad pointer cannot
be checked at all.

**2. The table is thread-affine**, one per thread, rather than process-global
behind a lock, and **a handle value identifies at most one runtime for the life
of the process**.

Handle uniqueness is part of the ruling, not a mechanic left to the
implementation, because thread affinity does not give it on its own. If slots
and generations are both numbered per thread, each thread's first runtime takes
the same handle value, so a handle from one thread can resolve against another's
table and drive a runtime the caller did not name — worse than the pointer it
replaces, because it looks like it worked. What makes handle values unique is
question 1 below; that they are is not open.

**3. It lands in v0.21, before story #859.**

## Why thread-affine rather than global behind a lock

- **It enforces the rule the ABI already states.**
  [`../design/c-abi.md`](../design/c-abi.md) says a caller must guarantee that
  no other call is in flight on the same runtime. Thread affinity, with decision
  2's uniqueness clause, turns a violation of that rule into a status rather
  than undefined behaviour — the same upgrade decision 1 makes for a stale
  handle. Neither gives it alone.
- **`DsRuntime` is neither `Send` nor `Sync`**, and nothing in
  `crates/dashscene-ffi/src/lib.rs` makes it either — `LiveScene` holds
  `Box<dyn LayoutSolver>` and boxed closures. A process-global table hands a
  non-`Send` value across threads, so it is not a contained change; it is a
  change to what the runtime is.
- **No lock on the frame path.** `ds_runtime_draw` runs per frame on the target
  hardware. What a lock would cost there is unmeasured, and this record does not
  claim a figure.

**It costs the existing host nothing.** Every `ds_runtime_*` call in the Android
host is in `impl Frames for DocumentFrames`, and `render_thread` invokes the
`frames` factory itself, so the runtime is created and used on one thread.

**And it is the narrower promise.** Widening later — a runtime that may move
between threads — is additive; narrowing after a host relies on it is not.

**This section argues from avoiding process-wide state, and decision 2's
uniqueness clause requires some** — "for the life of the process" cannot come
from per-thread counters. The two are reconciled rather than in tension, and
"What the implementation answered" below is where the mechanism that makes both
true is named. Read it before optimising this section's stated goal on its own:
that is exactly the implementation decision 2 forbids, and it has a test.

## Why now, and not at v1

**The handle is already held here**: by the JNI layer in
`crates/dashscene-android`, which is Rust; by
`crates/dashscene-ffi/tests/abi.c`, this repository's own conformance test; and
since story #1121 by the Unity C# host under
`unity/com.driftsys.dashscene/Runtime/`, which stores it as
`DashsceneRuntime._handle` and whose declarations `unity/ffi-check` executes.
Re-derive with `grep -rl ds_runtime_` rather than trusting this list, which has
been stale once — and note that it carries no count, because a count is the part
that goes stale.

The C# host is **not** outside this repository's control — the package is sited
here (`unity-package-sited-in-this-repository.md`), so they move together. iOS
in v1 is the first that will not. `DS_ABI_VERSION` moves either way; the
question is only how many hosts pay, and the answer is smallest now.

**It therefore lands before story #859**, which adds data-plane entry points:
written after this they take a handle by construction, written before it they
are surface this change has to revisit.

## What the implementing pull request must settle, and say why

Each of these was specified in an earlier draft of this record and got it wrong.
They need code and a test, not prose — with one exception, named here rather
than left as a contradiction a reader has to notice: **where decision 2 already
rules a candidate out, the question says which**. A question that leaves a
forbidden answer open is not a smaller record, it is an incomplete one.

**They were answered**, by pull request #1268, and the section after this one is
what the code says. The questions are kept as they were asked on 2026-08-18, so
this record shows what was open rather than a surface with no seam in it:

1. **The bit split**, and what each field's width bounds — subject to decision
   2. Two candidate layouts fail it, for the reason decision 2 gives: a
   per-thread slot index with a per-thread generation, and a thread field drawn
   from a registry that recycles thread numbers. Answered with question 5, which
   asks what happens when whatever supplies the uniqueness runs out.
2. **Whether a stale handle and a foreign-thread handle report the same status
   or different ones.** They have different remedies — stop using it, versus
   call from the owning thread — but a handle outliving its own thread is
   arguably the first rather than the second, and that case is ordinary on
   Android, where the render thread is joined per surface lifecycle.
3. **What the `DsStatus` additions are called**, appended at the tail as the
   header requires. Not `DS_INVALID_HANDLE`: `DS_UNSUPPORTED_HANDLE = 5` already
   exists and means an unsupported surface handle kind.
4. **What a reserved zero handle means at every entry point**, against what
   `docs/design/c-abi.md` and `abi.c` promise for a null pointer today
   (`DS_NULL_ARGUMENT`), and whether `ds_runtime_free` stays `void` — it was
   `void` when this was asked, so a double free could not report itself. It
   returns a `DsStatus` now; the answer below is where that is recorded.
5. **What happens when a counter wraps or exhausts**, for every counter in the
   handle. A wrapped value that hits a live slot is the failure this whole
   ruling exists to remove.
6. **What happens to a runtime still in an exiting thread's table**, including
   on the main thread, where `std`'s `LocalKey` documentation says pthread-based
   TLS destructors do not run.
7. **The threading rule stated once** in the header rather than on two of ten
   entry points, with `ds_runtime_tick` keeping a block that says how it departs
   — `docs/design/c-abi.md` records why it is the surprising member.

## What the implementation answered — story #1226

The questions above asked for code and a test rather than prose. This is what
the code says; each line names the test that holds it.

**The reconciliation the record did not state.** Its rationale argues for thread
affinity by avoiding process-wide state, while decision 2 requires a handle to
be unique for the life of the _process_ — which needs process-wide state. Both
hold because **uniqueness is a property of how a handle is minted, not of how it
is resolved**. One `AtomicU32` mints a thread number on each thread's first
`ds_runtime_new` and is read by no lookup and no frame-path call; everything
else is a `thread_local!`, which needs no lock because it is reachable from one
thread only. An implementer optimising the rationale's stated goal builds
per-thread counters and gives two threads the same first handle —
`two_threads_first_handles_are_different_values` is that defect's test, and it
was watched failing against exactly that implementation before this one was
written.

- **Question 1, the bit split** — `thread(20) | index(12) | generation(32)`. The
  thread field is the wide one because an Android host creates a render thread
  per surface lifecycle, so that is the counter that grows without bound;
  concurrently-live runtimes per thread do not (`handle.rs`,
  `no_field_bleeds_into_a_neighbour`).
- **Question 2, the statuses** — `DS_BAD_HANDLE = 16` for a value this thread
  has retired or never minted, and — since the checkout that keeps the ABI
  re-entrant — for one whose runtime is checked out by a call already in flight,
  which no host can reach until an entry point calls back into host code.
  `DS_WRONG_THREAD = 17` for a handle naming another thread's table. Appended,
  so nothing renumbers
  (`appending_a_status_is_free_and_the_handle_change_was_not`).
- **Question 3** — not `DS_UNSUPPORTED_HANDLE`, which already means an
  unsupported _surface_ handle kind and would collide.
- **Question 4, zero** — `ds_runtime_free(0)` is `DS_OK` and does nothing,
  standing exactly where `free(NULL)` stood; every other entry point answers
  `DS_NULL_ARGUMENT` for `0`, which is the answer a null pointer got. So no
  documented behaviour changes shape (`zero_names_no_runtime`, and `abi.c`).
- **Question 5, exhaustion** — a slot whose generation reaches `u32::MAX` is
  **retired**, not wrapped, and a full table refuses with `DS_HANDLES_EXHAUSTED`
  rather than overwriting (`a_full_table_refuses_rather_than_overwriting`).
- **Question 6, what happens to a runtime still in an exiting thread's table** —
  it is **leaked, deliberately**. Dropping it would run `wgpu::Surface`'s
  destructor at thread-exit time, which on Android is after `surfaceDestroyed`
  returned and `ANativeWindow_release` ran — a use-after-free of the window that
  the old design could not commit, since an unfreed handle was a `Box` that was
  simply never dropped. `Table`'s `Drop` forgets its runtimes, so the behaviour
  is exactly what it was: a host that does not free leaks, and cannot do worse
  than leak. The question also notes that pthread-based TLS destructors may not
  run on the main thread at all, which is a second reason no host may rely on
  this path for teardown.
- **What a handle reports from a thread that has since exited** — the other half
  of question 2. `DS_WRONG_THREAD`, the same as one from a live foreign thread.
  The two are **not** distinguished: telling them apart needs a process-wide
  registry of live threads, the shared state this design removes. A render
  thread ends per surface lifecycle on Android, so a host that keeps a handle
  past one meets this case as a matter of course — **though this repository's
  own Android host does not**, freeing on the render thread before the join;
  [`ds-wrong-thread-stands-for-a-dead-thread-too.md`](ds-wrong-thread-stands-for-a-dead-thread-too.md)
  reason 3 carries the refutation. Question 2 above states the case as it was
  asked, which is why it still reads the older way. Issue #1267 carried it for a
  ruling and **it was ruled on 2026-08-23**: one status for both, with the
  header saying that it does not report whether the owning thread is alive
  ([`ds-wrong-thread-stands-for-a-dead-thread-too.md`](ds-wrong-thread-stands-for-a-dead-thread-too.md))
  (`a_handle_from_another_thread_is_wrong_thread_and_drives_nothing_local`).
- **Question 4's second half, `ds_runtime_free`'s signature** — it returns
  `DsStatus` now. It can report, and a double free is `DS_BAD_HANDLE` rather
  than undefined behaviour, which is the property this whole record exists for.
- **Question 7, the threading rule stated once** — the header carries both the
  thread-affinity rule and "no other call in flight on the same runtime" on
  `DsRuntime` itself, rather than on two of ten entry points, and
  `ds_runtime_tick` keeps a block saying how it departs. The in-flight rule is
  now enforced rather than only asked for: a re-entrant call on the same handle
  answers `DS_BAD_HANDLE` instead of aliasing the runtime, and one on a
  different runtime resolves
  (`a_re_entrant_call_on_the_same_runtime_is_refused`,
  `a_re_entrant_call_on_another_runtime_resolves`).

`DS_ABI_VERSION` moved 1 → 2. Ten of the twelve exported entry points then
changed signature; `ds_abi_version` and `ds_last_error_message` take no runtime
and did not. The surface has grown since — story #859 and story #1124 — and
neither addition changed a signature, so neither moved the number again.

## What this is not

**Not an `unsafe` reduction.** `crates/dashscene-ffi/src/lib.rs` holds 154
occurrences of the word on `main` at `9a1e3c78`, but **103 are inside
`#[cfg(test)]`**. What the change removes is **nine** dereferences: eight
`&mut *runtime` in the entry points and `ds_runtime_free`'s `Box::from_raw`. It
touches none of the byte, font, window, path or out-pointer arguments the ABI
must still take. **The crate does not become safe.** Issue #1226 says 11 of 154,
and both figures mislead.

**Not a CodeQL fix.** All three `rust/access-invalid-pointer` alerts are
dismissed with consistent reasoning and CodeQL is not a required check, so there
is no live cost to buy off. #1226 also misreads issue #979 as recording that its
proposed fix "made the alert worse"; #979 says the fix did not _clear_ the alert
but improved the position — one dismissal at one site instead of one per test —
which is why PR #1223 kept the helper.

## Amendment, 2026-08-23 — the framing repaired, and the restatements checked

Issue #1266 filed seven statements about this ruling as stale or wrong across
six files, and three framing tensions inside this record. It asked for one pass
that re-derives every claim against the tree before rewriting it, and treats
deletion as the first option. This is that pass, and re-deriving mattered: some
of what the issue lists was already correct on `main`, and two statements it
does not list were wrong.

**Inside this record.** These repair the record's own reasoning rather than its
typing, so they are named rather than smoothed over:

- **The status line's "before any host is written against it" is deleted.** It
  named an event that had already not happened — the JNI layer and
  `crates/dashscene-ffi/tests/abi.c` held the handle when it was written.
  Deleted rather than corrected: decision 3 states the schedule, so the clause
  carried nothing this record needs.
- **"Why thread-affine rather than global behind a lock" now says that decision
  2's uniqueness clause needs process-wide state**, and points at the
  reconciliation below it. The tension the issue names is real; the answer was
  already in this record, and the reading order was the defect. An implementer
  who optimises that section's stated goal alone builds the thing decision 2
  forbids.
- **The questions' preamble says where prose is allowed.** It read "they need
  code and a test, not prose" while question 1 carries prose ruling out two
  candidate bit splits — correctly, because decision 2 forbids them. The
  exception is stated rather than left as a contradiction, and the preamble now
  says the questions were answered and by what.
- **Question 4 no longer says `ds_runtime_free` is `void` "today".** It was when
  the question was asked, and the answers section records that it returns a
  `DsStatus` now.
- **The list of who holds the handle carries no count**, and names
  `unity/ffi-check`. It said "three consumers" beside an instruction to
  re-derive with a grep that now finds a fourth holder. The count is the part
  that goes stale, which is the lesson
  [`../design/c-abi.md`](../design/c-abi.md) already applied to its own panic
  section.

**Outside this record, corrected:**

- [`README.md`](README.md)'s index entry claimed this record "corrects #1226's
  figures for both". True of the `unsafe` count and not of CodeQL, where what it
  corrects is #1226's account of issue #979 — so the clause is deleted rather
  than qualified. That entry also read in the future tense and omitted the
  handle-uniqueness property.
- [`../roadmap.md`](../roadmap.md)'s "changes the signature of every entry point
  the data plane would add to" is reworded. **Read whole it was right** — its
  subject is the entry points the data plane would add — and the issue quotes it
  truncated, which is a reading the wording allowed. It now says "decides the
  signature of every entry point the data plane would add".
- [`host-integration-in-three-layers.md`](host-integration-in-three-layers.md)
  said thread affinity makes a call from another thread "a diagnosable bad
  handle". Two defects in one clause: the diagnosis comes from decision 2's
  uniqueness rather than from affinity, and the status is `DS_WRONG_THREAD`
  rather than `DS_BAD_HANDLE`, which this ABI defines as a different thing.

**Outside this record, found already correct:**

- The "Why now, and not at v1" section's account of the Unity C# host, which the
  issue reports as calling it "the first consumer outside this repository's
  control". It says the opposite, and has since story #1121's pull request.
- [`slices-are-planned-against-their-inflow.md`](slices-are-planned-against-their-inflow.md),
  which the issue reports as saying "every entry point" and which already said
  "ten of its twelve". Only the singular "entry point" and the bare "twelve"
  changed, so that number does not read as today's count.
- [`../design/c-abi.md`](../design/c-abi.md)'s Versioning section, which the
  issue reports as reading "**1** and has never moved". It reads 2, and says
  when it moved, since pull request #1268.

**Two statements the ticket does not list**, both in
[`../design/c-abi.md`](../design/c-abi.md)'s panic boundary and both falsified
by this ruling rather than by anything else: `ds_runtime_free` was described as
catching an unwind directly with no status to report it in, and the number of
entry points holding that property without `guard` was given as two. Giving that
call a `DsStatus` moved it under `guard`. One entry point holds the property
without the helper, and it is `ds_last_error_message`.

**And the issue's own figures were stale**, which is the failure it exists to
correct rather than to repeat: it says `dashscene-ffi` exports twelve
`extern "C"` functions and that ten of them change. Twelve was the count when
this ruling landed, which is what the answers section says; fifteen are exported
today and thirteen take a runtime, because the three added since — by story #859
and story #1124 — were written taking one rather than changed to.

## Alternatives considered

**Keep the raw pointer and accept the exposure.** A legitimate outcome, and
rejected: the exposure it accepts is carried by hosts written in C#, Swift and
Kotlin by people who cannot diagnose it, and the price of reversing it rises
with each one.

**Defer past v0.21.** Rejected for the reason #1226 gives: deciding after three
hosts have shipped against the pointer is the outcome with no good version.

**Process-global table behind a mutex.** Rejected above.
