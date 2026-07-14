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

/// The version of this wire format. The Deno side reads it at load and refuses
/// a module it does not understand, so a stale `.wasm` fails with a sentence
/// instead of a misdecode.
pub const ABI_VERSION: u32 = 1;

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

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
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
            other => return Err(format!("unknown image format {other} (0 = png)")),
        };
        let asset = ImageAsset {
            format,
            bytes: reader.bytes()?.to_vec(),
        };
        if images.insert(image_ref.clone(), asset).is_some() {
            return Err(format!("imageRef {image_ref} appears twice"));
        }
    }
    reader.finish()?;

    Ok(CompileRequest {
        profile,
        json,
        images,
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

    /// Encodes a request the way the TypeScript codec does
    /// (`importers/figma/src/wasm.ts`). If this helper and that codec ever
    /// disagree, the Deno suite fails on the golden — which is the point.
    fn encode_request(profile: u32, json: &str, images: &[(&str, u32, &[u8])]) -> Vec<u8> {
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
        out
    }

    #[test]
    fn a_request_round_trips() {
        let bytes = encode_request(1, "{}", &[("abc", 0, &[1, 2, 3])]);
        let request = decode_compile_request(&bytes).expect("the request decodes");

        assert_eq!(request.profile, Profile::Full);
        assert_eq!(request.json, "{}");
        assert_eq!(request.images.len(), 1);
        let asset = &request.images["abc"];
        assert_eq!(asset.format, ImageFormat::Png);
        assert_eq!(asset.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn a_truncated_request_is_an_error_not_a_panic() {
        let bytes = encode_request(0, "{}", &[("abc", 0, &[1, 2, 3])]);
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
        let mut bytes = encode_request(0, "{}", &[]);
        bytes.push(0);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn an_unknown_profile_is_an_error() {
        let bytes = encode_request(7, "{}", &[]);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn an_unknown_image_format_is_an_error() {
        let bytes = encode_request(0, "{}", &[("abc", 9, &[1])]);
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
