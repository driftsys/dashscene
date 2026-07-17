//! The descriptive animation vocabulary (docs/design/architecture.md): data a
//! producer declares; the runtime advances it (P3). No resolved values
//! live here (P1) — endpoints bind at commit time.

/// Opaque per-track key. The caller encodes prop identity into it (the
/// packing math is `dashscene_core::prop_key` — node slot and channel —
/// and the engine exposes it as this key); `dashcue` only compares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropKey(pub u64);

/// Fixed cubic easing curves. Exotic shapes are data — use
/// [`TransitionSpec::Keyframes`] (docs/design/architecture.md, "keyframe track").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// Maps linear progress `t` in [0, 1] to eased progress. Polynomial
    /// only — bit-stable across machines (E5/E6 golden determinism).
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t * t,
            Easing::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Easing::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = 1.0 - t;
                    1.0 - 4.0 * u * u * u
                }
            }
        }
    }
}

/// One declared point of a keyframes curve.
///
/// `t` is a fraction of the spec's duration, strictly inside (0, 1) and
/// strictly increasing across a frame list. `value` is a progress
/// fraction of the bound `from → to` span — it may leave [0, 1]
/// (overshoot is data). The endpoints (0, 0) and (1, 1) are implicit.
/// Values are fractions, not absolute prop values, because a document
/// never carries resolved values (P1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub t: f32,
    pub value: f32,
}

/// How one prop travels from its old to its new resolved value.
/// Durations are seconds; spring parameters follow Compose's
/// `SpringSpec` (stiffness + damping ratio) so Compose specs map onto
/// this as data (docs/design/architecture.md, "Compose calibration").
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionSpec {
    Tween {
        duration: f32,
        easing: Easing,
    },
    Spring {
        stiffness: f32,
        damping_ratio: f32,
    },
    Keyframes {
        duration: f32,
        frames: Vec<Keyframe>,
    },
}

/// One prop's declared spec inside a variant transition.
#[derive(Debug, Clone, PartialEq)]
pub struct PropTransition {
    pub prop: PropKey,
    pub spec: TransitionSpec,
}

/// A variant transition (docs/design/architecture.md): per-prop specs plus a
/// stagger. Track `i` starts `stagger * i` seconds after the commit.
/// The `set_variant` switch itself is `dashscene-core`'s; this is the
/// data describing how the switch animates (docs/decisions/staged-mutation-v01-scope.md).
#[derive(Debug, Clone, PartialEq)]
pub struct VariantTransition {
    pub tracks: Vec<PropTransition>,
    pub stagger: f32,
}
