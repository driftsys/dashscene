//! The JNI entry points Java declares, and the gate that keeps them the ones
//! Rust exports (issue #1184).
//!
//! # The same hazard as `crate::face`, one level up
//!
//! [`crate::face`] pins the `DsFace` **fields** `host::read_face` reads,
//! because `host.rs` is behind `#[cfg(target_os = "android")]` and links into
//! no test binary — so a disagreement with Java surfaces as a
//! `NoSuchFieldError` at the first `surfaceChanged`, with the handle coming
//! back 0 and no glyph drawn (issue #1089).
//!
//! **The methods carry that hazard too, and nothing checked them.** A typo in
//! `Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreatedMapped`, or
//! a `native` declaration renamed on one side only, compiles, lints under `just
//! android-lint`, and packages under `just android-apk`. It fails as
//! `UnsatisfiedLinkError` the first time the method is called — which for
//! `nativeSurfaceDestroyed` is during teardown, on the path D4 exists to keep
//! correct.
//!
//! Issue #1035 is what prompted this: it added the **seventh** native method,
//! and nothing at all compared the two halves.
//!
//! **Both hosts, from here.** `demo-android` carries the identical pair, and
//! its `nativeStop` is that host's teardown call. It is gated from this module
//! rather than from its own crate because the comparison is `#[cfg(test)]`,
//! which is not visible across a crate boundary — gating it there would mean a
//! second copy of this file rather than a second call.
//!
//! # What it asks, and what it deliberately does not
//!
//! For each name in [`ENTRY_POINTS`]: does `DashsceneNative.java` declare a
//! `native` method with that name, **and** does `host.rs` export the JNI symbol
//! for it? That is the "is this known declaration present" direction, which
//! needs no grammar — the same choice `face` makes, and for the reason its own
//! doc records: two rounds of review found ten shapes that escaped a parser for
//! the opposite question.
//!
//! **Set equality on both sides**, which is stronger than the presence check
//! `face` makes of a field: an eighth method added to Java, or exported by
//! Rust, and left out of [`ENTRY_POINTS`] fails here rather than being
//! invisible. That is affordable because a `native` declaration and a
//! `#[unsafe(no_mangle)]` item are each recognisable without a grammar, where
//! "what fields does this class declare" is not.
//!
//! **Both of the mangled name's other halves are read from the Java file too.**
//! The symbol is the package, the class and the method name, and a rename of
//! any one of the three is the same run-time failure. The package and the class
//! are derived from the file's own `package` and `class` declarations rather
//! than written down here, and the declarations are read from **inside the
//! class body** rather than from anywhere in the file — a `native` method in a
//! nested class is mangled `…_00024Inner_…`, which is issue #1097's hole one
//! level up from the fields it was filed about.
//!
//! **It does not check the signature**, and that is a decision rather than an
//! oversight. Doing so means mapping Java parameter types to JNI descriptors
//! and reading a Rust parameter list — the "what does this file declare"
//! direction again. Named in issue #1184 as the half not taken.
//!
//! **What that leaves uncovered is worse than a link error, and worth stating
//! exactly.** JNI resolves a native method that is not overloaded by the short
//! name alone — `Java_pkg_Class_method`, with the descriptor never consulted —
//! so a parameter list that changes without the name changing produces the same
//! symbol and links. Two shapes reach that:
//!
//! - **A retyped parameter.** Six of the seven entry points here take
//!   `long handle`. Writing one as `int handle` in Java compiles, packages, and
//!   passes this gate; at the call the JVM pushes a `jint` where the Rust half
//!   reads a `jlong`, so the handle is a value the host never produced. For
//!   `nativeSurfaceDestroyed` that is a wild pointer dereferenced during
//!   teardown.
//! - **A reordered pair of same-typed parameters.**
//!   `nativeSurfaceChanged(long, int, int)` is the one place that is reachable,
//!   and its two `int`s are width and height, in that order, in both halves and
//!   in the ABI.
//!
//! Neither is an `UnsatisfiedLinkError`, which is the failure the rest of this
//! module is about: they link and then misbehave. The gate that would close
//! them is the descriptor comparison above.

// The whole module is test-only: it compares two files' text and exports
// nothing. `include_str!` of `host.rs` is why that matters — no shipped build
// carries either file's bytes.
#![cfg(test)]

/// Every native method `DashsceneNative` declares, by name.
///
/// One spelling each, here rather than in the test, so adding an entry point is
/// one edit and the gate fails until both halves carry it.
const ENTRY_POINTS: [&str; 7] = [
    "nativeAbiVersion",
    "nativeIsRunning",
    "nativeSurfaceChanged",
    "nativeSurfaceCreated",
    "nativeSurfaceCreatedMapped",
    "nativeSurfaceCreatedWithText",
    "nativeSurfaceDestroyed",
];

/// `demo-android`'s own JNI pair, which carries the identical hazard.
///
/// **Gated from here rather than from that crate**, because the comparison is
/// `#[cfg(test)]` and a `cfg(test)` item is not visible across a crate
/// boundary — so gating it there would mean a second copy of this file.
///
/// **The two `include_str!` paths reach outside this crate, and that is the
/// cost of the one copy.** `cargo package` copies only what is under the
/// package root, so these two files are not in the tarball and this module
/// does not compile from it. Nothing in the publish path reads them: the
/// module is `#![cfg(test)]`, and `cargo package --verify` and `cargo publish`
/// both build rather than test. What it costs is that `cargo test` run inside
/// an extracted or vendored tarball fails to compile — the gate is a
/// workspace-only one, which is where the drift it watches for happens.
const DEMO_NATIVE_JAVA: &str =
    include_str!("../../../demo-android/android/java/dev/driftsys/dashscene/demo/DemoNative.java");

const DEMO_HOST_RS: &str = include_str!("../../../demo-android/src/host.rs");

/// Every native method `DemoNative` declares, by name.
const DEMO_ENTRY_POINTS: [&str; 7] = [
    "nativeCommand",
    "nativeDrag",
    "nativeIsRunning",
    "nativeReadout",
    "nativeResize",
    "nativeStart",
    "nativeStop",
];

/// `DashsceneNative.java`, read at compile time.
///
/// `include_str!` rather than a path opened at run time, for the reason
/// `face` gives: it makes this test's compile depend on the file, so moving or
/// deleting it is a compile error rather than a test that quietly stops
/// checking anything.
const DASHSCENE_NATIVE_JAVA: &str =
    include_str!("../harness/java/dev/driftsys/dashscene/DashsceneNative.java");

/// `host.rs`'s own source, which is the only way to read it from a test.
///
/// The module is behind `#[cfg(target_os = "android")]`, so on the target this
/// test runs it is not compiled and its symbols do not exist to look up. Its
/// **text** is available on every target, and a symbol's presence is what this
/// asks.
const HOST_RS: &str = include_str!("host.rs");

mod tests {
    use super::*;
    use crate::face::{class_body, code_only};

    /// The package and the class name a Java source file declares, read from
    /// its own text.
    ///
    /// `code` must be [`code_only`] output, so that neither a comment nor a
    /// string literal can carry either word.
    ///
    /// Panics when either is absent, which for the two files this gate reads is
    /// not a state to recover from: without them there is no symbol prefix to
    /// compare against and nothing this module can say.
    fn package_and_class(code: &str) -> (String, String) {
        let package = code
            .split_once("package ")
            .and_then(|(_, rest)| rest.split_once(';'))
            .map(|(name, _)| name.trim().to_owned())
            .expect("a Java file this gate reads declares a package");
        let class = code
            .split_once(" class ")
            .and_then(|(_, rest)| rest.split([' ', '{', '<']).next())
            .filter(|name| !name.is_empty())
            .expect("a Java file this gate reads declares a class")
            .to_owned();
        (package, class)
    }

    /// The prefix JNI mangles every method of `class` in `package` to.
    ///
    /// **Derived rather than written down** (issue #1184, fix round). Both
    /// halves of the name are load-bearing and neither is checked by anything
    /// else: `DashsceneNative.java`'s own javadoc says renaming the class or
    /// the package breaks the link at run time rather than at build time. A
    /// prefix held as a constant here agrees with itself after such a rename,
    /// so both set comparisons pass and every call throws
    /// `UnsatisfiedLinkError`.
    ///
    /// **Only the `.` to `_` half of the mangling.** JNI also writes `_` in an
    /// identifier as `_1`, and a non-ASCII character as `_0xxxx`. No name here
    /// carries either, and [`assert_manglable`] says so rather than letting a
    /// later rename derive a symbol that is quietly wrong.
    fn symbol_prefix(package: &str, class: &str) -> String {
        assert_manglable("the package", package);
        assert_manglable("the class", class);
        format!("Java_{}_{class}_", package.replace('.', "_"))
    }

    /// Refuses a name this module's mangling cannot spell.
    ///
    /// **All three parts of the symbol, not two.** A JNI symbol is the package,
    /// the class and the method name, and the `_` to `_1` rule applies to each
    /// of them — so `nativeSurfaceCreated_v2` must be exported as
    /// `…_nativeSurfaceCreated_1v2`. With only the first two checked, adding an
    /// underscore to an entry point failed [`compare`] on the Rust half with
    /// "the entry points are not the set this gate names", which points the
    /// reader at a missing export rather than at the rule they broke.
    fn assert_manglable(what: &str, name: &str) {
        assert!(
            !name.contains('_'),
            "JNI mangles `_` in {what}, `{name}`, to `_1`, and this gate's \
             symbol derivation does not — teach it that rule before adding an \
             underscore to a package, a class or an entry point"
        );
    }

    /// Every name declared `native` **directly inside the class body**, in
    /// source order.
    ///
    /// `body` is [`class_body`] output, which is the whole point: JNI mangles a
    /// method of a nested class as `…_00024Inner_…`, so a declaration moved
    /// into one throws at the first call while remaining a `native` declaration
    /// somewhere in the file. Scanning the file rather than the body is the
    /// exact hole issue #1097 closed for `face`'s fields.
    ///
    /// A scan rather than a search for known names, which is what lets the gate
    /// below compare *sets*. The text is collapsed to single spaces before this
    /// runs, so a declaration reads
    /// `public static native boolean nativeIsRunning(long handle);` whatever its
    /// original spacing.
    ///
    /// Return types are taken and discarded rather than matched: a list of the
    /// ones seen so far would have missed `boolean`, which is exactly what the
    /// first draft of this gate did.
    fn declared_native_methods(body: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut rest = body;
        while let Some(at) = rest.find(" native ") {
            rest = &rest[at + " native ".len()..];
            let mut tokens = rest.split_whitespace();
            // The return type, which this does not check.
            let _ = tokens.next();
            if let Some(token) = tokens.next()
                && let Some(name) = token.split('(').next()
                && !name.is_empty()
            {
                names.push(name.to_owned());
            }
        }
        names
    }

    /// Every symbol `host.rs` exports to JNI, by mangled name.
    ///
    /// **Anchored on `#[unsafe(no_mangle)]`, not on the name.** Two earlier
    /// shapes were wrong in ways the gate existed to catch:
    ///
    /// - `rust.contains(&symbol)` is unterminated, and
    ///   `…_nativeSurfaceCreated` is a strict prefix of both
    ///   `…_nativeSurfaceCreatedMapped` and `…_nativeSurfaceCreatedWithText`.
    ///   Deleting or renaming `nativeSurfaceCreated` left the gate green.
    /// - Running [`crate::face::code_only`] over Rust source passes while
    ///   checking almost nothing: that stripper is for Java, where `'` opens a
    ///   character literal, and in Rust `'` is almost always a lifetime. It cut
    ///   `host.rs` from 38 058 bytes to 7 959 and removed whole parameter
    ///   lists, so which symbols survived depended on how many lifetimes the
    ///   file happened to carry.
    ///
    /// Reading the item after each `#[unsafe(no_mangle)]` answers what is
    /// actually exported, and catches an entry point that keeps its name but
    /// loses the attribute — which compiles, packages, and fails as
    /// `UnsatisfiedLinkError` at the first call.
    ///
    /// **The item has to be a function, and that is checked rather than
    /// assumed.** `#[unsafe(no_mangle)]` is legal on a `static` too, and a
    /// chunk runs to the *next* attribute — so taking the first `fn ` in it
    /// records whatever function textually follows the static, which is a name
    /// this reports as exported that JNI never sees. Nothing in `host.rs`
    /// exports a static today; a `DS_ANDROID_ABI` constant is the obvious way
    /// one arrives.
    fn exported_jni_symbols(rust: &str) -> Vec<String> {
        rust.split("#[unsafe(no_mangle)]")
            // Everything before the first attribute is not an exported item.
            .skip(1)
            .filter_map(|item| {
                let at = item.find("fn ")? + "fn ".len();
                // A `static` or `const` reached before the `fn` means this
                // chunk's own item is not a function, and the `fn` found is a
                // later one that this attribute does not apply to.
                let head = &item[..at];
                if head.contains(" static ") || head.contains(" const ") {
                    return None;
                }
                let rest = &item[at..];
                // A generic entry point is `fn Name<'local>(`, a plain one
                // `fn Name(`; either terminates the symbol.
                let end = rest.find(['<', '('])?;
                Some(rest[..end].trim().to_owned())
            })
            .collect()
    }

    /// **Whether Java's `native` declarations and Rust's exported symbols are
    /// both exactly the named set** (issue #1184).
    ///
    /// Set equality on each side, so a method added, removed or renamed in
    /// either half disagrees — including one added to both halves and left out
    /// of the list. The symbol prefix is derived from the Java file's own
    /// `package` and class name, so renaming either disagrees too.
    ///
    /// The Java side is read with comments and literals removed and then
    /// narrowed to the class body, so a commented-out declaration and one
    /// moved into a nested class both fail rather than pass. The Rust side is
    /// read raw: [`crate::face::code_only`] is a Java stripper and mis-lexes
    /// Rust lifetimes, and `#[unsafe(no_mangle)]` is not something a comment
    /// carries.
    ///
    /// **A `Result` rather than the assertions themselves**, so that a test can
    /// drive a disagreement and see one. Every mutation test below asserts
    /// `is_err`, which is what says this gate fails when it should: with the
    /// comparisons written as bare `assert_eq!` the only way to check that was
    /// to remove one and watch nothing happen.
    ///
    /// **The `Err` covers the two set comparisons, and nothing before them.**
    /// A file with no `package`, no `class` keyword, or a `_` in a name this
    /// gate mangles panics inside the helpers instead. That is deliberate:
    /// those are not two halves disagreeing, they are inputs this module cannot
    /// read at all, and a `Result` would invite a caller to treat "the gate
    /// could not run" as "the gate ran and found drift".
    fn compare(java: &str, rust: &str, names: &[&str]) -> Result<(), String> {
        let code = code_only(java);
        let (package, class) = package_and_class(&code);
        let prefix = symbol_prefix(&package, &class);
        for name in names {
            assert_manglable("the entry point", name);
        }
        let body = class_body(&code, &class).ok_or_else(|| {
            format!(
                "no body could be read for `class {class}`, so nothing was \
                 compared. The class this gate reads is named by the file's \
                 own `package` and `class` declarations."
            )
        })?;

        let mut expected: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        expected.sort();

        let mut declared = declared_native_methods(&body);
        declared.sort();
        if declared != expected {
            return Err(format!(
                "the Java half's `native` declarations are not the set this \
                 gate names for `{prefix}`: {declared:?} against {expected:?}. \
                 Adding an entry point is two edits — the Java declaration and \
                 the list here — and until both are made this gate says \
                 nothing about the new one. A declaration inside a nested \
                 class is not counted, because JNI mangles it differently."
            ));
        }

        let mut exported: Vec<String> = exported_jni_symbols(rust)
            .into_iter()
            .filter_map(|symbol| Some(symbol.strip_prefix(&prefix)?.to_owned()))
            .collect();
        exported.sort();
        if exported != expected {
            return Err(format!(
                "the Rust half's `#[unsafe(no_mangle)]` entry points are not \
                 the set this gate names for `{prefix}`: {exported:?} against \
                 {expected:?}. JNI looks a native method up by its exact \
                 mangled name, and neither `just android` nor `just \
                 android-apk` compares the two halves — the failure is an \
                 UnsatisfiedLinkError at the first call, which for a teardown \
                 method is during `surfaceDestroyed`."
            ));
        }
        Ok(())
    }

    /// [`compare`], as the two gates below read it.
    fn assert_halves_agree(java: &str, rust: &str, names: &[&str]) {
        if let Err(why) = compare(java, rust, names) {
            panic!("{why}");
        }
    }

    /// `dashscene-android`'s pair agrees.
    ///
    /// **This found a defect the moment it first ran**: `nativeIsRunning` was
    /// missing from the list, and its `boolean` return would have escaped the
    /// first draft's list of return types as well.
    #[test]
    fn every_entry_point_is_declared_by_java_and_exported_by_rust() {
        assert_halves_agree(DASHSCENE_NATIVE_JAVA, HOST_RS, &ENTRY_POINTS);
    }

    /// **`demo-android`'s pair agrees too.**
    ///
    /// The same hazard in the other host, gated by the same comparison rather
    /// than by a second copy of it. `nativeStop` is why it is worth covering:
    /// it is that host's teardown call, and an `UnsatisfiedLinkError` there is
    /// a `surfaceDestroyed` that never hands the surface back — the
    /// use-after-free D4 exists to prevent.
    #[test]
    fn the_demo_hosts_entry_points_agree_too() {
        assert_halves_agree(DEMO_NATIVE_JAVA, DEMO_HOST_RS, &DEMO_ENTRY_POINTS);
    }

    /// **The derived prefix is the one the symbols carry**, for both hosts.
    ///
    /// The two spellings this module held as constants until the fix round.
    /// Written here as an assertion rather than as an input, so the derivation
    /// is pinned without being the thing that decides the gate.
    #[test]
    fn the_symbol_prefix_is_derived_from_the_java_file() {
        let dashscene = code_only(DASHSCENE_NATIVE_JAVA);
        let (package, class) = package_and_class(&dashscene);
        assert_eq!(
            (package.as_str(), class.as_str()),
            ("dev.driftsys.dashscene", "DashsceneNative")
        );
        assert_eq!(
            symbol_prefix(&package, &class),
            "Java_dev_driftsys_dashscene_DashsceneNative_"
        );

        let demo = code_only(DEMO_NATIVE_JAVA);
        let (package, class) = package_and_class(&demo);
        assert_eq!(
            symbol_prefix(&package, &class),
            "Java_dev_driftsys_dashscene_demo_DemoNative_"
        );
    }

    /// **A renamed class fails**, which a written-down prefix cannot see.
    ///
    /// `DashsceneNative.java`'s own javadoc warns that the symbol names are
    /// derived from this exact package and class name and that renaming either
    /// breaks the link at run time. With the prefix held as a constant the two
    /// set comparisons still agreed with each other after such a rename.
    #[test]
    fn a_renamed_class_fails_the_comparison() {
        let renamed = DASHSCENE_NATIVE_JAVA.replace("class DashsceneNative", "class DashsceneJni");
        assert_ne!(renamed, DASHSCENE_NATIVE_JAVA, "the mutation did not apply");
        assert!(
            compare(&renamed, HOST_RS, &ENTRY_POINTS).is_err(),
            "the Rust symbols are mangled from the old class name"
        );
    }

    /// **A renamed package fails**, for the same reason and by the same route.
    #[test]
    fn a_renamed_package_fails_the_comparison() {
        let renamed = DASHSCENE_NATIVE_JAVA.replace(
            "package dev.driftsys.dashscene;",
            "package dev.driftsys.scene;",
        );
        assert_ne!(renamed, DASHSCENE_NATIVE_JAVA, "the mutation did not apply");
        assert!(
            compare(&renamed, HOST_RS, &ENTRY_POINTS).is_err(),
            "the Rust symbols are mangled from the old package"
        );
    }

    /// **A `native` method moved into a nested class fails** (issue #1097's
    /// hole, one level up).
    ///
    /// It is still a `native` declaration in the file, so a whole-file scan
    /// finds it and the gate stays green. JNI mangles it
    /// `…_DashsceneNative_00024Inner_nativeIsRunning`, which nothing exports,
    /// so every call throws.
    #[test]
    fn a_native_method_in_a_nested_class_is_not_counted() {
        let moved = DASHSCENE_NATIVE_JAVA.replace(
            "public static native boolean nativeIsRunning(long handle);",
            "public static final class Inner {\n        public static native boolean nativeIsRunning(long handle);\n    }",
        );
        assert_ne!(moved, DASHSCENE_NATIVE_JAVA, "the mutation did not apply");

        let code = code_only(&moved);
        assert!(
            code.contains("native boolean nativeIsRunning"),
            "the declaration is still in the file, which is the whole hazard"
        );
        let body = class_body(&code, "DashsceneNative").expect("the class body is still readable");
        assert!(
            !declared_native_methods(&body)
                .iter()
                .any(|name| name == "nativeIsRunning"),
            "a declaration below the class body's own depth is not counted"
        );
        assert!(
            compare(&moved, HOST_RS, &ENTRY_POINTS).is_err(),
            "and the gate says so"
        );
    }

    /// **A method removed from Java alone fails.**
    ///
    /// The disagreement the gate exists for, driven end to end: without a test
    /// that asserts `is_err`, dropping either comparison from [`compare`]
    /// leaves every other test in this module green.
    #[test]
    fn a_method_removed_from_java_alone_fails_the_comparison() {
        let removed = DASHSCENE_NATIVE_JAVA.replace(
            "public static native int nativeAbiVersion();",
            "public static int nativeAbiVersion() { return 0; }",
        );
        assert_ne!(removed, DASHSCENE_NATIVE_JAVA, "the mutation did not apply");
        assert!(compare(&removed, HOST_RS, &ENTRY_POINTS).is_err());
    }

    /// **The unmutated pair agrees**, which is what makes every `is_err` above
    /// evidence about the mutation rather than about the gate.
    #[test]
    fn the_comparison_is_ok_before_any_mutation() {
        assert_eq!(
            compare(DASHSCENE_NATIVE_JAVA, HOST_RS, &ENTRY_POINTS),
            Ok(())
        );
    }

    /// A rename on either side is caught, **and caught for the side that was
    /// renamed**.
    ///
    /// `nativeSurfaceCreated` on purpose: it is a strict prefix of two other
    /// symbols, so it is the name an unterminated `contains` cannot see going
    /// missing. That is what the first draft of this gate did.
    #[test]
    fn a_rename_on_either_side_is_not_found() {
        let renamed_java = DASHSCENE_NATIVE_JAVA.replace(
            "native long nativeSurfaceCreated(",
            "native long nativeSurfaceCreatedTypo(",
        );
        assert_ne!(
            renamed_java, DASHSCENE_NATIVE_JAVA,
            "the Java mutation did not apply"
        );
        let declared = dashscene_declarations(&renamed_java);
        assert!(
            !declared.iter().any(|name| name == "nativeSurfaceCreated"),
            "a renamed Java declaration must not still be found, got {declared:?}"
        );

        let renamed_rust = HOST_RS.replace(
            "fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreated<",
            "fn Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreatedTypo<",
        );
        assert_ne!(renamed_rust, HOST_RS, "the Rust mutation did not apply");
        let exported = exported_jni_symbols(&renamed_rust);
        assert!(
            !exported.iter().any(|symbol| symbol
                == "Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreated"),
            "a renamed Rust symbol must not still be found, got {exported:?}"
        );
    }

    /// An entry point that keeps its name but loses `#[unsafe(no_mangle)]` is
    /// not exported, and the gate says so.
    ///
    /// The failure a name-only search cannot see: the text is all still there.
    /// Both levels in one test — that `exported_jni_symbols` does not report it
    /// and that [`compare`] fails — because a second test applying the same
    /// mutation to assert the weaker of the two is one more thing to keep in
    /// step for nothing.
    #[test]
    fn an_entry_point_without_no_mangle_is_not_exported() {
        let stripped = HOST_RS.replacen(
            "#[unsafe(no_mangle)]\npub extern \"system\" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeAbiVersion",
            "pub extern \"system\" fn Java_dev_driftsys_dashscene_DashsceneNative_nativeAbiVersion",
            1,
        );
        assert_ne!(stripped, HOST_RS, "the mutation did not apply");
        let exported = exported_jni_symbols(&stripped);
        assert!(
            !exported
                .iter()
                .any(|symbol| symbol.ends_with("nativeAbiVersion")),
            "an item without the attribute is not exported to JNI, got {exported:?}"
        );
        assert!(
            compare(DASHSCENE_NATIVE_JAVA, &stripped, &ENTRY_POINTS).is_err(),
            "and the Rust-side set comparison reports it"
        );
    }

    /// **A `#[unsafe(no_mangle)]` static does not lend its attribute to the
    /// next function.**
    ///
    /// The attribute is legal on a static, and a chunk of the split runs to the
    /// next attribute — so reading the first `fn ` in it reported a function
    /// this gate has no business naming. An ABI-version constant is the obvious
    /// way such a static arrives.
    #[test]
    fn a_no_mangle_static_is_not_read_as_the_next_function() {
        let with_static = HOST_RS.replacen(
            "#[unsafe(no_mangle)]",
            "#[unsafe(no_mangle)]\npub static DS_ANDROID_ABI: u32 = 1;\n\nfn not_an_entry_point() {}\n\n#[unsafe(no_mangle)]",
            1,
        );
        assert_ne!(with_static, HOST_RS, "the mutation did not apply");

        let exported = exported_jni_symbols(&with_static);
        assert!(
            !exported.iter().any(|symbol| symbol == "not_an_entry_point"),
            "the function after the static is not what the attribute exports, got {exported:?}"
        );
        assert_eq!(
            exported,
            exported_jni_symbols(HOST_RS),
            "and the set is otherwise the one the unmutated file gives"
        );
    }

    /// **An entry point whose name carries `_` is refused rather than
    /// mis-derived**, because JNI writes it `_1`.
    #[test]
    #[should_panic(expected = "JNI mangles `_` in the entry point")]
    fn an_entry_point_with_an_underscore_is_refused() {
        let names = ["nativeSurfaceCreated_v2"];
        let _ = compare(DASHSCENE_NATIVE_JAVA, HOST_RS, &names);
    }

    /// A method added to Java alone fails the set comparison, naming it.
    #[test]
    fn a_method_added_to_java_alone_is_reported() {
        let added = DASHSCENE_NATIVE_JAVA.replace(
            "public static native int nativeAbiVersion();",
            "public static native int nativeAbiVersion();\n    public static native void nativeNewThing(long handle);",
        );
        assert_ne!(added, DASHSCENE_NATIVE_JAVA, "the mutation did not apply");
        let declared = dashscene_declarations(&added);
        assert!(
            declared.iter().any(|name| name == "nativeNewThing"),
            "a native declaration added to Java must be seen, got {declared:?}"
        );
        assert!(
            compare(&added, HOST_RS, &ENTRY_POINTS).is_err(),
            "and must make the comparison fail"
        );
    }

    /// [`declared_native_methods`] over `DashsceneNative`'s own class body, as
    /// the mutation tests read it.
    fn dashscene_declarations(java: &str) -> Vec<String> {
        let code = code_only(java);
        let body = class_body(&code, "DashsceneNative").expect("the class body is readable");
        declared_native_methods(&body)
    }
}
