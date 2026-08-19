# dashscene-validator — the four gates, diagnostics, waivers, and the rule set

As-built after stories #15 and #139 (v0.3), #41 (v0.7 — full diagnostics and
waivers) and #1127 (v0.21 — the contribution gate). The rationale is in
`docs/decisions/validator-three-gates.md` and
`docs/decisions/waivers-and-diagnostic-completion.md`; this record is the
component's shape and its rule table.

## Position

The validator sits _beside_ the semantic model, not inside it. It reads the
document (`dashbuf`) and boundary B (`dashpaint`), and **producers call it** —
the arena does not — with the contribution gate as the exception this record's
own gate table names: its caller is a host, not a producer. It has no
`dashscene-core` dependency: core is published earlier, and `CommittedScene`'s
accessors already hand out `dashpaint` types.

    producer source vocabulary ────────► triage ────────────────┐
                                                                │
    dashc ──► .dsb document ───────────► validate_document ─────┤
                                                                ├──► Report
    Arena::commit ──► CommittedScene ──► validate_scene ────────┤
                                                                │
    .dsb document + a host's bound ids ► validate_contributions ┘

## The four gates

| gate         | entry point                                                                    | input                                              | catches                                                                          |
| ------------ | ------------------------------------------------------------------------------ | -------------------------------------------------- | -------------------------------------------------------------------------------- |
| import       | `triage(Construct, Profile, NodePath) -> Diagnostic`                           | the producer's own vocabulary                      | out-of-profile constructs (`docs/specification/04-figma-vocabulary-profile.md`)  |
| load         | `validate_document(&Document) -> Report`                                       | a `.dsb`                                           | referential integrity, unknown enum values, geometry-free paint rules            |
| paint        | `validate_scene(&[RectEntry], &PaintTable, &ImageTable, &ClipTable) -> Report` | boundary B                                         | geometry budgets, runtime index resolution                                       |
| contribution | `validate_contributions(&Document, &[&str], Profile) -> Report`                | a `.dsb` **and** the host's bound contribution ids | a placeholder no host fills, and a binding no placeholder declares (story #1127) |

They are not interchangeable: each gate's failure class is one the others cannot
report. The contribution gate is a partial exception worth knowing about — it
reads the same `.dsb` as the load gate, so it can _detect_ an out-of-range
placeholder index, and deliberately stays silent because that finding is the
load gate's to make. More generally it asks what each side _declares_, never
whether what it declares is well-formed: an empty or out-of-range pool entry is
a document defect for the load gate to name (debt #1273). See the decision
record.

The contribution gate is the one whose second input is not an artifact at all:
the document states which nodes are placeholders and which ids they name, and
only the host knows which ids it binds. (Taking a second input is not itself new
— `validate_asset_payloads` below does too.) That is why the check cannot live
in `dashc`, which holds the first half and never the second (issue #851). It
does not weaken `validator-three-gates.md`: the caller supplies the half the
validator cannot obtain.

The load gate has a second entry point:

| half          | entry point                                              | input                              | catches                                                                      |
| ------------- | -------------------------------------------------------- | ---------------------------------- | ---------------------------------------------------------------------------- |
| load (assets) | `validate_asset_payloads(&Document, &[&[u8]]) -> Report` | a `.dsb` **and** its blob payloads | an entry whose recorded format or extent disagrees with the payload it names |

It is separate because an `AssetEntry` describes bytes the document does not
contain — the payload lives in its own section of the file. A caller holding
only the document cannot check that the two agree, so the check takes the
payloads explicitly rather than being folded into `validate_document` and
silently doing nothing when they are absent. `dashbuf::open_verified` returns
exactly the pair both halves need, and `dashc` runs both — over a file it is
checking, and over a document it has just emitted. That is the eager reader
because this check is a check: it needs every payload's bytes, which is the one
thing `dashbuf::open` deliberately does not read
(`docs/decisions/verification-moves-from-open-to-touch.md`).

The check needs an image header parser, which is why it did not exist before
v0.12: this crate publishes before `dashc`, where the only parser lived. Story
#437 moved the parser to `dashpaint`, which every writer and this crate reach
(`docs/decisions/image-header-parser-lives-in-dashpaint.md`). It header-parses
and never decodes, so a payload truncated after its header passes this gate and
fails in the painter — the only component that can find it, and the one the
target-hardware rules keep out of the trusted path.

## Diagnostic

`docs/archive/2026-07-14-design-1-seed.md` §6.1's tuple:

    pub struct Diagnostic {
        pub rule: &'static str,  // stable, greppable: "paint.gradient.no-stops"
        pub severity: Severity,  // Error blocks the document; Warning degrades
        pub at: Location,        // node, paint-pool entry, or image asset
        pub message: String,
    }

The tuple's fourth element — the **workaround hint** — is
`Diagnostic::workaround(&self) -> Option<&'static str>`, a rule-keyed derivation
rather than a stored field, and `Display` appends it. The hint is a pure
function of the rule id, and keeping it out of the struct leaves the
`Diagnostic` shape — and the wasm-ABI mirror `dashc` owns of it
(`docs/decisions/dashc-wasm-abi.md`) — unchanged. A hint exists where the
designer can act on the finding — a design choice, whichever gate found it — and
`None` where the rule stands in front of a producer bug, as the
referential-integrity and geometry rules do. The import gate's out-of-profile
constructs (§04's "bake it, slot it, design without it") are the largest group,
and they are not the only one: the MSDF size floor carries a hint, and so do the
contribution gate's two rules (story #1127). `rule::workaround`'s match arms are
the list; this paragraph is deliberately not a second copy of it. See
`docs/decisions/waivers-and-diagnostic-completion.md`.

`Report` collects them in the order the gate walked its input, which is document
order for the gates that walk a document — the contribution gate appends its
binding diagnostics after those, in the order the host listed them, because
their subjects have no position in the document. `has_errors()` answers "is the
document blocked", `is_empty()` answers "does a normal build carry no findings",
`has(rule)` / `find(rule)` are what tests and callers pin, and
`strict(&[Waiver])` is the release-mode gate (see "Waivers" below).

### A producer assembles its own `Report`

The import gate returns one bare `Diagnostic` per construct, and the producer
that owns the mapping (P5) is the only code that knows when it has found them
all. `Report` therefore implements `FromIterator<Diagnostic>` and
`Extend<Diagnostic>` (story #139): a producer collects its findings and
assembles them, and `Extend` is what lets `dashc::compile_figma` fold the load
gate's report into the import gate's so both gates decide emission from one
merged report. `push` stays `pub(crate)`.

Without this a producer could triage a construct and then have no way to report
it — a silent drop by construction. See
`docs/decisions/producer-assembles-its-own-diagnostics.md`.

### `Location` — not everything reported is a node

`Location`'s variants are the enum's own documentation — this record
deliberately does not carry a second copy of the list, which had already drifted
by three variants before story #1127 extended it, and which nothing compiles or
tests.

`Contribution` is the exception the rest of this section is not about: its
subject is a binding the document does not contain, which is what the diagnostic
carrying it reports, so it carries the id rather than any index. It is keyed by
id precisely because the waiver machinery below matches on `Location` equality —
a position in the caller's own list would make a waiver follow that list's
order.

Every pooled surface — paint entry, image asset, variant set, text style — is
shared by every node that references it, so each is reported **once, at its own
index** — repeating one authoring mistake per referencing node would bury the
rest of the report. Their indices are _pool_ indices, and `Location` is what
stops them being mistaken for node indices: both are small integers, so a
consumer that resolves a diagnostic to a layer (an editor jumping to it, or the
waiver machinery keying on it) would otherwise land silently on an unrelated
node. Each pooled surface therefore has its own variant — a pool index is never
wrapped in a `Node`. `dashc`'s wasm-ABI mirror (`crates/dashc/src/abi/json.rs`)
has a matching arm per variant, so a new pooled surface is an additive mirror
change, not a wire break.

`NodePath` carries the document DFS index — which is the rect-table index too —
and the name chain (`/screen/card/badge`) when the surface has names. Boundary B
has none, so a scene node diagnostic renders as `#3`.

## Profiles

`Profile::Core` (lean/native painters) and `Profile::Full` (Unity-class). They
diverge at the import gate, on the constructs
`docs/specification/04-figma-vocabulary-profile.md` annotates `(profile:full)`,
which a `Core` target can never honor and so cannot degrade to anything — and,
since story #1127, at the contribution gate, for an unrelated reason: there the
profile says whether the target has a host-content mechanism at all, not which
paint vocabulary it honours. That gate is the only place `Profile` selects on
something other than the vocabulary bands. Backdrop blur was one until story
#393 made it core vocabulary every painter honours
(`docs/decisions/backdrop-blur-is-core-vocabulary.md`); the advanced blend mode
is now the only one.

`validate_document` takes no profile: every construct the schema can express is
in the NOW band, so there is nothing to select — including the v0.8 shadow
vocabulary (story #45), which is NOW-band and profile-neutral (a drop or inner
shadow is not `(profile:full)`). It would regain a profile only if a
`(profile:full)` effect such as layer blur ever entered the schema. Backdrop
blur entered it at story #393 without doing so, because it entered as NOW-band
vocabulary rather than as a profile-gated one.

## Waivers (strict mode)

`docs/design/architecture.md`: an `Error` blocks the document; a `Warning` is a
declared degrade a normal build lets through. A **release build runs strict**
and refuses even a warning, unless a declared waiver records that the degrade is
acceptable for one specific target.

    pub struct Waiver { pub rule: String, pub at: Location, pub reason: String }

    Report::strict(&[Waiver]) -> StrictReport

`StrictReport::passes()` is the release gate: it passes only when no error
remains and every warning is covered by a valid waiver. Three properties, each
recorded in `docs/decisions/waivers-and-diagnostic-completion.md`:

- **Never a global mute, but target-complete.** A waiver matches by rule id
  **and** `Location`, so it suppresses that rule at one target — not a rule
  everywhere. When a target carries several _identical_ findings (the same rule
  at the same location, e.g. two advanced-blend-mode paints on one node), one
  waiver covers them all — they carry no discriminating information, so
  one-waiver-each would be empty ceremony.
- **An error is never waivable.** A waiver matching an error leaves it blocking
  and is itself diagnosed (`waiver.covers-an-error`); only a warning is a
  degrade a waiver can accept.
- **The waiver vocabulary is validated (P4).** A waiver naming a rule id not in
  `rule::ALL` is `waiver.unknown-rule` (error); a waiver matching nothing is
  `waiver.unused`, and a waiver duplicating another (covering nothing an earlier
  one did not) is `waiver.redundant` — both warnings, surfaced for hygiene,
  non-blocking. `StrictReport::applied()` lists the waivers that actually
  suppressed a warning — the audit trail of exceptions granted, one entry per
  waiver even when it covers several findings.

The strict gate exists here; wiring it into `dashc`/the importer with a
waiver-file format is a later importer step, on this contract.

## Rule set

Load gate — document referential integrity and schema evolution:

| rule                                     | why it exists                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `node.parent-out-of-range`               | the flatbuffer verifier checks structure, not references (#63)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `node.parent-not-before-child`           | the node array is in DFS order, so a parent's index is always lower; a forward reference cycles every consumer that walks up                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `paint.entry-out-of-range`               | #63                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `paint.conflicting-representation`       | `paint_entry` supersedes the v0.1 `paint` shorthand, so setting both silently discards one (#63)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `text.string-out-of-range`               | same bug class as #63                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `placeholder.string-out-of-range`        | a placeholder naming a string outside the pool (story #1126). Same bug class as #63, and the loader resolves both `contribution_id` and `fragment_ref` through `Document.strings` on this gate's word — `flatbuffers::Vector::get` asserts, so an unchecked index is a panic in `dashscene-core` rather than a diagnostic                                                                                                                                                                                                                                                                                                                                                                                          |
| `placeholder.declared-size-invalid`      | a placeholder `declared_size` that is not finite and non-negative (story #1126). The same predicate `geometry.rect-invalid-extent` applies to an authored box, and for the same reason: this is the size a measure callback reports, so it reaches the solve once activation lands                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `text.style-out-of-range`                | same bug class as #63                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `text.style-weight-out-of-range`         | a `TextStyle.weight` outside the CSS scale 100..=900 the schema pins. Font selection would otherwise clamp it silently or pick an unintended face — the silent vocabulary drop P4 forbids (#129). The load gate already iterates the text-style pool for `text.style-no-color`; the weight is one more read                                                                                                                                                                                                                                                                                                                                                                                                        |
| `text.style-below-msdf-floor`            | a text style whose reachable em size is under `MSDF_MIN_PX_PER_EM` (14): v0 bakes no per-size bitmap page, so the MSDF field smears under the floor — dots and harakat first (`docs/decisions/q1-msdf-below-14px.md`). A **warning**: the floor is a measured legibility threshold, not a schema range, so a target that accepts the degrade waives it. Checked against the smallest size the document can _reach_: `min_text_scale` classifies every binding channel and override arm, and no arm reaches `TextStyle.size` (#373)                                                                                                                                                                                 |
| `vocabulary.unknown-enum`                | the schema's enums are append-only, so a v0.3 reader handed a v0.8 document gets the new value as a raw integer — "range-check and emit a named diagnostic, never default silently". Covers every enum field: `LayoutMode`, `MainAxisAlign`, `CrossAxisAlign`, `AxisSizing`, the `Fill` union tag, `GradientKind`, `ScaleMode`, `StrokeAlign`, `ImageFormat`, `Binding.channel` (#167), and `ShadowKind` (#45)                                                                                                                                                                                                                                                                                                     |
| `binding.signal-out-of-range`            | a binding row referencing a signal declaration the document does not carry; the loader resolves it unchecked, so a dangling index would panic at load (#167)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `binding.reflow-not-contained`           | a binding on a layout channel (`X`, `Y`, `Width`, `Height`, `Gap`) whose target node sits under an ancestor that hugs its content on either axis. The hug ancestor resizes with what it contains, so the write travels up to the nearest fixed ancestor and back down through everything under it — the reflow leaves the bound node's subtree and R4's statically provable frame cost no longer holds. A **warning**: the document renders correctly and the cost is authored intent, so a target that accepts it declares a waiver (#257)                                                                                                                                                                        |
| `binding.node-out-of-range`              | same bug class, for the row's node index (#167)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `binding.fill-channel-on-non-solid-fill` | a binding on a fill channel (`FillR`, `FillG`, `FillB`, `FillA`) whose target node is filled with a gradient or an image. A fill channel writes one component of a solid colour, so the runtime keeps a per-node colour and stages the whole of it on every flush; a gradient has no such component, and the flush replaced it outright — measured as a linear gradient plus `FillA` at 0.5 committing as an opaque black at half alpha. An **error**, for the reason `paint.conflicting-representation` is one: the producer has stated two opinions about the fill and one is discarded, so no reading honours both. `Opacity` is deliberately not caught — it is its own prop and never touches the fill (#667) |
| `signal.name-duplicate`                  | two signal declarations with one non-empty name; a runtime looks a document signal up by name (`dashlang::attach_live`), so a duplicate would silently shadow one declaration (#167)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `asset.image-no-bytes`                   | an image asset whose `bytes` vector is present but empty. The painter decodes behind `expect("image asset decodes (validated upstream, P4)")`, so reading the asset table only for its length would leave that `expect` with no upstream                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `asset.payload-unreadable`               | an entry's payload matches no container signature the closure knows, or its header is malformed. Raised by `validate_asset_payloads`, so it needs the payloads; a painter would otherwise discover it inside its decoder (story #437)                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `asset.format-mismatch`                  | the payload's own signature names a different container than the entry's recorded `format`. A painter dispatches its decoder on the recorded format, so it would hand PNG bytes to a JPEG decoder (story #437, debt #416)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `asset.extent-mismatch`                  | the payload's header reports a different intrinsic extent than the entry's recorded `width`/`height`. Layout runs on the recorded extent before the payload is resident, so the frame would reflow once the real size arrived (story #437, debt #416)                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `asset.payload-missing`                  | no payload was supplied for an entry. `dashbuf::open_verified` returns one per entry, so this names a caller that paired a document with the wrong payload list rather than a defect in the document; reported once, at the first entry that has none                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `grid.track-invalid-value`               | a `Fixed` track that is not finite and non-negative, or a `Fraction` weight that is not finite and positive — the same numeric-domain posture as `weight` and stroke width (story #43)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `grid.span-zero`                         | a grid span of 0 spans no tracks and has no meaning; the engine floors it at 1 rather than inventing one, so the honest diagnosis is here (story #43)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `grid.anchor-out-of-range`               | an anchor past its parent's declared track list on that axis — or, with no declared list, past 32766, the largest 0-based anchor whose 1-based line index fits the solver's `i16` lines (the engine saturates the conversion; story #43)                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `vector.shape-geometry-missing`          | a `VectorShape` carrying no `atlas_rect` or no `plane_bounds` (#1021). A flatbuffers struct field with no `(required)` is **absent** rather than defaulted, and `dashscene-core`'s loader reads both behind an `expect` documented as "validated upstream (P4)" — so before this rule such a document validated clean and then panicked the loader. The rest of the story-B1 `vector.*` family predates this table and is still absent from it                                                                                                                                                                                                                                                                     |
| `grid.fraction-track-under-hug`          | a `Fraction` track on an axis the grid container hugs: a fraction divides free space, a hug axis has none, and the track — and everything anchored to it — would silently collapse to zero (story #43)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

Paint vocabulary — run by **both** the load gate and the paint gate, since a
scene can be built without ever passing through a document:

| rule                                 | stands in front of                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paint.gradient.no-stops`            | the painter's `stops.first().expect(..)`. `(required)` mandates presence, not non-emptiness — the false assurance #100 names                                                                                                                                                                                                                                                                                                                                                                         |
| `paint.gradient.stop-budget`         | the painter's assertion against `dashpaint::MAX_GRADIENT_STOPS`                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `paint.gradient.stop-order`          | offsets that do not increase. Each is individually in `0..=1`, so no range rule catches them, but painters take the offsets as a monotonically increasing ramp (Skia's `positions` array) — unordered stops rasterize unpredictably and differ between painters. Equal offsets are allowed: that is how a hard color stop is authored                                                                                                                                                                |
| `paint.gradient.stop-offset-invalid` | a non-finite offset, or one outside `0..=1`                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `paint.stroke.invalid-width`         | a negative or non-finite width                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `geometry.corner-radius-invalid`     | a negative or non-finite corner radius (#128). Runs on both gates like a stroke width, because corners are geometry-free authored intent (`Paint.corners`) and the load gate is the only gate `compile_figma` runs. A clipping node's corners are copied verbatim into every `ClipBox` of its subtree (`crates/dashscene-core/src/arena.rs`), so checking a paint entry's corners catches an out-of-spec clip at its source — the painter's `RRect::new_rect_radii` does not clamp a negative radius |
| `paint.image-out-of-range`           | `ImageTable::resolve`'s panic (#63), for an image fill's asset index and — since issue #1021's rule went in beside it — for a coverage field's                                                                                                                                                                                                                                                                                                                                                       |
| `paint.shadow.invalid-geometry`      | a shadow whose offset or spread is non-finite, or whose blur radius is non-finite or negative (story #45). The painter feeds a sigma derived from `blur` to Skia as a mask-filter sigma and offsets/spreads the shadow geometry, none of which tolerate NaN; a negative Gaussian is meaningless. Spread may be negative (it shrinks the shadow), so only the blur is floored. Runs on both gates like the corner radius                                                                              |
| `geometry.rect-invalid-extent`       | a non-finite (NaN/infinite) or negative width or height (#128). It names what `paint.stroke.exceeds-box` only declines to judge on a non-finite box. Paint gate only until issue #1048: a document carries no _resolved_ extent (P1), which is why the paint gate needs it, but `Node.layout` carries an authored one and the paint gate has no production caller                                                                                                                                    |
| `geometry.rect-invalid-origin`       | a non-finite `x` or `y` (#1048). The extent rule's sibling over the other two members of the same box, and a finiteness rule only — a negative origin is an ordinary offset. Measured on both painters, such a node draws nothing rather than drawing wrongly, so this names a drop under P4 rather than preventing a wrong picture                                                                                                                                                                  |
| `vector.shape-draws-nothing`         | a coverage field with no atlas texels, or a plane quad whose width or height is not finite and positive (#1021). A **warning**: `dashpaint::VectorField::draws` calls this a legal state and both painters take that answer, so the document renders and the node is simply absent. The rule **calls** that predicate rather than restating it — a restatement would be the third copy, which is what #1000 and #1034 were each filed for                                                            |
| `paint.shadow.color-out-of-range`    | a shadow color channel that is non-finite or outside `0..=1` (story #45); the painter multiplies it into a premultiplied surface                                                                                                                                                                                                                                                                                                                                                                     |

Paint gate only — needs the solved box:

| rule                       | stands in front of                                                                                                                                                                                                                                                                                                              |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paint.stroke.exceeds-box` | an `Inside` stroke wider than `min(w, h)`. The painter insets by half the width per side, so the stroked box is `w - width` by `h - width`; above the smaller extent it inverts and the stroke collapses instead of drawing (#100). The threshold is strict: exactly at `min(w, h)` the stroke covers the box, which is correct |
| `paint.entry-out-of-range` | `PaintTable::resolve`'s panic, for a rect whose index misses                                                                                                                                                                                                                                                                    |
| `clip.index-out-of-range`  | `ClipTable::resolve`'s panic, for a rect whose resolved clip region misses (#97). Clip regions exist only on a scene: a document carries clip _intent_ (`Paint.clip`, a bool), while the region a painter consumes is the ancestor-intersected result core computes at commit — and by P1 a result never appears in a document  |

A pool entry is validated **once, at its own index** — it is shared by every
rect referencing it, so reporting per referencing rect would repeat one
authoring mistake N times and bury the rest of the report.

Contribution gate — the document's placeholders against the host's bindings.
Which of the four states warn, when `placeholder.unfilled` is suppressed, and
which two placeholder shapes deliberately do not raise it:
`docs/decisions/a-host-binds-a-contribution-by-id.md`.

| rule                              | why it exists                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `placeholder.unfilled`            | a placeholder whose contribution id no host binding fills (story #1127). A **warning**: nothing here says the document is malformed, only that a migration is unfinished. Suppressed on a `Profile::Core` target **that binds nothing a host contribution can fill** — a lean painter has no host-content mechanism, so the document is correct as it stands; a `Core` caller that binds such an id has contradicted that and is told what it left unfilled. Neither a binding matching nothing (a misspelled id) nor one matching only a placeholder a fragment fills lifts the suppression                                                                                                                                                                                                              |
| `placeholder.undeclared-overload` | a contribution the host binds that no placeholder declares — host content covering a node the designer believes ships, so they keep maintaining artwork nobody sees (issue #851). The one state nothing else catches, and the only cost paid continuously rather than at load. A **warning**, on both profiles. Not raised at all when any placeholder's id is out of range: the gate cannot then read the whole of what the document declares, so the rule is off for the whole document. Findings already made are kept, so the report is not necessarily empty — the signal is the load gate's `placeholder.string-out-of-range`, not the shape of this report. On a `Core` target the same unreadable id can suppress `placeholder.unfilled` too, since the arming set cannot contain it (debt #1275) |

## Import-gate vocabulary

`Construct` names `docs/specification/04-figma-vocabulary-profile.md`'s LATER
and REJECT bands, and nothing else — the NOW band is simply the schema.

    LATER (warn)    LayerBlur, AdvancedBlendMode*, CornerSmoothing,
                    LuminanceMask, ClipOnRotated, KashidaJustification
    REJECT (error)  NoiseOrTextureEffect, ProgressiveBlur,
                    AnimatedBooleanOp, AnimatedVariableFontAxis,
                    VariableWidthStroke (#145)

    * profile:full-only — an Error under profile:core.

The producer maps its source vocabulary onto these (P5: the validator never
parses Figma JSON). `dashc` owns that mapping from #16.

## What the painter may now assume

Every `expect`/`assert` in `dashscene-skia` documented as "validated upstream
(P4)" has a named rule standing in front of it. `MAX_GRADIENT_STOPS` moved from
a private constant in the painter to `dashpaint` (boundary B) so the painter's
assertion, its test, and the validator's rule read one number and cannot drift.

Story #97 closed the last of them: `PaintEntry::clip` no longer exists, the
painter's `unimplemented!` on it is gone, and the `ClipTable::resolve` panic
that replaced it has `clip.index-out-of-range` standing in front of it.
