# Text across the C ABI — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the C ABI an entry point that loads a `.dsb` and the fonts and
atlases its text needs, so a document containing text draws its glyphs on
Android instead of laying its text nodes out as empty leaves.

**Architecture:** `dashscene-ffi` marshals and `dashscene-engine` assembles. A
new `TextResources::from_faces` in engine turns owned byte descriptors into the
`Typesetter` and the atlas list, building both from one family-major walk so the
font-slot pairing cannot be got wrong. A new
`ds_runtime_load_document_with_text` converts C pointers into those descriptors
and calls it. `dashscene-android` gains a second JNI entry point beside the
shipped one.

**Tech Stack:** Rust 2024, `extern "C"` with `catch_unwind` at every boundary,
`dashscene-typeset` (`Typesetter`, `Font`, `AtlasMetrics`), `dashpaint`
(`Atlas`, `ImageAsset`), `jni` 0.22.4, a hand-written C header.

**Spec:** `docs/wip/2026-08-13-text-across-the-c-abi.md`

## A note on this file's own formatting

Every code block here is **fenced and indented two spaces**, so it sits inside
its list item. That is not a style preference. A four-space-indented block
inside a `- [ ]` item is list-item _continuation prose_ under CommonMark, not a
code block — GitHub renders it as `<p>` — and `prim fmt` reflows it to 80
columns, which silently destroys the code. That happened to the first draft of
this file. The records under `docs/decisions/` use indented blocks safely
because theirs sit at the top level, outside any list.

## Global Constraints

- **`DS_ABI_VERSION` stays `1`.** New symbols, new structs and `DsStatus`
  variants **at the tail** are additive. Never renumber an existing variant and
  never change a shipped signature.
- **The three ABI rules hold for every new entry point.** No panic crosses the
  boundary (`guard`), no failure is representable only as a formatted string
  (branch on `DsStatus`; the message is diagnostic), every pointer is checked
  (`DsStatus::NullArgument`, never a dereference).
- **`crates/dashscene-ffi/include/dashscene.h` is hand-written and IS the
  contract.** It is reviewed as one. Never generate it.
- **Commit scopes are pinned** in `.git-std.toml`. Use `dashscene-engine`,
  `dashscene-ffi`, `dashscene-android`, `docs`. There is no `decisions` scope.
- **`Refs #947`, never a closing keyword** next to an issue number, in any
  commit message. `close`, `fix` and `resolve` fire from commit messages that
  land on `main`, in any inflection, and a negation is not a defence.
- **Prose is plain literal English.** No idioms.
- **Format `TypesetError` and `AtlasError` with `{error:?}`.** An earlier
  revision of this line said they derive `Debug` only and implement no
  `Display`. That is false — both implement `Display` by hand
  (`text/mod.rs:1051`, `atlas/mod.rs:199`). `{error:?}` is still what these call
  sites use, because the string is a diagnostic rather than a message to a
  person.
- **Nothing may describe Android as working.** That waits on #885's hardware
  measurement — not a record, not a document, not an issue, not a commit
  message.
- **Run `just test` before every commit** (about 7 s). Run `just build` before
  pushing; `just verify`, which the pre-push hook runs, executes **no test
  tier**.

---

### Task 1: `TextResources::from_faces` in `dashscene-engine`

The assembly and its invariant, testable with no C involved.

**Files:**

- Modify: `crates/dashscene-engine/Cargo.toml` — add
  `dashpaint.workspace = true`
- Modify: `crates/dashscene-engine/src/lib.rs` — beside `TextResources`
- Test: `crates/dashscene-engine/tests/text_resources.rs` (create)

**Interfaces:**

- Consumes: `TextResources::new(Typesetter, Arc<Vec<Atlas>>)`,
  `Typesetter::with_named_font_families(Vec<FontFamily>)`,
  `FontFamily::new(name, Vec<WeightedFont>)`, `WeightedFont::new(Font, u16)`,
  `Font::from_bytes(Vec<u8>, u32)`, `AtlasMetrics::from_bytes(&[u8])`,
  `Atlas::new(ImageAsset, u32, u32, u16, f32, Vec<AtlasGlyph>)`.
- Produces: `dashscene_engine::FaceBytes` with public fields `family: String`,
  `weight: u16`, `font: Vec<u8>`, `face_index: u32`,
  `atlas: Option<AtlasBytes>`; `dashscene_engine::AtlasBytes` with
  `png: Vec<u8>`, `metrics: Vec<u8>`; `dashscene_engine::TextResourcesError`;
  and
  `TextResources::from_faces(Vec<FaceBytes>) -> Result<TextResources, TextResourcesError>`.
  Task 2 maps the error variants onto `DsStatus`.

- [ ] **Step 1: Write the failing test**

  Create `crates/dashscene-engine/tests/text_resources.rs`. The
  non-contiguous-family case is the one that fails if the grouping walk is
  wrong, so it is the headline test rather than an extra.

  ```rust
  //! `TextResources::from_faces` — the assembly the C ABI marshals into
  //! (story #947).
  //!
  //! The property under test is the one that fails **silently** everywhere
  //! else: `TextResources::atlases` is indexed by the slot of the face that
  //! shaped a glyph, so a list in the wrong order samples the wrong face
  //! rather than failing.

  use dashscene_engine::{AtlasBytes, FaceBytes, TextResources, TextResourcesError};

  const INTER_REGULAR: &str = concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../corpus/fonts/inter/Inter-Regular.otf"
  );
  const INTER_SEMIBOLD: &str = concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../corpus/fonts/inter/Inter-SemiBold.otf"
  );
  const ARABIC: &str = concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf"
  );
  const ATLAS_REGULAR: &str =
      concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/inter-ascii");
  const ATLAS_SEMIBOLD: &str = concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/../../corpus/atlas/inter-ascii-semibold"
  );
  const ATLAS_ARABIC: &str =
      concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/atlas/arabic");

  fn read(path: &str) -> Vec<u8> {
      std::fs::read(path).unwrap_or_else(|error| panic!("corpus file {path}: {error}"))
  }

  fn sheet(dir: &str) -> AtlasBytes {
      AtlasBytes {
          png: read(&format!("{dir}/atlas.png")),
          metrics: read(&format!("{dir}/atlas.metrics")),
      }
  }

  fn face(family: &str, weight: u16, font: &str, atlas: Option<&str>) -> FaceBytes {
      FaceBytes {
          family: family.to_string(),
          weight,
          font: read(font),
          face_index: 0,
          atlas: atlas.map(sheet),
      }
  }

  /// The pairing survives a caller that lists one family's faces
  /// **non-contiguously**, which is the case a per-face atlas is for.
  ///
  /// `Typesetter::with_named_font_families` flattens family-major over the
  /// order it is given, so the slot order here is Inter 400, Inter 600, Noto
  /// — not the argument order. An implementation that collected atlases in
  /// argument order would put the Arabic sheet at slot 1, and every SemiBold
  /// glyph would sample it: Arabic letterforms for Latin text, with nothing
  /// failing.
  #[test]
  fn a_faces_atlas_follows_it_through_the_family_major_flatten() {
      let resources = TextResources::from_faces(vec![
          face("Inter", 400, INTER_REGULAR, Some(ATLAS_REGULAR)),
          face("Noto Sans Arabic", 400, ARABIC, Some(ATLAS_ARABIC)),
          face("Inter", 600, INTER_SEMIBOLD, Some(ATLAS_SEMIBOLD)),
      ])
      .expect("the corpus faces and their committed sheets assemble");

      assert_eq!(
          resources.typesetter.fonts().len(),
          3,
          "three faces flatten to three slots"
      );
      assert_eq!(
          resources.typesetter.weights(),
          [400, 600, 400],
          "family-major: Inter's two faces take slots 0 and 1, so the argument order \
           does not survive the flatten"
      );
      assert_eq!(resources.atlases.len(), 3, "one sheet per slot");

      // Compared by the sheet's own bytes. `Atlas::image` is a public field
      // holding the PNG it was built from, so each slot is matched against the
      // file that belongs at it — a real pairing assertion, not a length check.
      let carried: Vec<&[u8]> = resources
          .atlases
          .iter()
          .map(|atlas| atlas.image.bytes.as_slice())
          .collect();
      let expected: Vec<Vec<u8>> = [ATLAS_REGULAR, ATLAS_SEMIBOLD, ATLAS_ARABIC]
          .iter()
          .map(|dir| read(&format!("{dir}/atlas.png")))
          .collect();
      assert_eq!(
          carried,
          expected.iter().map(Vec::as_slice).collect::<Vec<_>>(),
          "each slot carries the sheet of the face that occupies it, in flatten order — \
           the Arabic sheet is at slot 2 though it was argument 1"
      );
  }

  /// Empty is the measure-only cascade and stays legal — it is what
  /// `TextResources::new` already allows, and it is not the same mistake as a
  /// short list.
  #[test]
  fn no_atlases_at_all_is_the_measure_only_cascade() {
      let resources = TextResources::from_faces(vec![face("Inter", 400, INTER_REGULAR, None)])
          .expect("a cascade with no sheets assembles");
      assert!(resources.atlases.is_empty());
  }

  /// A short list resolves an index past its end and a reordered one samples
  /// the wrong face. Neither fails on its own, so the set is rejected here.
  #[test]
  fn a_mixed_set_is_rejected_rather_than_truncated() {
      let error = TextResources::from_faces(vec![
          face("Inter", 400, INTER_REGULAR, Some(ATLAS_REGULAR)),
          face("Inter", 600, INTER_SEMIBOLD, None),
      ])
      .expect_err("some faces carrying a sheet and some not is not representable");
      assert!(matches!(error, TextResourcesError::MixedAtlases));
  }

  #[test]
  fn an_empty_face_list_is_named_rather_than_asserted_on() {
      assert!(matches!(
          TextResources::from_faces(Vec::new()),
          Err(TextResourcesError::NoFaces)
      ));
  }

  /// An empty family name is rejected because nothing could ever select it.
  ///
  /// `FontFamily::name_matches` trims both sides and returns false when either
  /// is empty, so an empty-named family in a **named** cascade occupies font
  /// slots — and therefore atlas slots — that no document request can reach.
  /// Whitespace is the same case, because that function trims first.
  ///
  /// It is **not** rejected to avoid a panic.
  /// `Typesetter::with_named_font_families` asserts on a family whose *faces*
  /// are empty and never inspects the name, and `FontFamily::unnamed`
  /// deliberately constructs an empty name for the pre-#385 cascade shape.
  #[test]
  fn an_unselectable_family_name_is_named_rather_than_silently_kept() {
      for name in ["", "   "] {
          assert!(
              matches!(
                  TextResources::from_faces(vec![face(name, 400, INTER_REGULAR, None)]),
                  Err(TextResourcesError::EmptyFamily { index: 0 })
              ),
              "a family named {name:?} can never be matched, so it is refused"
          );
      }
  }

  #[test]
  fn bytes_that_are_not_a_face_are_named_with_their_index() {
      let error = TextResources::from_faces(vec![
          face("Inter", 400, INTER_REGULAR, None),
          FaceBytes {
              family: "Junk".to_string(),
              weight: 400,
              font: vec![0; 64],
              face_index: 0,
              atlas: None,
          },
      ])
      .expect_err("junk is not a parseable face");
      assert!(matches!(error, TextResourcesError::Font { index: 1, .. }));
  }

  #[test]
  fn metrics_that_do_not_decode_are_named_with_their_index() {
      let error = TextResources::from_faces(vec![FaceBytes {
          family: "Inter".to_string(),
          weight: 400,
          font: read(INTER_REGULAR),
          face_index: 0,
          atlas: Some(AtlasBytes {
              png: read(&format!("{ATLAS_REGULAR}/atlas.png")),
              metrics: vec![0xff; 32],
          }),
      }])
      .expect_err("junk is not a postcard AtlasMetrics");
      assert!(matches!(error, TextResourcesError::Atlas { index: 0, .. }));
  }
  ```

- [ ] **Step 2: Run the test to verify it fails**

  Run: `cargo test -p dashscene-engine --test text_resources`

  Expected: FAIL to compile — `FaceBytes`, `AtlasBytes`, `TextResourcesError`
  and `from_faces` do not exist.

  **No dev-dependency is needed and none should be added.** The test names only
  `dashscene_engine` and reads files with `std::fs`; it compares sheets through
  `Atlas::image`, a public field, so it never decodes metrics itself. Reaching
  for `dashscene-typeset` to re-derive the expected value would be both a wider
  dependency and a weaker assertion, because a mutation in that decode would
  move both sides of the comparison together.

- [ ] **Step 3: Add the `dashpaint` dependency**

  In `crates/dashscene-engine/Cargo.toml`, under `[dependencies]`, after
  `dashscene-core.workspace = true`:

  ```toml
  # `ImageAsset`, for `TextResources::from_faces`. `dashscene-core` uses this
  # type in its own public signatures (`Arena::add_image`) but does not
  # re-export it, and `dashpaint` has no dependencies of its own, so this edge
  # adds nothing to the graph but the name.
  dashpaint.workspace = true
  ```

- [ ] **Step 4: Write the implementation**

  In `crates/dashscene-engine/src/lib.rs`, immediately after the
  `impl TextResources` block. Add `use dashpaint::{ImageAsset, ImageFormat};`
  and `use dashscene_typeset::atlas::AtlasMetrics;`, add `AtlasGlyph` to the
  existing `use dashscene_core::{...}` list, and add `Font`, `FontFamily` and
  `WeightedFont` to the existing `use dashscene_typeset::text::{...}` list.

  ```rust
  /// One face a host supplies, with the atlas its shaped glyphs sample.
  ///
  /// Owned bytes rather than borrowed, because the one caller is a C ABI whose
  /// pointers are valid only for the length of the call.
  #[derive(Debug)]
  pub struct FaceBytes {
      /// The family this face belongs to. Faces sharing a name become one
      /// family, in first-appearance order, however they are ordered here.
      pub family: String,
      /// The CSS weight this face stands for.
      pub weight: u16,
      /// The font file's bytes.
      pub font: Vec<u8>,
      /// Which face within a collection. Zero for a single-face file.
      pub face_index: u32,
      /// The committed sheet, or [`None`] for a measure-only cascade. Either
      /// every face carries one or none does.
      pub atlas: Option<AtlasBytes>,
  }

  /// A committed atlas as its two files' bytes — what `corpus/atlas/*/` holds
  /// and what the MSDF tool emits.
  ///
  /// **Nothing bakes one of these at run time.**
  /// `dashscene_typeset::atlas::generate` shells out to an external pinned
  /// binary and reads its font from a path, so a host arrives with a sheet or
  /// it gets no glyphs.
  #[derive(Debug)]
  pub struct AtlasBytes {
      /// The sheet, PNG-encoded.
      pub png: Vec<u8>,
      /// The postcard `AtlasMetrics` beside it.
      pub metrics: Vec<u8>,
  }

  /// Why a set of [`FaceBytes`] is not a cascade.
  ///
  /// Every variant names the entry it came from, because the caller assembling
  /// the list is a host that cannot see this one.
  #[derive(Debug)]
  #[non_exhaustive]
  pub enum TextResourcesError {
      /// No faces at all. `Typesetter::with_named_font_families` asserts on
      /// this, so it is caught rather than reached.
      NoFaces,
      /// A face declared a family name that is empty once trimmed, which
      /// nothing could ever select: `FontFamily::name_matches` trims both
      /// sides and returns false when either is empty, so such a family
      /// occupies font slots — and therefore atlas slots — that no document
      /// request can reach. Not a panic guard; the assertion in
      /// `with_named_font_families` inspects a family's faces, never its name.
      EmptyFamily { index: usize },
      /// A face's bytes are not a parseable font.
      Font { index: usize, message: String },
      /// An atlas's metrics did not decode.
      Atlas { index: usize, message: String },
      /// Some faces carry a sheet and some do not. The list is indexed by font
      /// slot, so a short one resolves past its end — which is why this is
      /// rejected rather than padded or truncated.
      MixedAtlases,
  }

  impl TextResources {
      /// Assembles a cascade and its atlases from bytes a host supplied.
      ///
      /// **The two lists are built from one walk**, which is the whole point.
      /// Faces are grouped by family in first-appearance order and
      /// `Typesetter::with_named_font_families` flattens family-major over
      /// exactly that order, so a face's atlas lands at the slot its glyphs
      /// will carry however the caller ordered the argument. Building the
      /// atlas list separately is what would let a caller mis-order it, and a
      /// mis-ordered list samples the wrong face rather than failing.
      pub fn from_faces(faces: Vec<FaceBytes>) -> Result<Self, TextResourcesError> {
          if faces.is_empty() {
              return Err(TextResourcesError::NoFaces);
          }
          let sheets = faces.iter().filter(|face| face.atlas.is_some()).count();
          if sheets != 0 && sheets != faces.len() {
              return Err(TextResourcesError::MixedAtlases);
          }

          // Group by family name, keeping first appearance. Indices only, so
          // the owned bytes move once, below, in the flatten's order.
          let mut names: Vec<&str> = Vec::new();
          let mut members: Vec<Vec<usize>> = Vec::new();
          for (index, face) in faces.iter().enumerate() {
              // Trimmed, because `FontFamily::name_matches` trims before
              // comparing: a name of only spaces is as unselectable as "".
              if face.family.trim().is_empty() {
                  return Err(TextResourcesError::EmptyFamily { index });
              }
              match names.iter().position(|name| *name == face.family) {
                  Some(slot) => members[slot].push(index),
                  None => {
                      names.push(&face.family);
                      members.push(vec![index]);
                  }
              }
          }
          let names: Vec<String> = names.into_iter().map(str::to_string).collect();

          let mut taken: Vec<Option<FaceBytes>> = faces.into_iter().map(Some).collect();
          let mut families = Vec::with_capacity(names.len());
          let mut atlases = Vec::new();
          for (name, group) in names.into_iter().zip(members) {
              let mut weighted = Vec::with_capacity(group.len());
              for index in group {
                  let face = taken[index]
                      .take()
                      .expect("each index is grouped exactly once");
                  let font = Font::from_bytes(face.font, face.face_index).map_err(|error| {
                      TextResourcesError::Font {
                          index,
                          message: format!("{error:?}"),
                      }
                  })?;
                  if let Some(sheet) = face.atlas {
                      atlases.push(atlas_from_bytes(sheet, index)?);
                  }
                  weighted.push(WeightedFont::new(font, face.weight));
              }
              families.push(FontFamily::new(name, weighted));
          }
          Ok(Self::new(
              Typesetter::with_named_font_families(families),
              Arc::new(atlases),
          ))
      }
  }

  /// A committed sheet's two files, as the boundary-B atlas a staged run
  /// samples.
  ///
  /// Only glyphs that paint carry a quad, so an empty-outline glyph such as
  /// the space is dropped — the same filter `corpus/showcase` applies, and the
  /// reason `Atlas::new`'s sorted-and-unique assertion still holds:
  /// `AtlasMetrics::glyphs` is sorted by glyph id and filtering preserves
  /// order.
  fn atlas_from_bytes(sheet: AtlasBytes, index: usize) -> Result<Atlas, TextResourcesError> {
      let metrics = AtlasMetrics::from_bytes(&sheet.metrics).map_err(|error| {
          TextResourcesError::Atlas {
              index,
              message: format!("{error:?}"),
          }
      })?;
      let glyphs = metrics
          .glyphs
          .iter()
          .filter_map(|glyph| {
              Some(AtlasGlyph {
                  glyph_id: u32::from(glyph.glyph_id),
                  plane_em: glyph.plane_em?,
                  atlas_px: glyph.atlas_px?,
              })
          })
          .collect();
      Ok(Atlas::new(
          ImageAsset {
              format: ImageFormat::Png,
              bytes: sheet.png,
          },
          metrics.atlas.width,
          metrics.atlas.height,
          metrics.atlas.px_per_em,
          metrics.atlas.distance_range_px,
          glyphs,
      ))
  }
  ```

- [ ] **Step 5: Run the tests to verify they pass**

  Run: `cargo test -p dashscene-engine --test text_resources`

  Expected: PASS. Confirm all seven are in the PASS list **by name** rather than
  reading a total — nextest runs concurrently, and a count attributed to the
  wrong line has been wrong in this repository before.

  `Atlas`'s fields are public (`image`, `width`, `height`, `px_per_em`,
  `distance_range_px`); only `glyphs` is private, behind `Atlas::glyph`. So
  `atlas.image.bytes` is a field access rather than an accessor call.

- [ ] **Step 6: Mutate the fix to prove the test can fail**

  A test that passes with the mechanism removed proves nothing, and story #838's
  headline test did exactly that. Temporarily replace the in-walk atlas push
  with a separate pass that collects sheets in **argument** order, before the
  grouping:

  ```rust
  // Temporary mutation, reverted at the end of this step.
  let atlases: Vec<Atlas> = faces
      .iter()
      .enumerate()
      .filter_map(|(index, face)| {
          face.atlas.as_ref().map(|_| /* build from face.atlas in argument order */)
      })
      .collect::<Result<_, _>>()?;
  ```

  Run: `cargo test -p dashscene-engine --test text_resources`

  Expected: FAIL on `a_faces_atlas_follows_it_through_the_family_major_flatten`,
  because the Arabic sheet lands at slot 1 rather than slot 2. **Confirm the
  failure names that test**, then revert the mutation and re-run to green. If it
  passes, the test is not testing what it claims and must be fixed before going
  on.

- [ ] **Step 7: Commit**

  ```bash
  just test
  git add crates/dashscene-engine/Cargo.toml crates/dashscene-engine/src/lib.rs \
    crates/dashscene-engine/tests/text_resources.rs Cargo.lock
  git commit -m "feat(dashscene-engine): assemble text resources from a host's bytes

  A cascade and its atlases from owned bytes, for the caller that cannot hand
  over Rust values — a C ABI, whose pointers live only for the length of the
  call.

  The two lists are built from one family-major walk rather than separately.
  That is the whole design: the atlas list is indexed by the font slot of the
  face that shaped a glyph, so a list in any other order samples the wrong face
  rather than failing, and a walk that emits both cannot disagree with itself.

  Refs #947."
  ```

---

### Task 2: `ds_runtime_load_document_with_text` in `dashscene-ffi`

**Files:**

- Modify: `crates/dashscene-ffi/src/lib.rs` — `DsStatus`, a shared load body,
  the struct, the entry point, and tests
- Modify: `crates/dashscene-ffi/include/dashscene.h`
- Modify: `crates/dashscene-ffi/tests/abi.c`

**Interfaces:**

- Consumes: Task 1's
  `dashscene_engine::{AtlasBytes, FaceBytes, TextResources,
  TextResourcesError}`
  and `TextResources::from_faces`.
- Produces: `DsFontFace`, a `#[repr(C)]` struct with fields
  `family:
  *const c_char`, `weight: u16`, `face_index: u32`,
  `font_bytes: *const u8`, `font_len: usize`, `atlas_png: *const u8`,
  `atlas_png_len: usize`, `atlas_metrics: *const u8`,
  `atlas_metrics_len: usize`; the exported `ds_runtime_load_document_with_text`;
  and `DsStatus::FontFace = 9` / `DsStatus::Atlas = 10`. Task 3 calls the entry
  point and builds the struct.

- [ ] **Step 1: Write the failing tests**

  Append to the `mod tests` block in `crates/dashscene-ffi/src/lib.rs`. The
  positive test needs no new dependency: the whole point of the new API is that
  bytes are enough, so it reads the corpus files itself.

  ```rust
  /// **A loaded document draws its text**, which is the story's whole
  /// deliverable, asserted on the **committed** tables rather than on the
  /// arena calls — the painter reads `committed()`, and a test that asserted
  /// the document would pass while the feature rendered nothing.
  ///
  /// The `None` half beside it is the pre-#947 picture, and is what says the
  /// fonts are the cause rather than the document.
  /// `docs/decisions/font-resolution-order.md` records this same fixture
  /// measuring four rects, zero glyph runs, and its text node at 0 x 0.
  #[test]
  fn a_document_loaded_with_fonts_stages_glyph_runs_and_measures_its_text() {
      let document = std::fs::read(concat!(
          env!("CARGO_MANIFEST_DIR"),
          "/../../goldens/dsb/v07-text-hug-in-fill.dsb"
      ))
      .expect("the committed text fixture is present");
      let font = std::fs::read(concat!(
          env!("CARGO_MANIFEST_DIR"),
          "/../../corpus/fonts/inter/Inter-Regular.otf"
      ))
      .expect("the corpus font is present");
      let png = std::fs::read(concat!(
          env!("CARGO_MANIFEST_DIR"),
          "/../../corpus/atlas/inter-ascii/atlas.png"
      ))
      .expect("the committed sheet is present");
      let metrics = std::fs::read(concat!(
          env!("CARGO_MANIFEST_DIR"),
          "/../../corpus/atlas/inter-ascii/atlas.metrics"
      ))
      .expect("the committed metrics are present");

      // Held in a local: the pointer must outlive the call.
      let family = std::ffi::CString::new("Inter").expect("no interior nul");
      let face = DsFontFace {
          family: family.as_ptr(),
          weight: 400,
          face_index: 0,
          font_bytes: font.as_ptr(),
          font_len: font.len(),
          atlas_png: png.as_ptr(),
          atlas_png_len: png.len(),
          atlas_metrics: metrics.as_ptr(),
          atlas_metrics_len: metrics.len(),
      };

      /// Glyph runs, and the text node's resolved size.
      fn measured(runtime: *mut DsRuntime) -> (usize, f32, f32) {
          let arena = &unsafe { &*runtime }.arena;
          let scene = arena.committed();
          let row = (0..scene.rects().len() as u32)
              .find(|&row| arena.text(scene.node_of(row)).is_some())
              .expect("the fixture carries a text node");
          let rect = scene.rects()[row as usize];
          (scene.glyphs().runs().len(), rect.w, rect.h)
      }

      let load = |faces: *const DsFontFace, count: usize| {
          let mut runtime = std::ptr::null_mut();
          assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
          assert_eq!(
              unsafe {
                  ds_runtime_load_document_with_text(
                      runtime,
                      document.as_ptr(),
                      document.len(),
                      faces,
                      count,
                  )
              },
              DsStatus::Ok
          );
          let out = measured(runtime);
          unsafe { ds_runtime_free(runtime) };
          out
      };

      let (runs, width, height) = load(&face, 1);
      assert!(
          runs > 0,
          "the host supplied a face and its sheet, so the document's text must reach the \
           painter as glyph runs"
      );
      assert!(
          width > 1.0 && height > 1.0,
          "and the hug-sized text node must measure to its shaped size rather than \
           collapse: {width} x {height}"
      );
      assert_eq!(
          load(std::ptr::null(), 0),
          (0, 0.0, 0.0),
          "and without them it is the pre-#947 picture — no glyphs, and a text node that \
           makes its siblings lay out around a box the design did not specify"
      );
  }

  /// A null face array with a non-zero count is a status, not a dereference.
  #[test]
  fn a_null_face_array_with_a_count_is_a_status() {
      let mut runtime = std::ptr::null_mut();
      assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
      let junk = [0_u8; 32];
      assert_eq!(
          unsafe {
              ds_runtime_load_document_with_text(
                  runtime,
                  junk.as_ptr(),
                  junk.len(),
                  std::ptr::null(),
                  3,
              )
          },
          DsStatus::NullArgument
      );
      unsafe { ds_runtime_free(runtime) };
  }

  /// Bytes that are not a face are `FontFace` — not a panic, and not `Open`.
  /// The faces are validated before the document is opened, which is what
  /// makes junk document bytes safe to use here.
  #[test]
  fn junk_font_bytes_are_a_font_face_status() {
      let family = std::ffi::CString::new("Junk").expect("no interior nul");
      let not_a_font = [0_u8; 64];
      let face = DsFontFace {
          family: family.as_ptr(),
          weight: 400,
          face_index: 0,
          font_bytes: not_a_font.as_ptr(),
          font_len: not_a_font.len(),
          atlas_png: std::ptr::null(),
          atlas_png_len: 0,
          atlas_metrics: std::ptr::null(),
          atlas_metrics_len: 0,
      };
      let not_a_document = [0_u8; 32];
      let mut runtime = std::ptr::null_mut();
      assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
      assert_eq!(
          unsafe {
              ds_runtime_load_document_with_text(
                  runtime,
                  not_a_document.as_ptr(),
                  not_a_document.len(),
                  &face,
                  1,
              )
          },
          DsStatus::FontFace
      );
      unsafe { ds_runtime_free(runtime) };
  }

  /// The shipped symbols are unchanged, which is what "additive" has to mean.
  #[test]
  fn the_abi_version_did_not_move() {
      assert_eq!(DS_ABI_VERSION, 1);
      assert_eq!(DsStatus::Panic as i32, 8);
  }
  ```

- [ ] **Step 2: Run the tests to verify they fail**

  Run: `cargo test -p dashscene-ffi`

  Expected: FAIL to compile — `DsFontFace`, `ds_runtime_load_document_with_text`
  and `DsStatus::FontFace` do not exist.

- [ ] **Step 3: Add the two status variants**

  In `crates/dashscene-ffi/src/lib.rs`, at the **tail** of `enum DsStatus`,
  after `Panic = 8`:

  ```rust
  /// A face descriptor is unusable: its `family` is not UTF-8, or its
  /// `font_bytes` do not parse as a font face.
  FontFace = 9,
  /// An atlas is unusable: its `atlas_metrics` did not decode, or the set is
  /// mixed — some faces carrying a sheet and some not. The atlas list is
  /// indexed by font slot, so a short one resolves past its end.
  Atlas = 10,
  ```

- [ ] **Step 4: Extract the shared load body**

  Replace the body of `ds_runtime_load_document` with a call to a private
  function, so both entry points run one implementation. Keep the existing
  comments about the fresh arena and `document_replaced`. **Delete** the comment
  saying nothing equivalent to a `TextResources` can cross a C boundary: this
  task falsifies it, and a stale comment asserting what the code no longer does
  is this repository's most common defect.

  ```rust
  /// The load both entry points run. `text` is what the caller could supply.
  fn load_into(
      runtime: &mut DsRuntime,
      bytes: &[u8],
      text: Option<TextResources>,
  ) -> DsStatus {
      let (document, payloads) = match dashbuf::open_verified(bytes) {
          Ok(opened) => opened,
          Err(error) => {
              set_last_error(format!("{error:?}"));
              return DsStatus::Open;
          }
      };
      let report = dashscene_validator::validate_document(&document);
      if report.has_errors() {
          set_last_error(format!("{report:?}"));
          return DsStatus::Gate;
      }
      // A fresh arena per load, so a second load does not stack a second
      // document on the first. The generation restart that implies is exactly
      // what `document_replaced` is for, and it is reported below.
      runtime.arena = Arena::new();
      dashscene_core::load_document(&document, &payloads, &mut runtime.arena);
      // `owning` rather than `with_text`: `attach_live` keeps its
      // `Box<dyn LayoutSolver>` for the life of the scene, so the solver in it
      // is `'static` and a borrowed typesetter cannot travel in it.
      let solver: Box<dyn dashscene_core::LayoutSolver> = match text {
          Some(text) => Box::new(TaffySolver::owning(text)),
          None => Box::new(TaffySolver::new()),
      };
      runtime.scene = Some(dashlang::attach_live(&mut runtime.arena, solver));
      if let Some(surface) = runtime.surface.as_mut() {
          surface.document_replaced();
      }
      DsStatus::Ok
  }
  ```

- [ ] **Step 5: Add the struct**

  Add `use dashscene_engine::{TaffySolver, TextResources, TextResourcesError};`
  to the imports, replacing the existing `TaffySolver` import.

  ```rust
  /// One face a host hands [`ds_runtime_load_document_with_text`], with the
  /// atlas its shaped glyphs sample.
  ///
  /// **The atlas is in here rather than in a second array on purpose.** The
  /// atlas list is indexed by the font slot of the face that shaped a glyph,
  /// so a list in any other order samples the wrong face rather than failing.
  /// Pairing them here means the library builds both from one walk and a
  /// caller cannot get the order wrong — including when it lists one family's
  /// faces non-contiguously.
  ///
  /// `atlas_png` and `atlas_metrics` may both be null, which is a measure-only
  /// cascade: text is shaped and measured, and no glyph run is staged. Either
  /// every face carries a sheet or none does; [`DsStatus::Atlas`] rejects a
  /// mixed set.
  #[repr(C)]
  #[derive(Debug, Clone, Copy)]
  pub struct DsFontFace {
      /// The family, as NUL-terminated UTF-8. Faces sharing a name become one
      /// family however they are ordered in the array.
      pub family: *const c_char,
      /// The CSS weight this face stands for.
      pub weight: u16,
      /// Which face within a collection. Zero for a single-face file.
      pub face_index: u32,
      pub font_bytes: *const u8,
      pub font_len: usize,
      pub atlas_png: *const u8,
      pub atlas_png_len: usize,
      pub atlas_metrics: *const u8,
      pub atlas_metrics_len: usize,
  }
  ```

- [ ] **Step 6: Add the marshalling helper**

  ```rust
  /// Reads the descriptors into owned bytes, or says which pointer was null.
  ///
  /// # Safety
  ///
  /// `faces` must point to `count` readable descriptors whose own pointers are
  /// valid for the lengths beside them.
  unsafe fn faces_from_c(
      faces: *const DsFontFace,
      count: usize,
  ) -> Result<Vec<dashscene_engine::FaceBytes>, DsStatus> {
      let faces = unsafe { std::slice::from_raw_parts(faces, count) };
      let mut out = Vec::with_capacity(count);
      for (index, face) in faces.iter().enumerate() {
          if face.family.is_null() || face.font_bytes.is_null() {
              set_last_error(format!(
                  "ds_runtime_load_document_with_text: face {index} has a null family or \
                   font_bytes"
              ));
              return Err(DsStatus::NullArgument);
          }
          let family = match unsafe { std::ffi::CStr::from_ptr(face.family) }.to_str() {
              Ok(family) => family.to_string(),
              Err(error) => {
                  set_last_error(format!("face {index}: family is not UTF-8: {error}"));
                  return Err(DsStatus::FontFace);
              }
          };
          let atlas = if face.atlas_png.is_null() || face.atlas_metrics.is_null() {
              None
          } else {
              Some(dashscene_engine::AtlasBytes {
                  png: unsafe {
                      std::slice::from_raw_parts(face.atlas_png, face.atlas_png_len)
                  }
                  .to_vec(),
                  metrics: unsafe {
                      std::slice::from_raw_parts(face.atlas_metrics, face.atlas_metrics_len)
                  }
                  .to_vec(),
              })
          };
          out.push(dashscene_engine::FaceBytes {
              family,
              weight: face.weight,
              font: unsafe { std::slice::from_raw_parts(face.font_bytes, face.font_len) }
                  .to_vec(),
              face_index: face.face_index,
              atlas,
          });
      }
      Ok(out)
  }
  ```

- [ ] **Step 7: Add the entry point**

  ```rust
  /// Loads a `.dsb` held in memory, with the fonts and atlases its text needs.
  ///
  /// [`ds_runtime_load_document`] is this call with no faces, and stays
  /// exactly that. A null `faces`, or a zero `face_count`, is a document
  /// loaded without text: its text nodes lay out as empty leaves and no glyph
  /// run is staged.
  ///
  /// **What a host must supply, and what it cannot get here.** A face is its
  /// font file's bytes plus the family and weight it stands for. An atlas is a
  /// committed MSDF sheet — a PNG and the metrics blob beside it — and
  /// **nothing bakes one at run time**: the generator is an external pinned
  /// binary that reads a font from a path, so these arrive with the host or
  /// its text is measured and never drawn.
  ///
  /// Adding this symbol did not move [`DS_ABI_VERSION`].
  ///
  /// # Safety
  ///
  /// `bytes` must point to `len` readable bytes, `runtime` must be live, and
  /// `faces` must point to `face_count` readable [`DsFontFace`] whose own
  /// pointers are valid for the lengths beside them. Nothing is retained:
  /// every byte is copied before this returns.
  #[unsafe(no_mangle)]
  pub unsafe extern "C" fn ds_runtime_load_document_with_text(
      runtime: *mut DsRuntime,
      bytes: *const u8,
      len: usize,
      faces: *const DsFontFace,
      face_count: usize,
  ) -> DsStatus {
      guard(|| {
          if runtime.is_null() || bytes.is_null() {
              set_last_error("ds_runtime_load_document_with_text: runtime or bytes is null");
              return DsStatus::NullArgument;
          }
          if faces.is_null() && face_count != 0 {
              set_last_error(
                  "ds_runtime_load_document_with_text: faces is null but face_count is not 0",
              );
              return DsStatus::NullArgument;
          }
          let runtime = unsafe { &mut *runtime };
          let bytes = unsafe { std::slice::from_raw_parts(bytes, len) };

          // The faces are read and assembled BEFORE the document is opened, so
          // a bad cascade is reported as itself rather than as whatever the
          // document turned out to be. `tests/abi.c` depends on that ordering.
          let text = if face_count == 0 {
              None
          } else {
              let described = match unsafe { faces_from_c(faces, face_count) } {
                  Ok(described) => described,
                  Err(status) => return status,
              };
              match TextResources::from_faces(described) {
                  Ok(text) => Some(text),
                  Err(error) => {
                      set_last_error(format!("{error:?}"));
                      return match error {
                          TextResourcesError::Atlas { .. }
                          | TextResourcesError::MixedAtlases => DsStatus::Atlas,
                          _ => DsStatus::FontFace,
                      };
                  }
              }
          };
          load_into(runtime, bytes, text)
      })
  }
  ```

- [ ] **Step 8: Run the tests to verify they pass**

  Run: `cargo test -p dashscene-ffi`

  Expected: PASS. Confirm
  `a_document_loaded_with_fonts_stages_glyph_runs_and_measures_its_text` is in
  the PASS list **by name**; it is the one that carries the deliverable.

- [ ] **Step 9: Mutate the fix to prove the positive test can fail**

  Temporarily change `load_into` so the `Some(text)` arm builds
  `TaffySolver::new()` too:

  ```rust
  // Temporary mutation, reverted at the end of this step.
  let solver: Box<dyn dashscene_core::LayoutSolver> = Box::new(TaffySolver::new());
  ```

  Run: `cargo test -p dashscene-ffi`

  Expected: FAIL on
  `a_document_loaded_with_fonts_stages_glyph_runs_and_measures_its_text`, at
  `runs > 0`. **Confirm the failure names that test**, then revert and re-run to
  green.

- [ ] **Step 10: Add the header entries**

  In `crates/dashscene-ffi/include/dashscene.h`: the two values after
  `DS_PANIC = 8` (which gains a trailing comma), the struct after
  `DsSurfaceKind`, and the prototype after `ds_runtime_load_document`. The
  header is the contract, so this is written rather than generated.

  ```c
    /* A face descriptor is unusable: family is not UTF-8, or font_bytes do
     * not parse as a font face. */
    DS_FONT_FACE = 9,
    /* An atlas is unusable: atlas_metrics did not decode, or the set is mixed
     * — some faces carrying a sheet and some not. */
    DS_ATLAS = 10
  } DsStatus;

  /*
   * One face, with the atlas its shaped glyphs sample.
   *
   * The atlas is in here rather than in a second array on purpose: the atlas
   * list is indexed by the font slot of the face that shaped a glyph, so a
   * list in any other order samples the wrong face RATHER THAN FAILING.
   * Pairing them here means the library builds both from one walk and you
   * cannot get the order wrong — including when you list one family's faces
   * non-contiguously.
   *
   * atlas_png and atlas_metrics may both be NULL: that is a measure-only
   * cascade, where text is shaped and measured and no glyph is drawn. Either
   * every face carries a sheet or none does; a mixed set is DS_ATLAS.
   */
  typedef struct DsFontFace {
    const char *family;  /* NUL-terminated UTF-8 */
    uint16_t weight;     /* CSS weight */
    uint32_t face_index; /* index within a collection; 0 for one face */
    const uint8_t *font_bytes;
    size_t font_len;
    const uint8_t *atlas_png;
    size_t atlas_png_len;
    const uint8_t *atlas_metrics;
    size_t atlas_metrics_len;
  } DsFontFace;

  /*
   * Loads a .dsb held in memory, with the fonts and atlases its text needs.
   *
   * ds_runtime_load_document is this call with no faces. A NULL faces, or a
   * zero face_count, loads without text: text nodes lay out as empty leaves
   * and no glyph is drawn.
   *
   * WHAT YOU MUST SUPPLY. A face is a font file's bytes plus the family and
   * weight it stands for. An atlas is a committed MSDF sheet — a PNG and the
   * metrics blob beside it. NOTHING BAKES ONE AT RUN TIME: the generator is
   * an external pinned binary that reads a font from a path, so these arrive
   * with you or your text is measured and never drawn.
   *
   * The faces are validated before the document is opened, so a bad cascade is
   * reported as itself rather than as whatever the document turned out to be.
   *
   * Nothing is retained: every byte is copied before this returns.
   *
   * Adding this symbol did not move DS_ABI_VERSION.
   */
  DsStatus ds_runtime_load_document_with_text(DsRuntime *runtime,
                                              const uint8_t *bytes, size_t len,
                                              const DsFontFace *faces,
                                              size_t face_count);
  ```

- [ ] **Step 11: Exercise it from C**

  In `crates/dashscene-ffi/tests/abi.c`, before the teardown. This is the check
  only a C caller can make: that the header declares what the library exports,
  so a missing or renamed symbol is a link error here.

  ```c
  /* The text entry point and its struct exist as this header declares them.
   * Junk bytes, so this exercises the symbol and the argument checks rather
   * than a document: a real .dsb is not reachable from this program, and the
   * faces are validated first in any case. */
  DsFontFace face;
  memset(&face, 0, sizeof face);
  face.family = "Inter";
  face.weight = 400;
  uint8_t not_a_font[64] = {0};
  face.font_bytes = not_a_font;
  face.font_len = sizeof not_a_font;
  uint8_t not_a_document[32] = {0};
  check(ds_runtime_load_document_with_text(runtime, not_a_document,
                                           sizeof not_a_document, &face,
                                           1) == DS_FONT_FACE,
        "a face that does not parse is DS_FONT_FACE from C");
  check(ds_runtime_load_document_with_text(runtime, not_a_document,
                                           sizeof not_a_document, NULL,
                                           3) == DS_NULL_ARGUMENT,
        "a null face array with a count is a status, not a crash");
  ```

  `abi.c` already includes `<string.h>`, so `memset` needs no new include —
  confirm that before adding one.

- [ ] **Step 12: Run the C test**

  Run: `just c-abi`

  Expected: every line `ok`, exit 0.

- [ ] **Step 13: Commit**

  ```bash
  just test
  git add crates/dashscene-ffi/src/lib.rs crates/dashscene-ffi/include/dashscene.h \
    crates/dashscene-ffi/tests/abi.c
  git commit -m "feat(dashscene-ffi): a document load that receives fonts

  ds_runtime_load_document_with_text takes an array of face descriptors, each
  pairing one face with the atlas its shaped glyphs sample, so a .dsb
  containing text is measured and drawn through the C ABI.

  A new symbol rather than a parameter on the shipped one, so DS_ABI_VERSION
  stays 1 and a host built against the older header keeps working. The atlas
  sits inside the descriptor because the atlas list is indexed by font slot: a
  list in any other order samples the wrong face rather than failing, and one
  walk cannot disagree with itself.

  Refs #947."
  ```

---

### Task 3: The JNI half in `dashscene-android`

**This compiles for `aarch64-linux-android` and nowhere else, and it cannot be
run without a device.** Its correctness rests on compilation and review. Do not
write, in a commit message or anywhere else, that it was exercised.

**Files:**

- Modify: `crates/dashscene-android/src/host.rs` — `DocumentFrames`, `attach`,
  and a second JNI entry point
- Modify:
  `crates/dashscene-android/harness/java/dev/driftsys/dashscene/DashsceneNative.java`

**Interfaces:**

- Consumes: Task 2's `DsFontFace` and `ds_runtime_load_document_with_text`.
- Produces:
  `Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreatedWithText`,
  and the Java `nativeSurfaceCreatedWithText` declaration beside it.

- [ ] **Step 1: Hold the faces on `DocumentFrames`**

  The bytes must be held for the same reason the document is: `attach` runs
  again after a recoverable surface loss, and it needs them then. Add the field
  to the struct in `crates/dashscene-android/src/host.rs`, and the type beside
  it. Update both existing `DocumentFrames { .. }` constructions to pass
  `faces: Vec::new()`.

  ```rust
  /// One face as this host holds it, so `attach` can rebuild the borrowed
  /// `DsFontFace` array on every surface cycle.
  ///
  /// The family is a `CString` rather than a `String` because the ABI takes a
  /// NUL-terminated pointer, and building one per attach would leave the
  /// pointer dangling the moment the temporary dropped.
  struct OwnedFace {
      family: std::ffi::CString,
      weight: u16,
      font: Vec<u8>,
      atlas_png: Vec<u8>,
      atlas_metrics: Vec<u8>,
  }

  // ...and on `struct DocumentFrames`:

      /// The cascade and its sheets, **kept for the life of this object**, for
      /// the same reason `document` is: a rebuild after a recoverable surface
      /// loss detaches — which frees the runtime — and attaches again, and an
      /// attach needs them. Empty is a document loaded without text.
      faces: Vec<OwnedFace>,
  ```

- [ ] **Step 2: Load through the new entry point**

  In `impl Frames for DocumentFrames`, replace the `ds_runtime_load_document`
  call. An empty `faces` yields a zero count, which is what the old call did.

  ```rust
  let descriptors: Vec<dashscene_ffi::DsFontFace> = self
      .faces
      .iter()
      .map(|face| dashscene_ffi::DsFontFace {
          family: face.family.as_ptr(),
          weight: face.weight,
          face_index: 0,
          font_bytes: face.font.as_ptr(),
          font_len: face.font.len(),
          atlas_png: face.atlas_png.as_ptr(),
          atlas_png_len: face.atlas_png.len(),
          atlas_metrics: face.atlas_metrics.as_ptr(),
          atlas_metrics_len: face.atlas_metrics.len(),
      })
      .collect();
  // SAFETY: `runtime` is live, `document` is a readable slice, and every
  // pointer in `descriptors` borrows a field of `self.faces`, which outlives
  // this call.
  let loaded = unsafe {
      dashscene_ffi::ds_runtime_load_document_with_text(
          runtime,
          self.document.as_ptr(),
          self.document.len(),
          descriptors.as_ptr(),
          descriptors.len(),
      )
  };
  if loaded != DsStatus::Ok {
      return Err(format!("load_document: {loaded:?} {}", last_error()));
  }
  ```

  A face with no sheet is deliberately not representable here: this host either
  has both or supplies no faces at all. The ABI rejects a mixed set, and a
  measure-only cascade is not something a drawing host wants.

- [ ] **Step 3: Add the second JNI entry point**

  Beside `Java_..._nativeSurfaceCreated`, which is **not changed**: its Java
  signature is the contract with any embedder. Add `JIntArray`, `JObjectArray`
  and `JString` to the `use jni::objects::{...}` import, and **verify each name
  and method against the pinned `jni` 0.22.4** rather than trusting this plan.

  ```rust
  /// Creates a host that draws a compiled `.dsb` with the fonts its text
  /// needs, and starts its frame loop.
  ///
  /// The five arrays are parallel and must be the same length: one entry per
  /// face — a family name, a CSS weight, a font file's bytes, and the
  /// committed MSDF sheet the face's glyphs sample. A length disagreement is a
  /// 0 handle and a log line rather than a cascade assembled from entries that
  /// do not belong together.
  ///
  /// **Nothing bakes an atlas at run time**, so a host reads these from its
  /// own assets. `nativeSurfaceCreated` is this call with no faces.
  ///
  /// # Safety
  ///
  /// Called by the JVM with a valid environment and a live `Surface`.
  #[unsafe(no_mangle)]
  pub extern "system" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreatedWithText<
      'local,
  >(
      mut unowned: EnvUnowned<'local>,
      _class: JClass<'local>,
      surface: JObject<'local>,
      document: JByteArray<'local>,
      families: JObjectArray<'local>,
      weights: JIntArray<'local>,
      fonts: JObjectArray<'local>,
      atlas_pngs: JObjectArray<'local>,
      atlas_metrics: JObjectArray<'local>,
      width: jint,
      height: jint,
  ) -> jlong {
      unowned
          .with_env(|env| -> jni::errors::Result<jlong> {
              let bytes = env.convert_byte_array(&document)?;

              // Every array must agree with `families`. `weights` is checked
              // apart from the other three because it is a primitive array and
              // they are object arrays, so the two cannot share one list.
              let count = env.get_array_length(&families)?;
              let mismatched = if env.get_array_length(&weights)? != count {
                  Some("weights")
              } else if env.get_array_length(&fonts)? != count {
                  Some("fonts")
              } else if env.get_array_length(&atlas_pngs)? != count {
                  Some("atlasPngs")
              } else if env.get_array_length(&atlas_metrics)? != count {
                  Some("atlasMetrics")
              } else {
                  None
              };
              if let Some(what) = mismatched {
                  log(&format!(
                      "nativeSurfaceCreatedWithText: {what} has a different length from families"
                  ));
                  return Ok(0);
              }

              let mut faces = Vec::with_capacity(count as usize);
              for index in 0..count {
                  let name: String = env
                      .get_string(&JString::from(
                          env.get_object_array_element(&families, index)?,
                      ))?
                      .into();
                  let Ok(family) = std::ffi::CString::new(name) else {
                      log("nativeSurfaceCreatedWithText: a family name contains a NUL");
                      return Ok(0);
                  };
                  let mut weight = [0_i32; 1];
                  env.get_int_array_region(&weights, index, &mut weight)?;
                  faces.push(OwnedFace {
                      family,
                      weight: weight[0].clamp(1, 1000) as u16,
                      font: env.convert_byte_array(&JByteArray::from(
                          env.get_object_array_element(&fonts, index)?,
                      ))?,
                      atlas_png: env.convert_byte_array(&JByteArray::from(
                          env.get_object_array_element(&atlas_pngs, index)?,
                      ))?,
                      atlas_metrics: env.convert_byte_array(&JByteArray::from(
                          env.get_object_array_element(&atlas_metrics, index)?,
                      ))?,
                  });
              }

              // SAFETY: `env` and `surface` are the JVM's own, valid for this
              // call.
              let window = unsafe {
                  ndk_sys::ANativeWindow_fromSurface(
                      env.get_raw().cast(),
                      surface.as_raw().cast(),
                  )
              };
              if window.is_null() {
                  log("ANativeWindow_fromSurface returned null");
                  return Ok(0);
              }
              let frames = move || -> Box<dyn Frames> {
                  Box::new(DocumentFrames {
                      runtime: std::ptr::null_mut(),
                      document: bytes,
                      faces,
                  })
              };
              // SAFETY: `window` is the reference `fromSurface` returned, which
              // this crate owns until the handshake completes.
              let host = unsafe {
                  loop_::start(
                      window.cast(),
                      frames,
                      width.max(0) as u32,
                      height.max(0) as u32,
                  )
              };
              if host.is_null() {
                  // SAFETY: the one reference `fromSurface` gave this crate.
                  unsafe { ndk_sys::ANativeWindow_release(window) };
                  return Ok(0);
              }
              Ok(host as jlong)
          })
          .resolve::<LogErrorAndDefault>()
  }
  ```

- [ ] **Step 4: Declare it in Java**

  In `DashsceneNative.java`, after `nativeSurfaceCreated`.

  ```java
  /**
   * Hands over a Surface, the document to draw, and the fonts its text needs,
   * and starts the frame loop on its own thread.
   *
   * <p>The five arrays are parallel and must be the same length: one entry per
   * face. A length disagreement returns 0 and logs, rather than assembling a
   * cascade from entries that do not belong together.
   *
   * <p>An atlas is a committed MSDF sheet — a PNG and the metrics blob beside
   * it. <b>Nothing bakes one at run time</b>, so read them from your own
   * assets. {@link #nativeSurfaceCreated} is this call with no faces, and a
   * document loaded that way lays its text nodes out as empty leaves.
   *
   * @param families one family name per face; faces sharing a name become one
   *     family however they are ordered
   * @param weights CSS weight per face, parallel to families
   * @param fonts the font file's bytes per face
   * @param atlasPngs the sheet per face
   * @param atlasMetrics the metrics blob per face
   * @return an opaque handle, or 0. The same caveat as
   *     {@link #nativeSurfaceCreated}: a non-zero handle does not mean the
   *     runtime started.
   */
  public static native long nativeSurfaceCreatedWithText(
          Surface surface, byte[] document, String[] families, int[] weights,
          byte[][] fonts, byte[][] atlasPngs, byte[][] atlasMetrics,
          int width, int height);
  ```

- [ ] **Step 5: Cross-compile and lint behind the platform cfg**

  ```bash
  just android
  cargo clippy -p dashscene-android --target aarch64-linux-android \
    --all-targets -- -D warnings
  ```

  Expected: both green. This is the **only** gate this task has — there is no
  device. `just lint` cannot see behind the platform `cfg`, which is why the
  second command is run by hand.

- [ ] **Step 6: Commit**

  ```bash
  just test
  git add crates/dashscene-android/src/host.rs \
    crates/dashscene-android/harness/java/dev/driftsys/dashscene/DashsceneNative.java
  git commit -m "feat(dashscene-android): a JNI entry point that carries fonts

  nativeSurfaceCreatedWithText takes five parallel arrays — one entry per face
  — and holds them for the life of the host, for the same reason the document
  is held: a rebuild after a recoverable surface loss attaches again, and an
  attach needs them.

  A second entry point rather than a changed one. The shipped Java signature is
  the contract with any embedder, and renaming or re-typing it breaks the link
  at run time rather than at build time.

  Cross-compiled for aarch64-linux-android. Not run: that needs a device.

  Refs #947."
  ```

---

### Task 4: The records, and what the crate documentation now says

**Files:**

- Modify: `crates/dashscene-ffi/src/lib.rs` — the module documentation's text
  section
- Modify: `docs/decisions/font-resolution-order.md` — the consequence naming the
  C ABI as unfixed
- Move: `docs/wip/2026-08-13-text-across-the-c-abi.md` and
  `docs/wip/2026-08-14-text-across-the-c-abi-PLAN.md` to `docs/archive/`
- Do **not** modify: `docs/wip/README.md`

- [ ] **Step 1: Rewrite the ABI's text section**

  Replace the whole
  `# Text is absent from the document load, and this is the
  last host it is`
  section in `crates/dashscene-ffi/src/lib.rs`. It must state what a host
  supplies, must say the part that does not go away, and must not say that
  Android works.

  ```rust
  //! # What a host supplies for text
  //!
  //! [`ds_runtime_load_document_with_text`] takes the fonts and atlases a
  //! document's text needs, because the document carries neither and cannot:
  //! `docs/decisions/font-resolution-order.md` makes an embedded font step 1
  //! and records why nothing implements it, and a rasterised atlas must never
  //! be embedded at all — it is a result, and P1 forbids results in the
  //! document.
  //!
  //! A face is its font file's bytes plus the family and the CSS weight it
  //! stands for. An atlas is a committed MSDF sheet: a PNG and the postcard
  //! metrics blob beside it, which is what `corpus/atlas/*/` holds.
  //!
  //! **Nothing bakes an atlas at run time**, and that is a constraint a host
  //! plans around rather than a gap that will close on its own.
  //! `dashscene_typeset::atlas::generate` shells out to an external pinned
  //! binary and reads its font from a path
  //! (`docs/decisions/atlas-gen-external-pinned-binary.md`). So a sheet is
  //! built where the build runs, and travels with the host.
  //!
  //! [`ds_runtime_load_document`] is the same call with no faces, and stays
  //! exactly that: a document loaded through it lays its text nodes out as
  //! **empty leaves** and draws **no glyphs**, and the damage is not confined
  //! to the missing letters — a hug-sized text node that measures to nothing
  //! makes its siblings lay out around a box the design did not specify. That
  //! is now a choice a caller makes rather than one made for it, which is what
  //! story #947 changed.
  ```

- [ ] **Step 2: Edit the decision record in place**

  In `docs/decisions/font-resolution-order.md`, the consequence beginning **The
  C ABI is not fixed and the reason is different in kind** is now false. Replace
  it, keeping the record's voice, its list indentation and its 80-column wrap.
  Note that the records use **indented** code blocks and no fenced ones; this
  replacement is prose, so it needs neither.

  > **The C ABI is fixed too, by a second entry point** (story #947).
  > `ds_runtime_load_document_with_text` takes an array of face descriptors,
  > each pairing a face — its font bytes, family and CSS weight — with the
  > committed sheet its glyphs sample. Neither a `Typesetter` nor an `Atlas`
  > crosses the boundary; their **inputs** do, and
  > `dashscene_engine::TextResources::from_faces` assembles them on the far
  > side. A new symbol rather than a parameter, so `DS_ABI_VERSION` stays 1.
  >
  > The atlas sits inside the descriptor rather than in a parallel array because
  > the atlas list is indexed by the font slot of the face that shaped a glyph:
  > a list in any other order samples the wrong face rather than failing, and
  > one family-major walk that emits both lists cannot disagree with itself.
  >
  > **What does not change is that a host must arrive with a baked sheet.** The
  > generator is an external pinned binary that reads a font from a path, so no
  > entry point can make one. That is the constraint step 1 still waits on,
  > rather than a limitation of this ABI's shape.

  Do not touch the consequence about what stays blocked — the document carrying
  its own fonts, the `AssetKind` finding and the bank question are all unchanged
  by this story.

- [ ] **Step 3: File the duplication as debt**

  `corpus/showcase/src/resources.rs`'s `load_atlas` now does what engine's
  private `atlas_from_bytes` does. Leave the showcase alone and file it **on a
  milestone** — debt with no milestone is invisible at every slice close.

  ```bash
  gh issue create --label debt \
    --milestone "v0.19 — Android, the C ABI, and layer 0" \
    --title "corpus/showcase re-implements the atlas conversion dashscene-engine now owns" \
    --body 'Story #947 added `dashscene_engine::TextResources::from_faces`, whose private
  `atlas_from_bytes` turns a committed sheet PNG and its postcard metrics into a
  boundary-B `Atlas`. The `load_atlas` in `corpus/showcase/src/resources.rs`
  does the same conversion, including the same drop of empty-outline glyphs.

  Left alone in #947 deliberately: the showcase version is a `LazyLock` static
  with its own panic messages, and refactoring adjacent working code was outside
  that story.

  The two can drift. A change to the glyph filter, or to `Atlas::new` arguments,
  has to be made twice, and nothing fails if only one is.

  Refs #947.'
  ```

- [ ] **Step 4: Archive the working memory**

  The spec and this plan go straight to `docs/archive/`, verbatim, in the commit
  that writes the records above. They never appear in `docs/wip/` on `main`, so
  **`docs/wip/README.md` is not edited and its count does not move** — the rule
  that file states for the blur-colour-space prompt.

  ```bash
  git mv docs/wip/2026-08-13-text-across-the-c-abi.md docs/archive/
  git mv docs/wip/2026-08-14-text-across-the-c-abi-PLAN.md docs/archive/
  git ls-files docs/wip/ | grep -v 'README.md$' | wc -l
  ```

  Expected: `10`, the same number `docs/wip/README.md` states. If it is not 10,
  stop and re-read that ledger before editing anything — it has gone stale seven
  times, always through a commit that moved a file without it.

- [ ] **Step 5: Check no record cites the moved files**

  Nineteen records in `docs/decisions/` carry a `docs/wip/` citation and one has
  pointed at nothing since 2026-07-29. Neither file moved here was cited by any
  record — both were created on this branch — but confirm rather than assume.

  ```bash
  grep -rn "wip/2026-08-13-text-across-the-c-abi\|wip/2026-08-14-text-across-the-c-abi" \
    docs/ crates/ --include=*.md --include=*.rs | grep -v '^docs/archive/'
  ```

  Expected: no output.

- [ ] **Step 6: Verify and commit**

  ```bash
  just build
  ```

  Quote its `Summary` line in the pull request body. Then, because `Cargo.lock`
  moved when `dashpaint` was added to `dashscene-engine`, and `Cargo.lock` is in
  the `packer` filter:

  ```bash
  just calibrate
  ```

  Expected: 10 tests, about 54 s, green.

  ```bash
  git add -A
  git commit -m "docs(docs): what a host supplies for text, and the record it changes

  The ABI module documentation stops describing a gap and states what a host
  must supply, including the part that does not go away: nothing bakes an atlas
  at run time, so a sheet is built where the build runs and travels with the
  host.

  font-resolution-order.md has its consequence naming the C ABI as unfixed
  edited in place rather than superseded, which is the rule for a decision that
  changes a recorded one.

  The spec and plan are archived verbatim. They never landed in docs/wip/ on
  main, so that directory ledger is unchanged and its count does not move.

  Refs #947."
  ```

---

## Before the pull request

- **Open it as an ordinary pull request, never a draft.** A draft requests no
  reviewers, and `/code-review` stops without reviewing.
- **Run `/code-review` while CI runs, not after it.** Neither answer depends on
  the other. Capture every finding as a checklist in the description; never drop
  one silently.
- **Ask for the fan-out, not an author pass.** On the Android branch the fan-out
  found a soundness bug (an FFI enum taken by value), a false claim that a
  target was built when no recipe built it, and a `mark_shown` placement that
  contradicted the documented contract. The author pass found none of them.
  Point it at `faces_from_c` in particular, which is the one new place in this
  diff where a host's pointer is dereferenced.
- **Name the tier in the body.** `just build` runs the regression tier;
  `just calibrate` was run because `Cargo.lock` moved. Never report a tier as
  run that was not run, and do not describe the Android half as exercised.
- **Re-read the milestone's open issues before merging**, not only this story's:
  `gh issue list --milestone "v0.19 — Android, the C ABI, and layer 0" --state open`.
- **Rebase onto the latest `main` before merging** — another session works this
  repository, and `main` moved four times during one day on 2026-08-13 — then
  squash to one commit, force-push, and merge with `gh pr merge --merge`.
- **After merging, check `gh issue view <n> --json state` for every issue the
  branch's commits named**, not only those in the description.
