//! The document binding vocabulary — signal declarations and binding rows
//! as *intent* (story #167).
//!
//! A binding — "this node's gap follows the signal named `size/gap`" — is
//! intent a designer can author (in Figma, through Variables), so at v0.7
//! it becomes a document construct: the vocabulary lives here and in
//! `dashbuf`'s schema, and the arena stores the staged tables
//! (`docs/decisions/reactive-layer-home-and-staging.md`,
//! `docs/decisions/bindings-are-explicit-and-flat.md`). Signal *values*
//! stay producer-side always — a value is a result, and P1 keeps results
//! out of the document. The arena stores the tables and ignores them at
//! commit: flushing a signal's value through a binding is a producer-side
//! runtime (`dashlang`'s reactive layer), never core's.
//!
//! The vocabulary is bounded and declarative (P4): a [`Channel`] is one
//! scalar prop slot, and a [`ScalarTransform`] is data. `dashlang`'s
//! `Custom` closure transform is deliberately absent — a closure does not
//! serialize, so it never enters the document tables; it stays a
//! `dashlang`-only escape hatch.

use crate::arena::NodeId;

/// One scalar prop slot a binding can target — the §23 binding channel
/// vocabulary (`docs/archive/2026-07-14-scope-decisions.md`), completed
/// with the paint channels and `Gap` at v0.7 (debt #201).
///
/// The discriminants are the wire codes: `dashbuf`'s `BindingChannel`
/// enum and the packed low bits of `dashscene-engine`'s `PropKey` both
/// carry exactly these values, so a serialized binding row and a
/// scheduler track address agree by construction. Append new channels at
/// the tail; never renumber (R7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Channel {
    X = 0,
    Y = 1,
    Width = 2,
    Height = 3,
    /// A flex container's main-axis gap (`Prop::Gap`). Layout-affecting:
    /// a gap write redistributes the container's children.
    Gap = 4,
    /// The red channel of the node's solid fill. Paint-only, like the
    /// other three fill channels: a fill write never reflows anything
    /// (`docs/decisions/visible-is-layout-opacity-is-paint.md`).
    FillR = 5,
    FillG = 6,
    FillB = 7,
    FillA = 8,
}

impl Channel {
    /// Every channel, in code order.
    pub const ALL: [Channel; 9] = [
        Channel::X,
        Channel::Y,
        Channel::Width,
        Channel::Height,
        Channel::Gap,
        Channel::FillR,
        Channel::FillG,
        Channel::FillB,
        Channel::FillA,
    ];

    /// The channel's stable wire code (the enum discriminant).
    pub fn code(self) -> u8 {
        self as u8
    }

    /// The channel a wire code names, or `None` for a code this build
    /// does not know — the caller names the failure (P4), never defaults.
    pub fn from_code(code: u8) -> Option<Channel> {
        Channel::ALL.into_iter().find(|c| c.code() == code)
    }
}

/// A declarative scalar transform — the serializable subset of the §23
/// transform vocabulary (D8). Applied to a signal's value before the
/// result is written to the bound channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalarTransform {
    Identity,
    /// `value * factor`.
    Scale(f32),
    /// Linear remap from `[in_lo, in_hi]` to `[out_lo, out_hi]`,
    /// unclamped.
    MapRange {
        in_lo: f32,
        in_hi: f32,
        out_lo: f32,
        out_hi: f32,
    },
    /// Clamp to `[lo, hi]`.
    Clamp {
        lo: f32,
        hi: f32,
    },
}

impl ScalarTransform {
    /// Applies the transform to one signal value. One implementation of
    /// the transform math, shared by every consumer of the table.
    pub fn apply(&self, v: f32) -> f32 {
        match *self {
            ScalarTransform::Identity => v,
            ScalarTransform::Scale(factor) => v * factor,
            ScalarTransform::MapRange {
                in_lo,
                in_hi,
                out_lo,
                out_hi,
            } => {
                let span = in_hi - in_lo;
                let t = if span == 0.0 { 0.0 } else { (v - in_lo) / span };
                out_lo + t * (out_hi - out_lo)
            }
            ScalarTransform::Clamp { lo, hi } => v.clamp(lo, hi),
        }
    }
}

/// Packs one `(node, channel)` prop address into the opaque `u64` that
/// `dashcue`'s `PropKey` wraps: the node's arena slot in the high bits,
/// the [`Channel`] wire code in the low eight. This is the **one**
/// packing (debt #208): `dashscene-engine` exposes it as the typed
/// `dashcue::PropKey` for FLIP tracks, and `dashlang`'s reactive layer
/// builds its scheduler keys from it, so one `(node, channel)` yields
/// one key everywhere. It lives here — beside [`Channel`], over core
/// types only — because core cannot depend on `dashcue` and both
/// consumers already depend on core
/// (`docs/decisions/binding-table-in-the-document.md`).
pub fn prop_key(node: NodeId, channel: Channel) -> u64 {
    ((node.index() as u64) << 8) | channel.code() as u64
}

/// Decodes a [`prop_key`]-packed address back to its node slot and
/// channel — the one canonical decoder (debts #207/#208). Returns `None`
/// when the low byte is not a known [`Channel`] code or the slot does
/// not fit an arena index: such a key was not built by [`prop_key`], and
/// the caller names the failure (P4) rather than mis-binding it.
pub fn decode_prop_key(key: u64) -> Option<(u32, Channel)> {
    let channel = Channel::from_code((key & 0xFF) as u8)?;
    let slot = key >> 8;
    if slot > u32::MAX as u64 {
        return None;
    }
    Some((slot as u32, channel))
}

/// Stable handle to a signal declaration in one [`crate::Arena`],
/// returned by `Txn::declare_signal`. Only meaningful for the arena that
/// produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignalId(pub(crate) u32);

impl SignalId {
    /// The declaration's dense slot — its position in
    /// [`crate::Arena::signals`].
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One declared signal: an optional lookup name and the initial value
/// every binding seeds from. The name is how a runtime producer finds a
/// document-authored signal (a Figma variable's name, mode-qualified —
/// `docs/decisions/binding-table-in-the-document.md`); a `dashlang`
/// signal declared without a name stages `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalDecl {
    pub name: Option<String>,
    pub initial: f32,
}

/// One binding row: a signal, one channel on one node, and a transform.
/// Explicit and flat by design — a binding never relates two nodes
/// (`docs/decisions/bindings-are-explicit-and-flat.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Binding {
    pub signal: SignalId,
    pub node: NodeId,
    pub channel: Channel,
    pub transform: ScalarTransform,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prop_keys_round_trip_through_the_canonical_decoder() {
        // NodeId cannot be constructed here (its field is private to the
        // arena), so the round trip over real ids lives in the arena
        // integration tests and in dashscene-engine's; this pins the
        // decode-refusal contract on raw keys.
        assert_eq!(decode_prop_key(0xFF), None, "0xFF is no channel code");
        assert_eq!(
            decode_prop_key((7 << 8) | Channel::Gap.code() as u64),
            Some((7, Channel::Gap))
        );
        assert_eq!(
            decode_prop_key((u32::MAX as u64 + 1) << 8),
            None,
            "a slot past u32 was not packed by prop_key"
        );
    }

    #[test]
    fn channel_codes_round_trip() {
        for channel in Channel::ALL {
            assert_eq!(Channel::from_code(channel.code()), Some(channel));
        }
        assert_eq!(Channel::from_code(9), None);
        assert_eq!(Channel::from_code(u8::MAX), None);
    }

    #[test]
    fn transforms_apply_their_declared_math() {
        assert_eq!(ScalarTransform::Identity.apply(3.5), 3.5);
        assert_eq!(ScalarTransform::Scale(2.0).apply(3.0), 6.0);
        assert_eq!(
            ScalarTransform::MapRange {
                in_lo: 0.0,
                in_hi: 10.0,
                out_lo: 0.0,
                out_hi: 100.0,
            }
            .apply(2.5),
            25.0
        );
        // A zero-width input range maps to the output floor instead of
        // dividing by zero.
        assert_eq!(
            ScalarTransform::MapRange {
                in_lo: 5.0,
                in_hi: 5.0,
                out_lo: 1.0,
                out_hi: 2.0,
            }
            .apply(5.0),
            1.0
        );
        assert_eq!(ScalarTransform::Clamp { lo: 0.0, hi: 4.0 }.apply(9.0), 4.0);
    }
}
