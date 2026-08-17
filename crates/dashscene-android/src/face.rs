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
//! It asks, of each field this crate reads: **does `DsFace` itself still
//! declare it, with that type?** That is the whole of the failure issue #1089
//! describes, and answering it needs no Java grammar — only the file's text,
//! with the four constructs that can hold text which is not code removed and
//! the search anchored to that one class's own body (issue #1097). A
//! declaration that has moved into a nested class, or into a second top-level
//! class, or that survives only inside a string literal, is not one
//! `GetFieldID` can resolve on a `DsFace` instance, and is no longer one this
//! gate accepts.
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

/// One `DsFace` field: the name `GetFieldID` looks it up by, and the JNI
/// descriptor it is looked up **with**.
///
/// The two travel together because `GetFieldID` resolves a field by both, so
/// they fail together: a `weight` widened from `int` to `long` throws
/// `NoSuchFieldError` exactly as a rename does. Splitting them let the name be
/// pinned to `host.rs` while the descriptor was not, which is issue #1096.
pub(crate) struct Field {
    /// NUL-terminated because that is what JNI takes; `host::jni_name`
    /// converts it in a `const`.
    pub(crate) name: &'static CStr,
    /// As written in a JNI signature — `I`, `[B`, `Ljava/lang/String;`.
    pub(crate) descriptor: &'static str,
}

/// Whether two JNI descriptors are the same text, in a `const` context.
///
/// `str::eq` is not `const`, so the comparison is spelled out. Its only caller
/// is the `const` assertion inside `host::face_field!`, which is what makes a
/// descriptor that disagrees with the `jni_sig!` literal beside it a compile
/// error rather than a `NoSuchFieldError` at the first `surfaceChanged`
/// (issue #1096).
///
/// **A second copy of `dashpack::ktx2::str_eq`**, which exists for the same
/// shape of assertion — a literal welded to a constant. Not shared: that one is
/// private to another crate this one does not depend on, and taking a
/// dependency to reach ten lines would cost more than the duplication. Issue
/// #1178 settled that as the ruling and
/// `docs/decisions/crate-name-map.md` records it.
///
/// **Both copies are checked now**, which they were not when this comment was
/// written: this one by `same_descriptor_compares_the_whole_string`, without
/// which a broken comparison would disable the whole #1096 gate in silence, and
/// that one by `const` assertions beside its own pin. The mechanisms differ for
/// a reason — this crate's pin is behind `#[cfg(target_os = "android")]`, so no
/// host build evaluates it and a runtime test is the only thing that can, where
/// `dashpack`'s is unconditional and can fail the build itself.
pub(crate) const fn same_descriptor(one: &str, other: &str) -> bool {
    let (one, other) = (one.as_bytes(), other.as_bytes());
    if one.len() != other.len() {
        return false;
    }
    let mut at = 0;
    while at < one.len() {
        if one[at] != other[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// The family name. `DsFontFace::family` on the C side.
pub(crate) const FAMILY: Field = Field {
    name: c"family",
    descriptor: "Ljava/lang/String;",
};

/// The CSS weight. The `1..=1000` range is judged by the ABI; `read_face`
/// refuses only what a `u16` cannot carry to it at all.
pub(crate) const WEIGHT: Field = Field {
    name: c"weight",
    descriptor: "I",
};

/// The face's index inside a font collection — the field the descriptor class
/// exists for, and the one five parallel arrays could not carry.
pub(crate) const FACE_INDEX: Field = Field {
    name: c"faceIndex",
    descriptor: "I",
};

/// The font file's bytes.
pub(crate) const FONT: Field = Field {
    name: c"font",
    descriptor: "[B",
};

/// The committed MSDF sheet, or an empty array for none.
pub(crate) const ATLAS_PNG: Field = Field {
    name: c"atlasPng",
    descriptor: "[B",
};

/// The sheet's metrics blob, under the same rule as [`ATLAS_PNG`].
pub(crate) const ATLAS_METRICS: Field = Field {
    name: c"atlasMetrics",
    descriptor: "[B",
};

/// Every field the JNI half reads.
///
/// The six constants above rather than written again, so each name and each
/// descriptor has one spelling in this crate. Since issue #1096 the descriptors
/// are **also** tied to the `jni_sig!` literals in `host.rs`: that macro takes
/// a literal and cannot read a constant, so `host::face_field!` writes the
/// literal once and asserts it against [`Field::descriptor`] in a `const`
/// block. What *this* table pins is the pair against `DsFace.java`, which is
/// where the drift issue #1089 describes actually happens.
///
/// `#[cfg(test)]` because the test below is its only reader: `host.rs` uses the
/// six constants directly. Without it this is dead code on Android, where this
/// module deliberately does not allow that. Private for the same reason — the
/// only reader is the child module below, and publishing a test-only table to
/// the crate invites a second consumer that would then have to be kept in step
/// with the gate.
#[cfg(test)]
const FACE_FIELDS: [&Field; 6] = [
    &FAMILY,
    &WEIGHT,
    &FACE_INDEX,
    &FONT,
    &ATLAS_PNG,
    &ATLAS_METRICS,
];

/// `source` with everything that is not code removed and whitespace
/// collapsed.
///
/// Four constructs, recognised in one pass because each can hold text that
/// reads as a declaration or as a brace and is neither: a block comment, a
/// line comment, a string literal and a character literal. The literals are
/// what issue #1097's second case turns on — a field holding the text
/// `"public final int weight;"` satisfied a gate that stripped only
/// comments — and the character literal matters to
/// [`class_body`] rather than here, since a `'{'`
/// would miscount the depth.
///
/// **Deliberately approximate, and safe because of which way it errs.**
/// One pass rather than three is also what makes it *less* wrong than
/// before: a `/*` inside a string no longer opens a comment, and a `//`
/// inside one no longer truncates the line. What remains approximate is
/// unterminated constructs, which swallow the rest of the file.
///
/// Removing too much can only make its consumers fail: `face` asks whether a
/// declaration is *present*, and `entry` compares the set of `native`
/// declarations against a list. Removing too little is the direction that
/// could pass wrongly. So every way this can be wrong is a loud failure
/// rather than a silent one, which is the property a drift gate needs and
/// the reason it is not worth a real lexer.
///
/// **Java only.** `entry` feeds it `DashsceneNative.java` and reads Rust
/// source raw, because `'` opens a character literal here and is almost
/// always a lifetime in Rust — running this over `host.rs` cut it from
/// 38 058 bytes to 7 959 and removed whole parameter lists, so which
/// symbols survived depended on how many apostrophes the file happened to
/// carry.
#[cfg(test)]
pub(crate) fn code_only(source: &str) -> String {
    // **Bytes, not chars** (issue #1169). Every construct recognised here
    // is ASCII — `/`, `*`, `"`, `'`, `\` — and a UTF-8 continuation byte is
    // always >= 0x80, so it can never be mistaken for one of them. The
    // two-byte escape skip can land mid-character, which is harmless: it is
    // inside a literal, where nothing is copied out. Collecting the file as
    // `Vec<char>` to look one byte ahead cost four bytes per character
    // before any work began.
    let bytes = source.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let this = bytes[at];
        let next = bytes.get(at + 1).copied();
        if this == b'/' && next == Some(b'*') {
            // A space in place of a comment, which can legitimately sit
            // between the tokens of a declaration.
            out.push(b' ');
            at += 2;
            while at < bytes.len() && !(bytes[at] == b'*' && bytes.get(at + 1) == Some(&b'/')) {
                at += 1;
            }
            at += 2;
        } else if this == b'/' && next == Some(b'/') {
            out.push(b' ');
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
        } else if this == b'"' || this == b'\'' {
            // **A quote pair, not a space**, for the reason `class_body`
            // uses `{}`: a literal never sits between the tokens of a
            // declaration, so what is on either side of one was never
            // adjacent, and a space joins them. `public final int"x"weight;`
            // declares nothing and collapsed to exactly the text this gate
            // looks for. A quote pair cannot appear inside a declaration, so
            // it cannot complete one.
            //
            // Not `{}` here: that would feed a brace to `class_body`'s
            // depth counter and move the class body's boundaries.
            out.extend_from_slice(b"\"\"");
            at += 1;
            while at < bytes.len() && bytes[at] != this {
                // A backslash escapes the next byte, the closing quote
                // included.
                at += if bytes[at] == b'\\' { 2 } else { 1 };
            }
            at += 1;
        } else {
            out.push(this);
            at += 1;
        }
    }
    // Every byte copied out came from outside a comment or a literal, so
    // the boundaries of any multi-byte character are intact.
    let out = String::from_utf8(out).expect("only whole characters are copied out");

    let mut collapsed = String::with_capacity(out.len());
    for word in out.split_whitespace() {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }
        collapsed.push_str(word);
    }
    collapsed
}

/// The text **directly inside `class`'s own body**, with everything nested
/// inside a further brace dropped (issue #1097).
///
/// `code` must have been through [`code_only`] first: this counts braces, and
/// a `'{'` in a character literal or a `{` in a comment would move the body's
/// boundaries.
///
/// **Depth, not brace matching.** A nested class is *between* `class Foo {` and
/// its closing brace, so matching that pair is not enough — everything below
/// depth 1 has to go. Dropping the nested text also drops a constructor body,
/// which is exactly right: `this.weight = weight;` declares nothing.
///
/// `None` when the class header is not found, when what follows it is not a
/// class body, or when the body never closes. Each caller decides what that
/// means for it.
///
/// **Two consumers, one copy.** `face` asks it for `DsFace`, because
/// `GetFieldID` is asked of a `DsFace` instance; `entry` asks it for
/// `DashsceneNative` and `DemoNative`, because a `native` method moved into a
/// nested class is mangled `…_00024Inner_…` and throws at the first call. The
/// hole is the same hole and it is closed once.
#[cfg(test)]
pub(crate) fn class_body(code: &str, class: &str) -> Option<String> {
    let header = format!("class {class}");
    let mut from = 0;
    let open = loop {
        let at = from + code[from..].find(&header)?;
        let after = code[at + header.len()..].chars().next();
        // `class DsFaceInner` is a different class. Nothing needs checking
        // on the left: the `class` keyword is part of the needle.
        if after.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            from = at + header.len();
            continue;
        }
        // The brace need not be adjacent — `class DsFace implements Foo {`
        // — but only an `extends`/`implements` clause may sit between, and
        // that carries neither `;` nor `}`. Without this check a class whose
        // own opening brace is missing matches the **constructor's**
        // brace several declarations later and reports that body as the
        // class's, which reads as `family` having gone missing.
        let rest = &code[at + header.len()..];
        let brace = rest.find('{')?;
        if rest[..brace].contains(';') || rest[..brace].contains('}') {
            return None;
        }
        break at + header.len() + brace;
    };

    let mut depth = 0_u32;
    let mut body = String::new();
    for character in code[open..].chars() {
        match character {
            '{' => {
                depth += 1;
                if depth == 2 {
                    // **Braces, not a space**, and the difference is a false
                    // pass. `code_only` elides something that sits *where a
                    // token could*, so a space there keeps two real tokens
                    // apart. Here a whole nested construct is elided and the
                    // text either side of it was never adjacent, so a space
                    // *joins* them: `public final int{ }weight;` collapsed
                    // to `public final int weight;` and the gate reported a
                    // field that nothing declares as declared. A brace pair
                    // cannot appear inside a declaration, so it cannot
                    // complete one.
                    body.push_str("{}");
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(body);
                }
            }
            _ if depth == 1 => body.push(character),
            _ => {}
        }
    }
    // Unbalanced braces are not valid Java. Reported as no body at all
    // rather than as a truncated one, so nothing is searched that was not
    // read to its end.
    None
}

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

    /// The text **directly inside `DsFace`'s own class body** (issue #1097).
    ///
    /// `GetFieldID` is asked of a `DsFace` instance, so only what that class
    /// declares can answer it. The gate matched anywhere in the file, which two
    /// measured cases satisfied while the field was not where JNI looks: one
    /// moved into a nested `public static final class Inner`, and one into a
    /// second top-level class.
    ///
    /// `None` for the reasons [`class_body`] gives. [`gate`] reports that as
    /// [`Gate::NoBody`] rather than folding it into a missing field: an empty
    /// body reports the *first* entry of [`FACE_FIELDS`] missing, which is
    /// exactly what a genuine `family` rename reports, and points the reader at
    /// the wrong repair.
    fn ds_face_body(code: &str) -> Option<String> {
        class_body(code, "DsFace")
    }

    /// The exact text `DsFace.java` must contain for one field.
    ///
    /// One function rather than a `format!` at each site: it is the single
    /// thing this gate compares, and the shape of the match is the thing most
    /// likely to change. Four spellings of it would mean four edits, with the
    /// ones in the tests below quietly still checking the old shape.
    fn declaration(field: &Field) -> String {
        let name = field
            .name
            .to_str()
            .expect("every DsFace field name is ASCII, so it is UTF-8");
        format!("public final {} {name};", java_type(field.descriptor))
    }

    /// What the gate found when it read `source`.
    ///
    /// Three outcomes rather than an `Option`, because "no `DsFace` body could
    /// be read" and "the body does not declare `family`" are different repairs
    /// and were previously the same value: `unwrap_or_default()` turned a
    /// renamed class into an empty body, which reports the *first* field of
    /// [`FACE_FIELDS`] missing — byte for byte what a genuine `family` rename
    /// reports.
    #[derive(Debug)]
    enum Gate {
        /// The body was read and declares every field.
        Declared,
        /// The body was read; this declaration is not in it.
        Missing(String),
        /// No `class DsFace { … }` body could be read at all.
        NoBody,
    }

    impl Gate {
        /// The declaration reported missing.
        ///
        /// **Panics on [`Gate::NoBody`]**, which is what the mutation tests
        /// below want: a mutation meant to move one field, that instead makes
        /// the class unreadable, would otherwise report that field missing and
        /// be recorded as caught for entirely the wrong reason.
        fn missing(self) -> Option<String> {
            match self {
                Gate::Missing(declaration) => Some(declaration),
                Gate::Declared => None,
                Gate::NoBody => panic!(
                    "this case expects a readable DsFace body, and the mutation \
                     removed one — so it proves nothing about the field it moved"
                ),
            }
        }
    }

    /// Reads `source` once and answers for every field of [`FACE_FIELDS`].
    ///
    /// Searched over `DsFace`'s own body rather than the whole file (issue
    /// #1097).
    fn gate(source: &str) -> Gate {
        let Some(body) = ds_face_body(&code_only(source)) else {
            return Gate::NoBody;
        };
        match FACE_FIELDS
            .iter()
            .map(|field| declaration(field))
            .find(|declaration| !body.contains(declaration.as_str()))
        {
            Some(declaration) => Gate::Missing(declaration),
            None => Gate::Declared,
        }
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
    /// these is held by review. The **descriptors** it reads them with are no
    /// longer held that way — `host::face_field!` pins each one against this
    /// module's own table with a `const` assertion (issue #1096) — but that
    /// assertion fires only where `host.rs` compiles, which is `just android`
    /// and `just android-lint` and no test tier.
    #[test]
    fn every_field_the_jni_half_reads_is_declared_by_dsface() {
        match gate(DS_FACE_JAVA) {
            Gate::Declared => {}
            Gate::NoBody => panic!(
                "no `class DsFace {{ … }}` body could be read in DsFace.java at \
                 all, so this gate compared against nothing. Three causes: the \
                 class was renamed, its braces do not balance, or a string \
                 literal or block comment anywhere in the file is unterminated \
                 — the last swallows the rest of the file, so it need not be \
                 near the class. Fix that first: it is not a field problem, \
                 and reading it as one would name a field that is fine."
            ),
            Gate::Missing(declaration) => panic!(
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
            ),
        }
    }

    /// **`same_descriptor` is the whole of the #1096 gate, and nothing else
    /// runs it.**
    ///
    /// Its only other caller is a `const` assertion behind
    /// `#[cfg(target_os = "android")]`, evaluated there only on inputs that
    /// agree — so a broken implementation is never exercised by a build that
    /// could notice. Weakening it to `while at < 0`, or dropping the length
    /// guard so `"I"` matches `"II"`, would leave every tier green while the
    /// descriptor gate accepted any mismatch.
    #[test]
    fn same_descriptor_compares_the_whole_string() {
        assert!(same_descriptor("I", "I"));
        assert!(same_descriptor("Ljava/lang/String;", "Ljava/lang/String;"));
        assert!(same_descriptor("", ""));
        assert!(!same_descriptor("I", "J"), "same length, different byte");
        assert!(!same_descriptor("I", "II"), "a prefix is not a match");
        assert!(!same_descriptor("[B", "B"), "nor is a suffix");
        // Over every pair of the six, including the five that share a
        // descriptor: `weight` and `faceIndex` are both `I`, and the three
        // arrays are all `[B`. So this asserts that `same_descriptor` agrees
        // with `==` on equal pairs as well as unequal ones — a comparison
        // answering `false` for everything would satisfy the cases above.
        //
        // That sharing is also why `host::read_face` builds each `Bound` at the
        // read rather than binding six locals: a descriptor two fields share
        // makes them interchangeable to the `const` assertion.
        for (index, one) in FACE_FIELDS.iter().enumerate() {
            for other in FACE_FIELDS.iter().skip(index + 1) {
                assert_eq!(
                    same_descriptor(one.descriptor, other.descriptor),
                    one.descriptor == other.descriptor,
                    "{:?} against {:?}",
                    one.descriptor,
                    other.descriptor
                );
            }
        }
    }

    /// **A renamed class is reported as a renamed class**, not as the first
    /// field of [`FACE_FIELDS`] going missing (issue #1097).
    ///
    /// The two were the same value until this change: `unwrap_or_default()` on
    /// an unreadable body searched an empty string, which reports `family`
    /// missing — byte for byte what a genuine `family` rename reports, and the
    /// wrong thing to fix.
    #[test]
    fn a_body_that_cannot_be_read_is_not_reported_as_a_missing_field() {
        for broken in [
            // The class renamed.
            ("class DsFace", "class DsFaceV2"),
            // Its opening brace gone, so the body never closes.
            ("public final class DsFace {", "public final class DsFace"),
        ] {
            let mutated = DS_FACE_JAVA.replace(broken.0, broken.1);
            assert_ne!(
                mutated, DS_FACE_JAVA,
                "the mutation {broken:?} did not apply"
            );
            assert!(
                matches!(gate(&mutated), Gate::NoBody),
                "{broken:?} leaves no readable DsFace body, and reporting it as \
                 a missing field points the next reader at the wrong file"
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
                declaration(&ATLAS_PNG),
            ),
            ("public final byte[] atlasPng;", "", declaration(&ATLAS_PNG)),
            (
                "public final int weight;",
                "public final long weight;",
                declaration(&WEIGHT),
            ),
        ];
        for (from, to, expected) in cases {
            let mutated = DS_FACE_JAVA.replace(from, to);
            assert_ne!(mutated, DS_FACE_JAVA, "the mutation {from:?} did not apply");
            assert_eq!(
                gate(&mutated).missing().as_deref(),
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
        let expected = declaration(&WEIGHT);
        for commented in [
            "// public final int weight;",
            "/* public final int weight; */",
        ] {
            let mutated = DS_FACE_JAVA.replace(&expected, commented);
            assert_ne!(mutated, DS_FACE_JAVA, "the mutation did not apply");
            assert_eq!(
                gate(&mutated).missing().as_deref(),
                Some(expected.as_str()),
                "a declaration inside {commented:?} was read as a declaration"
            );
        }
    }

    /// **A declaration that has left `DsFace`'s own body is not a
    /// declaration of `DsFace`** (issue #1097).
    ///
    /// `GetFieldID` is asked of a `DsFace` instance, so a field that moved into
    /// a nested type — or into a second top-level class in the same file —
    /// throws `NoSuchFieldError` exactly as a rename does. The match was
    /// unanchored and found the text wherever it sat.
    #[test]
    fn a_declaration_outside_the_dsface_body_is_not_found() {
        let expected = declaration(&WEIGHT);

        // Nested inside `DsFace`, which brace *matching* alone does not
        // exclude: it sits between `class DsFace {` and its closing brace, so
        // only tracking depth keeps it out.
        let nested = DS_FACE_JAVA.replace(
            &expected,
            "public static final class Inner { public final int weight; }",
        );
        assert_ne!(
            nested, DS_FACE_JAVA,
            "the nested-class mutation did not apply"
        );
        assert_eq!(
            gate(&nested).missing().as_deref(),
            Some(expected.as_str()),
            "a field declared by a nested class is not one `GetFieldID` can \
             resolve on a DsFace instance"
        );

        // A second **top-level** class, appended after `DsFace`'s own closing
        // brace. Built by removing the declaration and adding a whole class,
        // rather than by injecting a stray `}` mid-body: that shape truncates
        // `DsFace`'s body at the injection point, so the assertion would hold
        // for an implementation that stopped at the first `}` and tracked no
        // depth at all — and would hold only because `weight` happens to sit
        // second in `FACE_FIELDS`.
        let sibling = format!(
            "{}\nfinal class Other {{ {expected} }}\n",
            DS_FACE_JAVA.replace(&expected, "")
        );
        assert!(
            sibling.contains(&expected),
            "the declaration must still be in the file, or this proves only \
             that a deleted field is missing"
        );
        assert_eq!(
            gate(&sibling).missing().as_deref(),
            Some(expected.as_str()),
            "a field declared by a sibling top-level class is not one \
             `GetFieldID` can resolve on a DsFace instance"
        );
    }

    /// **Text either side of an elided nested block does not join into a
    /// declaration.**
    ///
    /// The false pass this gate's design says is impossible, and it was real:
    /// `ds_face_body` pushed a *space* where it dropped a nested block, so
    /// `public final int{ }weight;` — which declares nothing — collapsed to
    /// exactly the text the gate looks for, and `weight` was reported as
    /// declared. A brace pair cannot appear inside a declaration, so it cannot
    /// complete one.
    ///
    /// Contrived, like the two cases above. It is here because the module's
    /// standing claim is that every way the stripping can be wrong is a loud
    /// failure, and each of these was a counterexample.
    ///
    /// **Every construct that gets elided, not only the nested block.** The
    /// block was fixed first and the two literal forms were left, which is the
    /// same defect surviving in the sibling branch of the same function — so
    /// each elided construct is driven here.
    #[test]
    fn an_elided_construct_does_not_join_the_text_around_it() {
        let expected = declaration(&WEIGHT);
        for joiner in [
            // A nested block, elided by `ds_face_body`.
            "public final int{ }weight;",
            // A string and a character literal, elided by `code_only`.
            "public final int\"x\"weight;",
            "public final int'x'weight;",
        ] {
            let joined = DS_FACE_JAVA.replace(&expected, joiner);
            assert_ne!(
                joined, DS_FACE_JAVA,
                "the mutation {joiner:?} did not apply"
            );
            assert_eq!(
                gate(&joined).missing().as_deref(),
                Some(expected.as_str()),
                "{joiner:?} declares nothing, and reporting it as a declaration \
                 is the one direction this gate must never fail in"
            );
        }
    }

    /// **A declaration inside a string literal is not a declaration** (issue
    /// #1097).
    ///
    /// Comments were stripped and literals were not, so a field holding the
    /// text of another field's declaration satisfied the gate for a field that
    /// no longer existed.
    #[test]
    fn a_declaration_inside_a_string_literal_is_not_found() {
        let expected = declaration(&WEIGHT);
        let mutated = DS_FACE_JAVA.replace(
            &expected,
            "public final String note = \"public final int weight;\";",
        );
        assert_ne!(mutated, DS_FACE_JAVA, "the mutation did not apply");
        assert_eq!(
            gate(&mutated).missing().as_deref(),
            Some(expected.as_str()),
            "the needle was found inside a string literal, so the gate passed \
             for a field that is not declared"
        );
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
        let expected = declaration(&WEIGHT);
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
                gate(&mutated).missing(),
                None,
                "this spelling was not found: {spelling:?}"
            );
        }
    }
}
