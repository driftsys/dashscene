# The C ABI's runtime handle is a generational integer, in a thread-affine table

    status   accepted (2026-08-18, owner's ruling on issue #1226) — binds the
             C ABI's identity type and its threading model; lands in v0.21
             before story #859 and before any host is written against it
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

## Why now, and not at v1

**Two consumers hold the handle today**: the JNI layer in
`crates/dashscene-android`, which is Rust, and
`crates/dashscene-ffi/tests/abi.c`, which is this repository's own conformance
test. Every other match for `ds_runtime_` in the tree is a comment.

Story #1121 writes the Unity C# host on this slice, and it is **the first
consumer outside this repository's control**. iOS follows. `DS_ABI_VERSION`
moves either way; the question is only how many hosts pay, and the answer is
smallest now.

**It therefore lands before story #859**, which adds data-plane entry points:
written after this they take a handle by construction, written before it they
are surface this change has to revisit.

## What the implementing pull request must settle, and say why

Each of these was specified in an earlier draft of this record and got it wrong.
They need code and a test, not prose:

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
   (`DS_NULL_ARGUMENT`), and whether `ds_runtime_free` stays `void` — it is
   `void` today, so a double free cannot report itself.
5. **What happens when a counter wraps or exhausts**, for every counter in the
   handle. A wrapped value that hits a live slot is the failure this whole
   ruling exists to remove.
6. **What happens to a runtime still in an exiting thread's table**, including
   on the main thread, where `std`'s `LocalKey` documentation says pthread-based
   TLS destructors do not run.
7. **The threading rule stated once** in the header rather than on two of ten
   entry points, with `ds_runtime_tick` keeping a block that says how it departs
   — `docs/design/c-abi.md` records why it is the surprising member.

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

## Alternatives considered

**Keep the raw pointer and accept the exposure.** A legitimate outcome, and
rejected: the exposure it accepts is carried by hosts written in C#, Swift and
Kotlin by people who cannot diagnose it, and the price of reversing it rises
with each one.

**Defer past v0.21.** Rejected for the reason #1226 gives: deciding after three
hosts have shipped against the pointer is the outcome with no good version.

**Process-global table behind a mutex.** Rejected above.
