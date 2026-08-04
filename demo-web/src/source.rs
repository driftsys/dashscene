//! What the page asked the host to draw.
//!
//! The native host takes this off the command line (`demo/src/scenes.rs`); a
//! page has no command line, so it comes off the query string instead. The
//! vocabulary is deliberately the same one, so a scene name that works in one
//! host works in the other.
//!
//! ```text
//! /                        the default showcase scene
//! /?scene=typography       a named showcase scene
//! /?dsb=/goldens/dsb/v03-paint.dsb   a compiled document, fetched by range
//! ```

/// Where this run's content comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A scene authored through the producer API, from `corpus/showcase`.
    Showcase(String),
    /// A compiled `.dsb`, fetched from this URL and replayed through the same
    /// producer API by the loader.
    Document(String),
}

/// Resolves a query string — with or without its leading `?` — to a source.
///
/// `dsb` wins over `scene` when both are present, rather than being refused: a
/// page that names a document is asking for the loading path, which is the one
/// thing a showcase scene cannot exercise.
///
/// An unknown scene name is **not** resolved here. This returns what was asked
/// for; whether a scene by that name exists is `showcase::by_name`'s answer,
/// and reporting it is the caller's, which keeps this function free of the
/// scene registry.
pub fn select(query: &str) -> Source {
    let mut scene = None;
    for (key, value) in pairs(query) {
        match key.as_str() {
            "dsb" => return Source::Document(value),
            "scene" if scene.is_none() => scene = Some(value),
            _ => {}
        }
    }
    Source::Showcase(scene.unwrap_or_else(|| showcase::DEFAULT.to_owned()))
}

/// The `key=value` pairs in a query string, percent-decoded.
fn pairs(query: &str) -> impl Iterator<Item = (String, String)> + '_ {
    query
        .trim_start_matches('?')
        .split('&')
        .map(|part| match part.split_once('=') {
            Some((key, value)) => (decode(key), decode(value)),
            None => (decode(part), String::new()),
        })
}

/// Percent-decoding.
///
/// Written out rather than taken from a crate: a query string with one value in
/// it is the whole of this host's parsing, and the alternative is a dependency
/// for a dozen lines. A byte that is not valid UTF-8 after decoding is replaced
/// rather than refused — this names a scene or a URL, and a mangled name simply
/// fails to resolve, which is already reported.
///
/// `+` is **not** read as a space. That rule belongs to form encoding, not to
/// URLs, and neither a scene name nor a path here contains a space — so
/// applying it could only corrupt a URL that legitimately carries a `+`.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                // Read the two digits out of the byte array rather than by
                // slicing `text`. A `&str` slice must land on a character
                // boundary, and `index + 3` need not: a `%` followed by a
                // multi-byte character puts the end of the slice inside that
                // character, which panics — and a panic in wasm is a page that
                // stops.
                match hex_pair(bytes[index + 1], bytes[index + 2]) {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    // Not a hex pair: a literal `%`, which is what a browser
                    // sends for one that was never an escape.
                    None => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The byte two hex digits denote, or [`None`] if either is not a hex digit.
fn hex_pair(high: u8, low: u8) -> Option<u8> {
    Some(hex(high)? << 4 | hex(low)?)
}

fn hex(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Source, select};

    /// An empty query is the ordinary case — someone opened the page.
    #[test]
    fn no_query_selects_the_default_showcase_scene() {
        assert_eq!(
            select(""),
            Source::Showcase(showcase::DEFAULT.to_owned()),
            "an empty query"
        );
        assert_eq!(select("?"), Source::Showcase(showcase::DEFAULT.to_owned()));
    }

    /// The default has to resolve against the registry, or the page opens on a
    /// canvas with nothing in it. The same assertion `demo/src/scenes.rs`
    /// makes, for the same reason.
    #[test]
    fn the_default_scene_is_one_of_the_scenes() {
        assert!(showcase::by_name(showcase::DEFAULT).is_some());
    }

    #[test]
    fn a_scene_parameter_names_a_showcase_scene() {
        assert_eq!(
            select("?scene=typography"),
            Source::Showcase("typography".to_owned())
        );
        assert_eq!(
            select("scene=typography"),
            Source::Showcase("typography".to_owned()),
            "the leading ? is optional, because `Location::search` includes it \
             and a hand-written string usually does not"
        );
    }

    #[test]
    fn a_dsb_parameter_names_a_document() {
        assert_eq!(
            select("?dsb=/goldens/dsb/v03-paint.dsb"),
            Source::Document("/goldens/dsb/v03-paint.dsb".to_owned())
        );
    }

    /// A document is the loading path, which is the one thing a showcase scene
    /// cannot exercise, so it wins rather than being ambiguous.
    #[test]
    fn a_document_wins_over_a_scene() {
        assert_eq!(
            select("?scene=typography&dsb=/a.dsb"),
            Source::Document("/a.dsb".to_owned())
        );
        assert_eq!(
            select("?dsb=/a.dsb&scene=typography"),
            Source::Document("/a.dsb".to_owned()),
            "and in either order — a rule that depended on position would be \
             decided by however the page happened to write the URL"
        );
    }

    /// A parameter this host does not know is ignored rather than refused. A
    /// page may carry a cache-buster or an analytics tag, and neither is a
    /// reason not to draw.
    #[test]
    fn an_unknown_parameter_is_ignored() {
        assert_eq!(
            select("?t=1738000000&scene=layout"),
            Source::Showcase("layout".to_owned())
        );
    }

    /// The first `scene` wins, so a repeated parameter has one answer rather
    /// than depending on which one the loop happened to see last.
    #[test]
    fn a_repeated_scene_parameter_takes_the_first() {
        assert_eq!(
            select("?scene=layout&scene=typography"),
            Source::Showcase("layout".to_owned())
        );
    }

    /// A `.dsb` URL is the value most likely to carry an escape, since a path
    /// can hold one.
    ///
    /// Both letter cases, because the hex is case-insensitive and clients emit
    /// both: with only the uppercase form here, deleting the lowercase arm of
    /// the digit table passed every test.
    #[test]
    fn a_percent_escape_is_decoded_in_either_case() {
        assert_eq!(
            select("?dsb=%2Fgoldens%2Fdsb%2Fv03-paint.dsb"),
            Source::Document("/goldens/dsb/v03-paint.dsb".to_owned())
        );
        assert_eq!(
            select("?dsb=%2fgoldens%2fdsb%2fv03-paint.dsb"),
            Source::Document("/goldens/dsb/v03-paint.dsb".to_owned())
        );
        assert_eq!(
            select("?scene=%6Cayout"),
            Source::Showcase("layout".to_owned()),
            "and a digit from each half of the table"
        );
    }

    /// A bare `%` is not an escape. Decoding it as one would consume the two
    /// characters after it, which silently corrupts the rest of the value.
    #[test]
    fn a_bare_percent_is_left_alone() {
        assert_eq!(
            select("?scene=100%"),
            Source::Showcase("100%".to_owned()),
            "a trailing % has no pair to read"
        );
        assert_eq!(
            select("?scene=a%zz"),
            Source::Showcase("a%zz".to_owned()),
            "and a pair that is not hex is not an escape either"
        );
    }

    /// A `%` followed by a multi-byte character.
    ///
    /// The decoder counts in bytes, and a `&str` slice must land on a character
    /// boundary. Taking the two digits by slicing `text` puts the end of the
    /// slice inside the character and panics — reproduced on `100%€` before
    /// this was read out of the byte array instead. Every other case here is
    /// ASCII, so none of them could reach it.
    #[test]
    fn a_percent_before_a_multibyte_character_does_not_panic() {
        assert_eq!(
            select("?scene=100%€ off"),
            Source::Showcase("100%€ off".to_owned())
        );
        assert_eq!(select("?dsb=%€"), Source::Document("%€".to_owned()));
    }

    /// A `%` with exactly one character after it — the boundary where reading
    /// a two-character pair runs off the end of the string.
    ///
    /// Worth its own case because the failure is not a wrong answer: slicing
    /// past the end panics, and a panic in wasm is a page that stops. A
    /// mutation widening the bound to `<=` survived every other test here.
    #[test]
    fn a_percent_with_one_character_after_it_does_not_run_off_the_end() {
        assert_eq!(select("?scene=a%z"), Source::Showcase("a%z".to_owned()));
        assert_eq!(select("?scene=%4"), Source::Showcase("%4".to_owned()));
    }
}
