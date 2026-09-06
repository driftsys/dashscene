//! A C# scanner just wide enough to ask where a token sits.
//!
//! **Not a parser, and it refuses what it cannot read.** Everything here exists
//! so a gate over `Runtime/Engine/BrgPainter.cs` — a file no CI job compiles —
//! can ask "is this call still written inside that method" without the answer
//! being defeated by a comment or a string. A first version stripped comments by
//! truncating each line at `//`, and its own remarks conceded that a `//` inside
//! a string literal would fool it. That was measured: a `Debug.Log("see
//! https://…")` written above a second, live read of the SRP-Batcher global hid
//! the read completely, and the gate passed over issue #1317 restored.
//!
//! So the rule is that anything unhandled **panics** rather than degrading. A
//! gate that quietly reads less than it claims is worse than no gate: it reports
//! success over the defect it was written for.

/// The source with comment and string bodies blanked, offsets preserved.
///
/// Every byte keeps its position, so an index into the result is an index into
/// the original. Blanked bytes become spaces, and newlines survive so line
/// numbers still work.
///
/// **Interpolation holes are kept.** Inside `$"…{expr}…"` the literal text is
/// blanked and `expr` is not, because "does this message name the rung" is a
/// question about the expressions a message interpolates, not about its prose.
///
/// # Panics
///
/// On a verbatim string (`@"…"`) or an unterminated string or block comment.
/// The painter uses neither today; each would need handling this deliberately
/// does not guess at.
pub fn blank_comments_and_strings(source: &str) -> String {
    blank(source, true)
}

/// The source with comment bodies blanked and string bodies kept, offsets
/// preserved.
///
/// **For the one question [`blank_comments_and_strings`] cannot answer**: is a
/// literal message written in code rather than quoted in a comment above it.
/// That scanner blanks the literal being looked for, so a test using it would
/// find nothing whatever the file says; truncating each line at `//` instead is
/// the defect this module's own header records. This keeps the string bodies and
/// drops the comments, so a comment quoting the message does not satisfy the
/// assertion and the message itself does.
///
/// # Panics
///
/// On a verbatim string (`@"…"`) or an unterminated string or block comment, for
/// [`blank_comments_and_strings`]'s reasons — the two share one scanner.
pub fn blank_comments_only(source: &str) -> String {
    blank(source, false)
}

/// The one scanner both public functions are. `strings` says whether a string
/// body is blanked as a comment is, or kept as code is.
fn blank(source: &str, strings: bool) -> String {
    let b: Vec<char> = source.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(b.len());
    let mut i = 0;
    // Brace depth inside the interpolated string being scanned, if any.
    let mut interp: Vec<u32> = Vec::new();

    while i < b.len() {
        let c = b[i];

        // A verbatim string needs different escaping rules; refuse rather than
        // mis-read one.
        if c == '@' && i + 1 < b.len() && b[i + 1] == '"' {
            panic!(
                "this scanner does not handle verbatim strings (@\"…\"), and one \
                 appears at byte {i}. Every question asked of the scanned text \
                 would be asked of that string's contents too. Handle it rather \
                 than deleting the check."
            );
        }

        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }

        if c == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            let start = i;
            out.push(' ');
            out.push(' ');
            i += 2;
            loop {
                if i + 1 >= b.len() {
                    panic!("unterminated block comment opened at byte {start}");
                }
                if b[i] == '*' && b[i + 1] == '/' {
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                    break;
                }
                out.push(if b[i] == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }

        if c == '"' {
            let interpolated = i > 0 && b[i - 1] == '$';
            let start = i;
            out.push('"');
            i += 1;
            let mut depth: u32 = 0;
            loop {
                if i >= b.len() {
                    panic!("unterminated string literal opened at byte {start}");
                }
                let d = b[i];
                if depth == 0 && d == '\\' {
                    let escaped = if i + 1 < b.len() { b[i + 1] } else { ' ' };
                    out.push(if strings { ' ' } else { d });
                    out.push(if strings { ' ' } else { escaped });
                    i += 2;
                    continue;
                }
                if interpolated && d == '{' {
                    // `{{` is a literal brace, not a hole.
                    if depth == 0 && i + 1 < b.len() && b[i + 1] == '{' {
                        out.push(if strings { ' ' } else { '{' });
                        out.push(if strings { ' ' } else { '{' });
                        i += 2;
                        continue;
                    }
                    depth += 1;
                    out.push('{');
                    i += 1;
                    continue;
                }
                if interpolated && d == '}' && depth > 0 {
                    depth -= 1;
                    out.push('}');
                    i += 1;
                    continue;
                }
                if depth > 0 {
                    // Inside a hole: this is code, keep it.
                    out.push(d);
                    i += 1;
                    continue;
                }
                if d == '"' {
                    out.push('"');
                    i += 1;
                    break;
                }
                out.push(if !strings || d == '\n' { d } else { ' ' });
                i += 1;
            }
            interp.clear();
            continue;
        }

        out.push(c);
        i += 1;
    }

    out.into_iter().collect()
}

/// The body of the member whose declaration contains `signature`, braces
/// matched, as a `(start, end)` range over the scanned text.
///
/// `scanned` must already have been through [`blank_comments_and_strings`], so
/// a brace inside a comment or a string cannot move the match.
///
/// **Not [`blank_comments_only`], whose output violates that precondition.**
/// That scanner keeps string bodies, so a `{` inside a literal counts here and
/// shifts the matched range — a check that would then assert over the wrong
/// span, which is the silent degradation this module's header refuses. Use it
/// for `contains` over the whole file, never as this function's input.
///
/// # Panics
///
/// If the signature is absent, or its braces do not balance.
pub fn member_body(scanned: &str, signature: &str) -> (usize, usize) {
    let at = scanned
        .find(signature)
        .unwrap_or_else(|| panic!("no member matching `{signature}`"));
    let open = scanned[at..]
        .find('{')
        .unwrap_or_else(|| panic!("`{signature}` is followed by no body"))
        + at;

    let bytes = scanned.as_bytes();
    let mut depth = 0usize;
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return (open, offset);
                }
            }
            _ => {}
        }
    }
    panic!("`{signature}`'s body never closes");
}

/// The condition of the first `if` in `body`, parentheses matched.
///
/// # Panics
///
/// If there is no `if (`, or its parentheses do not balance.
pub fn first_if_condition(body: &str) -> &str {
    let at = body
        .find("if (")
        .unwrap_or_else(|| panic!("no `if (` in this member's body"));
    let open = at + 3;
    let bytes = body.as_bytes();
    let mut depth = 0usize;
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &body[open + 1..offset];
                }
            }
            _ => {}
        }
    }
    panic!("the first `if`'s condition never closes");
}

/// The text of one `switch` arm: from its `case` label to the next `case` or
/// `default:` label, or to the end of `scanned` if it is the last.
///
/// **Bounded by the next label, not by the arm's `return`.** An arm that stops
/// returning — `break` instead of `return`, say — would otherwise extend the
/// slice to some later `return` hundreds of lines away, and every assertion over
/// it would then be satisfied by unrelated code. That was measured.
///
/// # Panics
///
/// If the label is absent.
pub fn switch_arm<'a>(scanned: &'a str, label: &str) -> &'a str {
    let at = scanned
        .find(label)
        .unwrap_or_else(|| panic!("no `{label}` arm"));
    let rest = &scanned[at + label.len()..];
    let next = rest
        .find("case ")
        .into_iter()
        .chain(rest.find("default:"))
        .min()
        .unwrap_or(rest.len());
    &rest[..next]
}

/// `scanned` with every internal run of whitespace collapsed to one space, and
/// the leading and trailing runs removed.
///
/// **So an assertion can name a whole expression without naming its line
/// breaks.** A multi-line initialiser is one expression to a reader and several
/// to `contains`, and pinning it line by line pins the formatter as much as the
/// code. Collapsing first lets a test quote the expression as it reads.
pub fn squeeze(scanned: &str) -> String {
    let mut out = String::with_capacity(scanned.len());
    let mut in_space = false;
    for c in scanned.chars() {
        if c.is_whitespace() {
            in_space = true;
            continue;
        }
        if in_space && !out.is_empty() {
            out.push(' ');
        }
        in_space = false;
        out.push(c);
    }
    out
}

/// How many times `name` is ASSIGNED in `scanned`.
///
/// **Counting a spaced literal is not counting an assignment.** A test that
/// searched for `"sortStep ="` was defeated by writing `sortStep=0.0f;` one
/// line below the declaration — legal, compiling, and uncaught, because no
/// formatter covers `Runtime/Engine/`. This matches the identifier on its own
/// word boundaries, allows any run of whitespace before the `=`, and refuses
/// `==` so a comparison is not read as a write.
pub fn assignment_count(scanned: &str, name: &str) -> usize {
    let mut count = 0;
    let mut from = 0;
    while let Some(at) = scanned[from..].find(name) {
        let start = from + at;
        let end = start + name.len();
        from = end;

        let before_ok = scanned[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let rest = scanned[end..].trim_start();
        let after_ok = !scanned[end..].starts_with(|c: char| c.is_alphanumeric() || c == '_');

        if before_ok && after_ok && rest.starts_with('=') && !rest.starts_with("==") {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_double_slash_inside_a_string_is_not_a_comment() {
        // The hole that let issue #1317 be restored invisibly: a line-based
        // strip truncated here and hid everything after it.
        let src = "Log(\"see https://x\"); if (!G.flag) { Warn(); }\n";
        let out = blank_comments_and_strings(src);
        assert_eq!(out.len(), src.len(), "offsets must be preserved");
        assert!(out.contains("if (!G.flag)"), "code after a string survives");
        assert!(!out.contains("https"), "the string's body is blanked");
    }

    #[test]
    fn a_comment_cannot_contribute_a_token() {
        let src = "// G.flag\nvar x = 1;\n";
        let out = blank_comments_and_strings(src);
        assert!(!out.contains("G.flag"));
        assert!(out.contains("var x = 1;"));
        assert_eq!(out.len(), src.len());
    }

    #[test]
    fn an_interpolation_hole_is_kept_but_the_prose_is_not() {
        let src = "Warn($\"rung {Rung} target {target} off\");\n";
        let out = blank_comments_and_strings(src);
        assert!(out.contains("{Rung}"), "an interpolated expression is code");
        assert!(out.contains("{target}"));
        assert!(!out.contains("rung "), "the literal prose is blanked");
    }

    #[test]
    fn blanking_only_comments_keeps_the_literal_and_drops_the_quotation_of_it() {
        // The two halves a "is this message written in code" test needs: the
        // comment quoting the message must not satisfy it, and the message must.
        let src = "// Log(\"drew it\");\nLog($\"drew {n} it\");\n";
        let out = blank_comments_only(src);
        assert_eq!(out.len(), src.len(), "offsets must be preserved");
        assert_eq!(
            out.matches("drew").count(),
            1,
            "the comment's copy is gone and the literal's is kept: {out}"
        );
        assert!(out.contains("Log($\"drew {n} it\");"), "{out}");
    }

    /// The escape branch, in both modes.
    ///
    /// `blank` handles `\\` inside a string separately from the ordinary body
    /// byte, and that branch is the one the two modes disagree about. Without
    /// this, making it always blank — reverting `blank_comments_only`'s promise
    /// for exactly this case — left every test green, measured.
    #[test]
    fn an_escape_survives_only_when_the_string_is_kept() {
        let src = "Log($\"a\\tb\");\n";
        let kept = blank_comments_only(src);
        assert!(kept.contains("a\\tb"), "the escape is code here: {kept}");
        assert_eq!(kept.len(), src.len());

        let blanked = blank_comments_and_strings(src);
        assert!(
            !blanked.contains('\\'),
            "the escape is prose there: {blanked}"
        );
        assert_eq!(blanked.len(), src.len());
    }

    /// The doubled-brace branch, in both modes, for the escape branch's reason.
    #[test]
    fn a_doubled_brace_survives_only_when_the_string_is_kept() {
        let src = "Log($\"{{x}} {y}\");\n";
        let kept = blank_comments_only(src);
        assert!(
            kept.contains("{{x}}"),
            "the literal braces are code here: {kept}"
        );
        assert!(
            kept.contains("{y}"),
            "the hole is kept in both modes: {kept}"
        );
        assert_eq!(kept.len(), src.len());

        let blanked = blank_comments_and_strings(src);
        assert!(
            !blanked.contains("{{x}}"),
            "the literal braces are prose there: {blanked}"
        );
        assert!(
            blanked.contains("{y}"),
            "the hole is code in both modes: {blanked}"
        );
        assert_eq!(blanked.len(), src.len());
    }

    #[test]
    fn blanking_only_comments_does_not_read_a_slash_inside_a_string() {
        let src = "Log(\"see https://x\"); var y = 1;\n";
        let out = blank_comments_only(src);
        assert!(out.contains("https://x"), "the string body is kept: {out}");
        assert!(out.contains("var y = 1;"), "code after it survives: {out}");
        assert_eq!(out.len(), src.len());
    }

    #[test]
    #[should_panic(expected = "verbatim strings")]
    fn a_verbatim_string_is_refused_rather_than_mis_read() {
        blank_comments_and_strings("var p = @\"C:\\x\";\n");
    }

    #[test]
    fn a_body_is_bounded_by_its_own_braces() {
        let src = "void A() { int a; if (b) { c(); } }\nvoid B() { dead(); }\n";
        let scanned = blank_comments_and_strings(src);
        let (start, end) = member_body(&scanned, "void A()");
        let body = &scanned[start..=end];
        assert!(body.contains("c();"));
        assert!(!body.contains("dead();"), "a later member is outside");
    }

    #[test]
    fn an_arm_ends_at_the_next_label_not_at_a_return() {
        let src = "case X: doX(); break;\ncase Y: Warn(); return;\n";
        let scanned = blank_comments_and_strings(src);
        let arm = switch_arm(&scanned, "case X:");
        assert!(arm.contains("doX();"));
        assert!(!arm.contains("Warn();"), "the next arm is outside");
    }

    #[test]
    fn squeeze_makes_a_multi_line_expression_one_line() {
        let src = "var x = A(\n    b,\n    c);\n";
        assert_eq!(squeeze(src), "var x = A( b, c);");
    }

    #[test]
    fn squeeze_keeps_no_leading_space() {
        assert_eq!(squeeze("\n   a b\n"), "a b");
    }

    #[test]
    fn an_assignment_is_counted_however_it_is_spaced() {
        let src = "var x = 1; x=2; x  =  3; if (x == 4) {} xs = 5; yx = 6;";
        assert_eq!(assignment_count(src, "x"), 3, "spaced, unspaced and padded");
        assert_eq!(
            assignment_count(src, "xs"),
            1,
            "a longer name is its own identifier"
        );
    }
}
