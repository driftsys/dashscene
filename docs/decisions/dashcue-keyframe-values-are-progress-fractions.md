# dashcue keyframe values are progress fractions, not absolute values

    status   accepted (story #21, 2026-07-12)
    scope    crates/dashcue; binds story #22 (dashscene-engine binds
             from/to at commit) and any future dashbuf schema field for
             `Keyframe`

## Context

`docs/design/dashcue.md` calibrates the vocabulary against Jetpack Compose's
API, where `keyframes {}` declares absolute values at each timestamp
(for example, `100f at 300`). But principle P1 (`AGENTS.md`) forbids a
document from carrying resolved values, and a variant transition's
`from`/`to` are not known until the engine resolves them at commit time
(issue #22). A `Keyframe` had to represent its value some way that does
not require knowing the endpoints in advance.

## Options

1. Absolute values, matching Compose's `keyframes {}` shape:
   `Keyframe { t: f32, value: f32 }` where `value` is the actual prop
   value at that timestamp.
2. Progress fractions of the bound `from → to` span: `Keyframe { t:
   f32, value: f32 }` where `value` is `0.0` at `from` and `1.0` at
   `to` (values outside `[0, 1]` are overshoot).

## Choice

Option 2.

## Why

- Option 1 cannot be expressed: a document — and the vocabulary data
  it carries — must not contain resolved positions (P1), and `dashcue`
  itself never sees `from`/`to` until `Scheduler::start` /
  `start_transition` is called at commit time. An absolute-valued
  frame would need the endpoints to be known when the transition is
  authored, which contradicts the seam
  (`docs/decisions/staged-mutation-v01-scope.md`): the
  transition spec is declared independently of any particular
  `set_variant`'s resolved values.
- Fractions keep retarget (R4) defined the same way tweens are: rebind
  `from`/`to` at the new target, reuse the same curve. An
  absolute-valued curve would need re-scaling on every retarget.
- The deviation from Compose's `keyframes {}` is deliberate and local
  to this one type; the rest of the vocabulary (tween duration/easing,
  spring stiffness/damping ratio) maps onto Compose's `SpringSpec` /
  tween shapes unchanged.
