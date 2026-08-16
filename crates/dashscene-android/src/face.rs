//! The `DsFace` field names and types, and the gate that keeps them the ones
//! Java declares (issue #1089).
//!
//! # Why this is a module of its own, on no Android API
//!
//! Story #981 replaced five parallel JNI arrays with one `DsFace[]`. That
//! removed a coupling nothing checked — the five lengths having to agree — and
//! added another: **six field names, as strings**, read by `host::read_face`.
//!
//! Rename one in `DsFace.java` and every gate stayed green. `just build` cannot
//! see it, because `host.rs` is behind `#[cfg(target_os = "android")]` and no
//! test binary links it. `just android-lint` does lint that triple since issue
//! #1086, but a lint reads one side and cannot compare it with Java. `just
//! android` and `just android-apk` compile and package the two halves without
//! comparing them. The failure
//! arrived at the first `surfaceChanged` as a `NoSuchFieldError`, the handle
//! came back 0, and the harness fell back to the no-text call and drew no
//! glyphs.
//!
//! **The C header has `just c-abi` for exactly this** — it compiles the
//! committed header from C and asserts the two halves agree. This is the JNI
//! half's equivalent, and it is deliberately the cheaper of the two shapes the
//! issue named: a JVM-free comparison against `DsFace.java`'s own source rather
//! than a device smoke test, because the second needs hardware and this needs
//! `just test`.
//!
//! By this crate's own rule — a decision that binds no NDK symbol belongs where
//! a host test can reach it — the list lives here rather than in `host.rs`, and
//! the test below runs in the sanity tier on every platform.
//!
//! # The type is checked too, not only the name
//!
//! `GetFieldID` resolves a field by **name and descriptor**, so changing
//! `public final int weight` to `long` fails exactly as a rename does — the
//! same `NoSuchFieldError`, the same zero handle, the same blank frame. A gate
//! that compared names alone would pass that change while claiming to close
//! this issue, so each name here carries the JNI descriptor `read_face` looks
//! it up with.
//!
//! # What the gate asks, and what it deliberately does not
//!
//! It asks, of each field this crate reads: **does `DsFace.java` still declare
//! it, with that type?** That is the whole of the failure issue #1089
//! describes, and answering it needs no Java grammar — only the file's text.
//!
//! It does **not** ask what else the file declares, so a seventh field added to
//! `DsFace` does not fail it. Such a field is one the native half ignores —
//! `read_face` reads these six and builds a `DsFontFace` from them — so nothing
//! reads it and nothing fails at run time. The reverse question needs a Java
//! parser, and two rounds of review found ten shapes that escaped one; the test
//! below records them, and why that approach was abandoned.
//!
//! # Why `&CStr`
//!
//! `jni` is an Android-only dependency of this crate, so a list spelled in
//! `JNIStr` could not be reached by the test below at all. `CStr` is `core`'s,
//! and JNI wants NUL-terminated modified UTF-8 — which every name here, being
//! ASCII, already is. `host.rs` converts each one in a `const`, so a name that
//! could not cross would be a compile error rather than a run-time panic inside
//! a JNI entry point.

// Off Android nothing but the test below reads these: `host` is not compiled,
// so the six constants have no other caller. That is the arrangement working
// rather than a problem, and the allowance is narrowed to the target where it
// is true so a genuinely unused item on Android is still reported — the same
// shape, for the same reason, that `crate::machine` carries.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use std::ffi::CStr;

/// The family name. `DsFontFace::family` on the C side.
pub(crate) const FAMILY: &CStr = c"family";

/// The CSS weight. The `1..=1000` range is judged by the ABI; `read_face`
/// refuses only what a `u16` cannot carry to it at all.
pub(crate) const WEIGHT: &CStr = c"weight";

/// The face's index inside a font collection — the field the descriptor class
/// exists for, and the one five parallel arrays could not carry.
pub(crate) const FACE_INDEX: &CStr = c"faceIndex";

/// The font file's bytes.
pub(crate) const FONT: &CStr = c"font";

/// The committed MSDF sheet, or an empty array for none.
pub(crate) const ATLAS_PNG: &CStr = c"atlasPng";

/// The sheet's metrics blob, under the same rule as [`ATLAS_PNG`].
pub(crate) const ATLAS_METRICS: &CStr = c"atlasMetrics";

/// Every field the JNI half reads, with the descriptor it reads each one with.
///
/// The names are the six constants above rather than written again, so there is
/// one spelling of each in this crate. The descriptors are written here and
/// **are not mechanically tied to the `jni_sig!` literals in `host.rs`** — that
/// macro takes a literal, so the two sit adjacent at each call site and agree
/// by review, and issue #1096 carries what a gate for it would look like. What
/// this pins is the pair against `DsFace.java`, which is where the drift issue
/// #1089 describes actually happens.
///
/// `#[cfg(test)]` because the test below is its only reader: `host.rs` uses the
/// six constants directly. Without it this is dead code on Android, where this
/// module deliberately does not allow that. Private for the same reason — the
/// only reader is the child module below, and publishing a test-only table to
/// the crate invites a second consumer that would then have to be kept in step
/// with the gate.
#[cfg(test)]
const FACE_FIELDS: [(&CStr, &str); 6] = [
    (FAMILY, "Ljava/lang/String;"),
    (WEIGHT, "I"),
    (FACE_INDEX, "I"),
    (FONT, "[B"),
    (ATLAS_PNG, "[B"),
    (ATLAS_METRICS, "[B"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// `DsFace.java`, read at compile time.
    ///
    /// `include_str!` rather than a path opened at run time: it makes **this
    /// test's** compile depend on the file, so moving or deleting it is a
    /// compile error rather than a test that quietly stops checking anything.
    /// `just test` and `just build` both compile tests, so both see it; a plain
    /// `cargo build` does not, and this does not claim otherwise. It also needs
    /// no working directory, which `cargo test` and `cargo nextest` do not
    /// agree about.
    const DS_FACE_JAVA: &str = include_str!("../harness/java/dev/driftsys/dashscene/DsFace.java");

    /// The Java type a JNI field descriptor is written as.
    ///
    /// Only the three this descriptor uses. An unknown one panics rather than
    /// guessing: it means [`FACE_FIELDS`] gained a descriptor this check cannot
    /// look for, and silently looking for the wrong text would be a gate that
    /// passes while checking nothing.
    fn java_type(descriptor: &str) -> &'static str {
        match descriptor {
            "Ljava/lang/String;" => "String",
            "I" => "int",
            "[B" => "byte[]",
            other => panic!("no Java spelling known for the descriptor {other}"),
        }
    }

    /// `source` with comments removed and whitespace collapsed.
    ///
    /// **Deliberately approximate, and safe because of which way it errs.**
    /// This is not a Java lexer: it does not know about string literals, so a
    /// `//` inside one truncates that line, and a `/*` written inside a line
    /// comment opens a block. Both make it remove *too much*.
    ///
    /// Removing too much can only make the assertion below fail, because that
    /// assertion asks whether a declaration is *present*. Removing too little
    /// is the direction that could pass wrongly — a field commented out but
    /// still matched — and stripping comments at all is what closes it. So
    /// every way this can be wrong is a loud failure rather than a silent one,
    /// which is the property a drift gate needs and the reason it is not worth
    /// a real lexer.
    fn source_without_comments(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut rest = source;
        while let Some(start) = rest.find("/*") {
            out.push_str(&rest[..start]);
            out.push(' ');
            match rest[start + 2..].find("*/") {
                Some(end) => rest = &rest[start + 2 + end + 2..],
                // An unterminated block comment is not valid Java; the rest of
                // the file is comment either way.
                None => rest = "",
            }
        }
        out.push_str(rest);

        let mut collapsed = String::with_capacity(out.len());
        for line in out.lines() {
            let code = match line.find("//") {
                Some(at) => &line[..at],
                None => line,
            };
            for word in code.split_whitespace() {
                if !collapsed.is_empty() {
                    collapsed.push(' ');
                }
                collapsed.push_str(word);
            }
        }
        collapsed
    }

    /// The exact text `DsFace.java` must contain for one field.
    ///
    /// One function rather than a `format!` at each site: it is the single
    /// thing this gate compares, and the shape of the match is the thing most
    /// likely to change. Four spellings of it would mean four edits, with the
    /// ones in the tests below quietly still checking the old shape.
    fn declaration(name: &CStr, descriptor: &str) -> String {
        let name = name
            .to_str()
            .expect("every DsFace field name is ASCII, so it is UTF-8");
        format!("public final {} {name};", java_type(descriptor))
    }

    /// The first field of [`FACE_FIELDS`] that `source` does not declare.
    fn missing_from(source: &str) -> Option<String> {
        let source = source_without_comments(source);
        FACE_FIELDS
            .iter()
            .map(|(name, descriptor)| declaration(name, descriptor))
            .find(|declaration| !source.contains(declaration.as_str()))
    }

    /// **Every field the JNI half reads is declared by `DsFace`, with the type
    /// it is read with** (issue #1089).
    ///
    /// `GetFieldID` resolves a field by name **and** descriptor, so a rename, a
    /// removal and a widened `int` are one failure: `NoSuchFieldError` at the
    /// first `surfaceChanged`, a zero handle, and a frame with no glyphs. This
    /// asks, for each entry in [`FACE_FIELDS`], whether `DsFace.java` still
    /// declares it — and fails naming the declaration it could not find.
    ///
    /// # What this does not check, and why it does not try
    ///
    /// **A seventh field added to `DsFace.java` does not fail this**, and that
    /// is deliberate rather than a gap left open. Such a field is one the
    /// native half ignores: `read_face` reads these six and builds a
    /// `DsFontFace` from them, so nothing reads a seventh and nothing fails at
    /// run time. It is untidy, not broken.
    ///
    /// Catching it needs the opposite question — "what does this file declare?"
    /// — which needs a Java parser. Two rounds of review found ten shapes that
    /// escaped one: an annotation, a trailing comment, a doubled space, a line
    /// break, a brace initializer, a call in an initializer, a generic type, an
    /// argument-carrying annotation, `final public` in the other order, and two
    /// declarators in one statement. Each round closed the shapes it found and
    /// the next round found more, because the grammar is larger than anything a
    /// gate this size can recognise. Asking whether a known declaration is
    /// *present* needs no grammar at all.
    ///
    /// It also does not read `host.rs`, which is behind the platform `cfg` and
    /// links into no test binary: that `read_face` reads these six and only
    /// these is held by review, as is the agreement between each descriptor
    /// here and the `jni_sig!` literal beside it (issue #1096). The match is
    /// also unanchored, so a declaration that has left the class body still
    /// satisfies it (issue #1097).
    #[test]
    fn every_field_the_jni_half_reads_is_declared_by_dsface() {
        if let Some(declaration) = missing_from(DS_FACE_JAVA) {
            panic!(
                "DsFace.java does not declare `{declaration}`, which is what \
                 this crate reads that field as.\n\n\
                 Either the field was renamed, removed or retyped — which \
                 compiles and packages on both sides and fails at the first \
                 surfaceChanged as NoSuchFieldError, with the handle coming \
                 back 0 and no glyph drawn — or it is still correct and simply \
                 written in a shape this literal match does not cover (an \
                 initializer, `final public` in the other order, a space before \
                 the `;`, or `byte font[]`). Check which before changing \
                 anything."
            );
        }
    }

    /// A rename, a removal and a type change are each caught, **and caught for
    /// the field the case mutates**.
    ///
    /// Driven against a mutated copy of the real file rather than a fixture, so
    /// what is exercised is the source the gate actually reads. Asserting the
    /// specific declaration rather than "something is missing": a `java_type`
    /// that returned the wrong Java spelling for one descriptor would report
    /// *that* field missing under every mutation, and a test asking only
    /// whether any field is missing would call all three cases caught while the
    /// gate was blind to each of them.
    #[test]
    fn a_renamed_removed_or_retyped_field_is_not_found() {
        let cases = [
            (
                "public final byte[] atlasPng;",
                "public final byte[] atlasPNG;",
                declaration(ATLAS_PNG, "[B"),
            ),
            (
                "public final byte[] atlasPng;",
                "",
                declaration(ATLAS_PNG, "[B"),
            ),
            (
                "public final int weight;",
                "public final long weight;",
                declaration(WEIGHT, "I"),
            ),
        ];
        for (from, to, expected) in cases {
            let mutated = DS_FACE_JAVA.replace(from, to);
            assert_ne!(mutated, DS_FACE_JAVA, "the mutation {from:?} did not apply");
            assert_eq!(
                missing_from(&mutated).as_deref(),
                Some(expected.as_str()),
                "the gate did not report {expected:?} missing after {from:?} \
                 became {to:?}"
            );
        }
    }

    /// A field that is commented out is not found, which is the one direction
    /// the comment stripping has to get right.
    ///
    /// Over-stripping can only make the assertion above fail. Under-stripping
    /// is what would pass wrongly, and a declaration left in a comment is
    /// exactly that case.
    #[test]
    fn a_commented_out_declaration_does_not_count_as_a_declaration() {
        let expected = declaration(WEIGHT, "I");
        for commented in [
            "// public final int weight;",
            "/* public final int weight; */",
        ] {
            let mutated = DS_FACE_JAVA.replace(&expected, commented);
            assert_ne!(mutated, DS_FACE_JAVA, "the mutation did not apply");
            assert_eq!(
                missing_from(&mutated).as_deref(),
                Some(expected.as_str()),
                "a declaration inside {commented:?} was read as a declaration"
            );
        }
    }

    /// Whitespace **between tokens** is collapsed, so a declaration split
    /// across lines or written with extra spaces reads the same as a one-line
    /// one.
    ///
    /// Only between them: this is a literal match over collapsed text, so
    /// `weight ;` and `byte [] font` are not found. That is a false alarm
    /// rather than a false pass, and the assertion message says so.
    #[test]
    fn a_declaration_is_found_however_it_is_spaced() {
        let expected = declaration(WEIGHT, "I");
        for spelling in [
            "public  final  int  weight;",
            "public final int\n            weight;",
            "public final\n    int weight;",
        ] {
            let mutated = DS_FACE_JAVA.replace(&expected, spelling);
            assert_ne!(
                mutated, DS_FACE_JAVA,
                "the spelling {spelling:?} did not replace anything, so this \
                 case tested nothing"
            );
            assert_eq!(
                missing_from(&mutated),
                None,
                "this spelling was not found: {spelling:?}"
            );
        }
    }
}
