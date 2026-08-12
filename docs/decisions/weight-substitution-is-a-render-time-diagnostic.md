# Decision: `text.weight-substituted` is a render-time diagnostic, not a compile-time one

    status   accepted (story #368, epic #344 — the design gate the
             repository owner approved before implementation)
    scope    crates/dashscene-typeset — where a weight gap is named; and
             the render walk in goldens/tooling, which surfaces it
    binds    dashc and dashscene-validator by exclusion: neither records a
             weight substitution in the document

## Context

P4 requires that a gap be a named diagnostic and never a silent substitution.
Resolving a requested weight to a different face is a gap, because the reader
sees text at a thickness the document did not ask for
(`docs/decisions/css-fonts-4-weight-matching-non-fatal.md`). So it must be
named. The open question was where.

## Options

1. **A compile-time diagnostic**, raised by `dashc` or the validator. The `.dsb`
   would record that weight 700 was requested and something else will be used.
2. **A render-time diagnostic at cascade resolution.** The typesetter reports
   the substitution it actually made, through the same kind of surface the atlas
   pipeline already uses for `missing_codepoints` — a named, non-fatal report
   whose severity the caller decides.

## Choice

Option 2. The typesetter accumulates
`WeightSubstitution { family,
requested, resolved }` values, readable through
`Typesetter::weight_substitutions()`, deduplicated per distinct triple and in
first-seen order. The production render walk prints each as
`warning: text.weight-substituted: family N has no face at weight R;
using weight S`
on stderr, which is that path's only caller-visible surface — its return value
is the PNG.

## Why

- **Option 1 conflicts with P1.** Which weights exist is a property of the
  renderer's asset set, not of the document's intent. A document compiled once
  and rendered by two runtimes with different corpora substitutes differently,
  so recording one runtime's substitution in the `.dsb` would store a result in
  a document that is supposed to carry only intent — and would state it as
  though it had been authored.
- **The thing that made the substitution is the thing that can describe it.** At
  compile time the answer is a prediction; at cascade resolution it is a fact,
  and it carries the family, the requested weight and the resolved weight, so
  the report reads as a specific, actionable statement rather than a generic
  warning.
- **The precedent already exists in the same crate.** The atlas pipeline's
  `missing_codepoints` is a named diagnostic surface, never a silent drop, with
  the caller deciding severity. Reusing that shape was preferable to inventing a
  surface.
- **Deduplicating per triple rather than per layout keeps the report readable.**
  A screen with nineteen bold nodes would otherwise produce nineteen identical
  lines.

## Consequences

- **A report means a substitution the reader can actually see.** Weight
  resolution runs over every family in the cascade, but coverage selects only
  some of them, so reporting is driven by the layout output rather than by the
  resolution: a family is reported only if its resolved face both differs from
  the requested weight and actually tagged glyphs. A pure-Latin bold run against
  a cascade that also carries an Arabic family resolves that Arabic family too,
  but no Arabic glyph exists, so reporting it would name a substitution that did
  not happen. A diagnostic that fires when nothing was substituted makes the
  true reports unreadable.
- The check is ordered accordingly: the render walk reports after staging is
  finished, not during it.
- The common case costs nothing. When every resolved face is at the requested
  weight there is nothing to report, and the glyph walk that determines which
  families were used is skipped entirely.
- **The absence of a report is itself evidence.** The `text-bold` import-oracle
  frame renders three rows at weights 400/600/700 and emits no
  `text.weight-substituted` report, which is what confirms all three resolved to
  an exact committed face.
- `dashc` and `dashscene-validator` gain nothing here, deliberately. A
  producer-side diagnostic about weight would have to guess a corpus.
