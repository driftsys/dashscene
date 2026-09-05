//! What four records say `just unity-render`'s order phase does, held to the
//! file it does it in — and to the recipe that stages its fixture.
//!
//! **The gate is in the class issue #1350 describes**: no CI job compiles
//! `unity/render-gate/DashsceneRenderGate.cs`, so the order phase could be
//! unhooked, a probe renamed, or the control loosened, and
//! `docs/specification/07-embedding-and-distribution.md` (R-E22, "Met since
//! issue #1402"), `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`
//! D1, `docs/design/unity-csharp-host.md` and
//! `docs/technotes/batch-renderer-group.md` §5b would go on claiming it.
//! `editor_gate_claims.rs` is the precedent: text over a file no CI job
//! compiles, which is what `unity/package-gate` is for.
//!
//! **What a text scan holds, and how.** A branch is held by its body, located
//! by walking the braces after its condition, so a `Fail` inside it cannot be
//! swapped for a log line; the probe names are read from the `probes`
//! initialiser with comments stripped and strings kept, so a name left in a
//! comment satisfies nothing; the recipe's staging is held by its `cp` lines'
//! sources and destinations, not by the paths' other mentions. What it does
//! not hold: the seven predicates' arithmetic, and the gate's behaviour on a
//! rung-3 painter or a throwing `SetAtlases` — those run only under an editor.

use package_gate::cs_scan::{blank_comments_and_strings, member_body, squeeze};

const GATE: &str = "unity/render-gate/DashsceneRenderGate.cs";
const BUILD: &str = "unity/render-gate/RenderGateBuild.cs";

/// The seven probe names the records quote, in the gate's own order.
const PROBES: [&str; 7] = [
    "backdrop",
    "fill",
    "regular-glyph",
    "veil-over-backdrop",
    "veil-over-fill",
    "veil-over-regular-glyph",
    "bold-glyph",
];

/// What the `unity-render` recipe copies for the order phase, as (a token of
/// the source, the destination under `StreamingAssets`): the fixture and the
/// two faces, each with its sheet.
const STAGED: [(&str, &str); 7] = [
    ("${order}", "StreamingAssets/order.dsb"),
    (
        "Inter-Regular.otf",
        "StreamingAssets/cascade/Inter-Regular.otf",
    ),
    ("Inter-Bold.otf", "StreamingAssets/cascade/Inter-Bold.otf"),
    ("inter-ascii/atlas.png", "cascade/regular/atlas.png"),
    ("inter-ascii/atlas.metrics", "cascade/regular/atlas.metrics"),
    ("inter-ascii-bold/atlas.png", "cascade/bold/atlas.png"),
    (
        "inter-ascii-bold/atlas.metrics",
        "cascade/bold/atlas.metrics",
    ),
];

fn read(relative: &str) -> String {
    let path = package_gate::root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The brace-delimited block that follows `needle`, braces included.
fn block_after<'a>(text: &'a str, needle: &str, what: &str) -> &'a str {
    let start = text
        .find(needle)
        .unwrap_or_else(|| panic!("{what}: no `{needle}`"));
    let open = start
        + text[start..]
            .find('{')
            .unwrap_or_else(|| panic!("{what}: `{needle}` opens no block"));
    let mut depth = 0usize;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("{what}: the block after `{needle}` never closes")
}

/// C# with its comments removed and its strings kept — the reverse of
/// `blank_comments_and_strings`, for the one scan that needs the literals.
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(n) = next {
                    out.push(n);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// The plan's end reaches the order phase, the order phase is judged over
/// every probe, its guards stand, and the control fails the run on a single
/// probe the undrawn frame satisfies.
#[test]
fn the_order_phase_is_reached_judged_and_controlled_per_probe() {
    let scanned = blank_comments_and_strings(&read(GATE));
    let (open, close) = member_body(&scanned, "bool Advance(");
    assert!(
        squeeze(&scanned[open..=close]).contains("if (!BeginOrder())"),
        "{GATE}: `Advance` no longer begins the order phase, so the plan ends without it \
         and every record claiming the order is pinned on every run is wrong."
    );
    let (open, close) = member_body(&scanned, "void Update(");
    assert!(
        squeeze(&scanned[open..=close]).contains("JudgeOrder();"),
        "{GATE}: `Update` no longer judges the order frame."
    );

    let (open, close) = member_body(&scanned, "void JudgeOrder(");
    let judge = squeeze(&scanned[open..=close]);
    let control = block_after(&judge, "if (controlHolds.Count != 0)", GATE);
    assert!(
        control.contains("Fail(") && control.contains("return;"),
        "{GATE}: the order phase's control no longer fails the run on a probe the undrawn \
         frame satisfies — its branch is `{control}`. A first version failed only when all \
         seven held, which two contradictory probes made unreachable."
    );
    assert_eq!(
        judge.matches("foreach (var probe in probes)").count(),
        3,
        "{GATE}: `JudgeOrder` walks the probes other than three times — the fixture \
         cross-check, the control and the verdict — so one of them reads a subset."
    );
    assert!(
        judge.contains("if (holds != probes.Length)"),
        "{GATE}: the verdict no longer requires every probe to hold on the order frame."
    );
    assert!(
        control.contains("Fail(")
            && judge.contains("Read(control, ToViewport(probe.Document))")
            && judge.contains("controlHolds.Add(probe.Name);"),
        "{GATE}: the control no longer reads the CONTROL frame at each probe, so it would judge \
         the drawn frame's negation instead of the undrawn frame."
    );
    for guard in [
        "regularAtlas == boldAtlas",
        "packer.InstanceAtlas[",
        "regularIndices.Max() < veil",
        "veil < boldIndices.Min()",
    ] {
        assert!(
            judge.contains(guard),
            "{GATE}: `JudgeOrder` no longer holds `{guard}` — the two runs' atlases (R-E22) \
             and the glyph runs' places in the emission order are what the glyph probes \
             are written against."
        );
    }
}

/// The seven probes keep the names the records quote, and there are seven —
/// read from the initialiser, with comments stripped.
#[test]
fn the_seven_probes_keep_their_names() {
    let code = without_comments(&read(GATE));
    let probes = block_after(&code, "var probes = new[]", GATE);
    for probe in PROBES {
        assert!(
            probes.contains(&format!("Name = \"{probe}\"")),
            "{GATE}: the `probes` initialiser names no order probe `{probe}`; the records \
             quote it."
        );
    }
    assert_eq!(
        probes.matches("new OrderProbe").count(),
        PROBES.len(),
        "{GATE}: the `probes` initialiser constructs a different number of order probes \
         than the seven the records count."
    );
}

/// The recipe copies the fixture and both faces with their sheets into the
/// player's `StreamingAssets`, which is what makes the bold glyphs a second
/// text material.
#[test]
fn the_recipe_stages_the_fixture_and_both_faces() {
    let code = package_gate::recipe_code(&read("justfile"), "unity-render");
    for (source, destination) in STAGED {
        let copied = code.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("cp ") && line.contains(source) && line.contains(destination)
        });
        assert!(
            copied,
            "the `unity-render` recipe no longer copies `{source}` to `{destination}`, so \
             the order phase would load nothing, or one sheet."
        );
    }
}

/// The veil's lower bound and the camera's clear colour are two literals in
/// two files; a change to either moves what the per-probe control tests.
#[test]
fn the_veil_bound_and_the_clear_colour_stay_equal() {
    assert!(
        read(GATE).contains("MidLow = 0.15f;"),
        "{GATE}: `MidLow` is no longer 0.15, the clear colour's red and green; the \
         constants' doc says why they are held together."
    );
    assert!(
        read(BUILD).contains("new Color(0.15f, 0.15f, 0.18f, 1.0f)"),
        "{BUILD}: the camera's clear colour moved; `MidLow` in {GATE} is written against \
         it."
    );
}
