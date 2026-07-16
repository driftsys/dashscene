//! Figma constructs mapped onto `dashscene-validator`'s `Construct`.
//!
//! The producer owns the mapping, the validator owns the verdict (P5). A
//! `figma` module inside the validator was rejected on exactly those grounds
//! (`docs/decisions/validator-three-gates.md`).
//!
//! Only vocabulary *outside* the NOW band appears here. docs/specification/04-figma-vocabulary-profile.md's NOW
//! band — the four gradient kinds, image fills and scale modes, axis-aligned
//! and rounded clip — is simply the schema, and needs no verdict.

use dashscene_validator::Construct;

use crate::figma::rest::{Effect, Node};

/// The out-of-profile constructs `node` carries, and the constructs the
/// document cannot express at all.
///
/// The first list triages through the validator. The second names constructs
/// with no `Construct` variant — an unmapped effect such as a baked shadow
/// (debt #144) — which the walk reports under its own `figma.unsupported`
/// rule; P4 forbids dropping them in silence. Both lists come back from one
/// pass, so a node carrying one of each reports both (debt #149).
pub(crate) fn constructs_of(node: &Node) -> (Vec<Construct>, Vec<String>) {
    let mut found = Vec::new();
    let mut unsupported = Vec::new();

    for effect in node.effects.iter().filter(|e| e.visible != Some(false)) {
        match effect_construct(effect) {
            Ok(construct) => found.push(construct),
            Err(what) => unsupported.push(what),
        }
    }

    // Figma carries a blendMode on the node and on every paint. Both are
    // triaged: a paint-level blend mode is just as invisible a drop.
    if !is_plain_blend(node.blend_mode.as_deref()) {
        found.push(Construct::AdvancedBlendMode);
    }
    for paint in node
        .fills
        .iter()
        .chain(node.strokes.iter())
        .filter(|p| p.visible != Some(false))
    {
        if !is_plain_blend(paint.blend_mode.as_deref()) {
            found.push(Construct::AdvancedBlendMode);
        }
    }

    if node.corner_smoothing.is_some_and(|s| s > 0.0) {
        found.push(Construct::CornerSmoothing);
    }

    (found, unsupported)
}

fn effect_construct(effect: &Effect) -> Result<Construct, String> {
    match effect.kind.as_str() {
        "NOISE" | "TEXTURE" => Ok(Construct::NoiseOrTextureEffect),
        // A progressive blur serializes as a LAYER_BLUR carrying
        // `blurType: PROGRESSIVE` — pinned by effects-2025.json. Plain layer
        // blur only warns; progressive blur is an error.
        "LAYER_BLUR" => Ok(match effect.blur_type.as_deref() {
            Some("PROGRESSIVE") => Construct::ProgressiveBlur,
            _ => Construct::LayerBlur,
        }),
        "BACKGROUND_BLUR" => Ok(Construct::BackdropBlur),
        // Shadows are NOW-band, but Document cannot express them yet. No Construct
        // fits, so it fails loudly rather than vanishing (debt #144).
        other => Err(format!("effect {other}")),
    }
}

/// `PASS_THROUGH` is a frame's default and `NORMAL` a paint's; anything else
/// is an advanced blend mode.
fn is_plain_blend(mode: Option<&str>) -> bool {
    matches!(mode, None | Some("NORMAL") | Some("PASS_THROUGH"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figma::rest::FigmaFile;

    const EFFECTS_2025: &str = include_str!("../../../../corpus/figma-fixtures/effects-2025.json");
    const V03_PAINT: &str = include_str!("../../../../corpus/figma-fixtures/v03-paint.json");

    fn find<'a>(file: &'a FigmaFile, name: &str) -> &'a Node {
        fn walk<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
            if node.name == name {
                return Some(node);
            }
            node.children.iter().find_map(|child| walk(child, name))
        }
        walk(&file.document, name).expect("fixture has the node")
    }

    fn file(json: &str) -> FigmaFile {
        serde_json::from_str(json).expect("the fixture parses")
    }

    #[test]
    fn noise_and_texture_are_the_reject_band() {
        let f = file(EFFECTS_2025);
        assert_eq!(
            constructs_of(find(&f, "noise")).0,
            vec![Construct::NoiseOrTextureEffect],
        );
        assert_eq!(
            constructs_of(find(&f, "texture")).0,
            vec![Construct::NoiseOrTextureEffect],
        );
    }

    #[test]
    fn a_layer_blur_rejects_only_when_it_is_progressive() {
        // The discrimination the capture forced: the effect type alone cannot
        // decide the band.
        let f = file(EFFECTS_2025);
        assert_eq!(
            constructs_of(find(&f, "progressive-blur")).0,
            vec![Construct::ProgressiveBlur],
        );
    }

    #[test]
    fn the_paint_fixture_carries_no_out_of_profile_construct() {
        // v03-paint is entirely NOW-band, so it must triage to nothing at all
        // — otherwise it could never emit, and the manifest says emits: true.
        let f = file(V03_PAINT);
        for name in [
            "fill-solid",
            "gradient-angular",
            "image-fit",
            "stroke-outside",
            "corners-uniform",
            "corners-per-corner",
            "clip-frame",
        ] {
            assert_eq!(
                constructs_of(find(&f, name)).0,
                vec![],
                "{name} must be in the NOW band",
            );
        }
    }

    #[test]
    fn a_shadow_is_unsupported_rather_than_silently_dropped() {
        // Baked shadows are NOW-band per docs/specification/04-figma-vocabulary-profile.md, but Document cannot express
        // them, so there is no Construct to triage. P4 forbids dropping it in
        // silence, so it fails loudly instead.
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "card",
            "type": "FRAME",
            "effects": [{ "type": "DROP_SHADOW", "visible": true }],
        }))
        .unwrap();

        assert_eq!(
            constructs_of(&node),
            (vec![], vec!["effect DROP_SHADOW".to_string()]),
        );
    }

    #[test]
    fn a_hidden_fill_or_stroke_with_an_advanced_blend_mode_triages_to_nothing() {
        // A hidden fill or stroke cannot produce a visible diagnostic — the
        // designer cannot see it, so it must not surface as
        // AdvancedBlendMode. Regression test for the bug where fills and
        // strokes, unlike effects, were triaged without a visibility filter.
        //
        // Both halves are pinned: filtering the fills but leaving the strokes
        // unfiltered is the shape the fix is most likely to regress into, and
        // a fill-only test would pass against it.
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "hidden-paints",
            "type": "FRAME",
            "fills": [{ "type": "SOLID", "visible": false, "blendMode": "MULTIPLY" }],
            "strokes": [{ "type": "SOLID", "visible": false, "blendMode": "MULTIPLY" }],
        }))
        .unwrap();

        assert_eq!(constructs_of(&node), (vec![], vec![]));
    }

    #[test]
    fn a_node_level_blend_mode_is_an_advanced_blend_mode() {
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "node-blend",
            "type": "FRAME",
            "blendMode": "MULTIPLY",
        }))
        .unwrap();

        assert_eq!(constructs_of(&node).0, vec![Construct::AdvancedBlendMode],);
    }

    #[test]
    fn a_paint_level_blend_mode_is_an_advanced_blend_mode_for_fill_and_stroke() {
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "paint-blend",
            "type": "FRAME",
            "fills": [{ "type": "SOLID", "blendMode": "MULTIPLY" }],
            "strokes": [{ "type": "SOLID", "blendMode": "SCREEN" }],
        }))
        .unwrap();

        assert_eq!(
            constructs_of(&node).0,
            vec![Construct::AdvancedBlendMode, Construct::AdvancedBlendMode],
        );
    }

    #[test]
    fn corner_smoothing_above_zero_is_corner_smoothing() {
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "smoothed-corners",
            "type": "FRAME",
            "cornerSmoothing": 0.6,
        }))
        .unwrap();

        assert_eq!(constructs_of(&node).0, vec![Construct::CornerSmoothing],);
    }

    #[test]
    fn a_plain_layer_blur_is_layer_blur() {
        // No blurType at all, and a non-PROGRESSIVE blurType, both land in
        // Construct::LayerBlur rather than Construct::ProgressiveBlur.
        let no_blur_type: Node = serde_json::from_value(serde_json::json!({
            "name": "layer-blur",
            "type": "FRAME",
            "effects": [{ "type": "LAYER_BLUR", "visible": true }],
        }))
        .unwrap();
        assert_eq!(constructs_of(&no_blur_type).0, vec![Construct::LayerBlur],);

        let non_progressive: Node = serde_json::from_value(serde_json::json!({
            "name": "layer-blur-normal",
            "type": "FRAME",
            "effects": [{ "type": "LAYER_BLUR", "visible": true, "blurType": "NORMAL" }],
        }))
        .unwrap();
        assert_eq!(
            constructs_of(&non_progressive).0,
            vec![Construct::LayerBlur],
        );
    }

    #[test]
    fn a_background_blur_is_backdrop_blur() {
        let node: Node = serde_json::from_value(serde_json::json!({
            "name": "backdrop-blur",
            "type": "FRAME",
            "effects": [{ "type": "BACKGROUND_BLUR", "visible": true }],
        }))
        .unwrap();

        assert_eq!(constructs_of(&node).0, vec![Construct::BackdropBlur],);
    }
}
