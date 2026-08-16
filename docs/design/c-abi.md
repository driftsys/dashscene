# The C ABI: `dashscene-ffi`

    status  as-built at the v0.19 close (2026-08-16). Story #840 built it;
            stories #841 and #947 and issues #925, #945 and #884 changed it
            afterwards, and this record describes the result rather than
            any one of them.
    source  story #840, epic #833. Amended by #841 (the detach call), #947
            (fonts), #925 (the mapped load), #945 (the renumbering gate)
            and #884 (surface loss).
    why     [`../decisions/host-integration-in-three-layers.md`](../decisions/host-integration-in-three-layers.md)
            D2 decides that one C ABI exists and that every platform host
            sits on it; this record carries what was built.

The seam a non-Rust host reaches the runtime through when it **loads a
document** rather than building a scene in code. Kotlin reaches it by JNI today;
the iOS and Unity hosts that follow inherit it rather than getting their own —
**for control**. Unity draws the frame with its own renderer, and nothing here
carries a boundary-B row for it to draw, which is issue #859 and is open on the
same slice Unity lands in.

    a Java host --JNI--> dashscene-android --> dashscene-ffi --> dashscene-gpu
                                                        |
                        (iOS, Unity: later, same seam) -+

`demo-android` is **not** on that path: a scene built in code needs an `Arena`,
and this ABI's lives inside an opaque `DsRuntime`, so it links
`dashscene-android`, `dashscene-gpu`, `dashpaint`, `dashscene-core` and
`dashlang` directly — `dashpaint` for the `Painter` trait it drives and
`dashlang` for the frame gate it reads, both of which the ABI would otherwise
have owned for it. The JNI boundary is between the Java host and
`dashscene-android`; that crate reaches `dashscene-ffi` as an ordinary Rust
dependency.

**It is a control plane and not a data plane.** Everything below moves a
document, a surface, a clock and a status. No entry point takes or returns a
boundary-B row, so a host that wants to draw the frame itself has nothing to
call — that is issue #859, named here rather than left to be discovered.

## The twelve entry points

Four groups, and the grouping is the lifecycle:

    ds_abi_version                       what this library implements
    ds_last_error_message                the last failure, as text

    ds_runtime_new                       make a runtime
    ds_runtime_free                      drop it

    ds_runtime_load_document             bytes
    ds_runtime_load_document_with_text   bytes + font faces
    ds_runtime_load_document_mapped      a path + fonts, bounded by a root

    ds_runtime_attach_surface            a platform handle becomes a target
    ds_runtime_detach_surface            drop the target, keep the document
    ds_runtime_resize                    the target's extent changed
    ds_runtime_tick                      advance the scene by dt
    ds_runtime_draw                      paint the committed frame

`ds_runtime_detach_surface` exists because of D4 rather than symmetry. The
Android host must drop its surface and keep its document when `surfaceDestroyed`
arrives, and freeing the runtime would drop both. Story #841 found that by
driving the ABI as a C caller, which is also what established the rest of it was
sufficient for layer 0.

## Three loaders, and why they are three rather than one

They differ in what the caller can promise, not in what they produce.

- **`ds_runtime_load_document`** takes `(ptr, len)` and owns every payload it
  copies. The caller keeps no lifetime rule and gets no bound: an owning load
  copies each payload into an `ImageAsset` whether or not anything draws it.
- **`ds_runtime_load_document_with_text`** is the same load plus an array of
  `DsFontFace`. Added at story #947, because without it the ABI could load a
  document containing text and draw **zero glyphs** — the solver had no
  typesetter and nothing said so.
- **`ds_runtime_load_document_mapped`** takes a **path**, a **required**
  `ShownRoot` ordinal **and the same `(faces, face_count)` pair as the load
  above** — it carries fonts too, and a caller passing `(NULL, 0)` simply has
  none. It maps the file and reads out of its cold half only the assets that
  root's subtree draws. Added at issue #925, which is where R5 first reached
  this ABI at all.

**The root selection sits on the mapped load and on no other**, deliberately.
The two byte-taking loaders own every payload already, so a bound there would be
accepted and change nothing measurable — and would have cost an ABI version for
it. A new symbol is free under the rule below; a changed signature is not.

**The root is named once, at load.** No call changes it afterwards, so a host
showing a different artboard loads again.

## Versioning: a new symbol is free, a changed signature is not

`ds_abi_version` returns `DS_ABI_VERSION`, which is **1** and has never moved.
It is not the crate's version — it is what this library implements.

`DsStatus` has grown from nine variants to sixteen without moving it, because
every addition went on the **tail**: `FontFace` and `Atlas` at #947, then `Map`,
`NoSuchRoot`, `Derived` and `Payload` at #925, then `SurfaceLost` at #884. A C
caller compiled against an earlier header still reads every discriminant it knew
at the value it knew.

That is why the shapes above were chosen: three loaders rather than one with a
widening signature, and a detach call rather than a flag on free.

**It is not the whole of the rule, and `SurfaceLost` is the case that shows the
gap.** That variant did not only appear at the tail — it **re-routed an existing
condition**, a lost swapchain that used to arrive as `DsStatus::Surface`. A host
built against an earlier header meets a value it does not recognise and stops,
losing a recovery it previously had. Its own doc comment says so and says the
rule does not yet price a re-routed condition. **Adding a variant is free;
moving a condition onto it is not**, and nothing in the rule as written stops
the next one.

## What a caller must guarantee

- **No other call may be in flight on the same runtime.** The header states this
  on `ds_runtime_detach_surface` and on `ds_runtime_tick`, and nowhere else —
  **not** on `ds_runtime_attach_surface`, whose block says nothing about
  concurrency. `ds_runtime_free`'s Rust safety documentation carries it too, so
  the rule is wider than the header shows. The header states no general
  thread-safety note at all, while `ds_runtime_tick`'s block appeals to one —
  "the same rule the rest of this header states" — that is not there. That
  contradiction is the actionable half of the gap. `ds_runtime_tick` is the
  surprising member and is why it is said there at all: since issue #945 it
  reads the renumbering gate and may call `document_replaced` on the attached
  surface, so it touches more than the scene.
- **A handle from `ds_runtime_new` is valid until `ds_runtime_free`.** A null
  **where a value is required** is `DS_NULL_ARGUMENT` rather than a trusted
  pointer. Three entry points are deliberately outside that: `ds_abi_version`
  takes no pointer, `ds_runtime_free(NULL)` returns silently as `free` does, and
  `ds_last_error_message(NULL, 0)` is the **documented size query** below.
- **`ds_last_error_message` returns the size _needed_, including the
  terminator** — never the number of bytes written. Query with `(NULL, 0)`,
  allocate that, call again, then drop the terminator.
  `crates/dashscene-android/src/host.rs`'s `last_error` is the reference for the
  sequence — a private item in an Android-only module, so read the file rather
  than expecting a path to resolve — and reading the return as bytes-written is
  a mistake this repository has already made once, in a test.

## The panic boundary

**Every entry point that can panic catches the unwind**, because one crossing
`extern "C"` is undefined behaviour. Eleven of the twelve wrap their body in
`catch_unwind`; `ds_abi_version` is the twelfth and returns a constant.

They do not all do it the same way, and the difference is what they can say
afterwards:

- The **nine returning a `DsStatus`** use the `guard` helper, which turns an
  unwind into `DsStatus::Panic`. **The panic payload is deliberately not
  formatted into the message** — that would run arbitrary `Display` code on the
  way out of a panic — so the text is fixed and the payload is lost.
- **`ds_runtime_free`** and **`ds_last_error_message`** catch one directly.
  Neither has a status to report it in, so each swallows it: the free returns,
  the message call answers 0.

**Do not re-derive this by counting calls to `guard`.** Story #843 did, read
"nine of twelve" as "three unguarded", and put that in this record, in
`architecture.md`, in `docs/features.md` and in the crate's own rule 1 before
the review caught it. Two entry points hold the property without the helper, and
`ds_last_error_message`'s own comment records that it was made to hold it
precisely because rule 1 had claimed it already did.

A caught panic leaves the runtime alive, and **only the mapped load accounts for
that**. It clears `runtime.scene` immediately before replacing the arena — the
two statements are adjacent — rather than leaving it until the scene is rebuilt.
**Three calls sit between that clear and the reassignment**:
`load_document_mapped`, `show_appended_root` and `attach_live`, each of which
can panic. Without the early clear, a caught panic in any of them would leave a
runtime holding a new arena and the previous document's `LiveScene`, driving
`NodeId`s against an arena that does not have them.

## The header is hand-written, and one gate checks the halves agree

`include/dashscene.h` is written by hand rather than generated. `just c-abi`
compiles it from C against the built library and runs a test binary through it —
**the only thing in the workspace that checks the two halves of the contract
agree**, and it runs no Rust test — the recipe builds the library, compiles
`tests/abi.c` against the header, and runs that binary. It reaches many statuses
from C and pins others by value, and those pins are the only thing that can see
a header-only typo, because a Rust test over `DsStatus` cannot read the header.
**Not for all of them**: `DS_GATE` and `DS_SURFACE` appear in that file nowhere,
so retyping either discriminant in the header passes `just c-abi`. That file
also asks not to be described by a count — three successive comments claimed a
correspondence with the Rust test and each was wrong — so add a pin when a
variant is added rather than restating how many there are.

It cannot see prose drift. A header comment that describes an entry point
wrongly compiles exactly as well as one that describes it correctly, which is
why issue #945's change to `ds_runtime_tick` had to be written into the header
by hand.

## Known gaps, named

**This record's own shape is one of them** — it restates the crate's module
documentation rather than citing it, and it states counts the repository's
convention says not to state. Issue #1190 carries both, and the test that would
make the counts unnecessary.

- **The two byte-taking loads do not clear the scene before replacing the
  arena** — issue #1183, found while writing this record. A panic caught inside
  `load_into` leaves a fresh arena paired with the previous document's
  `LiveScene`. It is the mapped loader that has the guard.
- **No data plane** — issue #859. Nothing here carries a boundary-B row, so a
  host that wants to draw the frame with its own renderer cannot.
- ~~No host calls the mapped load~~ — **closed 2026-08-16 (issue #1035)**, while
  this record was in review. `dashscene-android` gained
  `nativeSurfaceCreatedMapped`, which takes a path and an ordinal and reaches
  `ds_runtime_load_document_mapped`. The bounded path is now used.
- **The renumbering gate is correct here and unreachable.** The root is named
  once at load, so nothing can raise a renumbering the load's own
  `document_replaced` has not already covered. It is read anyway so the host is
  right the day a root-switching symbol lands.
- **`show_appended_root` aborts on a precondition its signature does not carry**
  — issue #1061. Public API of `dashscene-core` rather than of this crate, but
  this crate is one of the three callers whose correctness the argument rests
  on.
