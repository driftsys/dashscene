//! The wasm ABI, driven exactly as the Deno importer drives it — but natively.
//!
//! These call the real exports: allocate a request in the module's allocator,
//! write it, call, decode the length-prefixed response, free. The response is a
//! length-prefixed buffer rather than a `(ptr, len)` pair packed into a `u64`
//! precisely so this test can exist: a packed pair assumes a 32-bit pointer,
//! which is true on wasm and false here.
//!
//! What this cannot cover is the TypeScript codec on the other side. That is
//! what the shared golden `.dsb` is for (`goldens/dsb/v03-paint.dsb`): both
//! languages pin the same bytes, so identity is transitive.

use dashc_wasm::abi::{
    dashc_abi_version, dashc_alloc, dashc_compile_figma, dashc_figma_image_refs, dashc_free,
};

const V03_PAINT: &str = include_str!("../../../corpus/figma-fixtures/v03-paint.json");
const IMAGE_REF: &str = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
const IMAGE_PNG: &[u8] = include_bytes!(
    "../../../corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png"
);
const GOLDEN: &[u8] = include_bytes!("../../../goldens/dsb/v03-paint.dsb");

/// One decoded response envelope.
struct Response {
    status: u32,
    blob: Vec<u8>,
    json: String,
}

/// Encodes a request the way `importers/figma/src/wasm.ts` does.
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

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

/// Drives one export the way the Deno codec does: reserve, write, call, read,
/// release.
fn call(export: unsafe extern "C" fn(*const u8, u32) -> *mut u8, request: &[u8]) -> Response {
    // SAFETY: the request buffer comes from `dashc_alloc` and is written for
    // exactly its own length; the response pointer and its length prefix come
    // from the export, and each buffer is freed exactly once with the length it
    // was allocated with.
    unsafe {
        let request_ptr = dashc_alloc(request.len() as u32);
        assert!(!request_ptr.is_null(), "the request allocation succeeds");
        std::ptr::copy_nonoverlapping(request.as_ptr(), request_ptr, request.len());

        let response_ptr = export(request_ptr, request.len() as u32);
        dashc_free(request_ptr, request.len() as u32);
        assert!(!response_ptr.is_null(), "the response allocation succeeds");

        let total = read_u32(std::slice::from_raw_parts(response_ptr, 4), 0) as usize;
        let response = std::slice::from_raw_parts(response_ptr, 4 + total).to_vec();
        dashc_free(response_ptr, (4 + total) as u32);

        let status = read_u32(&response, 4);
        let blob_len = read_u32(&response, 8) as usize;
        let blob = response[12..12 + blob_len].to_vec();
        let json_len = read_u32(&response, 12 + blob_len) as usize;
        let json_at = 12 + blob_len + 4;
        let json = String::from_utf8(response[json_at..json_at + json_len].to_vec())
            .expect("the json field is UTF-8");

        Response { status, blob, json }
    }
}

#[test]
fn the_abi_version_is_pinned() {
    // A bump is a deliberate break: the Deno loader refuses a version it does
    // not know, so this constant and `importers/figma/src/wasm.ts` move together.
    assert_eq!(dashc_abi_version(), 1);
}

#[test]
fn the_fixture_compiles_to_the_golden_dsb() {
    let request = encode_request(0, V03_PAINT, &[(IMAGE_REF, 0, IMAGE_PNG)]);
    let response = call(dashc_compile_figma, &request);

    assert_eq!(
        response.status, 0,
        "status 0 = ok; json was {}",
        response.json
    );
    assert_eq!(response.json, r#"{"diagnostics":[]}"#);
    assert_eq!(
        response.blob, GOLDEN,
        "the ABI emits the same bytes as the library call",
    );
}

#[test]
fn an_unresolved_image_is_a_tagged_error() {
    let request = encode_request(0, V03_PAINT, &[]);
    let response = call(dashc_compile_figma, &request);

    assert_eq!(response.status, 1);
    assert!(
        response.json.contains(r#""kind":"unresolvedImage""#),
        "json was {}",
        response.json,
    );
    assert!(response.json.contains(IMAGE_REF));
    assert!(
        response.blob.is_empty(),
        "a blocked document emits no bytes (R6)"
    );
}

#[test]
fn a_malformed_request_is_a_status_not_a_trap() {
    let response = call(dashc_compile_figma, &[0, 0]);

    assert_eq!(response.status, 2);
    assert!(response.blob.is_empty());
    assert!(
        !response.json.is_empty(),
        "a malformed request explains itself"
    );
}

#[test]
fn image_refs_crosses_the_abi() {
    let response = call(dashc_figma_image_refs, V03_PAINT.as_bytes());

    assert_eq!(response.status, 0);
    assert_eq!(response.json, format!(r#"["{IMAGE_REF}"]"#));
    assert!(response.blob.is_empty(), "image_refs carries no blob");
}
