//! The wasm ABI wire format.
//!
//! Little-endian, `u32` lengths, no padding. The format is deliberately dull:
//! it is read by hand-written code on both sides (see
//! `docs/decisions/dashc-wasm-abi.md` for why this is not wasm-bindgen), so
//! every field is a length followed by exactly that many bytes.
//!
//! Nothing here panics. A wasm trap kills the module instance, so a malformed
//! request has to come back as a value — `Err(String)`, which the caller turns
//! into a [`Status::MalformedRequest`] response.

use std::collections::BTreeMap;

use dashpaint::{ImageAsset, ImageFormat};
use dashscene_validator::Profile;

use crate::figma::{BoundValue, BoundVariable};

/// The version of this wire format. The Deno side reads it at load and refuses
/// a module it does not understand, so a stale `.wasm` fails with a sentence
/// instead of a misdecode.
///
/// Version 2 (story #167) appends the joined variable-binding rows to the
/// compile request — see [`decode_compile_request`]. The framing and every
/// earlier field are unchanged; the version handshake is what makes the
/// append safe across a stale module.
pub const ABI_VERSION: u32 = 2;

/// The first field of every response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Status {
    Ok = 0,
    CompileError = 1,
    MalformedRequest = 2,
}

/// A decoded `dashc_compile_figma` request.
#[derive(Debug, Clone, PartialEq)]
pub struct CompileRequest {
    pub profile: Profile,
    pub json: String,
    pub images: BTreeMap<String, ImageAsset>,
    /// The importer's joined variable-binding rows (story #167); empty
    /// for an import without a vartable.
    pub bindings: Vec<BoundVariable>,
    /// The emit policy (story S0-impl): an optional trailing flag whose
    /// absence decodes as [`crate::EmitPolicy::Strict`], so a caller that
    /// predates the field still refuses-hard.
    pub policy: crate::EmitPolicy,
}

/// A cursor that runs out of bytes instead of panicking.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn u32(&mut self) -> Result<u32, String> {
        let end = self.at.checked_add(4).ok_or("length overflow")?;
        let field = self.bytes.get(self.at..end).ok_or_else(|| {
            format!(
                "want 4 bytes at offset {}, have {}",
                self.at,
                self.remaining()
            )
        })?;
        self.at = end;
        Ok(u32::from_le_bytes(
            field.try_into().expect("a 4-byte slice is 4 bytes"),
        ))
    }

    fn bytes(&mut self) -> Result<&'a [u8], String> {
        let len = self.u32()? as usize;
        let end = self.at.checked_add(len).ok_or("length overflow")?;
        let field = self.bytes.get(self.at..end).ok_or_else(|| {
            format!(
                "want {len} bytes at offset {}, have {}",
                self.at,
                self.remaining()
            )
        })?;
        self.at = end;
        Ok(field)
    }

    fn string(&mut self) -> Result<String, String> {
        let bytes = self.bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|e| format!("not UTF-8: {e}"))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.u32()?.to_le_bytes()))
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }

    /// A `u32` at the tail, or `None` at end-of-buffer — the tolerant read the
    /// appended emit-policy flag needs (story S0-impl). A request that predates
    /// the field ends here with nothing left, so it reads as `None`; a request
    /// carrying the flag reads it, and a partial (1–3 byte) tail is a decode
    /// error like any other truncation.
    fn optional_u32(&mut self) -> Result<Option<u32>, String> {
        if self.remaining() == 0 {
            return Ok(None);
        }
        Ok(Some(self.u32()?))
    }

    /// Trailing bytes mean the two sides disagree about the format, which is
    /// exactly the bug this ABI exists to make loud.
    fn finish(&self) -> Result<(), String> {
        match self.remaining() {
            0 => Ok(()),
            n => Err(format!("{n} trailing byte(s) after the request")),
        }
    }
}

/// Decodes one `dashc_compile_figma` request.
pub fn decode_compile_request(bytes: &[u8]) -> Result<CompileRequest, String> {
    let mut reader = Reader::new(bytes);

    let profile = match reader.u32()? {
        0 => Profile::Core,
        1 => Profile::Full,
        other => return Err(format!("unknown profile {other} (0 = core, 1 = full)")),
    };
    let json = reader.string()?;

    let count = reader.u32()?;
    let mut images = BTreeMap::new();
    for _ in 0..count {
        let image_ref = reader.string()?;
        let format = match reader.u32()? {
            0 => ImageFormat::Png,
            1 => ImageFormat::Jpeg,
            2 => ImageFormat::Gif,
            other => {
                return Err(format!(
                    "unknown image format {other} (0 = png, 1 = jpeg, 2 = gif)"
                ));
            }
        };
        let asset = ImageAsset {
            format,
            bytes: reader.bytes()?.to_vec(),
        };
        if images.insert(image_ref.clone(), asset).is_some() {
            return Err(format!("imageRef {image_ref} appears twice"));
        }
    }

    // The joined variable-binding rows (ABI v2, story #167): per row, the
    // Figma node id, the property path, the mode-qualified signal name,
    // and a tagged value — 0 = one f32, 1 = four f32 color components.
    let count = reader.u32()?;
    let mut bindings = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let node_id = reader.string()?;
        let property = reader.string()?;
        let signal = reader.string()?;
        let value = match reader.u32()? {
            0 => BoundValue::Float(reader.f32()?),
            1 => BoundValue::Color {
                r: reader.f32()?,
                g: reader.f32()?,
                b: reader.f32()?,
                a: reader.f32()?,
            },
            other => {
                return Err(format!(
                    "unknown bound value type {other} (0 = float, 1 = color)"
                ));
            }
        };
        bindings.push(BoundVariable {
            node_id,
            property,
            signal,
            value,
        });
    }

    // The emit-policy flag (story S0-impl), appended after the bindings:
    // `1` or absent ⇒ Strict (an old caller refuses-hard), `0` ⇒ Partial.
    let policy = match reader.optional_u32()? {
        Some(0) => crate::EmitPolicy::Partial,
        _ => crate::EmitPolicy::Strict,
    };
    reader.finish()?;

    Ok(CompileRequest {
        profile,
        json,
        images,
        bindings,
        policy,
    })
}

/// Frames one response: a `u32` byte count, then the envelope.
///
/// The count is what lets the caller release the buffer with a single
/// `dashc_free(ptr, 4 + count)` — see `docs/decisions/dashc-wasm-abi.md` on why
/// the length is a prefix rather than a pointer/length pair packed into a
/// `u64`: the packed form assumes a 32-bit pointer, which is true on wasm and
/// false on the native target that tests these exports.
pub fn encode_response(status: u32, blob: &[u8], json: &str) -> Vec<u8> {
    let body_len = 4 + 4 + blob.len() + 4 + json.len();
    let mut out = Vec::with_capacity(4 + body_len);

    out.extend_from_slice(&(body_len as u32).to_le_bytes());
    out.extend_from_slice(&status.to_le_bytes());
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(blob);
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test row's value, mirroring the TypeScript codec's tagging.
    enum WireValue {
        Float(f32),
        Color([f32; 4]),
    }

    /// Encodes a request the way the TypeScript codec does
    /// (`importers/figma/src/wasm.ts`). If this helper and that codec ever
    /// disagree, the Deno suite fails on the golden — which is the point.
    fn encode_request(
        profile: u32,
        json: &str,
        images: &[(&str, u32, &[u8])],
        bindings: &[(&str, &str, &str, WireValue)],
        strict: Option<u32>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&profile.to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(json.as_bytes());
        out.extend_from_slice(&(images.len() as u32).to_le_bytes());
        for (image_ref, format, bytes) in images {
            out.extend_from_slice(&(image_ref.len() as u32).to_le_bytes());
            out.extend_from_slice(image_ref.as_bytes());
            out.extend_from_slice(&format.to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out.extend_from_slice(&(bindings.len() as u32).to_le_bytes());
        for (node_id, property, signal, value) in bindings {
            for field in [node_id, property, signal] {
                out.extend_from_slice(&(field.len() as u32).to_le_bytes());
                out.extend_from_slice(field.as_bytes());
            }
            match value {
                WireValue::Float(v) => {
                    out.extend_from_slice(&0u32.to_le_bytes());
                    out.extend_from_slice(&v.to_le_bytes());
                }
                WireValue::Color(components) => {
                    out.extend_from_slice(&1u32.to_le_bytes());
                    for component in components {
                        out.extend_from_slice(&component.to_le_bytes());
                    }
                }
            }
        }
        // The trailing emit-policy flag (story S0-impl): appended only when the
        // caller sets it, so `None` reproduces the pre-existing wire shape an
        // old caller sends — which must still decode (as Strict).
        if let Some(flag) = strict {
            out.extend_from_slice(&flag.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_request_round_trips() {
        let bytes = encode_request(1, "{}", &[("abc", 0, &[1, 2, 3])], &[], None);
        let request = decode_compile_request(&bytes).expect("the request decodes");

        assert_eq!(request.profile, Profile::Full);
        assert_eq!(request.json, "{}");
        assert_eq!(request.images.len(), 1);
        let asset = &request.images["abc"];
        assert_eq!(asset.format, ImageFormat::Png);
        assert_eq!(asset.bytes, vec![1, 2, 3]);
        assert!(request.bindings.is_empty());
    }

    /// Figma re-encodes opaque uploads to Jpeg (story #342), so the wire
    /// format needs a tag for it — additive, the framing does not change.
    #[test]
    fn a_jpeg_image_format_round_trips() {
        let bytes = encode_request(0, "{}", &[("abc", 1, &[0xff, 0xd8, 0xff])], &[], None);
        let request = decode_compile_request(&bytes).expect("the request decodes");
        assert_eq!(request.images["abc"].format, ImageFormat::Jpeg);
    }

    /// Static Gif fills (story #342); animated Gif is refused upstream by
    /// the importer, so this ABI only ever carries a static one.
    #[test]
    fn a_gif_image_format_round_trips() {
        let bytes = encode_request(0, "{}", &[("abc", 2, &[0x47, 0x49, 0x46, 0x38])], &[], None);
        let request = decode_compile_request(&bytes).expect("the request decodes");
        assert_eq!(request.images["abc"].format, ImageFormat::Gif);
    }

    #[test]
    fn a_request_without_a_policy_flag_decodes_as_strict() {
        // The pre-existing wire shape (no trailing u32) must still decode.
        let bytes = encode_request(0, "{}", &[], &[], None);
        let request = decode_compile_request(&bytes).expect("decodes");
        assert_eq!(request.policy, crate::EmitPolicy::Strict);
    }

    #[test]
    fn a_trailing_zero_flag_decodes_as_partial() {
        let bytes = encode_request(0, "{}", &[], &[], Some(0));
        let request = decode_compile_request(&bytes).expect("decodes");
        assert_eq!(request.policy, crate::EmitPolicy::Partial);
        let bytes = encode_request(0, "{}", &[], &[], Some(1));
        let request = decode_compile_request(&bytes).expect("decodes");
        assert_eq!(request.policy, crate::EmitPolicy::Strict);
    }

    #[test]
    fn binding_rows_round_trip_both_value_types() {
        let bytes = encode_request(
            0,
            "{}",
            &[],
            &[
                ("1:8", "itemSpacing", "size/gap", WireValue::Float(16.0)),
                (
                    "1:9",
                    "fills[0].color",
                    "color/accent@dark",
                    WireValue::Color([0.4, 0.65, 1.0, 1.0]),
                ),
            ],
            None,
        );
        let request = decode_compile_request(&bytes).expect("the request decodes");

        assert_eq!(request.bindings.len(), 2);
        assert_eq!(
            request.bindings[0],
            BoundVariable {
                node_id: "1:8".to_string(),
                property: "itemSpacing".to_string(),
                signal: "size/gap".to_string(),
                value: BoundValue::Float(16.0),
            }
        );
        assert_eq!(
            request.bindings[1].value,
            BoundValue::Color {
                r: 0.4,
                g: 0.65,
                b: 1.0,
                a: 1.0
            }
        );
    }

    #[test]
    fn an_unknown_bound_value_type_is_an_error() {
        let mut bytes = encode_request(0, "{}", &[], &[], None);
        // Rewrite the binding count to 1 and append a row with tag 9.
        let at = bytes.len() - 4;
        bytes[at..].copy_from_slice(&1u32.to_le_bytes());
        for field in ["1:8", "itemSpacing", "size/gap"] {
            bytes.extend_from_slice(&(field.len() as u32).to_le_bytes());
            bytes.extend_from_slice(field.as_bytes());
        }
        bytes.extend_from_slice(&9u32.to_le_bytes());
        bytes.extend_from_slice(&1.0f32.to_le_bytes());
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn a_truncated_request_is_an_error_not_a_panic() {
        let bytes = encode_request(0, "{}", &[("abc", 0, &[1, 2, 3])], &[], None);
        for cut in 0..bytes.len() {
            // Every prefix must decode to an error. A panic here would trap the
            // wasm module, turning a bad request into an unrecoverable importer.
            assert!(
                decode_compile_request(&bytes[..cut]).is_err(),
                "prefix of {cut} bytes"
            );
        }
    }

    #[test]
    fn trailing_bytes_are_an_error() {
        let mut bytes = encode_request(0, "{}", &[], &[], None);
        bytes.push(0);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn an_unknown_profile_is_an_error() {
        let bytes = encode_request(7, "{}", &[], &[], None);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn an_unknown_image_format_is_an_error() {
        let bytes = encode_request(0, "{}", &[("abc", 9, &[1])], &[], None);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn a_response_is_length_prefixed() {
        let response = encode_response(Status::Ok as u32, &[7, 8], "{}");

        let total = u32::from_le_bytes(response[0..4].try_into().unwrap()) as usize;
        assert_eq!(
            total,
            response.len() - 4,
            "the prefix counts everything after itself"
        );
        assert_eq!(u32::from_le_bytes(response[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(response[8..12].try_into().unwrap()), 2);
        assert_eq!(&response[12..14], &[7, 8]);
        assert_eq!(u32::from_le_bytes(response[14..18].try_into().unwrap()), 2);
        assert_eq!(&response[18..20], b"{}");
    }
}
