//! Fetching byte ranges of a file over HTTP.
//!
//! One request, one range. What makes this a module rather than a call is that
//! a server is allowed to ignore `Range` and send the whole file — several
//! static servers do, `python3 -m http.server` among them — and a host that
//! assumed otherwise would read the first 64 bytes of the file as the whole of
//! its answer and treat everything after as absent.
//!
//! [`content_range_total`] is deliberately outside the browser half, so it
//! compiles and is tested on the host platform. It is the only parsing here,
//! and parsing is where this module can be wrong without a browser noticing.

/// The browser half, which exists only on the target that has a browser.
#[cfg(target_arch = "wasm32")]
mod browser {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Headers, Request, RequestInit, Response};

    use super::content_range_total;
    use crate::HostError;

    /// What one range request came back with.
    pub(crate) struct Fetched {
        /// The bytes the server returned — the range asked for, or the whole
        /// file if it ignored the range.
        pub bytes: Vec<u8>,
        /// The file's total length.
        pub total: u64,
        /// Whether the server honoured the range.
        ///
        /// `false` means the whole file arrived in one response. The host
        /// carries on rather than refusing: a static server without range
        /// support is the ordinary case, and one wasted transfer is a better
        /// answer than a blank canvas. It is reported, because the difference
        /// is the whole point of the loading path.
        pub ranged: bool,
    }

    /// Requests `start..=end` of `url`.
    pub(crate) async fn range(url: &str, start: u64, end: u64) -> Result<Fetched, HostError> {
        let headers = Headers::new().map_err(js)?;
        headers
            .append("Range", &format!("bytes={start}-{end}"))
            .map_err(js)?;

        let init = RequestInit::new();
        init.set_method("GET");
        init.set_headers_headers(&headers);

        let request = Request::new_with_str_and_init(url, &init).map_err(js)?;
        let window = web_sys::window().ok_or(HostError::NoWindow)?;
        let response: Response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(js)?
            .dyn_into()
            .map_err(|_| HostError::NotAResponse)?;

        if !response.ok() {
            return Err(HostError::Http {
                url: url.to_owned(),
                status: response.status(),
            });
        }

        // 206 is the only status meaning "this is the range you asked for". A
        // 200 carrying a `Content-Range` would still be the whole file, so the
        // status decides and the header is read only once the status says it
        // can be trusted.
        let ranged = response.status() == 206;
        let stated = response
            .headers()
            .get("Content-Range")
            .ok()
            .flatten()
            .as_deref()
            .and_then(content_range_total);

        let buffer = JsFuture::from(response.array_buffer().map_err(js)?)
            .await
            .map_err(js)?;
        let bytes = js_sys::Uint8Array::new(&buffer).to_vec();

        // When the range was ignored the body *is* the file, so its length is
        // the total and no header is needed. When it was honoured the body is a
        // slice, and only the header can say how long the file is — without it
        // there is no number to bound the envelope with, and inventing one
        // would defeat the check it exists for.
        let total = match (ranged, stated) {
            (false, _) => bytes.len() as u64,
            (true, Some(total)) => total,
            (true, None) => return Err(HostError::NoTotal(url.to_owned())),
        };

        Ok(Fetched {
            bytes,
            total,
            ranged,
        })
    }

    /// Carries a browser-side failure across as a message.
    ///
    /// A `JsValue` is not an `Error` and does not outlive the page, so what is
    /// kept is its rendering. This host reports to a console; nothing matches
    /// on these.
    fn js(value: JsValue) -> HostError {
        HostError::Js(
            value
                .as_string()
                .unwrap_or_else(|| String::from(js_sys::Object::from(value).to_string())),
        )
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use browser::range;

/// The total length stated by a `Content-Range` value — the `4189` in
/// `bytes 0-63/4189`.
///
/// [`None`] for `*`, which is a server saying it does not know, and for
/// anything that is not a byte range at all.
fn content_range_total(value: &str) -> Option<u64> {
    let rest = value.trim().strip_prefix("bytes")?;
    // The unit and the range are separated by space in the grammar. Without
    // this, a unit merely *starting* with "bytes" would be read as one.
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let (_covered, total) = rest.trim_start().split_once('/')?;
    total.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::content_range_total;

    /// The ordinary answer to a range request: the response says which bytes
    /// these are and how many there are altogether. The total is what
    /// `dashbuf::prefix::Envelope::read` needs, and the only reason this header
    /// is read at all.
    #[test]
    fn a_content_range_states_the_total() {
        assert_eq!(content_range_total("bytes 0-63/4189"), Some(4189));
    }

    /// A server may know the range and not the total. There is nothing to
    /// recover from that, and guessing would put a wrong length into the
    /// envelope's own bounds check.
    #[test]
    fn an_unknown_total_is_not_a_total() {
        assert_eq!(content_range_total("bytes 0-63/*"), None);
    }

    /// The form a server sends when it cannot satisfy the range at all. The
    /// total is still authoritative, and it is exactly what tells a host the
    /// file is shorter than the header it asked for.
    #[test]
    fn an_unsatisfied_range_still_states_the_total() {
        assert_eq!(content_range_total("bytes */4189"), Some(4189));
    }

    /// Header values arrive as the server wrote them, and the grammar allows
    /// space a strict split would trip on.
    #[test]
    fn surrounding_space_is_tolerated() {
        assert_eq!(content_range_total("  bytes 0-63/4189  "), Some(4189));
    }

    /// Anything that is not a byte range is not one. Notably `none`, which is
    /// what `Accept-Ranges` carries and is easy to read from the wrong header.
    #[test]
    fn a_value_that_is_not_a_byte_range_is_refused() {
        assert_eq!(content_range_total(""), None);
        assert_eq!(content_range_total("none"), None);
        assert_eq!(content_range_total("items 0-63/4189"), None);
        assert_eq!(content_range_total("bytes 0-63"), None);
        assert_eq!(content_range_total("bytes 0-63/abc"), None);
        assert_eq!(
            content_range_total("bytesish 0-63/4189"),
            None,
            "a unit that merely starts with the right letters is not that unit"
        );
    }

    /// A total that does not fit a `u64` is not a length any host can use, and
    /// saturating it would hand the envelope reader a number the file cannot
    /// have.
    #[test]
    fn a_total_too_large_for_a_u64_is_refused() {
        assert_eq!(
            content_range_total("bytes 0-63/99999999999999999999999999"),
            None
        );
    }
}
