//! The baked gradient strip: one 256-texel row per interned gradient, so a
//! gradient pixel costs one texture sample rather than a walk over the stops.
//!
//! # What is baked and what is not
//!
//! Only the **ramp** — the colour a gradient takes at a normalized position
//! `t`. The mapping from a fragment's position to `t` is the gradient's frame,
//! and that stays per painter and per fragment: a gradient row is interned and
//! shared, while the node box the three handles are normalized to is per
//! instance, so the frame cannot live on the row at all
//! (`gradient_colour` in `crates/dashscene-gpu/src/shaders/paint.wgsl` says the
//! same thing from the other side). P1 is untouched: a ramp is the intent the
//! document already carries, evaluated at 256 positions, not a resolved pixel
//! of the composed picture.
//!
//! # Where the error is, and how large
//!
//! A painter samples the row with bilinear filtering along `t`, so between two
//! texel centres the result is the linear interpolation the ramp is already
//! made of, and the two agree exactly. Two things do differ:
//!
//! - **Quantisation.** A texel holds eight bits per channel, so a channel is
//!   within half a code point of the analytic ramp.
//! - **A stop between texel centres.** The texels either side of it are on the
//!   ramp, so the step lands on the nearer texel centre — within `1/512` of `t`
//!   of where the analytic ramp puts it.
//!
//! # One ramp, one statement of the rules
//!
//! [`ramp`] is the only CPU evaluation of a stop list in this workspace. It was
//! `reference_ramp` in `crates/dashscene-gpu/tests/layer2_conformance.rs`,
//! where it is the independent reference `gradient_ramp` in `sdf.wgsl` is
//! measured against; that test now imports it from here, so the hard-stop rule
//! below has one statement rather than two.

use crate::{Color, Gradient, GradientStop};

/// Texels along one row — the resolution a ramp is baked at.
///
/// 256 is what a `Canvas` team ships a gradient PNG at
/// (`docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`,
/// D2 rule 3), and it is what puts the sampling error at `1/512` of `t`.
pub const STRIP_WIDTH: usize = 256;

/// Bytes in one baked row: [`STRIP_WIDTH`] texels of straight-alpha RGBA8.
///
/// Named because it is the stride a consumer reading the rows out over a C ABI
/// is held to, and a stride stated twice is a stride that drifts.
pub const STRIP_ROW_BYTES: usize = STRIP_WIDTH * 4;

/// The baked rows of one paint table's gradients, in gradient-row order.
///
/// `rgba8` is `rows * STRIP_ROW_BYTES` bytes, tightly packed: row `i` is
/// `rgba8[i * STRIP_ROW_BYTES..][..STRIP_ROW_BYTES]`. Straight alpha, not
/// premultiplied — a painter premultiplies at the end of its own shading, and
/// a premultiplied ramp would interpolate differently from the stops.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripImage {
    /// How many rows, which is how many gradients the table interned. Zero for
    /// a document with no gradient fill, whose `rgba8` is then empty.
    pub rows: usize,
    pub rgba8: Vec<u8>,
}

/// The colour a stop list takes at normalized position `t`.
///
/// # The rules, stated once
///
/// **Clamped at both ends, not repeated.** Below the first stop the first
/// colour, above the last stop the last colour — `TileMode::Clamp`, which is
/// what every gradient in `dashscene-skia` is built with. A stop range that
/// does not start at 0 or end at 1 is an ordinary case rather than a
/// degenerate one: it is what a producer authors whenever it moves a handle
/// instead of a stop.
///
/// **Interpolation is a plain linear blend of the stored components**, which
/// is sRGB-encoded space — `docs/decisions/blur-blends-in-srgb-encoded-space.md`
/// makes that a term of the boundary-B contract rather than a per-painter
/// choice.
///
/// **The offsets are non-decreasing**, the same precondition Skia's own
/// gradient shaders carry. This does not sort them; an out-of-order stop makes
/// it disagree with the painters rather than fail.
///
/// # The two ends are not symmetric, and that is the ramp's own rule
///
/// The lower clamp is **strict** and the upper one is not. Both sides of a hard
/// stop are reachable at the same `t`, and the ramp is right-continuous: the
/// colour *at* a repeated offset is the later stop's. So a `t` equal to the
/// first offset must not short-circuit to the first colour — with the first two
/// stops repeated, the answer is the second colour, and a `<=` here would
/// disagree with `gradient_ramp` at exactly that point. The upper clamp stays
/// inclusive for the same reason read the other way: at a `t` equal to the last
/// offset, the last colour is the later of whatever pair meets there.
///
/// Written as "the first stop past `t`, and the segment before it" rather than
/// as a transliteration of the shader's overwriting walk, because it is the
/// independent reference that walk is measured against.
///
/// # Panics
///
/// Panics on an empty stop list. A gradient the paint table holds carries at
/// least one stop, validated upstream (P4). [`bake_row`] is what turns the
/// empty case into a row rather than a panic, and it says what it turns it
/// into.
pub fn ramp(stops: &[GradientStop], t: f32) -> Color {
    let first = stops[0];
    let last = stops[stops.len() - 1];
    if t < first.offset {
        return first.color;
    }
    if t >= last.offset {
        return last.color;
    }
    let above = stops
        .iter()
        .position(|stop| stop.offset > t)
        .expect("t is below the last stop, so some stop is above it");
    let lo = stops[above - 1];
    let hi = stops[above];
    // `above` is the first stop past `t` and `t` is past `lo`, so this segment
    // has width — a repeated offset is never the divisor here, which is the
    // shape difference from the shader's form.
    let u = (t - lo.offset) / (hi.offset - lo.offset);
    Color {
        r: lo.color.r + (hi.color.r - lo.color.r) * u,
        g: lo.color.g + (hi.color.g - lo.color.g) * u,
        b: lo.color.b + (hi.color.b - lo.color.b) * u,
        a: lo.color.a + (hi.color.a - lo.color.a) * u,
    }
}

/// Bakes one stop list into one row: the ramp at each of [`STRIP_WIDTH`] texel
/// centres, RGBA8.
///
/// A texel's centre, not its left edge: `(x + 0.5) / STRIP_WIDTH`. That is
/// where a bilinear sample of the finished row reads the value back, so it is
/// where the value has to have been taken.
///
/// **An empty stop list bakes a transparent row.** It is the answer
/// `gradient_ramp` gives for a zero count, and for its reason: there is no
/// first colour to take, and drawing nothing is the one answer that cannot
/// paint a wrong one. No interned gradient produces it.
pub fn bake_row(stops: &[GradientStop], out: &mut [u8; STRIP_ROW_BYTES]) {
    if stops.is_empty() {
        out.fill(0);
        return;
    }
    for x in 0..STRIP_WIDTH {
        let t = (x as f32 + 0.5) / STRIP_WIDTH as f32;
        let c = ramp(stops, t);
        out[x * 4..x * 4 + 4].copy_from_slice(&[q(c.r), q(c.g), q(c.b), q(c.a)]);
    }
}

/// Bakes every gradient of a paint table into its own row, in gradient-row
/// order — so a painter samples row `i` for the gradient at index `i`.
///
/// `gradients` and `stops` are [`PaintTable::all_gradients`] and
/// [`PaintTable::all_stops`]; each gradient's [`StopRange`] indexes the second.
///
/// [`PaintTable::all_gradients`]: crate::PaintTable::all_gradients
/// [`PaintTable::all_stops`]: crate::PaintTable::all_stops
/// [`StopRange`]: crate::StopRange
///
/// # Panics
///
/// Panics if a gradient's stop range runs past `stops`, the same
/// index-integrity contract [`PaintTable::stops`] is held to: a miss is a
/// broken contract between crates, not a row to draw approximately (P4).
///
/// [`PaintTable::stops`]: crate::PaintTable::stops
pub fn bake(gradients: &[Gradient], stops: &[GradientStop]) -> StripImage {
    let mut rgba8 = vec![0u8; STRIP_ROW_BYTES * gradients.len()];
    for (i, gradient) in gradients.iter().enumerate() {
        let start = gradient.stops.offset as usize;
        let end = start + gradient.stops.count as usize;
        let row_stops = stops.get(start..end).unwrap_or_else(|| {
            panic!(
                "stop range {start}..{end} out of range: the table holds {}",
                stops.len()
            )
        });
        let row: &mut [u8; STRIP_ROW_BYTES] = (&mut rgba8
            [i * STRIP_ROW_BYTES..(i + 1) * STRIP_ROW_BYTES])
            .try_into()
            .expect("the slice is one row long by construction");
        bake_row(row_stops, row);
    }
    StripImage {
        rows: gradients.len(),
        rgba8,
    }
}

/// One channel, quantised to eight bits: round to nearest, clamped to the
/// representable range.
///
/// The clamp is the guard rather than a formality — a `Color` is four plain
/// `f32`s with no invariant on their range, and `as u8` on an out-of-range
/// float saturates in Rust but a NaN becomes zero, so the clamp is what makes
/// this total rather than what makes it correct.
fn q(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(offset: f32, rgba: [f32; 4]) -> GradientStop {
        GradientStop {
            offset,
            color: Color {
                r: rgba[0],
                g: rgba[1],
                b: rgba[2],
                a: rgba[3],
            },
        }
    }

    #[test]
    fn a_three_stop_ramp_bakes_the_hand_computed_texels() {
        let stops = [
            stop(0.0, [0.0, 0.0, 0.0, 1.0]),
            stop(0.37, [1.0, 0.5, 0.0, 1.0]),
            stop(1.0, [0.0, 0.0, 1.0, 1.0]),
        ];
        let mut row = [0u8; STRIP_ROW_BYTES];
        bake_row(&stops, &mut row);
        let texel = |t: f32| {
            let x = ((t * STRIP_WIDTH as f32) as usize).min(STRIP_WIDTH - 1);
            &row[x * 4..x * 4 + 4]
        };
        let near = |got: &[u8], want: [u8; 4]| {
            got.iter()
                .zip(want)
                .all(|(a, e)| (i32::from(*a) - i32::from(e)).abs() <= 1)
        };
        // Midway through the first segment: (0.5, 0.25, 0, 1); through the
        // second: (0.5, 0.25, 0.5, 1). All four channels, because a swapped
        // pair of channels is the mistake a single-channel check cannot see.
        assert!(near(texel(0.185), [128, 64, 0, 255]), "{:?}", texel(0.185));
        assert!(
            near(texel(0.685), [128, 64, 128, 255]),
            "{:?}",
            texel(0.685)
        );
        // The end texels are sampled at their centres, 1/512 in from t = 0 and
        // t = 1 — so neither is the end colour exactly.
        assert!(near(texel(0.001), [1, 1, 0, 255]), "{:?}", texel(0.001));
        assert!(near(texel(0.999), [1, 0, 254, 255]), "{:?}", texel(0.999));
    }

    #[test]
    fn a_two_stop_ramp_is_exact_at_every_texel_centre() {
        // A different ramp per channel, so a channel swap or a dropped channel
        // fails rather than passing on a coincidence — and **alpha ramps too**,
        // which is what pins the straight-alpha contract. With every fixture at
        // alpha one, a bake that premultiplied before quantising would agree
        // with this everywhere and the rule would be stated only in prose.
        let stops = [
            stop(0.0, [0.0, 1.0, 0.0, 1.0]),
            stop(1.0, [1.0, 0.0, 0.5, 0.25]),
        ];
        let mut row = [0u8; STRIP_ROW_BYTES];
        bake_row(&stops, &mut row);
        for x in 0..STRIP_WIDTH {
            let t = (x as f32 + 0.5) / STRIP_WIDTH as f32;
            let e = |v: f32| (v * 255.0 + 0.5) as u8;
            assert_eq!(
                &row[x * 4..x * 4 + 4],
                &[e(t), e(1.0 - t), e(0.5 * t), e(1.0 + (0.25 - 1.0) * t)],
                "texel {x}"
            );
        }
    }

    #[test]
    fn an_empty_stop_list_bakes_a_transparent_row() {
        // The rule `gradient_ramp` states for a zero count, on this side. No
        // interned gradient produces it, so nothing else in the workspace would
        // notice this branch answering opaque white instead.
        let mut row = [0xABu8; STRIP_ROW_BYTES];
        bake_row(&[], &mut row);
        assert!(row.iter().all(|&b| b == 0), "every byte is zero");
    }

    #[test]
    fn a_stop_between_texel_centres_lands_on_the_nearer_texel() {
        // A hard step — two stops at one offset — at t = 0.5 + 1/1024. Texel
        // 128's centre is at 0.501953, past the step, so texel 127 is black and
        // texel 128 is white.
        let step = 0.5 + 1.0 / 1024.0;
        let stops = [
            stop(0.0, [0.0, 0.0, 0.0, 1.0]),
            stop(step, [0.0, 0.0, 0.0, 1.0]),
            stop(step, [1.0, 1.0, 1.0, 1.0]),
            stop(1.0, [1.0, 1.0, 1.0, 1.0]),
        ];
        let mut row = [0u8; STRIP_ROW_BYTES];
        bake_row(&stops, &mut row);
        assert_eq!(row[127 * 4], 0, "the texel below the step");
        assert_eq!(row[128 * 4], 255, "the texel above the step");
        // And the step offset itself divides by nothing: the hard-stop rule,
        // which is why `ramp` takes the segment *after* the repeated offset.
        let at_step = ramp(&stops, step);
        assert_eq!(
            at_step.r, 1.0,
            "a hard stop takes the later colour, never a NaN"
        );
    }

    #[test]
    fn stops_inside_the_range_clamp_to_the_end_colours() {
        // The ordinary case a producer authors by moving a handle rather than a
        // stop: the stops sit at 0.25 and 0.75 and the ends clamp.
        let stops = [
            stop(0.25, [0.0, 0.0, 0.0, 1.0]),
            stop(0.75, [1.0, 1.0, 1.0, 1.0]),
        ];
        let mut row = [0u8; STRIP_ROW_BYTES];
        bake_row(&stops, &mut row);
        assert_eq!(
            &row[0..4],
            &[0, 0, 0, 255],
            "below the first stop, the first colour"
        );
        assert_eq!(
            &row[255 * 4..256 * 4],
            &[255, 255, 255, 255],
            "above the last stop, the last colour"
        );
        let midpoint = 128.5 / STRIP_WIDTH as f32;
        assert_eq!(
            row[128 * 4],
            ((midpoint - 0.25) / 0.5 * 255.0 + 0.5) as u8,
            "the midpoint texel sits on the ramp"
        );
    }

    #[test]
    fn every_gradient_gets_its_own_row_in_index_order() {
        // Two gradients whose stop ranges are *not* in table order, so a bake
        // that walked the flat stop array rather than each gradient's own range
        // produces the rows the other way round.
        let stops = [
            stop(0.0, [1.0, 0.0, 0.0, 1.0]),
            stop(1.0, [1.0, 0.0, 0.0, 1.0]),
            stop(0.0, [0.0, 0.0, 1.0, 1.0]),
            stop(1.0, [0.0, 0.0, 1.0, 1.0]),
        ];
        let gradient = |offset: u32| Gradient {
            kind: crate::GradientKind::Linear,
            handle_origin: crate::Vec2 { x: 0.0, y: 0.0 },
            handle_primary: crate::Vec2 { x: 1.0, y: 0.0 },
            handle_secondary: crate::Vec2 { x: 0.0, y: 1.0 },
            stops: crate::StopRange { offset, count: 2 },
        };
        let strip = bake(&[gradient(2), gradient(0)], &stops);
        assert_eq!(strip.rows, 2);
        assert_eq!(strip.rgba8.len(), 2 * STRIP_ROW_BYTES);
        assert_eq!(&strip.rgba8[0..4], &[0, 0, 255, 255], "row 0 is blue");
        assert_eq!(
            &strip.rgba8[STRIP_ROW_BYTES..STRIP_ROW_BYTES + 4],
            &[255, 0, 0, 255],
            "row 1 is red"
        );
        // A document with no gradient bakes no row at all, which is the case
        // both painters branch on: the lean one binds a placeholder texture and
        // the C ABI hands out an empty slice.
        assert_eq!(bake(&[], &stops), StripImage::default());
    }
}
