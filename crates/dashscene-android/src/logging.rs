//! One line to logcat, which is the only diagnostic channel this crate has.
//!
//! `println!` reaches logcat only when the app has asked the runtime to redirect
//! stdout, which an embedder should not have to know about, so this writes to
//! the log directly.
//!
//! **The `cfg` here names the symbol, not the product.** `__android_log_write`
//! is linkable on exactly one target and nowhere else, so this is the boundary
//! rather than a choice about which platform deserves diagnostics.

/// The tag every line from this crate carries.
#[cfg(target_os = "android")]
const TAG: &str = "dashscene";

/// Writes one line to logcat.
///
/// Silently does nothing if the message cannot be made into a C string — a
/// diagnostic that panicked while reporting a failure would replace the failure
/// with its own.
#[cfg(target_os = "android")]
pub fn log(message: &str) {
    let Ok(text) = std::ffi::CString::new(message) else {
        return;
    };
    let Ok(tag) = std::ffi::CString::new(TAG) else {
        return;
    };
    // SAFETY: both pointers are NUL-terminated and live for the call.
    unsafe {
        ndk_sys::__android_log_write(
            ndk_sys::android_LogPriority::ANDROID_LOG_INFO.0 as std::os::raw::c_int,
            tag.as_ptr(),
            text.as_ptr(),
        );
    }
}

/// Discards the line: off Android there is no logcat to write it to.
///
/// This crate has no non-Android host — its build for one exists so that the
/// parts deciding nothing about an NDK symbol can be tested (issue #888), and
/// the frame loop's state machine is one of them. Discarding rather than
/// writing to stderr
/// keeps a test run's output clean, and having the function at all is what keeps
/// every call site one shape instead of a `cfg` per diagnostic.
#[cfg(not(target_os = "android"))]
pub fn log(_message: &str) {}
