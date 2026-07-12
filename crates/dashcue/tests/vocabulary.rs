//! Vocabulary-type tests (issue #21): dashcue's public API only.

use dashcue::{Easing, Keyframe, PropKey, PropTransition, TransitionSpec, VariantTransition};

#[test]
fn easing_polynomials_hit_their_fixed_values() {
    // Linear: t. EaseIn: t^3. EaseOut: 1-(1-t)^3.
    // EaseInOut: 4t^3 below 1/2, 1-4(1-t)^3 above.
    for e in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
    ] {
        assert_eq!(e.apply(0.0), 0.0);
        assert_eq!(e.apply(1.0), 1.0);
    }
    assert_eq!(Easing::Linear.apply(0.25), 0.25);
    assert_eq!(Easing::EaseIn.apply(0.25), 0.015625);
    assert_eq!(Easing::EaseIn.apply(0.5), 0.125);
    assert_eq!(Easing::EaseOut.apply(0.5), 0.875);
    assert_eq!(Easing::EaseOut.apply(0.75), 0.984375);
    assert_eq!(Easing::EaseInOut.apply(0.25), 0.0625);
    assert_eq!(Easing::EaseInOut.apply(0.5), 0.5);
    assert_eq!(Easing::EaseInOut.apply(0.75), 0.9375);
}

#[test]
fn vocabulary_types_are_plain_comparable_data() {
    let transition = VariantTransition {
        tracks: vec![PropTransition {
            prop: PropKey(7),
            spec: TransitionSpec::Keyframes {
                duration: 0.3,
                frames: vec![Keyframe { t: 0.5, value: 1.5 }],
            },
        }],
        stagger: 0.05,
    };
    assert_eq!(transition.clone(), transition);
    assert_ne!(PropKey(7), PropKey(8));
}
