# Indic scripts: why they do not arrive with CJK, and what closure has to become

    status   WIP — design-discussion capture (2026-07-27, user + Opus).
             Nothing here is implemented. No code was changed to produce
             this note. Its decided half — coverage is declared at build
             time, dynamic generation is a deferred painter capability —
             is gardened into
             docs/decisions/glyph-coverage-is-declared-at-build-time.md.
             Tracked as track D of epic #463.
    scope    Indic script support: which scripts, what breaks, and what
             the atlas closure has to become. Not shaping, which works.
    builds on docs/decisions/atlas-closure-cmap-plus-extras.md,
             docs/design/atlas-pipeline.md,
             docs/decisions/glyph-coverage-is-declared-at-build-time.md,
             docs/wip/2026-07-27-glyph-coverage-sets-and-text-residency.md

## The question that produced this

Whether Indic scripts arrive with CJK. They do not. They are close to
opposite problems, and bundling them would give Indic the wrong design work
behind the wrong dependency.

|               | CJK                                    | Indic                                              |
| ------------- | -------------------------------------- | -------------------------------------------------- |
| glyph count   | very large                             | moderate base, multiplied by conjuncts             |
| shaping       | mostly one-to-one, little substitution | reordering, conjunct formation, split vowels, reph |
| the hard part | **scale** — residency                  | **shaping, and therefore closure**                 |

CJK's answer is paging. Indic's answer is a closure model that handles
clusters it cannot enumerate. Neither solves the other.

## Which scripts

Not all twenty-two scheduled languages. The commercially load-bearing set
for automotive India is about eight scripts, one of which covers three
languages:

**Devanagari** (Hindi, Marathi, Nepali) — first by a wide margin. Then
**Tamil**, **Telugu**, **Bengali**, **Kannada**, **Gujarati**,
**Malayalam**, **Gurmukhi**. Karnataka and Gujarat matter disproportionately
for automotive specifically.

## What is already fine

**Shaping is not the gap.** `rustybuzz` carries the full HarfBuzz Indic
shaper — `src/complex/indic.rs`, with below-form, vattu and post-form
handling. Reordering, conjunct formation and split vowels work in a shaped
run today.

**The architecture's key choice already accommodates it.** Atlases are keyed
by **glyph id, never codepoint** (`docs/design/atlas-pipeline.md`, confirmed
by spike #25), which is exactly what a script producing glyphs with no
codepoint requires. That was established for Arabic and generalises.

## What breaks: the closure is pairwise

`crates/dashscene-typeset/src/atlas/closure.rs` states its own boundary:

> Each character is shaped in the four Arabic joining contexts
> (isolated/initial/medial/final), each haraka on a base letter, and **every
> ordered character pair** (for ligatures such as lam-alef and the Latin
> `fi`).
>
> **Ligatures longer than two characters** (for example `ffi`, or the Allah
> ligature) **are outside the pairwise sweep**; a shaped run that reaches one
> is the painter's named missing-glyph diagnostic (#30), never a silent drop.

That is sufficient for Arabic, where joining forms are per-character and
lam-alef is a pair, and more than sufficient for CJK, where closure is
nearly the charset itself.

**Devanagari conjuncts are routinely three and four consonants** joined by
virama. A pairwise sweep misses them, so an Indic atlas built by today's
closure would be systematically incomplete — and the failure would surface
as the missing-glyph diagnostic firing on ordinary words rather than on
exotic edge cases.

Extending to triples and quadruples does not rescue it. The cluster space is
combinatorial, and which clusters a font actually supports varies by font.
Arabic's four joining forms are a bounded multiplier; Indic conjuncts are
not.

## The answer: closure driven by text, not by charset

Shape the strings the document actually carries and union the resulting
glyph ids. For a Figma-sourced document every string is known at compile
time, and `dashc` already has them.

This is exact rather than approximate: you get precisely the conjuncts the
product uses and nothing else. A UI's real vocabulary is a few hundred
clusters per script rather than the thousands a font carries.

**It is also necessary, not merely elegant.** A Devanagari font carries well
over a thousand glyphs once conjuncts are counted. At the corpus's own
density of roughly 1310 texels per glyph at 32 px/em, eight scripts at full
font coverage would land in the tens of megabytes — importing CJK's problem
for no reason. Text-driven closure is what keeps Indic a shaping problem
rather than becoming a residency problem too.

### This is not a novel design

Web font subsetting is exactly this, in production at scale. `fonttools`
(`pyftsubset --text=`) and HarfBuzz's `hb-subset` both implement GSUB
closure — give them input glyphs, they retain everything reachable through
substitution. The Google Fonts API's `text=` parameter subsets on the same
principle. The approach exists **because** complex scripts made shipping
whole fonts unaffordable, which is the same pressure met here.

### And the primitive is closer to hand than the closure record implies

`atlas-closure-cmap-plus-extras.md` notes that "rustybuzz exposes shaping but
no standalone glyph-closure operation". True, but it reads as a larger gap
than it is: rustybuzz already implements

    pub fn would_substitute(&self, map: &Map, face: &Face, glyphs: &[GlyphId]) -> bool

which is the question a closure walk asks repeatedly. The Indic shaper uses
it internally to decide conjunct formation. It is simply not on the public
surface — `lib.rs` re-exports only `shape`, `Face`, `Buffer` and friends.

So it is an **export gap rather than an algorithm gap**, with `hb-subset` as
the reference for what to build above it.

**Unverified**: this was read in a cached rustybuzz 0.6.0 while the project
pins 0.20.1. The API has very likely moved. Confirm against the pinned
version before planning around it; the shaper's structure will not have
changed, since it is a port of HarfBuzz's.

## The runtime-string case, and its metrics consequence

For Latin a resident ASCII fallback covers almost any runtime-supplied name.
For Devanagari a contact name can need a conjunct no corpus contained, and
enumeration cannot close the gap.

The answer is the degradation the script already has: render the cluster
**unformed** — base consonants plus visible virama — rather than the
conjunct ligature. This is standard OpenType Indic behaviour when a font
lacks the ligature, and some Indian typefaces leave certain conjuncts
unformed as a typographic register rather than a failure. A native reader
notices; the text stays legible.

**The consequence that constrains the design**: an unformed cluster has a
different advance width from the conjunct it replaces. So degradation is a
**shaping** decision, not a painting one, and it changes metrics. That makes
declared coverage a document-level fact which must be identical across every
profile and painter — recorded in
`docs/decisions/glyph-coverage-is-declared-at-build-time.md`.

The resident fallback set therefore becomes base consonants, vowels, matras
and virama — bounded and small — with conjuncts as the corpus-driven layer
above it.

## What has to be built

1. **Text-driven closure** in `crates/dashscene-typeset/src/atlas/closure.rs`.
   The pairwise sweep stays for Arabic and Latin, where it is proven.
2. **`dashc` collects the document's string set** and records it as declared
   coverage — the same fact issue #460 needs for paging.
3. **Unformed-cluster fallback** at shaping, with its named diagnostic.
4. **Per-script fonts and metrics**, on the pattern `corpus/fonts/` sets.
   Weight synthesis (#467) matters more here than for Latin: eight scripts
   times four weights of atlases is not viable, and one atlas plus per-weight
   metrics is.

## Measure first

Take a real HMI string corpus in Hindi and Tamil, shape it, and count
distinct glyphs. That number says whether text-driven closure is a
comfortable win or a bare necessity, and it is the same measurement that
issue #460 needs anyway.

## Open

- **Whether the pairwise sweep should be replaced rather than supplemented.**
  Text-driven closure is strictly more accurate for every script, including
  Latin and Arabic. Keeping two mechanisms costs a second thing to maintain
  and a second thing to explain; keeping one means re-proving Arabic against
  the new path. Not decided here.
- **Where the string set lives in the document.** It is coverage, so it is
  intent rather than a result, but it is also large. Whether it rides in a
  hot section, is reduced to a glyph-id set at compile time, or is carried
  outside the document entirely is a design question for the story.
