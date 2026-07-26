//! The import gate: docs/specification/04-figma-vocabulary-profile.md's vocabulary triage.
//!
//! A producer maps its own source vocabulary onto [`Construct`] and asks
//! for the verdict. The validator never parses a source format — P5,
//! "Figma compatibility is a property of one producer" — so this table is
//! the policy, and `dashc` (issue #16) owns the Figma-JSON mapping onto
//! it.
//!
//! Only out-of-profile vocabulary is named here. docs/specification/04-figma-vocabulary-profile.md's NOW band
//! — all four gradient kinds, image fills and scale modes, axis-aligned +
//! rounded clip, full auto-layout — is simply the schema, and needs no
//! verdict.

use crate::{Diagnostic, Location, NodePath, Profile, Severity, rule};

/// A design-vocabulary construct outside the NOW band (docs/specification/04-figma-vocabulary-profile.md).
///
/// Constructing one means the producer *found* the construct; the verdict
/// says what happens next. Every variant is at least a warning — that is
/// what makes it a member of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Construct {
    // LATER (warn): deferred vocabulary with a designer-visible workaround.
    /// Budgeted at v1; a warning until the budget exists.
    ///
    /// Backdrop blur used to sit beside this as a `(profile:full)` construct.
    /// Story #393 moved it into the NOW band — it lowers into the schema and
    /// every painter honours it — so it is no longer a construct at all
    /// (`docs/decisions/backdrop-blur-is-core-vocabulary.md`).
    LayerBlur,
    /// Multiply, screen, overlay, … — `(profile:full)`, pending the
    /// `KHR_blend_equation_advanced` spike (Q-2).
    AdvancedBlendMode,
    /// Squircle corners.
    CornerSmoothing,
    LuminanceMask,
    /// Clip on a rotated node — the resolved clip region stops being
    /// axis-aligned.
    ClipOnRotated,
    KashidaJustification,

    // REJECT (error): each has a documented workaround — bake it, slot it,
    // or design without it.
    NoiseOrTextureEffect,
    ProgressiveBlur,
    AnimatedBooleanOp,
    AnimatedVariableFontAxis,
    /// A stroke whose width varies along its length (a 2025 Figma Draw
    /// effect). No paint entry can express a per-length width, so it is
    /// baked or dropped, never degraded (issue #145).
    VariableWidthStroke,
}

impl Construct {
    /// This construct's stable diagnostic id.
    pub fn rule(self) -> &'static str {
        match self {
            Self::LayerBlur => rule::LAYER_BLUR,
            Self::AdvancedBlendMode => rule::ADVANCED_BLEND_MODE,
            Self::CornerSmoothing => rule::CORNER_SMOOTHING,
            Self::LuminanceMask => rule::LUMINANCE_MASK,
            Self::ClipOnRotated => rule::CLIP_ON_ROTATED,
            Self::KashidaJustification => rule::KASHIDA_JUSTIFICATION,
            Self::NoiseOrTextureEffect => rule::NOISE_OR_TEXTURE_EFFECT,
            Self::ProgressiveBlur => rule::PROGRESSIVE_BLUR,
            Self::AnimatedBooleanOp => rule::ANIMATED_BOOLEAN_OP,
            Self::AnimatedVariableFontAxis => rule::ANIMATED_VARIABLE_FONT_AXIS,
            Self::VariableWidthStroke => rule::VARIABLE_WIDTH_STROKE,
        }
    }

    /// The verdict for this construct under `profile`.
    ///
    /// The REJECT band is an error in every profile. The LATER band is a
    /// warning — except for the two constructs docs/specification/04-figma-vocabulary-profile.md annotates
    /// `(profile:full)`, which a `Core` target can never honor at all, so
    /// there they are an error rather than a degrade.
    pub fn verdict(self, profile: Profile) -> Severity {
        match self {
            Self::NoiseOrTextureEffect
            | Self::ProgressiveBlur
            | Self::AnimatedBooleanOp
            | Self::AnimatedVariableFontAxis
            | Self::VariableWidthStroke => Severity::Error,

            Self::AdvancedBlendMode => match profile {
                Profile::Core => Severity::Error,
                Profile::Full => Severity::Warning,
            },

            Self::LayerBlur
            | Self::CornerSmoothing
            | Self::LuminanceMask
            | Self::ClipOnRotated
            | Self::KashidaJustification => Severity::Warning,
        }
    }

    /// What the producer found, in the diagnostic's own words.
    fn message(self, profile: Profile) -> String {
        let name = match self {
            Self::LayerBlur => "layer blur",
            Self::AdvancedBlendMode => "an advanced blend mode",
            Self::CornerSmoothing => "corner smoothing (squircle)",
            Self::LuminanceMask => "a luminance mask",
            Self::ClipOnRotated => "clip on a rotated node",
            Self::KashidaJustification => "kashida justification",
            Self::NoiseOrTextureEffect => "a noise or texture effect",
            Self::ProgressiveBlur => "progressive blur",
            Self::AnimatedBooleanOp => "an animated boolean operation",
            Self::AnimatedVariableFontAxis => "an animated variable-font axis",
            Self::VariableWidthStroke => "a variable-width stroke",
        };
        match self.verdict(profile) {
            Severity::Error => format!(
                "{name} is not in profile:{}; it is out of the supported vocabulary \
                 (docs/specification/04-figma-vocabulary-profile.md) and blocks the document",
                profile_name(profile)
            ),
            Severity::Warning => format!(
                "{name} is deferred vocabulary in profile:{} (docs/specification/04-figma-vocabulary-profile.md); it degrades \
                 as declared, and a strict build refuses it without a waiver",
                profile_name(profile)
            ),
        }
    }
}

fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::Core => "core",
        Profile::Full => "full",
    }
}

/// The import gate: one out-of-profile construct in, one named diagnostic
/// out. Never a silent drop (P4).
///
/// An out-of-profile construct is always found *on a node* — it is a
/// property the producer read off a layer — so this gate takes a
/// [`NodePath`] rather than the wider [`Location`] the pooled surfaces need.
pub fn triage(construct: Construct, profile: Profile, node: NodePath) -> Diagnostic {
    Diagnostic {
        rule: construct.rule(),
        severity: construct.verdict(profile),
        at: Location::Node(node),
        message: construct.message(profile),
    }
}
