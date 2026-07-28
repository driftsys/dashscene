//! The generated block-stress payload, shared by the two test binaries that
//! measure it: the profile-preview oracle's `profile-stress` scene, and the
//! perceptual calibration's high-frequency fixture (issue #544).
//!
//! Shared rather than copied. `crates/dashpack/tests/band_contract.rs`
//! generates equivalent content independently, and that copy is deliberate —
//! it lives in a crate that must not link Skia. Within this directory there is
//! no such constraint, so a second copy would be two generators that have to
//! agree with nothing holding them to it.

/// The generated fixture's `imageRef`, extent and detail amplitude.
///
/// Generated rather than committed, for the reason story #432 recorded when it
/// generated `detail-noise`: no committed corpus payload separates the two
/// profiles' area budgets, because the real image fills are a gradient and flat
/// rectangles that ASTC reproduces almost exactly at every footprint.
///
/// The amplitude was chosen by measurement. At 4 the LoFi ladder still bottoms
/// out at 12x12 and no mutation can fail its budget; at 16 both profiles
/// escalate to the lossless rung and neither budget binds; at 8 the two
/// profiles land on different rungs and both bands have something to say.
pub const STRESS_REF: &str = "block-stress";
pub const STRESS_EXTENT: u32 = 256;
pub const STRESS_AMPLITUDE: i32 = 8;

/// A deterministic integer hash — no floating point and no randomness, so the
/// generated fixture is identical on every machine and every build profile.
/// The same mixer story #432 uses, for the same reason.
fn splitmix(x: u32, y: u32, salt: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B) ^ salt;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h
}

/// A smooth three-channel gradient carrying a bounded per-texel perturbation —
/// the low-amplitude, high-spatial-frequency content block compression is worst
/// at.
pub fn block_stress(width: u32, height: u32, amplitude: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let base = [
                (x * 255 / width.max(1)) as i32,
                (y * 255 / height.max(1)) as i32,
                ((x + y) * 255 / (width + height).max(1)) as i32,
            ];
            for (channel, value) in base.iter().enumerate() {
                let span = 2 * amplitude as u32 + 1;
                let noise = (splitmix(x, y, channel as u32) % span) as i32 - amplitude;
                out.push((value + noise).clamp(0, 255) as u8);
            }
            out.push(255);
        }
    }
    out
}
