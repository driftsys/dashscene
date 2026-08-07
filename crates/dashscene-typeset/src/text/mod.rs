//! Runtime text pipeline (docs/design/architecture.md): bidi split
//! (UAX #9 level runs) → shape per run (rustybuzz, features and digit
//! shapes resolved from the run's context — `shape::RunContext`) →
//! greedy line break → positioned glyph runs, reordered per line for
//! display, with a resolved-levels cache in front of the bidi split and
//! a font-unit shaped-run cache in front of shaping.

mod bidi;
mod font;
mod layout;
mod shape;
mod weight;

pub use font::{Font, FontFamily, WeightedFont};

// The digit-shape mapping and the Arabic-strong predicate are the one
// definition the atlas closure shares (atlas/closure.rs), so coverage
// derivation cannot drift from production shaping.
pub(crate) use shape::{arabic_indic_digit, is_arabic_strong};

use bidi::{Bidi, Reorder};
use shape::ShapedText;

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use unicode_bidi::{BidiClass, ParagraphInfo};

// ShapedText/ShapedGlyph stay crate-private: they are the cache-value
// representation, and publishing them before a consumer exists (#29/
// #30) would freeze it into the public API.

/// One glyph placed in document space (y-down, layout origin at the
/// top-left): the pen position on the line's baseline with the
/// shaping offsets applied. The painter combines this with the atlas
/// blob's y-up `plane_em` quad — that conversion is the painter's
/// (see `docs/design/atlas-pipeline.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    pub glyph_id: u16,
    /// Index into the typesetter's font list of the font this glyph was
    /// shaped with — the fallback cascade's result (story #219). A
    /// mixed-script line carries glyphs from more than one font, so the
    /// boundary-B stager groups consecutive same-font glyphs into one
    /// glyph run per atlas. Zero for a single-font typesetter.
    pub font: u16,
    pub x: f32,
    pub y: f32,
}

/// One positioned line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub glyphs: Vec<PositionedGlyph>,
    /// Pen advance over the line's glyphs, scaled, less the line's own
    /// trailing letter-spacing step (story #336) — so this is the width
    /// the box extent and the alignment shift use, not the raw pen
    /// position after the last glyph. The two differ by exactly one
    /// `letter_spacing` when that is non-zero.
    pub width: f32,
    /// Baseline position from the layout's top, y-down.
    pub baseline_y: f32,
}

/// A laid-out text block — docs/design/architecture.md's positioned glyph runs. The
/// render size lives here once (one style per text node in v0.5);
/// the atlas page field arrives when multi-page atlases exist.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<Line>,
    /// Widest line.
    pub width: f32,
    /// Line count × the line advance (ascent − descent + line gap).
    pub height: f32,
    /// Render size (px per em in document units).
    pub size: f32,
}

/// Horizontal alignment of a line within the container width (story #310).
/// `Left` is the default and reproduces the pre-#310 flush placement: an LTR
/// line stays at x = 0, an RTL line flushes right by direction. `Center` and
/// `Right` shift every line by the container's free space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// The additive shaping knobs [`Typesetter::layout_with`] honors (story #310,
/// #341): a fixed line height, letter spacing, horizontal alignment, and
/// standard ligatures forced off. The [`Default`] reproduces the previous
/// behavior exactly — an auto line height (font metrics), no tracking,
/// flush-by-direction placement, and the per-run context's own ligature
/// posture (`shape::RunContext`) — so [`Typesetter::layout`] delegating with
/// the default is byte-for-byte the pre-#310 output (the E7 guard).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShape {
    /// A fixed line advance in pixels, or `None` for the intrinsic advance
    /// (ascent − descent + line gap of the fonts that shaped each line).
    pub line_height_px: Option<f32>,
    /// Letter spacing (tracking) added after each glyph, in pixels.
    pub letter_spacing: f32,
    /// Horizontal alignment within the container width.
    pub align: TextAlign,
    /// Forces standard ligatures (`liga`/`clig`) off for every run in this
    /// layout, regardless of the run's own context (story #341: Figma's
    /// OpenType `LIGA: 0`). A non-Arabic run already shapes with these
    /// features off by default (`docs/decisions/
    /// liga-clig-off-until-gsub-closure.md`), so `false` here is a no-op for
    /// it; an Arabic-context run's other default features (digit shapes,
    /// joining, `rlig`/`ccmp`, …) are unaffected either way.
    pub ligatures_off: bool,
}

impl Default for TextShape {
    fn default() -> Self {
        TextShape {
            line_height_px: None,
            letter_spacing: 0.0,
            align: TextAlign::Left,
            ligatures_off: false,
        }
    }
}

/// The vertical shift a fixed line height applies to a line's baseline.
/// Figma (like CSS inline layout) centers the line's intrinsic box within the
/// fixed line box, so half the leading — `line_height - intrinsic` — sits
/// above the ascent: negative leading (a line height below the intrinsic
/// advance) lifts the baseline, positive lowers it. The default auto line
/// height applies none. Pinned against Figma's own `GET /images` render by
/// the #332 import oracle.
fn half_leading(shape: &TextShape, intrinsic: f32) -> f32 {
    shape
        .line_height_px
        .map_or(0.0, |line_height| (line_height - intrinsic) / 2.0)
}

/// Cache observability for tests and the measure callback's caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    /// Shaped-run cache hits.
    pub hits: u64,
    /// Shaped-run cache misses — each one shaped a paragraph.
    pub misses: u64,
    /// How many times the full UAX #9 resolution actually ran (issue
    /// #225). One per distinct paragraph text this typesetter has ever
    /// been given, however many times it lays that paragraph out.
    pub bidi_resolutions: u64,
    /// How many embedding levels the per-line display reorder has copied
    /// (issue #226). Each line copies its own bytes' levels; before the
    /// fix each line copied the whole paragraph's.
    pub reorder_levels_copied: u64,
}

/// The named diagnostic `text.weight-substituted` (story #368): a layout
/// asked one family for a CSS weight it has no face at, and the CSS Fonts 4
/// rule (`weight::match_weight`) resolved it to a different one.
///
/// This is a **render-time** diagnostic, not a compile-time one. Which
/// weights exist is a property of the renderer's asset set, not of the
/// document's intent, so recording a substitution in the `.dsb` would put a
/// result in the document and violate P1: the same document rendered by two
/// runtimes with different corpora substitutes differently. The typesetter
/// that actually made the substitution is what reports it.
///
/// Non-fatal by design — the layout proceeds with the resolved face. It is
/// a named report so the gap is never a silent substitution (P4); the
/// caller decides severity, exactly as it does for the atlas pipeline's
/// `missing_codepoints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeightSubstitution {
    /// The family's index in the cascade passed to
    /// [`Typesetter::with_font_families`], counting from 0.
    pub family: usize,
    /// The CSS weight the layout asked for.
    pub requested: u16,
    /// The CSS weight of the face it got.
    pub resolved: u16,
}

impl std::fmt::Display for WeightSubstitution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "text.weight-substituted: family {} has no face at weight {}; \
             using weight {}",
            self.family, self.requested, self.resolved
        )
    }
}

/// One render-time family substitution: the renderer's cascade had no
/// family answering to the name the document asked for, so some of the
/// run shaped in another family (story #385,
/// `docs/decisions/font-resolution-order.md` step 3).
///
/// The render-time counterpart of the import diagnostic R6 describes, for
/// the reason `docs/decisions/weight-substitution-is-a-render-time-diagnostic.md`
/// gives for weight: which fonts exist is a property of the renderer's
/// asset set, not of the document, so recording it at compile time would
/// violate P1. Deduplicated per distinct (requested, resolved) pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilySubstitution {
    /// The family name the document asked for.
    pub requested: String,
    /// The name of the family that actually shaped glyphs instead.
    pub resolved: String,
}

impl std::fmt::Display for FamilySubstitution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "text.family-substituted: the cascade carries no family {:?}; \
             using {:?}",
            self.requested, self.resolved
        )
    }
}

/// Whether any glyph in `layout` was shaped by the face at flat slot
/// `slot`. Both substitution reports are driven by the output rather than
/// by the resolution, so both ask this question.
fn shaped_any(layout: &TextLayout, slot: u16) -> bool {
    layout
        .lines
        .iter()
        .flat_map(|line| &line.glyphs)
        .any(|glyph| glyph.font == slot)
}

/// The runtime pipeline facade: an ordered font list (primary first,
/// story #219 — the runtime resolves one document font reference to a
/// fallback list), a UAX #9 resolution cache and a shaped-run cache in
/// front of shaping.
///
/// The cache stores font-unit shaped runs keyed by paragraph text —
/// shaping output is size-independent, so one entry serves every
/// render size. The font list is fixed per typesetter (runtime
/// configuration, not a per-call axis), so the cascade — which font
/// each codepoint resolves to — is a pure function of the text; the
/// key stays the text alone, exactly as in the single-font case (the
/// design record explains how this refines DESIGN §7.2's "string+style"
/// key). It is unbounded: cockpit UI text is a bounded set, and an
/// eviction policy before a real producer shows growth would be
/// speculative.
///
/// Neither `ligatures_off` (story #341) nor the requested weight (story
/// #368) is a property of the text, and both change shaping output — a
/// ligated pair shapes to one glyph with its own advance, and a heavier
/// face has its own advances, kerning and potentially its own glyph ids.
/// A single map keyed by text alone would hand back an entry shaped under
/// a different posture, so `caches` holds one map per distinct posture:
/// `slot_sets[i]` names posture `i`, and `caches[i]` holds its entries.
/// Posture 0 is reserved for the all-weight-400 cascade with ligatures on
/// — every pre-#368 call site — so the default path is one lookup in one
/// map, exactly as before.
///
/// The UAX #9 resolution sits in its own map, `bidi_cache`, rather than
/// beside the shaped runs, because it has no posture: neither the
/// ligature setting nor the resolved slot set reaches it, so the same
/// paragraph rendered at two weights resolves its levels once (issue
/// #225). It is unbounded on the same reasoning as the shaped-run
/// caches, and holds two bytes per byte of paragraph text plus the
/// paragraph boundaries — a fraction of the shaped runs already kept for
/// the same key.
#[derive(Debug)]
pub struct Typesetter {
    /// The flat slot list, primary family first — what
    /// [`PositionedGlyph::font`] indexes. Always at least one element.
    fonts: Vec<Font>,
    /// Each slot's declared CSS weight, parallel to `fonts`.
    weights: Vec<u16>,
    /// Each family's contiguous slot range, in cascade order. A cascade
    /// built by [`with_fonts`](Self::with_fonts) has one slot per family.
    families: Vec<Range<usize>>,
    /// Each family's declared name, parallel to `families` (story #385).
    /// Empty for every cascade built by an unnamed constructor.
    family_names: Vec<String>,
    /// One shaped-run cache per posture; `caches[i]` is keyed by paragraph
    /// text and holds the entries shaped under `slot_sets[i]`.
    caches: Vec<HashMap<Box<str>, Arc<ShapedText>>>,
    /// Resolved UAX #9 state per paragraph text (issue #225). Keyed by
    /// the text alone and shared by every posture — see
    /// [`bidi::BASE_LEVEL`] for why the text is the whole key.
    bidi_cache: HashMap<Box<str>, Arc<bidi::Resolved>>,
    /// The per-line display-reorder buffer (issue #226), reused across
    /// every line of every call.
    reorder: Reorder,
    /// The interning table naming each posture: the resolved slot per
    /// family, plus the ligature posture. Index 0 is the default.
    slot_sets: Vec<(Vec<u16>, bool)>,
    /// Deduplicated `text.weight-substituted` reports, in first-seen order.
    substitutions: Vec<WeightSubstitution>,
    /// Deduplicated `text.family-substituted` reports, in first-seen order
    /// (story #385).
    family_substitutions: Vec<FamilySubstitution>,
    hits: u64,
    misses: u64,
    /// Bidi-cache misses: how many times [`bidi::Resolved::new`] ran.
    bidi_resolutions: u64,
}

impl Typesetter {
    /// A single-font typesetter — the pre-#219 constructor. Equivalent
    /// to [`with_fonts`](Self::with_fonts) with a one-element list; every
    /// glyph is tagged font 0.
    pub fn new(font: Font) -> Typesetter {
        Self::with_fonts(vec![font])
    }

    /// A typesetter over an ordered fallback list, primary font first.
    /// Each level run splits by coverage: a codepoint goes to the first
    /// font in `fonts` that covers it, and a codepoint no font covers
    /// stays in the primary as `.notdef` (P4).
    ///
    /// Every font is one family at weight 400 — the weight the document
    /// defaults to — so this is exactly
    /// [`with_font_families`](Self::with_font_families) with one face per
    /// family, and its flat slot order is the list as given. Every layout
    /// through [`layout`](Self::layout) or
    /// [`layout_with`](Self::layout_with) requests weight 400 and resolves
    /// to that one face, so this constructor's behavior is unchanged by
    /// story #368.
    ///
    /// # Panics
    ///
    /// Panics if `fonts` is empty — a typesetter has no primary font to
    /// fall back to.
    pub fn with_fonts(fonts: Vec<Font>) -> Typesetter {
        Self::with_font_families(
            fonts
                .into_iter()
                .map(|f| vec![WeightedFont::regular(f)])
                .collect(),
        )
    }

    /// A typesetter over a cascade of families, each family an ordered set
    /// of weighted faces (story #368). Selection runs in two steps, in this
    /// order:
    ///
    /// 1. **Coverage picks the family**, exactly as the flat cascade picks a
    ///    font today: a codepoint goes to the first family that covers it.
    ///    Coverage is a correctness property — an uncovered codepoint
    ///    renders `.notdef` and the reader loses the text — while weight is
    ///    a fidelity property, so correctness wins. A weight-700 Arabic run
    ///    in a cascade with no Arabic Bold resolves to Arabic Regular, never
    ///    to Latin Bold.
    /// 2. **The requested weight picks the face** within that family, by the
    ///    CSS Fonts 4 rule (`weight::match_weight`), reporting any
    ///    substitution as [`WeightSubstitution`].
    ///
    /// The families are flattened family-major into the one positional slot
    /// list [`PositionedGlyph::font`] indexes, so a boundary-B stager maps
    /// each slot to its atlas in exactly this order — families in cascade
    /// order, faces in declared order within each — and needs to know
    /// nothing about weight.
    ///
    /// Coverage is probed against each family's *weight-resolved* face — the
    /// one face step 2 picked for this layout, not a fixed representative —
    /// so a family whose faces differ in charset splits a run differently at
    /// different requested weights. The faces of one family are therefore
    /// expected to share a charset, as the weights of one typeface do; a
    /// family with a partial heavier face would drop codepoints out of that
    /// family only when the heavier face is the resolved one.
    ///
    /// # Panics
    ///
    /// Panics if `families` is empty or if any family is empty — a
    /// typesetter has no primary face to fall back to.
    pub fn with_font_families(families: Vec<Vec<WeightedFont>>) -> Typesetter {
        Self::with_named_font_families(families.into_iter().map(FontFamily::unnamed).collect())
    }

    /// A typesetter over a cascade of **named** families (story #385) —
    /// [`with_font_families`](Self::with_font_families) plus the family
    /// name a document's `TextStyle::family` is matched against, which is
    /// step 2 of `docs/decisions/font-resolution-order.md`.
    ///
    /// Selection gains one step in front of the two
    /// [`with_font_families`](Self::with_font_families) describes, and the
    /// full order becomes family, then coverage, then weight:
    ///
    /// 1. **The requested family is probed first.** The family whose name
    ///    matches moves to the head of the probe order for that layout;
    ///    every other family keeps its cascade order behind it.
    /// 2. **Coverage still picks the family that shapes each codepoint**,
    ///    walking that reordered probe order — so a codepoint the requested
    ///    family cannot cover still falls through to one that can, and an
    ///    Arabic run in a Latin-only family is never lost.
    /// 3. **The requested weight picks the face** within whichever family
    ///    coverage chose.
    ///
    /// Reordering the probe order is the whole mechanism: the flattened
    /// slot list is untouched, so [`PositionedGlyph::font`] keeps its
    /// meaning and a stager's parallel atlas list needs no change. It also
    /// means the requested family becomes the layout's *primary* — the
    /// face a blank line is measured by and an uncovered codepoint keeps
    /// its `.notdef` in — which is the #219 rule applied to the family the
    /// document actually asked for.
    ///
    /// A request no family answers to is **not** an error: coverage runs
    /// over the cascade unchanged and records a [`FamilySubstitution`],
    /// readable through
    /// [`family_substitutions`](Self::family_substitutions). Committed
    /// fixtures name families the cascade does not carry, so a hard error
    /// would break their goldens.
    ///
    /// # Panics
    ///
    /// Panics if `families` is empty or if any family is empty.
    pub fn with_named_font_families(families: Vec<FontFamily>) -> Typesetter {
        assert!(
            !families.is_empty(),
            "a typesetter needs at least one font family"
        );
        let mut fonts = Vec::new();
        let mut weights = Vec::new();
        let mut ranges = Vec::with_capacity(families.len());
        let mut names = Vec::with_capacity(families.len());
        for family in families {
            assert!(
                !family.faces.is_empty(),
                "a font family needs at least one face"
            );
            let start = fonts.len();
            names.push(family.name);
            for face in family.faces {
                fonts.push(face.font);
                weights.push(face.weight);
            }
            ranges.push(start..fonts.len());
        }
        // Posture 0: every family at its weight-400 resolution, in cascade
        // order, ligatures on — the posture every pre-#368 call site shapes
        // under, so the default path finds its map at a fixed index.
        let default_slots = resolve_slots(&weights, &ranges, 400);
        Typesetter {
            fonts,
            weights,
            families: ranges,
            family_names: names,
            caches: vec![HashMap::new()],
            bidi_cache: HashMap::new(),
            reorder: Reorder::default(),
            slot_sets: vec![(default_slots, false)],
            substitutions: Vec::new(),
            family_substitutions: Vec::new(),
            hits: 0,
            misses: 0,
            bidi_resolutions: 0,
        }
    }

    /// The primary font — the first in the cascade. A single-font
    /// typesetter has only this one; a multi-font typesetter falls back
    /// past it by coverage (story #219). Line metrics are taken per line
    /// from the fonts that actually shaped that line ([`layout`](Self::layout)
    /// through `line_box`), not from the primary alone.
    pub fn font(&self) -> &Font {
        &self.fonts[0]
    }

    /// The ordered font list, primary first — the cascade a
    /// [`PositionedGlyph::font`] indexes. A boundary-B stager maps each
    /// font to its atlas in this order.
    pub fn fonts(&self) -> &[Font] {
        &self.fonts
    }

    /// Each slot's declared CSS weight, parallel to [`fonts`](Self::fonts)
    /// (story #368) — the additive way to read which weight a
    /// [`PositionedGlyph::font`] slot stands for. A cascade built by
    /// [`with_fonts`](Self::with_fonts) reports 400 for every slot.
    pub fn weights(&self) -> &[u16] {
        &self.weights
    }

    /// The `text.weight-substituted` reports this typesetter has
    /// accumulated (story #368), deduplicated per distinct (family,
    /// requested, resolved) triple and in first-seen order. Empty when
    /// every requested weight found an exact face — including every
    /// [`with_fonts`](Self::with_fonts) cascade, which is asked for 400 and
    /// declares 400.
    ///
    /// A report means a face at another weight actually rendered glyphs. A
    /// family the layout resolved but coverage never selected — an Arabic
    /// family under a pure-Latin bold run — contributes nothing, so every
    /// entry here names a substitution the reader can see on screen.
    ///
    /// Deduplicating per triple rather than per layout keeps a screen with
    /// nineteen bold nodes from producing nineteen identical reports.
    pub fn weight_substitutions(&self) -> &[WeightSubstitution] {
        &self.substitutions
    }

    /// Every `text.family-substituted` this typesetter has recorded, in
    /// first-seen order and deduplicated per distinct (requested, resolved)
    /// pair (story #385). Empty for a cascade with no declared names, and
    /// for one where every request found its family.
    pub fn family_substitutions(&self) -> &[FamilySubstitution] {
        &self.family_substitutions
    }

    /// Each family's declared name, in cascade order — the sibling of
    /// [`weights`](Self::weights) for the family axis.
    pub fn family_names(&self) -> &[String] {
        &self.family_names
    }

    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            bidi_resolutions: self.bidi_resolutions,
            reorder_levels_copied: self.reorder.copied,
        }
    }

    /// Lays out `text` at `size` (px per em), wrapping greedily at
    /// spaces when `max_width` is given and breaking at `'\n'`
    /// always. Each paragraph resolves its base direction per UAX #9
    /// (first strong character): an RTL paragraph's lines sit
    /// flush-right within `max_width` (or within the widest line when
    /// `None`); LTR lines stay flush-left at x = 0. `width` stays the
    /// widest line's pen advance — the measure contract — so an RTL
    /// line's glyph positions can reach up to `max_width`, past
    /// `width`. Empty text produces an empty, zero-size layout.
    pub fn layout(&mut self, text: &str, size: f32, max_width: Option<f32>) -> TextLayout {
        self.layout_with(text, size, max_width, TextShape::default())
    }

    /// [`layout`](Self::layout) with the additive shaping knobs (story #310):
    /// `shape.line_height_px` overrides the per-line advance and centers each
    /// line's intrinsic box within the fixed box (`half_leading`, Figma's
    /// model), `shape.letter_spacing` tracks each glyph in the placement pen
    /// but excludes each line's own trailing step from the measured width
    /// (story #336 — Figma's model), and `shape.align` shifts each line
    /// within `max_width` (or the widest line when `None`). With
    /// [`TextShape::default`] this is exactly `layout`'s previous behavior, so
    /// every existing call site — the E7 oracle and goldens included — renders
    /// identically.
    pub fn layout_with(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        shape: TextShape,
    ) -> TextLayout {
        self.layout_weighted(text, size, max_width, shape, 400)
    }

    /// [`layout_with`](Self::layout_with) at a requested CSS weight (story
    /// #368). Each family resolves the request to one of its faces by the
    /// CSS Fonts 4 rule — see
    /// [`with_font_families`](Self::with_font_families) for the two-step
    /// selection — and the resolved faces shape, measure and tag the run,
    /// so a bold paragraph measures at bold advances rather than being
    /// sized for Regular and overflowing its box.
    ///
    /// Requesting 400 against an all-weight-400 cascade is exactly
    /// `layout_with`: same faces, same slot indices, same cache map, same
    /// output. That is what keeps every pre-#368 call site — the E7 oracle
    /// and the goldens included — rendering byte-identically.
    ///
    /// A request no family has an exact face for is **not** an error: it
    /// resolves to the nearest face by the rule and records a
    /// [`WeightSubstitution`], readable through
    /// [`weight_substitutions`](Self::weight_substitutions).
    pub fn layout_weighted(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        shape: TextShape,
        weight: u16,
    ) -> TextLayout {
        self.layout_styled(text, size, max_width, shape, weight, "")
    }

    /// [`layout_weighted`](Self::layout_weighted) at a requested **family**
    /// as well as a requested weight (story #385) — the entry point that
    /// makes a document's `TextStyle::family` affect the render, which is
    /// step 2 of `docs/decisions/font-resolution-order.md`.
    ///
    /// The named family is probed first and the rest of the cascade follows
    /// in declared order; see
    /// [`with_named_font_families`](Self::with_named_font_families) for the
    /// three-step selection. Coverage still decides which family shapes a
    /// given codepoint, so naming a family never costs a reader their text.
    ///
    /// Passing `""` is exactly [`layout_weighted`](Self::layout_weighted):
    /// no family is preferred, the cascade order is untouched, and no
    /// family diagnostic is recorded. That is what keeps every pre-#385
    /// call site — the goldens and both oracles included — rendering
    /// byte-identically.
    ///
    /// A family the cascade does not carry is **not** an error: it resolves
    /// by coverage as before and records a [`FamilySubstitution`], readable
    /// through [`family_substitutions`](Self::family_substitutions).
    pub fn layout_styled(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        shape: TextShape,
        weight: u16,
        family: &str,
    ) -> TextLayout {
        let resolved = resolve_slots(&self.weights, &self.families, weight);
        let order = self.probe_order(family);
        let slots: Vec<u16> = order.iter().map(|&family| resolved[family]).collect();
        let layout = self.layout_slots(text, size, max_width, shape, &slots);
        self.record_substitutions(weight, &slots, &order, &layout);
        self.record_family_substitutions(family, &slots, &order, &layout);
        layout
    }

    /// The cascade family indices in the order this layout probes them:
    /// the family matching `requested` first, then every other family in
    /// declared order. An unmatched or empty request leaves the cascade
    /// order exactly as declared, so the probe order — and therefore the
    /// interned posture and the shaped result — is bit-for-bit what it was
    /// before family matching existed.
    fn probe_order(&self, requested: &str) -> Vec<usize> {
        let preferred = self
            .family_names
            .iter()
            .position(|name| FontFamily::name_matches(name, requested));
        let mut order: Vec<usize> = (0..self.families.len()).collect();
        if let Some(preferred) = preferred {
            let family = order.remove(preferred);
            order.insert(0, family);
        }
        order
    }

    /// Records a [`WeightSubstitution`] for each family that both resolved
    /// to a face at a weight other than `requested` and actually shaped
    /// glyphs in `layout`.
    ///
    /// Reporting is driven by the output, not by the resolution, because
    /// resolution runs over every family in the cascade while coverage
    /// selects only some of them. A pure-Latin bold run against a cascade
    /// that also carries an Arabic family resolves that Arabic family too —
    /// but no Arabic glyph exists, so its Arabic Regular face was never
    /// used and reporting it would be a substitution that did not happen.
    /// P4 asks for a named diagnostic per real gap; a diagnostic that fires
    /// when nothing was substituted makes the true reports unreadable.
    ///
    /// Which face renders is unaffected — this decides only what is
    /// reported.
    /// `order[p]` is the cascade family index at probe position `p`, so a
    /// report names the family the cascade declared rather than the
    /// position family matching happened to probe it at (story #385).
    fn record_substitutions(
        &mut self,
        requested: u16,
        slots: &[u16],
        order: &[usize],
        layout: &TextLayout,
    ) {
        // The common case — every resolved face is at the requested weight
        // — has nothing to report, so it never walks the glyphs.
        if slots
            .iter()
            .all(|&slot| self.weights[slot as usize] == requested)
        {
            return;
        }
        for (probe, &slot) in slots.iter().enumerate() {
            let resolved = self.weights[slot as usize];
            if resolved == requested || !shaped_any(layout, slot) {
                continue;
            }
            let report = WeightSubstitution {
                family: order[probe],
                requested,
                resolved,
            };
            if !self.substitutions.contains(&report) {
                self.substitutions.push(report);
            }
        }
    }

    /// Records a [`FamilySubstitution`] for each family that shaped glyphs
    /// under a name other than the one the document asked for (story #385).
    ///
    /// Driven by the output for the same reason
    /// [`record_substitutions`](Self::record_substitutions) is: resolution
    /// covers every family, coverage reaches only some, and a diagnostic
    /// that fires when nothing was substituted makes the true reports
    /// unreadable.
    ///
    /// A run can legitimately produce a report even when the requested
    /// family was found: a document asking for a Latin-only family and
    /// containing Arabic has its Arabic shaped elsewhere, and that is a
    /// real family substitution the renderer should name (P4).
    fn record_family_substitutions(
        &mut self,
        requested: &str,
        slots: &[u16],
        order: &[usize],
        layout: &TextLayout,
    ) {
        // A cascade with no names, or a document naming no family,
        // expresses no preference — nothing was substituted.
        if requested.trim().is_empty() {
            return;
        }
        for (probe, &slot) in slots.iter().enumerate() {
            let name = &self.family_names[order[probe]];
            if name.trim().is_empty() || FontFamily::name_matches(name, requested) {
                continue;
            }
            if !shaped_any(layout, slot) {
                continue;
            }
            let report = FamilySubstitution {
                requested: requested.trim().to_string(),
                resolved: name.clone(),
            };
            if !self.family_substitutions.contains(&report) {
                self.family_substitutions.push(report);
            }
        }
    }

    /// The layout body, over an already-resolved slot per family.
    fn layout_slots(
        &mut self,
        text: &str,
        size: f32,
        max_width: Option<f32>,
        shape: TextShape,
        slots: &[u16],
    ) -> TextLayout {
        // Per-font pixel scale: each glyph's advances and offsets are in
        // its own font's units, so a different-upem fallback is scaled by
        // its own upem, not the primary's (story #219).
        let scales: Vec<f32> = self
            .fonts
            .iter()
            .map(|f| size / f32::from(f.units_per_em()))
            .collect();
        // The primary family's resolved slot — the face a line with no
        // glyphs of its own is measured by. At weight 400 against an
        // all-400 cascade this is slot 0, the pre-#368 seed.
        let primary = slots[0] as usize;
        // Lines stack down the page: `pen_y` accumulates the line boxes
        // above the current line, and each box is measured from the fonts
        // that actually shaped its own glyphs ([`line_box`]), not the
        // primary — a line shaped by a taller fallback gets a taller box
        // (story #219, applied per line).
        let mut pen_y = 0.0f32;
        let mut lines = Vec::new();
        // Parallel to `lines`: the owning paragraph's base direction.
        let mut rtl_lines = Vec::new();
        if !text.is_empty() {
            for paragraph in text.split('\n') {
                // The resolution is cached (issue #225): the measure
                // callback lays one text node out several times per Taffy
                // solve, and repeating UAX #9 each time bought nothing —
                // the levels are a pure function of this text.
                let resolved = self.resolve_bidi(paragraph);
                let bidi = Bidi::new(paragraph, &resolved);
                let shaped = self.shaped(paragraph, bidi, shape.ligatures_off, slots);
                // An empty chunk has no bidi paragraph: one empty line.
                if bidi.paragraphs.is_empty() {
                    // A blank line has no glyphs; measure its box by the
                    // primary font. A fixed line height overrides the advance
                    // and centers the intrinsic box within it (half-leading,
                    // story #310 corrected by #332).
                    let (ascent, intrinsic) = self.line_box(std::iter::empty(), &scales, primary);
                    lines.push(Line {
                        glyphs: Vec::new(),
                        width: 0.0,
                        baseline_y: pen_y + ascent + half_leading(&shape, intrinsic),
                    });
                    pen_y += shape.line_height_px.unwrap_or(intrinsic);
                    rtl_lines.push(false);
                    continue;
                }
                // Lines are produced per bidi paragraph: '\n' is split
                // above, and the other UAX #9 class-B separators (CR,
                // NEL, U+2029, …) end a paragraph here, so no line
                // ever spans two paragraphs — each line reorders and
                // aligns under its own paragraph's level.
                for para in &bidi.paragraphs {
                    let content = paragraph_content(bidi, para);
                    let gs = shaped
                        .glyphs
                        .partition_point(|g| (g.cluster as usize) < content.start);
                    let ge = shaped
                        .glyphs
                        .partition_point(|g| (g.cluster as usize) < content.end);
                    for range in layout::break_lines(
                        paragraph,
                        &shaped,
                        gs..ge,
                        &scales,
                        max_width,
                        shape.letter_spacing,
                    ) {
                        // The line's box comes from the fonts that shaped
                        // its glyphs, not the primary (story #219). A fixed
                        // line height overrides the advance and centers the
                        // intrinsic box within it — half-leading, Figma's
                        // model: the baseline sits at ascent plus half the
                        // leading, lifted under negative leading and lowered
                        // under positive (story #310, corrected by the #332
                        // import oracle against Figma's own render).
                        let (ascent, intrinsic) = self.line_box(
                            shaped.glyphs[range.clone()].iter().map(|g| g.font as usize),
                            &scales,
                            primary,
                        );
                        let baseline = pen_y + ascent + half_leading(&shape, intrinsic);
                        pen_y += shape.line_height_px.unwrap_or(intrinsic);
                        rtl_lines.push(para.level.is_rtl());
                        lines.push(layout::position_line(
                            &mut self.reorder,
                            bidi,
                            para,
                            &shaped,
                            range,
                            &scales,
                            baseline,
                            shape.letter_spacing,
                        ));
                    }
                }
            }
        }
        let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        // Horizontal alignment within the container (story #310). `Left` is the
        // default and reproduces the pre-#310 flush placement: an LTR line stays
        // at x = 0, an RTL-base line flushes right by direction. `Center` and
        // `Right` shift every line by the container's free space, regardless of
        // direction. A line wider than the container overflows leftward past
        // x = 0, mirroring the prior LTR overflow.
        let container = max_width.unwrap_or(width);
        for (line, rtl) in lines.iter_mut().zip(&rtl_lines) {
            let shift = match shape.align {
                TextAlign::Left if *rtl => container - line.width,
                TextAlign::Left => 0.0,
                TextAlign::Center => (container - line.width) / 2.0,
                TextAlign::Right => container - line.width,
            };
            if shift != 0.0 {
                for g in &mut line.glyphs {
                    g.x += shift;
                }
            }
        }
        let height = pen_y;
        TextLayout {
            lines,
            width,
            height,
            size,
        }
    }

    /// The vertical metrics of one line, taken from the fonts that
    /// actually shaped its glyphs — not the cascade's primary alone.
    /// Story #219 established the per-font principle for glyph scale
    /// (each glyph scaled by its own font's units-per-em); a line box
    /// has the same shape of dependency. The box spans the tallest
    /// ascender and the deepest descender across the line's fonts, plus
    /// the widest line gap, so a line shaped entirely by a taller
    /// fallback is measured by that fallback rather than by a shorter
    /// primary.
    ///
    /// `fonts_on_line` yields the font index of each glyph on the line;
    /// repeats are harmless, since the maximum and minimum ignore them.
    /// An empty iterator is a blank line, which carries no glyphs and is
    /// measured by `primary` — the primary family's weight-resolved slot,
    /// so a blank line in a bold paragraph takes the bold face's metrics
    /// rather than Regular's (story #368). Returns the line's ascent (the
    /// distance from the top of the box down to the baseline) and its
    /// baseline-to-baseline advance, both in pixels.
    fn line_box(
        &self,
        fonts_on_line: impl Iterator<Item = usize>,
        scales: &[f32],
        primary: usize,
    ) -> (f32, f32) {
        let metrics = |f: usize| {
            let scale = scales[f];
            (
                f32::from(self.fonts[f].ascender()) * scale,
                f32::from(self.fonts[f].descender()) * scale,
                f32::from(self.fonts[f].line_gap()) * scale,
            )
        };
        // Seed from the primary so a blank line (no glyphs) is measured
        // by it; the first shaping font on the line replaces the seed.
        let (mut ascent, mut descent, mut gap) = metrics(primary);
        let mut seen = false;
        for f in fonts_on_line {
            let (a, d, g) = metrics(f);
            if seen {
                ascent = ascent.max(a);
                descent = descent.min(d);
                gap = gap.max(g);
            } else {
                (ascent, descent, gap) = (a, d, g);
                seen = true;
            }
        }
        // The descender is negative, so subtracting it adds the depth
        // below the baseline: advance = ascent + |descent| + line gap.
        (ascent, ascent - descent + gap)
    }

    /// The resolved UAX #9 state for `paragraph`, from the cache or newly
    /// resolved (issue #225).
    ///
    /// **The key is the paragraph text and nothing else, and that key is
    /// complete.** [`unicode_bidi::BidiInfo::new`] takes two inputs: the
    /// text, and a base paragraph level. This crate always passes
    /// [`bidi::BASE_LEVEL`], which is `None` — UAX #9 P2/P3 auto-detection
    /// from the paragraph's own first strong character — so the resolution
    /// is a pure function of the text, and a paragraph that changes
    /// direction does so by changing its text and therefore its key. No
    /// other axis reaches the resolution: size, maximum width, alignment,
    /// line height, letter spacing, ligatures, requested weight and
    /// requested family all act after it. `bidi::BASE_LEVEL` documents the
    /// obligation to extend this key if a base direction ever becomes a
    /// per-call parameter, and a unit test pins the constant.
    fn resolve_bidi(&mut self, paragraph: &str) -> Arc<bidi::Resolved> {
        if let Some(resolved) = self.bidi_cache.get(paragraph) {
            return resolved.clone();
        }
        self.bidi_resolutions += 1;
        let resolved = Arc::new(bidi::Resolved::new(paragraph));
        self.bidi_cache.insert(paragraph.into(), resolved.clone());
        resolved
    }

    /// The cache key is the paragraph text within a **posture**: the
    /// per-run context (Arabic/Plain, `shape::RunContext`) is a pure
    /// function of the text alone, but the ligature setting (story #341)
    /// and the resolved slot set (story #368) are not — the text carries
    /// no trace of either, and both change shaping output. `posture`
    /// interns the pair and indexes `caches`, so entries shaped under
    /// different postures cannot collide. The default posture is index 0,
    /// so the pre-#368 path is one lookup in one map exactly as before.
    ///
    /// One entry serves every layout of the paragraph *under its posture*
    /// — and, since issue #225, that now holds for the bidi step too: the
    /// resolved levels this shaping consumes come from
    /// [`resolve_bidi`](Self::resolve_bidi)'s own cache rather than being
    /// re-resolved per call. Before that fix this comment overclaimed:
    /// the shaped runs were cached but the UAX #9 pass in front of them
    /// was repaid on every `layout()` call.
    fn shaped(
        &mut self,
        paragraph: &str,
        bidi: Bidi<'_>,
        ligatures_off: bool,
        slots: &[u16],
    ) -> Arc<ShapedText> {
        let posture = self.posture(slots, ligatures_off);
        if let Some(shaped) = self.caches[posture].get(paragraph).cloned() {
            self.hits += 1;
            return shaped;
        }
        self.misses += 1;
        let shaped = Arc::new(shape::shape_paragraph(
            &self.fonts,
            slots,
            bidi,
            ligatures_off,
        ));
        self.caches[posture].insert(paragraph.into(), shaped.clone());
        shaped
    }

    /// The index of the cache map for this (slot set, ligature) pair,
    /// interning it on first use. The table holds one entry per distinct
    /// posture a caller has actually asked for — a handful at most, since
    /// a corpus offers a handful of weights — so the linear scan is
    /// cheaper than hashing the slot vector, and the default posture is
    /// found at index 0 on the first comparison.
    fn posture(&mut self, slots: &[u16], ligatures_off: bool) -> usize {
        if let Some(i) = self
            .slot_sets
            .iter()
            .position(|(s, l)| *l == ligatures_off && s == slots)
        {
            return i;
        }
        self.slot_sets.push((slots.to_vec(), ligatures_off));
        self.caches.push(HashMap::new());
        self.slot_sets.len() - 1
    }
}

/// Resolves one slot per family for `requested` (story #368). `weights` is
/// the flat per-slot weight list and `families` the per-family slot ranges,
/// both as [`Typesetter`] holds them.
///
/// Resolution covers every family, including families this layout's coverage
/// split will never reach, because the resolved faces are what the split
/// probes. Reporting a substitution is therefore *not* done here — see
/// [`Typesetter::record_substitutions`], which reports against the output.
///
/// A free function rather than a method because the construction path calls
/// it before a `Typesetter` exists.
fn resolve_slots(weights: &[u16], families: &[Range<usize>], requested: u16) -> Vec<u16> {
    families
        .iter()
        .map(|range| {
            let available = &weights[range.clone()];
            (range.start + weight::match_weight(available, requested)) as u16
        })
        .collect()
}

/// A paragraph's content range: its bidi range minus the trailing
/// block-separator character (class B — CR, NEL, U+2029, …), which
/// ends the paragraph and renders on no line, exactly like the '\n'
/// the caller splits on. Every byte of a multi-byte separator carries
/// class B, so trimming by byte is trimming by char.
fn paragraph_content(bidi: Bidi<'_>, para: &ParagraphInfo) -> Range<usize> {
    let mut end = para.range.end;
    while end > para.range.start && bidi.original_classes[end - 1] == BidiClass::B {
        end -= 1;
    }
    para.range.start..end
}

/// Everything that can go wrong in the runtime pipeline. Shaping and
/// layout are infallible over a valid [`Font`]: unknown codepoints
/// shape to `.notdef` (the painter's named diagnostic surface at
/// paint time), and empty text lays out empty.
#[derive(Debug)]
pub enum TypesetError {
    /// The bytes are not a parseable font face.
    FontParse(String),
}

impl std::fmt::Display for TypesetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FontParse(m) => write!(f, "cannot parse font: {m}"),
        }
    }
}

impl std::error::Error for TypesetError {}
