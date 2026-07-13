//! The import gate: DESIGN §10.1's vocabulary triage.
//!
//! A producer maps its own source vocabulary onto [`Construct`] and asks
//! for the verdict. The validator never parses a source format — P5,
//! "Figma compatibility is a property of one producer" — so this table is
//! the policy, and `dashc` (issue #16) owns the Figma-JSON mapping onto
//! it.
//!
//! Only out-of-profile vocabulary is named here. DESIGN §10.1's NOW band
//! — all four gradient kinds, image fills and scale modes, axis-aligned +
//! rounded clip, full auto-layout — is simply the schema, and needs no
//! verdict.

use crate::{Diagnostic, Location, NodePath, Profile, Severity, rule};

/// A design-vocabulary construct outside the NOW band (DESIGN §10.1).
///
/// Constructing one means the producer *found* the construct; the verdict
/// says what happens next. Every variant is at least a warning — that is
/// what makes it a member of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Construct {
    // LATER (warn): deferred vocabulary with a designer-visible workaround.
    /// Budgeted at v1; a warning until the budget exists.
    LayerBlur,
    /// `(profile:full)` — a lean painter never gets it.
    BackdropBlur,
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
}

impl Construct {
    /// This construct's stable diagnostic id.
    pub fn rule(self) -> &'static str {
        match self {
            Self::LayerBlur => rule::LAYER_BLUR,
            Self::BackdropBlur => rule::BACKDROP_BLUR,
            Self::AdvancedBlendMode => rule::ADVANCED_BLEND_MODE,
            Self::CornerSmoothing => rule::CORNER_SMOOTHING,
            Self::LuminanceMask => rule::LUMINANCE_MASK,
            Self::ClipOnRotated => rule::CLIP_ON_ROTATED,
            Self::KashidaJustification => rule::KASHIDA_JUSTIFICATION,
            Self::NoiseOrTextureEffect => rule::NOISE_OR_TEXTURE_EFFECT,
            Self::ProgressiveBlur => rule::PROGRESSIVE_BLUR,
            Self::AnimatedBooleanOp => rule::ANIMATED_BOOLEAN_OP,
            Self::AnimatedVariableFontAxis => rule::ANIMATED_VARIABLE_FONT_AXIS,
        }
    }

    /// The verdict for this construct under `profile`.
    ///
    /// The REJECT band is an error in every profile. The LATER band is a
    /// warning — except for the two constructs DESIGN §10.1 annotates
    /// `(profile:full)`, which a `Core` target can never honor at all, so
    /// there they are an error rather than a degrade.
    pub fn verdict(self, profile: Profile) -> Severity {
        match self {
            Self::NoiseOrTextureEffect
            | Self::ProgressiveBlur
            | Self::AnimatedBooleanOp
            | Self::AnimatedVariableFontAxis => Severity::Error,

            Self::BackdropBlur | Self::AdvancedBlendMode => match profile {
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
            Self::BackdropBlur => "backdrop blur",
            Self::AdvancedBlendMode => "an advanced blend mode",
            Self::CornerSmoothing => "corner smoothing (squircle)",
            Self::LuminanceMask => "a luminance mask",
            Self::ClipOnRotated => "clip on a rotated node",
            Self::KashidaJustification => "kashida justification",
            Self::NoiseOrTextureEffect => "a noise or texture effect",
            Self::ProgressiveBlur => "progressive blur",
            Self::AnimatedBooleanOp => "an animated boolean operation",
            Self::AnimatedVariableFontAxis => "an animated variable-font axis",
        };
        match self.verdict(profile) {
            Severity::Error => format!(
                "{name} is not in profile:{}; it is out of the supported vocabulary \
                 (DESIGN §10.1) and blocks the document",
                profile_name(profile)
            ),
            Severity::Warning => format!(
                "{name} is deferred vocabulary in profile:{} (DESIGN §10.1); it degrades \
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
