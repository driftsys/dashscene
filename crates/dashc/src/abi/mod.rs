//! The wasm ABI: five `extern "C"` exports over a hand-written wire format.
//!
//! The exports are thin on purpose. Each one turns raw pointers into a slice,
//! calls a function that is ordinary safe Rust, and frames the result — so
//! everything worth testing is reachable from a native `cargo test`, and
//! `tests/abi.rs` drives these very symbols.
//!
//! Why hand-written rather than wasm-bindgen: see
//! `docs/decisions/dashc-wasm-abi.md`. The short version is that core wasm has
//! no string, array, or object type — only numbers and one linear memory — so
//! *every* option compiles down to "allocate, copy in, pass a pointer and a
//! length". wasm-bindgen would generate that, but it would not save the mirror
//! types in [`json`], which are the actual work: `dashscene-validator` and
//! `dashpaint` carry no `serde`.

pub mod json;
pub mod wire;

use std::alloc::{Layout, alloc, dealloc};

use crate::abi::wire::Status;
use crate::figma;

/// The version of the wire format this module speaks.
#[unsafe(no_mangle)]
pub extern "C" fn dashc_abi_version() -> u32 {
    wire::ABI_VERSION
}

/// Reserves `len` bytes in the module's linear memory, for the caller to write
/// a request into.
///
/// Returns null when `len` is 0 or the allocation fails — never panics, because
/// a panic here traps the module and takes the importer down with it.
#[unsafe(no_mangle)]
pub extern "C" fn dashc_alloc(len: u32) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(len as usize, 1) else {
        return std::ptr::null_mut();
    };
    if layout.size() == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: the layout is non-zero-sized, checked directly above.
    unsafe { alloc(layout) }
}

/// Releases a buffer obtained from [`dashc_alloc`], or a response buffer
/// returned by one of the compile exports.
///
/// # Safety
///
/// `ptr` must have come from this module, and `len` must be the length it was
/// allocated with — for a response buffer, `4 + total`, where `total` is the
/// `u32` the buffer starts with.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashc_free(ptr: *mut u8, len: u32) {
    let Ok(layout) = Layout::from_size_align(len as usize, 1) else {
        return;
    };
    if ptr.is_null() || layout.size() == 0 {
        return;
    }
    // SAFETY: the caller guarantees ptr and len came from this module's
    // allocator with exactly this layout.
    unsafe { dealloc(ptr, layout) }
}

/// Compiles Figma REST JSON into a `.dsb`.
///
/// The request is the framing in [`wire`]. The return is a length-prefixed
/// response buffer the caller owns and must release with [`dashc_free`].
///
/// # Safety
///
/// `ptr` must point at `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashc_compile_figma(ptr: *const u8, len: u32) -> *mut u8 {
    // SAFETY: the caller guarantees ptr and len describe a readable region.
    let request = unsafe { as_slice(ptr, len) };
    leak(compile_figma_response(request))
}

/// Names the `imageRef`s a lowering of this file will demand.
///
/// The request is the raw UTF-8 JSON, unframed. The return is the same response
/// buffer as [`dashc_compile_figma`], carrying the refs as a JSON array and no
/// blob.
///
/// # Safety
///
/// `ptr` must point at `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashc_figma_image_refs(ptr: *const u8, len: u32) -> *mut u8 {
    // SAFETY: the caller guarantees ptr and len describe a readable region.
    let request = unsafe { as_slice(ptr, len) };
    leak(image_refs_response(request))
}

/// # Safety
///
/// `ptr` must point at `len` readable bytes, or be null with `len` 0.
unsafe fn as_slice<'a>(ptr: *const u8, len: u32) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        return &[];
    }
    // SAFETY: the caller guarantees the region is readable for `len` bytes.
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Hands a response buffer to the caller, who now owns it.
///
/// `into_boxed_slice` shrinks capacity to length, which is what makes the
/// caller's `dashc_free(ptr, 4 + total)` the exact inverse of this allocation.
fn leak(response: Vec<u8>) -> *mut u8 {
    Box::into_raw(response.into_boxed_slice()).cast::<u8>()
}

/// Everything above the raw-pointer layer: safe Rust, and the reason the exports
/// are three lines each.
fn compile_figma_response(request: &[u8]) -> Vec<u8> {
    let request = match wire::decode_compile_request(request) {
        Ok(request) => request,
        Err(message) => {
            return wire::encode_response(Status::MalformedRequest as u32, &[], &message);
        }
    };

    match crate::compile_figma_with_bindings_and_policy(
        &request.json,
        request.profile,
        &request.images,
        &request.bindings,
        request.policy,
    ) {
        Ok((bytes, report)) => {
            wire::encode_response(Status::Ok as u32, &bytes, &json::report_json(&report))
        }
        Err(error) => wire::encode_response(
            Status::CompileError as u32,
            &[],
            &json::compile_error_json(&error),
        ),
    }
}

fn image_refs_response(request: &[u8]) -> Vec<u8> {
    let json_text = match std::str::from_utf8(request) {
        Ok(json_text) => json_text,
        Err(e) => {
            return wire::encode_response(
                Status::MalformedRequest as u32,
                &[],
                &format!("the file JSON is not UTF-8: {e}"),
            );
        }
    };

    // The same depth-guarded parse `compile_figma` runs — a file too deep to
    // compile must be refused here the same way, not trap the module.
    let file = match figma::parse_file(json_text) {
        Ok(file) => file,
        Err(error) => {
            return wire::encode_response(
                Status::CompileError as u32,
                &[],
                &json::compile_error_json(&error),
            );
        }
    };

    match figma::image_refs(&file) {
        Ok(refs) => wire::encode_response(Status::Ok as u32, &[], &json::image_refs_json(&refs)),
        Err(error) => wire::encode_response(
            Status::CompileError as u32,
            &[],
            &json::compile_error_json(&error),
        ),
    }
}
