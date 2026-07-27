# Glyph coverage is declared at build time; dynamic generation is a deferred painter capability

    status   accepted (2026-07-27, owner's call). The deferral has a
             stated reopening condition rather than being permanent.
    scope    how an atlas learns which glyphs it must contain, and where
             a glyph may be produced — crates/dashscene-typeset's atlas
             module, and the painter contract at boundary B
    related  docs/decisions/atlas-closure-cmap-plus-extras.md (the
             closure as built), docs/decisions/derivation-manifest-
             section.md (a profile must not change layout),
             docs/decisions/backdrop-blur-is-core-vocabulary.md (the
             render-time capability posture this follows),
             docs/wip/2026-07-27-indic-script-support.md,
             docs/specification/02-principles.md (P2, P3)

## Context

An atlas must contain every glyph a shaped run can produce. Two ways exist
to make that true, and the industry uses both:

- **Declare coverage at build time.** Close a charset or a text corpus over
  the font's substitution rules, and bake exactly the reachable glyphs. This
  is what web font subsetting does — `fonttools`' `pyftsubset --text=`,
  HarfBuzz's `hb-subset`, and the Google Fonts API's `text=` parameter all
  implement GSUB closure, at scale, and they exist because complex scripts
  made shipping whole fonts unaffordable.
- **Generate glyphs at runtime.** Populate the atlas on demand as text
  appears. Unity's TextMeshPro dynamic SDF assets, Qt Quick's distance-field
  glyph cache, and every browser's glyph cache work this way.

The question became live with Indic scripts. Their conjuncts are
combinatorial rather than enumerable, so the charset-driven closure this
project ships does not reach them
(`docs/wip/2026-07-27-indic-script-support.md`).

## Decision

**Coverage is declared at build time.** The closure becomes text-driven for
complex scripts — shape the strings the document actually carries and union
the resulting glyph ids — rather than charset-driven.

**Dynamic generation is deferred, and when it lands it is a painter
capability, not a profile property or a producer behaviour.**

## Why build time, beyond caution

Build-time closure works on **every** backend. Dynamic generation would give
one painter a capability the Skia reference painter and any future lean
painter do not have, and the render oracle's whole value is that it compares
our render against a design source through one painter whose coverage is the
same as everyone else's. A capability only one backend has stops the oracle
comparing like with like, and qualification forks.

It is also the deterministic choice: a document's coverage is a compile-time
fact that can be diffed, reviewed and reproduced, which is what R7 asks of
everything else in the pipeline.

## Why a capability rather than a profile

This is the part most likely to be got wrong later, so it is recorded
explicitly.

If a profile could generate glyphs and another could not, the two would have
**different coverage**. Coverage feeds shaping, shaping decides advance
widths, so the two profiles would lay out the same document differently.
That is exactly the property `derivation-manifest-section.md` protects: two
files of one design under two profiles must still share a document.

So:

- **Coverage stays a document fact** — declared once, identical across every
  profile and every painter.
- **Shaping is therefore deterministic** and produces the same metrics
  everywhere.
- **Dynamic generation only widens what a painter can draw**, never what was
  asked for. A painter that can generate never meets the missing-glyph case;
  a painter that cannot reports it at render time.

That is the posture `backdrop-blur-is-core-vocabulary.md` already
established for a painter that cannot sample the backdrop: the vocabulary is
core, and incapacity is a render-time report rather than a compile-time
refusal (P1). One backend may be better without being different.

## Why this does not violate P3 or P2

Worth stating, because "runtime generation" sounds like it must.

**P3** forbids _producer-side_ execution inside the frame loop. A
runtime-owned glyph cache is runtime infrastructure, in the same class as
texture upload — it is the runtime owning time, not a producer mutating
during a frame. Browsers and Qt Quick do exactly this.

**P2** holds as long as generation is **ink-only**. Metrics still come from
the font through the typesetter, so the painter is not measuring, wrapping,
kerning or moving anything. A painter that derived metrics from a glyph it
generated would violate P2, and that is the line.

## Consequences

- The closure gains a text-driven mode for complex scripts. The existing
  pairwise sweep stays for Arabic and Latin, where it is proven.
- `dashc` records the document's string set as declared coverage — the same
  fact the residency work needs for paging (issue #460), so the two converge
  rather than competing.
- A cluster outside declared coverage degrades by **shaping**, not by
  painting. Unformed clusters have different advance widths from the
  conjuncts they replace, so the choice changes metrics and must therefore
  be a document-level fact, identical everywhere. A painter cannot make it.
- The painter's named missing-glyph diagnostic (#30) remains the backstop
  and must never become a silent drop (P4).

## The reopening condition

Revisit dynamic generation when there is a **commercial requirement for
scripts whose coverage cannot be declared at compile time** — most plausibly
runtime-supplied strings in a complex script, where a contact name or track
title can need a conjunct no corpus contained.

Three constraints hold whenever it is taken up, and none is negotiable:

1. **Coverage stays a document fact.** Generation may satisfy coverage; it
   may not extend it.
2. **Metrics never vary by painter or profile.** Generation is ink-only.
3. **A dynamic generator must be this project's generator, or the
   divergence must be measured and recorded.** This one has a concrete trap
   attached: TextMeshPro's dynamic SDF is single-channel and uses its own
   generator, so a Unity backend adopting it would render text from a
   different field than the reference painter measures — breaking the weld
   the profile-preview work established (story #435). `fdsm` is pure Rust
   and wasm-clean, so running this project's generator on target is the
   path that keeps the oracle honest.

## Alternatives considered

**Dynamic generation now, on the Unity backend.** Rejected for the
qualification reason above, not on feasibility — TextMeshPro proves it
works. It would fork the oracle at the moment the oracle is the project's
main evidence of fidelity.

**Dynamic generation as a HiFi-profile property.** Rejected: it makes
coverage vary by profile, so one design lays out two ways. The capability
framing gets the same benefit without that cost.

**Enumerating the Indic cluster space at build time.** Rejected as not
reachable: conjuncts are combinatorial and font-dependent, so an enumeration
either misses forms or produces an atlas far larger than any product needs.
Text-driven closure is exact where enumeration is both incomplete and
wasteful.
