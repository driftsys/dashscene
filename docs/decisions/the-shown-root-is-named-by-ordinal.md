# A host names the shown root by its ordinal, one at a time

    status   accepted (2026-08-12), **as built** in story #837 (epic #833,
             slice v0.19)
    scope    how a host says which root it shows: the type, where it lives,
             what both integration crates take, and what a root that is not
             there does. Not the traversal — the solve, the committed table
             and the paint still cover every root, which is story #838.
    issue    #837, on D3 of
             `the-shown-root-bounds-the-load-not-the-paint.md`
    refs     #822, #838, #925, #792, #840,
             `the-shown-root-bounds-the-load-not-the-paint.md`,
             `the-integration-surface-is-two-published-crates.md`

## Context

`the-shown-root-bounds-the-load-not-the-paint.md` D3 names a selection concept
as the first of three pieces, and says why it has to come first:

> A selection concept has to exist first. No host can say which root it shows;
> both hardcode `first_root`. "Confine the paint to the shown root" is
> meaningless until something can name a different one.

That was the state. `dashbuf::prefetch::first_root` took a document and no
argument, so "the shown root" meant "root 0" in both integration crates — a
bound on the load and a synonym for the first root, not a choice anyone made.
The record left the shape open on purpose, under "What this does not decide":
whether a host may show more than one root at a time, and what the selection
surface looks like, with "its shape is the story's to settle".

This record settles it.

## Decision

**D1 — one root at a time, not a set.** A host shows one root. That is what both
integration crates do, what a full-screen panel needs, and what keeps the
traversal change in story #838 a single index space rather than a union of them.
The asymmetry decides the order: widening to a set later adds a second entry
point and breaks nothing, where narrowing from a set to one root would break
every caller. There is no case in the tree today that wants two, and inventing
the general answer before the case exists is what would make #838 harder.

**D2 — the name is an ordinal over the document's roots**, zero-based, in the
order the document declares them. `dashbuf::prefetch::ShownRoot` is that
ordinal, and `ShownRoot::FIRST` is what every host meant before this story.

**D3 — it is a newtype, not a bare `u32`.** `prefetch::resolve` turns a
`ShownRoot` into a **node** index, and both are `u32`. Passing one where the
other belongs would read the wrong subtree and report nothing, which is the
failure the boundary-B work already learned to prevent by type: a row index is
valid only in the table that assigned it. The newtype makes the two unmistakable
at every call site in the workspace.

**D4 — it lives in `dashbuf`, beside the prefetch it selects.** That is the
lowest crate that needs it: `dashbuf::prefetch::assets_of_root` is what a
selection bounds, and `dashscene-core` already depends on `dashbuf`, so the next
story (#838) can take the same type rather than defining a second one meaning
the same thing. Both integration crates **re-export** it, so an embedder naming
a root does not have to declare a dependency on the format crate and keep its
version in step — the same rule the `dashscene-gpu` re-exports in
`dashscene-web` follow.

**D5 — it is a parameter on the load call, not state on a handle.** Both
`dashscene_desktop::Document::load` and `dashscene_web::load_document` take a
`ShownRoot`. Neither remembers one. Both run again on every rebuild, and the
embedder is what knows which artboard is on screen; a field would have to be
kept in step with that anyway, and would make "which root is on screen"
answerable in two places.

**D6 — a root that is not there is refused by name, with the count.**
`DesktopError::NoSuchRoot` and `WebError::NoSuchRoot` carry the ordinal asked
for and how many roots the document has. One variant rather than two, because an
embedder's recovery is the same for "this document has no roots" and "this
document has fewer roots than that", and the count is what tells it which
mistake it made. Neither host clamps to the last root or falls back to the
first: drawing a picture the embedder did not ask for and reporting nothing is
the silent-drop failure P4 rules out one layer up.

**D7 — `first_root` is deleted rather than kept beside the new call.** A
convenience that means "root 0" is exactly what this story exists to remove, and
leaving it in the API is an invitation to hardcode the same thing again under a
different name. Every call site now names a `ShownRoot`, and the ones that mean
the first root say `ShownRoot::FIRST` — which is a statement rather than a
default.

**D8 — the C ABI does not take a root yet, and the reason is not this story.**
`dashscene-ffi`'s own documentation said root selection was absent because the
concept did not exist and "It joins when #837 lands". The concept exists now and
the ABI still does not carry it, so that sentence is replaced rather than
satisfied. `ds_runtime_load_document` takes the whole file as `(ptr, len)` and
hands every payload to `dashscene_core::load_document`, the **owning** loader,
which copies every payload into an owned `ImageAsset` — so on that path there is
nothing for a root selection to bound. A parameter added today would be accepted
and would change nothing measurable, which is worse than its absence. What
unblocks it is issue #925 (the ABI has no mapped path and no owner), or the
traversal change tracked as issue #838 (the paint follows the shown root),
whichever lands first; the ABI versioning rule says a new symbol is free and a
changed signature bumps `DS_ABI_VERSION`, so the cost is known either way.

## Consequences

- **Naming a different root changes what the desktop host reads, and does not
  change what either host draws.** The solve, the committed table and the paint
  still cover every root. `dashscene_desktop`'s two test pairs say the first
  half by exchanging which payload may be corrupt with the ordinal.
- **On the web it moves the reported `Bound` rather than the byte count.**
  `shown::layout` widens to the union whenever a root other than the shown one
  draws a payload, because a payload a browser did not fetch has no bytes at all
  — so for any multi-root document where more than one root draws, the ordinal
  cannot narrow the fetch. `Bound::ShownRoot` against `Bound::EveryRoot` is the
  whole of what the selector changes there, which is the honest statement of
  what this story delivers on that target, and story #838 is what makes the
  answer always `ShownRoot`.
- **Story #838 inherits a vocabulary rather than inventing one**, which is the
  whole reason D3 sequenced this first.
- **Neither host has a second artboard to name yet.** `demo` takes a document
  path and `demo-web` takes a url; the showcase's own scenes are single-root.
  Both pass `ShownRoot::FIRST` explicitly, so the call sites say what they mean
  rather than relying on a default.

## Alternatives considered

**A document-declared root name — "show the Home artboard".** The closest thing
to how a designer thinks, and the friendliest to pass across the C ABI as a
string. Rejected as the primitive on a fact about the schema rather than on
taste: `Node.name` is a plain optional `string`, so a lowered artboard need not
carry one and a name-keyed selection cannot address every root of every
document. It is also a producer's vocabulary reaching into the format's, which
P5 keeps apart — Figma names artboards, and the `.dsb` is not Figma's. A
name-to-ordinal lookup can be built over an ordinal later, and would be a
resolver rather than a second primitive; the reverse is not available.

**A node index — what `first_root` returned.** No new type, and it is what
`assets_of_root` already takes. Rejected because it asks an embedder to know
which entries of `Document.nodes` happen to be roots, which is a fact about the
node table's packing rather than about the picture, and because it is
indistinguishable from the resolved value it would be passed alongside.

**An opaque handle obtained from the document.** Validated at construction, so
`resolve` could not fail. Rejected: it ties the selection to a loaded document,
and an embedder wants to say "show root 3" before it has opened anything — the
desktop host maps at start-up and loads again on every rebuild. It also could
not cross the C ABI without an allocation and a lifetime, where an ordinal is a
`u32`.

**Keep `first_root` and add the selector beside it.** Smallest diff, and no call
site outside the two hosts has to change. Rejected under D7.

## What this does not decide

- **Whether a host may change the shown root while a scene is live**, and what
  that costs. Today it means loading again, which is what a rebuild already
  does. Once story #838 root-scopes `Arena::dfs_order`, a change of shown root
  becomes a renumbering event the dirty-set contract has to treat like
  `Present::document_replaced` — that is #838's to settle, and it is why this
  record stops at the load.
- **How a host discovers what roots a document has.**
  `dashbuf::prefetch::root_count` returns how many there are, and is what both
  `NoSuchRoot` variants report; `prefetch::roots` yields their node indices, for
  a consumer that walks every root rather than choosing between them. No error
  carries a node index — D3 is the reason. Anything richer — a name, an extent,
  a preview — is a producer question nothing has asked yet.
