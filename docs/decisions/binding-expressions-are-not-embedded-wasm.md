# Binding expressions are not embedded wasm

    status   accepted 2026-08-11, at the v0.18 close (epic #769). Gardened
             from docs/wip/2026-08-07-motion-in-the-document.md, which
             stays for the counter-proposal below — that half is an open
             input rather than a decision
    affects  dashbuf, dashscene-core, dashcue, dashlang, dashscene-validator
    related  docs/decisions/bindings-are-explicit-and-flat.md
             docs/decisions/binding-table-in-the-document.md
             docs/decisions/dashscene-document-is-the-ir.md
             docs/decisions/dashcue-keyframe-values-are-progress-fractions.md
             docs/decisions/publishable-and-the-first-version.md

Slint carries expression bindings in its markup — `width: parent.width / 2` —
and the obvious way to reach the same expressive power here is to compile such
an expression to wasm, carry it in the document, and embed a wasm runtime in the
player. That was proposed during the v0.18 design session and is **rejected**.
It is recorded so it is not re-proposed without new evidence, because the
proposal is reasonable on its face and the reasons it fails are properties of
this stack rather than of the idea.

## Six reasons, each a property this project already holds

**P1.** An expression is neither intent nor results; it is computation. A
document that carries one stops being a description and becomes a program.

**P4.** Arbitrary wasm is unanalysable, so a diagnostic cannot name what it
does. A wasm-carrying document is unvalidatable by construction, which is the
property `dashscene-validator` exists to enforce.

**P3.** A binding evaluated per frame is producer logic running inside the frame
loop, which is the case P3 names.

**Payload size.** A general wasm runtime is several megabytes. The embeddable
runtime measures **497 KiB brotli**
(`docs/decisions/publishable-and-the-first-version.md`), so the interpreter
would be several times the thing it interprets for. On the web it also means
shipping a wasm interpreter inside a wasm module.

Note that the earlier framing of this argument compared against 1.37 MB. That
number is `demo_web.wasm`, a host that links the whole compiler, and issue #776
opened with it before the distinction was drawn. The comparison holds either way
and is stronger against the correct number.

**Determinism.** Goldens, the R7 guard and `atlas-repro` all assume reproducible
output. This repository treats a divergence of 4 px in 65536 as a finding worth
its own record — it is why the atlas generator is a pinned external binary
(`docs/decisions/atlas-gen-external-pinned-binary.md`) — and an embedded VM is
the wrong direction from that standard.

**Security.** The document becomes executable content, so loading a `.dsb` from
an untrusted source is code execution. `docs/technotes/runtime-content.md` §3
leaves the admission policy undecided even for streamed _data_ fragments, which
is a much weaker thing to admit.

## The motivating example is layout, which Taffy already does

The expression that motivates the feature is `parent.width / 2`. That is layout,
and the solver already does flex, grid, percentages, min and max constraints,
and gap. Slint needs expression bindings partly because its layout vocabulary is
weaker. So the feature largely solves a problem this stack does not have.

## The boundary is already policed, which makes this a re-proposal

The schema states the rule for the case where a producer can express more than
the document can carry:

    A transform is data by construction — dashlang's Custom closure
    transform never serializes; a compiler refuses it by name instead.

So `dashlang` already reaches past `.dsb`, and the established response is to
refuse by name at compile time rather than to widen the document into a program.
Embedded wasm is that same boundary approached from the other side.

## What is not decided here

If more computed power is wanted in the file, the direction is to **widen the
declarative transform union rather than embed a VM**.
`dashscene_core::ScalarTransform` is `Identity | Scale | MapRange
| Clamp`
today; arms such as `Curve`, `Sum`, `Lerp` and `Select` would cover most of what
producers reach for, stay validatable and diffable, cost about two instructions
each, and the union is append-only by design. Anything beyond that belongs on
the arena path, where a producer runs in process and can hold a closure.

That is a proposal and not a ruling. No arm is scheduled, and nothing here
commits to one. It is held in `docs/wip/2026-08-07-motion-in-the-document.md`
until something needs it, which is why that capture did not empty at this close.
