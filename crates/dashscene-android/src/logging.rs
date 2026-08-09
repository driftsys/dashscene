//! One line to logcat, which is the only diagnostic channel this crate has.
//!
//! `println!` reaches logcat only when the app has asked the runtime to redirect
//! stdout, which an embedder should not have to know about, so this writes to
//! the log directly.

/// The tag every line from this crate carries.
const TAG: &str = "dashscene";

/// Writes one line to logcat.
///
/// Silently does nothing if the message cannot be made into a C string — a
/// diagnostic that panicked while reporting a failure would replace the failure
/// with its own.
pub(crate) fn log(message: &str) {
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
