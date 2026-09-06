//! The faithful Canvas's per-frame shape, and what stages it.
//!
//! Story #1444. Rule 5 of the fairness rules
//! (`docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
//! D2) says the Canvas is driven from the lease's dirty set, and that a Canvas
//! rewriting every element each frame would be the painter's advantage — an
//! advantage the record says is not taken. **A per-frame walk over every element
//! is the one way to lose the comparison silently**: the picture would be
//! identical, every capture would pass, and the CPU reading would be the
//! painter's own gap handed to the baseline.
//!
//! No CI job compiles `Samples~/`, so this is a scan rather than a test over
//! behaviour, and it is scoped to `Apply`'s own body — a needle found anywhere
//! in the file pins nothing about the method. What the scan cannot see is
//! whether the loop is correct; the `compare` action's run-time pin
//! (`AppliedLastFrame == frame.Dirty.CountAsLong` on a pulse frame) is that half,
//! and story #1444's mutation drives the scan by making `Apply` walk `Rects`.
//!
//! **The samples may hold no `unsafe`.** A sample is copied into a consumer's
//! `Assets/` and compiles into `Assembly-CSharp`, which does not allow unsafe
//! code and which nothing in this repository configures. `Runtime/FrameRows.cs`
//! is the seam that reads the runtime's rows, inside the package whose assembly
//! definition does allow it.

use package_gate::cs_scan;

/// Under `package_gate::PACKAGE_PATH`.
const SHOWCASE_DIR: &str = "Samples~/Showcase";

/// The files this story adds under that directory.
const ADDED: [&str; 4] = [
    "CanvasScene.cs",
    "CanvasSprites.cs",
    "DashsceneCanvasBaseline.cs",
    "CanvasText.cs",
];

fn showcase(name: &str) -> String {
    let path = package_gate::root()
        .join(package_gate::PACKAGE_PATH)
        .join(SHOWCASE_DIR)
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn justfile() -> String {
    let path = package_gate::root().join("justfile");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Rule 5: one loop, over the dirty set, and over nothing else.
#[test]
fn apply_walks_the_dirty_set_and_never_the_element_list() {
    let scanned = cs_scan::blank_comments_and_strings(&showcase("CanvasScene.cs"));
    let (start, end) = cs_scan::member_body(&scanned, "public void Apply(DsFrame frame)");
    let body = cs_scan::squeeze(&scanned[start..end]);

    assert!(
        body.contains("var dirty = FrameRows.Of<uint>(frame.Dirty);"),
        "the dirty set is the span the loop walks: {body}"
    );
    assert!(
        body.contains("for (var d = 0; d < dirty.Length; d++)"),
        "the one loop is bounded by the dirty count: {body}"
    );
    assert!(
        body.contains("var i = (int)dirty[d];"),
        "the element index comes from the dirty set: {body}"
    );

    // `ElementCount` and the two counts among them: each is a way to bound a
    // loop by the whole scene, and `_count` — the cached length the class keeps
    // for `ElementCount` — is refused here for the same reason, so a bounds
    // check cannot become a walk.
    for forbidden in [
        "_elements.Count",
        "frame.Rects.Count",
        "rects.Length",
        "foreach (",
        "ElementCount",
        "_count",
    ] {
        assert!(
            !body.contains(forbidden),
            "Apply reaches every element through `{forbidden}`: {body}"
        );
    }

    // **The loop's index must reach the row, not just exist.** Scoping the scan
    // to the bounds and the index source left `Isolate(_elements[0])` green — a
    // loop that walks the dirty set and applies the same element every time,
    // measured. Naming the three subscripted reads is what closes that.
    for used in [
        "Isolate(_elements[i]);",
        "Place(_elements[i], rects[i]);",
        "Tint(_elements[i], rects[i], PaintOf(rects[i], entries), frame);",
        "AppliedLastFrame++;",
    ] {
        assert!(
            body.contains(used),
            "Apply's loop does not carry `{used}`, so the index it reads from the \
             dirty set may reach no row: {body}"
        );
    }

    // **The counter is incremented, never assigned.** `AppliedLastFrame =
    // dirty.Length` before the loop made the run-time pin that compares it
    // against `frame.Dirty.CountAsLong` a tautology — the two are the same
    // number by construction, so emptying the loop body left it green.
    assert!(
        body.contains("AppliedLastFrame = 0;"),
        "AppliedLastFrame is zeroed before the loop and counted inside it; \
         assigning the span's length makes the run-time pin compare a number \
         against itself: {body}"
    );
    assert!(
        !body.contains("AppliedLastFrame = dirty.Length"),
        "AppliedLastFrame is assigned the dirty count rather than counting the \
         rows the loop actually wrote, which is the tautology the pin died of: \
         {body}"
    );

    assert_eq!(
        body.matches("for (").count(),
        1,
        "exactly one loop in Apply: {body}"
    );
    assert!(
        !body.contains("while ("),
        "a `while` walks as well as a `for` does, and the loop count above \
         cannot see one: {body}"
    );
}

/// The samples stay safe, and the package's seam is what reads the rows.
#[test]
fn no_sample_this_story_adds_declares_unsafe() {
    for name in ADDED {
        let scanned = cs_scan::blank_comments_and_strings(&showcase(name));
        assert!(
            !scanned.contains("unsafe"),
            "{SHOWCASE_DIR}/{name} declares `unsafe`. A sample compiles into \
             Assembly-CSharp, which does not allow it; Runtime/FrameRows.cs is the \
             seam that reads a DsSlice's rows."
        );
    }

    let seam = package_gate::root()
        .join(package_gate::PACKAGE_PATH)
        .join("Runtime/FrameRows.cs");
    let source =
        std::fs::read_to_string(&seam).unwrap_or_else(|e| panic!("{}: {e}", seam.display()));
    let scanned = cs_scan::blank_comments_and_strings(&source);
    assert!(
        scanned.contains("public static unsafe ReadOnlySpan<T> Of<T>(DsSlice slice)"),
        "Runtime/FrameRows.cs no longer carries the accessor the samples read rows \
         through, so the assertion above holds them to nothing."
    );
}

/// The `drew` line, in the shape `just unity-demo`'s `cycle` action reads.
///
/// **Read with the comments blanked and the STRINGS kept.** The scanner this
/// crate uses everywhere else blanks string bodies, so a test written on it
/// would find nothing whatever the file says; blanking only the comments is what
/// makes a comment quoting the message fail to satisfy this and the message
/// itself satisfy it.
#[test]
fn the_baseline_logs_the_drew_line_the_cycle_recipe_reads() {
    let code = cs_scan::blank_comments_only(&showcase("DashsceneCanvasBaseline.cs"));
    let squeezed = cs_scan::squeeze(&code);

    assert!(
        squeezed.contains("Debug.Log($\"[showcase] drew {Label(_index)}: {_scene.ElementCount} \""),
        "the canvas `drew` line names the entry and its element count in code"
    );
    assert!(
        squeezed.contains("element(s) through the canvas"),
        "the canvas `drew` line carries the phrase the report reads"
    );
    assert!(
        squeezed
            .contains("Debug.Log($\"[showcase] entries: {TotalCount} ({SceneCount} scene(s), \""),
        "the census line the `cycle` action waits for is written by whichever \
         component drives, and under `-renderer` that is this one"
    );
    assert!(
        squeezed.contains("Debug.Log($\"[showcase] all {TotalCount} entries drew \""),
        "the line the `cycle` action's bound is satisfied by is written here too"
    );
}

/// The bake shader is staged where a player build keeps it, by both recipes.
///
/// **It sits outside the package deliberately**, and that is what this pins from
/// the other side. `shader_pragmas.rs` holds every `.shader` INSIDE
/// `unity/com.driftsys.dashscene` to being a painter material class registered
/// by `PaintShaders` and sitting under `Runtime/Resources/`; this one registers
/// with no `BatchRendererGroup` and belongs to no material class. Outside the
/// package it is invisible to `Resources.Load` unless a recipe stages it, and a
/// shader `Shader.Find` cannot resolve is a `CanvasSprites` that throws at load
/// — which is a failure at the end of a player build rather than at the start of
/// one, so it is pinned here.
#[test]
fn both_demo_recipes_stage_the_bake_shader_into_resources() {
    let shader = package_gate::root().join("unity/demo/SpriteBake.shader");
    let source =
        std::fs::read_to_string(&shader).unwrap_or_else(|e| panic!("{}: {e}", shader.display()));
    assert_eq!(
        package_gate::declared_shader_name(&source).as_deref(),
        Some("Dashscene/Samples/SpriteBake"),
        "the bake shader's declared name is what CanvasSprites resolves it by"
    );
    assert!(
        source.contains("#include \"Packages/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl\""),
        "R-T5: the bake evaluates the generated shader library rather than a third \
         copy of the rounded-box distance written in C#"
    );

    let names: Vec<String> = package_gate::shader_sources()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert!(
        !names.iter().any(|path| path.ends_with("SpriteBake.shader")),
        "the bake shader has moved INSIDE the package, where four assertions in \
         shader_pragmas.rs hold every shader to being a registered painter material \
         class at Runtime/Resources/. The shaders the package ships are {names:?}."
    );

    let justfile = justfile();
    for recipe in ["unity-demo", "unity-demo-android"] {
        let body = package_gate::recipe_code(&justfile, recipe);
        // **One statement, not two substrings.** Asserting the source path and
        // the destination directory separately passed over a `cp` redirected to
        // `Assets/Plugins/` with the now-orphaned `mkdir -p .../Assets/Resources`
        // left in place — measured. The copy is what has to be right.
        assert!(
            body.contains(
                "cp \"${root}/unity/demo/SpriteBake.shader\" \"${project}/Assets/Resources/\""
            ),
            "`{recipe}` does not copy unity/demo/SpriteBake.shader into \
             Assets/Resources/ in one statement, so Shader.Find returns null in \
             the player it builds and a player build strips a shader no scene \
             references (issue #1313)."
        );
    }
}

/// Rule 4's inputs: the fonts are staged, the assets are built, and the two
/// sides agree on the name.
///
/// **The prefix is the whole contract between them.** `DemoFonts` writes
/// `Assets/Resources/<prefix><face key>.asset` and `CanvasText` loads
/// `<prefix><face key>`, where the key comes from `ds_demo_face_key` — so
/// neither side holds a list of faces and a disagreement about the PREFIX is the
/// one way they can both look right and never meet. A missing asset is a warning
/// and a counted refusal at run time, which is visible; a prefix that drifted
/// would make every run refuse, which reads as a font problem rather than as a
/// naming one.
#[test]
fn the_font_assets_the_canvas_loads_are_the_ones_the_recipes_build() {
    let canvas_text = showcase("CanvasText.cs");
    let scanned = cs_scan::blank_comments_only(&canvas_text);
    assert!(
        cs_scan::squeeze(&scanned)
            .contains("public const string ResourcePrefix = \"DashsceneFont-\";"),
        "CanvasText's resource prefix has moved; DemoFonts writes the other half of \
         this name."
    );

    let demo_fonts = package_gate::root().join("unity/demo/DemoFonts.cs");
    let source = std::fs::read_to_string(&demo_fonts)
        .unwrap_or_else(|e| panic!("{}: {e}", demo_fonts.display()));
    let fonts = cs_scan::squeeze(&cs_scan::blank_comments_only(&source));
    assert!(
        fonts.contains("public const string Prefix = \"DashsceneFont-\";"),
        "DemoFonts' prefix and CanvasText's ResourcePrefix are the same string, and \
         they have drifted apart."
    );
    assert!(
        fonts.contains("AssetDatabase.AddObjectToAsset(created.atlasTextures[0], created);")
            && fonts.contains("AssetDatabase.AddObjectToAsset(created.material, created);"),
        "a TMP font asset saved without its atlas texture and its material loads with \
         neither, which is a TMP_Text that renders nothing and reports nothing."
    );

    let justfile = justfile();
    for recipe in ["unity-demo", "unity-demo-android"] {
        let body = package_gate::recipe_code(&justfile, recipe);
        assert!(
            body.contains("-executeMethod DemoFonts.Create"),
            "`{recipe}` stages no font assets, so the Canvas draws no text at all — \
             which reads as a large difference rather than as a missing input."
        );
        // **Source and staged stem in one statement, for the shader's reason
        // and one more.** `Inter-Regular.otf` already appears twice in
        // `unity-demo` — once in the text document's cascade check and once
        // here — so a bare substring for it is satisfied by the OTHER line, and
        // deleting this copy left the assertion green with `Assets/Fonts`
        // missing the file `DemoFonts.Create` reads.
        //
        // The staged stem is the producer's own face key, which is what
        // `CanvasText` loads the asset by; pinning the pair here is what makes
        // a reweighted face a red gate rather than a silently text-free Canvas.
        for (source, staged) in [
            (
                "corpus/fonts/inter/Inter-Regular.otf",
                "Assets/Fonts/Inter-400.otf",
            ),
            (
                "corpus/fonts/inter/Inter-SemiBold.otf",
                "Assets/Fonts/Inter-600.otf",
            ),
            (
                "corpus/fonts/noto-sans-arabic/NotoSansArabic-Regular.ttf",
                "Assets/Fonts/NotoSansArabic-400.ttf",
            ),
        ] {
            let statement =
                format!("cp \"${{root}}/{source}\" \\\n      \"${{project}}/{staged}\"");
            assert!(
                body.contains(&statement),
                "`{recipe}` does not stage {source} as {staged} in one statement. \
                 The showcase's cascade is those three faces and the staged stem is \
                 the face key `ds_demo_face_key` answers, so a run shaped by a face \
                 the player does not carry under that name is a counted refusal \
                 rather than a drawn word."
            );
        }
    }
}

/// Both recipes give the project the package the Canvas needs.
///
/// `com.unity.ugui` carries `UnityEngine.UI` and TextMeshPro, and URP does not
/// depend on it — so without this line every sample this story adds fails to
/// compile, in the player build rather than in any check that runs in seconds.
#[test]
fn every_project_that_compiles_the_samples_resolves_ugui() {
    let justfile = justfile();
    for recipe in ["unity-demo", "unity-demo-android", "unity-editor"] {
        let body = package_gate::recipe_code(&justfile, recipe);
        assert!(
            body.contains("com.unity.ugui"),
            "`{recipe}` compiles the package's samples and writes a manifest \
             without com.unity.ugui. URP does not depend on it, so UnityEngine.UI \
             and TMPro resolve to nothing there."
        );
    }
}
