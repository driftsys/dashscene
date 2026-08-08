# dashscene-staging — task runner
#
# Recipe set mirrors driftsys/git-std's own justfile (house style, see
# docs/decisions/house-style.md), plus two dashscene-specific additions: `wasm`
# (dashc -> wasm32-unknown-unknown, needed by the Deno importer) and the
# `deno-*` recipes scoped to importers/figma/.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Default: list recipes.
default:
    @just --list

# Build every crate in the workspace.
assemble:
    cargo build --workspace

# Run the sanity tier — the loop between edits and before every commit.
# About 5 s. Tier definitions and the schedule: docs/decisions/test-tiers.md.
#
# `cargo test --doc` rides along in every tier recipe because nextest does not
# run doctests. Leaving it out would mean three recipes that each claim to run
# a tier and each silently skip the same three tests.
test:
    cargo nextest run --workspace -P sanity
    cargo test --workspace --doc

# Run the regression tier — what `check`, `build`, `verify`, the pre-push hook
# and the CI `test` job all run. About 33 s. This is the gate; `just test` is
# not.
test-regression:
    cargo nextest run --workspace -P regression
    cargo test --workspace --doc

# Run the calibration tier — the tests that re-derive the committed asset
# tables from the packer alone: one per calibration fixture, plus the band
# contract's own table. About 54 s. Run it when the diff touches a
# path in the `packer` filter of .github/workflows/ci.yml, and again at slice
# close. That filter is the list, and docs/decisions/test-tiers.md enumerates
# it with a reason per entry; a copy of the list here would be a fourth one to
# keep in step, and the partial copies have already drifted three times.
#
# The membership check runs first, and it is the reason the tier cannot rot
# quietly. The profiles select by exact test name, so renaming any one of them
# drops it out of the tier with no error — and no count catches that,
# because the tiers partition the suite and the totals still reconcile.
# Diffing the live listing against the pinned .config/calibration-tier.txt
# does catch it. The CI calibration job runs the same command; keep them
# identical.
#
# The listing is read as JSON, not as `nextest list`'s default output. That
# default is a human-facing rendering with no stability promise — nextest's
# own `list --help` points at `--message-format json` for machine reading —
# and it changed under this check: 0.9.87 printed a binary header with its
# tests indented beneath, 0.9.140 prints one `binary test` line each. CI
# installs whatever `get.nexte.st/latest` currently serves while a developer
# keeps whatever `bootstrap` installed once, so the two sides are routinely
# versions apart and only a machine-readable format makes them agree. The
# same rendering also carries ANSI colour whenever CARGO_TERM_COLOR is set,
# which `dtolnay/rust-toolchain` sets on every CI job.
calibrate:
    cargo nextest list --workspace -P calibration --message-format json \
        | jq -r '."rust-suites" | to_entries[] as $s | $s.value.testcases | to_entries[] | select(.value."filter-match".status == "matches") | "\($s.key) \(.key)"' \
        | LC_ALL=C sort \
        | diff -u .config/calibration-tier.txt -
    cargo nextest run --workspace -P calibration

# The v0 exit gate — story #49, epic #47. Asserts every test covering exit
# criteria E1 to E7 still exists and still passes, so a regression in any one
# of them fails a build rather than being noticed by a person.
#
# The membership check comes first, for the reason the calibration tier's does:
# the profile selects by exact name, so renaming a covering test drops it out
# with no error at all.
#
# This is the LOCAL half. E6 is proven by two suites in two CI jobs that never
# meet, and only the native one is reachable here — the Deno half runs in the
# `deno` job. E7's frames are asserted measured rather than pending by
# no_frame_is_pending_so_e7_is_asserted_over_all_of_them, in the profile. Run
# the CI `exit-gate` job for the whole claim.
exit-gate:
    cargo nextest list --workspace -P exit-gate --message-format json \
        | jq -r '."rust-suites" | to_entries[] as $s | $s.value.testcases | to_entries[] | select(.value."filter-match".status == "matches") | "\($s.key) \(.key)"' \
        | LC_ALL=C sort \
        | diff -u .config/exit-gate.txt -
    cargo nextest run --workspace -P exit-gate
    @echo "exit-gate: E1-E5 and E7 asserted, plus E6's native half."
    @echo "exit-gate: E6's wasm half runs in CI's deno job. See docs/specification/05-qualification.md."

# Every tier in one run.
test-all:
    cargo nextest run --workspace -P all
    cargo test --workspace --doc

# Rust + markdown + Deno lint gate: clippy, cargo fmt check, dprint check,
# markdownlint, deno fmt check.
lint: deno-fmt-check
    cargo clippy --workspace --all-targets -- -D warnings
    # Again for wasm32, because the line above never sees the browser half.
    # `crates/dashscene-web` gates host.rs and document.rs on
    # `target_arch = "wasm32"`, so a host-target clippy compiles neither — and
    # story #741 found two errors sitting in them, carried unchanged from the
    # host they were extracted from. A published crate whose main logic is
    # never linted is what this second line exists to prevent.
    cargo clippy -p dashscene-web --target wasm32-unknown-unknown --all-targets -- -D warnings
    cargo clippy -p demo-web --target wasm32-unknown-unknown --all-targets -- -D warnings
    cargo fmt --all -- --check
    # Intra-doc links, as a gate. A doc comment naming an item that does not
    # exist is this repository's most common defect, and until v0.16 nothing in
    # `just build` could see one: clippy does not resolve doc links, so a link
    # to a deleted function passed the whole gate (story #598 shipped one, and a
    # review agent running `cargo doc` is what found it). `-D warnings` here
    # covers `broken_intra_doc_links` and `private_intra_doc_links`, which is
    # the pair that catches a renamed or removed item.
    #
    # `--no-deps` so it documents this workspace and not its dependency tree.
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --quiet
    dprint check
    markdownlint '**/*.md' --ignore target --ignore node_modules

# Dependency vulnerability audit.
audit:
    cargo audit

# Full non-build verification: the regression tier + lint + audit. Not the
# sanity tier — `check` is what `build` and the pre-push hook run, so it takes
# the tier that is the gate (docs/decisions/test-tiers.md).
check: test-regression lint audit wasm-painter wasm-host

# Everything short of a PR: assemble + check.
build: assemble check

# Run before opening a PR (and as the pre-push hook): commit-message
# lint over commits not yet on origin/main, then build. The range is taken
# against origin/main rather than local main, because local main goes stale
# relative to the remote and would lint commits that are already upstream.
# The range can legitimately be empty (see the recipe) — that is issue
# #110, and it is handled rather than avoided by the choice of ref.
verify:
    #!/usr/bin/env bash
    set -euo pipefail
    # An unresolvable range and an empty range are different, and only the
    # second is benign — so check for origin/main before asking about the
    # range. `git rev-list missing/ref..HEAD` exits non-zero but prints
    # nothing, and `set -e` does not fire inside an `if` condition, so
    # testing the output alone would read a missing ref as "no commits" and
    # skip the lint silently. This gate must fail closed.
    if ! git rev-parse --verify --quiet origin/main >/dev/null; then
        echo "verify: origin/main is missing — run 'git fetch origin'." >&2
        echo "verify: refusing to skip the commit-message lint." >&2
        exit 1
    fi
    # An empty range IS benign: it is what a branch-deletion push, a re-push
    # of an already-pushed branch, and a push from an up-to-date main all
    # look like. `git std lint` exits non-zero on one, which blocked those
    # pushes outright (issue #110). `-n 1` because the question is only
    # whether any commit exists, not what they are.
    if [ -z "$(git rev-list -n 1 origin/main..HEAD)" ]; then
        echo "verify: no commits in origin/main..HEAD — nothing to lint"
    else
        git std lint --range origin/main..HEAD
    fi
    just build

# Reformat everything in place (Rust + markdown).
fmt:
    cargo fmt --all
    dprint fmt

# Open the rustdoc build in a browser.
doc:
    cargo doc --workspace --no-deps --open

# Serve the mdBook docs locally.
book:
    mdbook serve

# Cut a release: git-std bumps versions, writes the changelog, tags.
release:
    git std bump

# Publish every crate to crates.io in dependency order.
publish:
    cargo publish -p dashbuf
    cargo publish -p dashpaint
    cargo publish -p dashscene-core
    cargo publish -p dashscene-typeset
    cargo publish -p dashcue
    cargo publish -p dashscene-engine
    cargo publish -p dashscene-validator
    cargo publish -p dashscene-skia
    cargo publish -p dashlang
    cargo publish -p dashc
    cargo publish -p dashpack-astcenc-sys
    cargo publish -p dashpack
    cargo publish -p dashscene-unity
    cargo publish -p dashscene-gpu
    cargo publish -p dashscene-web
    cargo publish -p dashscene

# Install local toolchain bits (git hooks, git-std, dprint, markdownlint-cli).
install:
    ./bootstrap

# Remove build artifacts.
clean:
    cargo clean

# Build dashc's cdylib for wasm32 — the module the Deno importer loads.
#
# --lib on purpose. Without it, cargo also builds the `dashc` bin for wasm,
# producing a second artifact (dashc.wasm) that is the CLI: it reads files and
# reads the environment, and it exports none of the ABI. Two .wasm files where
# one is a decoy is a trap — the importer loads dashc_wasm.wasm.
wasm:
    cargo build -p dashc --lib --release --target wasm32-unknown-unknown

# Build the lean painter for wasm32 — the target the web host runs on.
#
# A gate rather than an artifact, and the only thing enforcing that no blocking
# wait reaches the web path. `pollster` is a native-only dependency of
# dashscene-gpu, so a `pollster::block_on` reachable from wasm fails to compile
# here. In a browser it would instead deadlock at runtime against the very event
# loop that resolves the promise it waits on — which no native test can catch,
# because natively it works (story #587).
#
# Separate from `wasm`, which several recipes depend on for dashc's module
# alone; folding this in would build the painter for every Deno run.
wasm-painter:
    cargo build -p dashscene-gpu --target wasm32-unknown-unknown

# Build the browser host for wasm32 — a gate, like `wasm-painter`.
#
# Separate from it because they fail for different reasons: that one catches a
# blocking wait reaching the web path, this one catches the host itself, whose
# browser half compiles on no other target and would otherwise be checked by
# nothing until someone opened a page.
wasm-host:
    cargo build -p demo-web --target wasm32-unknown-unknown

# Assemble the browser host into `target/web`, ready to serve.
#
# `wasm-bindgen` post-processes the module cargo produced into the JS glue a
# page imports. The CLI's version and the `wasm-bindgen` crate's are two halves
# of one ABI: a mismatch fails in the browser rather than at build time, so the
# pair is checked here instead of being discovered there.
#
# The CLI is not installed by `bootstrap`. It builds from source in minutes and
# is needed only by this demonstration, so every clone paying for it would be
# the wrong trade; the check below prints the exact command instead.
web-build:
    #!/usr/bin/env bash
    set -euo pipefail
    locked=$(awk '/^name = "wasm-bindgen"$/{found=1; next} found && /^version = /{gsub(/"/, "", $3); print $3; exit}' Cargo.lock)
    if ! command -v wasm-bindgen >/dev/null 2>&1; then
      echo "web-build: wasm-bindgen is not installed" >&2
      echo "web-build:   cargo install wasm-bindgen-cli --version ${locked}" >&2
      exit 1
    fi
    have=$(wasm-bindgen --version | awk '{print $2}')
    if [ "${have}" != "${locked}" ]; then
      echo "web-build: wasm-bindgen ${have} does not match the ${locked} crate" >&2
      echo "web-build: the two are one ABI, and a mismatch fails in the browser" >&2
      echo "web-build:   cargo install wasm-bindgen-cli --version ${locked}" >&2
      exit 1
    fi
    cargo build -p demo-web --release --target wasm32-unknown-unknown
    wasm-bindgen --target web --no-typescript \
      --out-dir target/web \
      target/wasm32-unknown-unknown/release/demo_web.wasm
    cp demo-web/index.html target/web/index.html
    # The documents the page can load. Copied rather than served from the
    # repository root, so the served tree holds what the demonstration needs
    # and not the whole working copy.
    mkdir -p target/web/goldens/dsb
    cp goldens/dsb/*.dsb target/web/goldens/dsb/
    echo "web-build: target/web is ready — 'just web' serves it"

# Serve the browser host on 127.0.0.1, with byte ranges honoured.
#
# The server is `demo-web/serve.py` rather than `python3 -m http.server`, which
# does not implement `Range`. Without ranges the host still draws — it notices
# the whole file arrived — but the prefix loading this story exists to
# demonstrate never happens.
web port="8787": web-build
    python3 demo-web/serve.py target/web {{ port }}

# Type-check the Deno importer's entry points.
deno-check:
    cd importers/figma && deno task check

# Run the Deno importer's test suite. Depends on `wasm`: the suite loads
# dashc_wasm.wasm and asserts its output against the golden .dsb.
deno-test: wasm
    cd importers/figma && deno task test

# Format the Deno importer sources.
deno-fmt:
    cd importers/figma && deno task fmt

# Check the Deno importer sources are already formatted, without rewriting
# them. Matches the CI deno job's formatting gate (.github/workflows/ci.yml);
# `deno-fmt` alone cannot fail, so this is the recipe that actually gates it.
deno-fmt-check:
    cd importers/figma && deno task fmt --check

# Capture the Figma fixture corpus, image-fill bytes included. Needs
# FIGMA_TOKEN (docs/decisions/figma-access-plan-and-pat-policy.md). Never commit the token.
deno-capture: wasm
    cd importers/figma && deno task capture

# Empirical import probe: rebuilds wasm, then runs the Deno importer against a
# live Figma file and prints the sorted, unique diagnostics — blockers on a
# refused import, or whatever warnings (e.g. skipped-node `figma.unsupported`
# under partial-emit) rode along on a successful one — the harness the
# full-real-file-import epic re-runs every wave to re-derive the frontier
# (docs/wip/2026-07-18-epic-full-real-file-import.md). Reads FIGMA_TOKEN from
# the macOS keychain (`security add-generic-password -a "$USER" -s figma-pat
# -w <token>`); the token is read, never printed — only its length. `root` is
# optional: with none, the importer lists the file's declarable roots instead
# of guessing one. The compiled `.dsb` lands at /tmp/reprobe.dsb, outside git
# — public Figma files are live-only content, never committed.
#
# Epic targets:
#   just reprobe MRk9I5cYY6yJa8JhljzkBn 2411:10795  # first-light
#   just reprobe S30AJmYfnDKGeSQmzuXEUk              # hero: root TBD — run
#                                                     # rootless first to list
#                                                     # declarable roots, then
#                                                     # rerun with the chosen
#                                                     # --root
reprobe key root="": wasm
    #!/usr/bin/env bash
    set -euo pipefail
    token=$(security find-generic-password -a "$USER" -s figma-pat -w)
    export FIGMA_TOKEN="$token"
    echo "reprobe: FIGMA_TOKEN loaded (${#token} chars)" >&2

    root_flag=""
    if [ -n "{{root}}" ]; then
        root_flag="--root {{root}}"
    fi

    tmp_dsb="importers/figma/.reprobe-tmp.dsb"
    # The importer writes this sidecar itself, next to `-o`'s output
    # (`sidecarPath` in import.ts: `<out minus .dsb>.vars.json`) — not written
    # or read by this recipe, only cleaned up alongside the .dsb.
    tmp_vars="importers/figma/.reprobe-tmp.vars.json"
    err_file="$(mktemp)"
    trap 'rm -f "$tmp_dsb" "$tmp_vars" "$err_file"' EXIT

    set +e
    (cd importers/figma && deno task import "{{key}}" $root_flag -o .reprobe-tmp.dsb) \
        >/dev/null 2>"$err_file"
    status=$?
    set -e

    # Every diagnostic this pipeline can raise — the Deno closure's own
    # (figma.closure.*) and dashc's (figma.unsupported, figma.no-content, the
    # validator's Report) — formats as `severity[rule]: message`. A blocking
    # one rides an Error's `.message`, so an uncaught one prints inside
    # Deno's own "Uncaught (in promise) <Name>: ..." wrapper; a non-blocking
    # warning on a successful (partial) emit prints as its own plain line via
    # `deps.error`. Either way strip ANSI color codes, then pull that shape
    # wherever it sits in the line. `|| true`: with no such line, grep's
    # no-match exit code is 1, and `pipefail` would otherwise carry that into
    # the assignment and abort the script under `set -e`.
    extract_diagnostics() {
        sed -E 's/\x1b\[[0-9;]*m//g' "$err_file" \
            | grep -oE '(error|warning)\[[^]]+\]: .*$' \
            | sort -u || true
    }

    # `trimmed: <type> "<name>" (<id>) — <reason>` lines (import.ts's
    # `reportTrim`), one per subtree the pre-closure trim pass removed — a
    # separate shape from `extract_diagnostics` above (no `severity[rule]:`
    # prefix), so it was previously invisible in the frontier report even
    # though the removal is named (P4). Surfaced on both the emitted and the
    # blocked path, since trim runs before the closure either way.
    extract_trimmed() {
        sed -E 's/\x1b\[[0-9;]*m//g' "$err_file" \
            | grep -oE '^trimmed: .*$' || true
    }

    report_trimmed() {
        local trimmed
        trimmed=$(extract_trimmed)
        if [ -n "$trimmed" ]; then
            local count
            count=$(echo "$trimmed" | wc -l | tr -d ' ')
            echo "TRIMMED — ${count} subtree(s) removed before the closure:"
            echo "$trimmed"
        else
            echo "(nothing trimmed)"
        fi
    }

    if [ "$status" -eq 0 ]; then
        cp "$tmp_dsb" /tmp/reprobe.dsb
        size=$(wc -c < /tmp/reprobe.dsb | tr -d ' ')
        echo "EMITTED — wrote /tmp/reprobe.dsb (${size} bytes)"
        report_trimmed
        diagnostics=$(extract_diagnostics)
        if [ -n "$diagnostics" ]; then
            echo "$diagnostics"
        else
            echo "(no diagnostics)"
        fi
        exit 0
    fi

    report_trimmed
    blockers=$(extract_diagnostics)
    if [ -n "$blockers" ]; then
        echo "$blockers"
    else
        echo "reprobe: no structured blocker diagnostics found (exit ${status}) — raw stderr:" >&2
        sed -E 's/\x1b\[[0-9;]*m//g' "$err_file"
    fi
    exit "$status"

# Live render: import a Figma file to a .dsb, render it through the v0 Skia
# reference painter, and write a PNG to /tmp for review — the "renders through
# Skia" half of the full-real-file-import exit criterion (story Sf-1,
# docs/wip/2026-07-18-render-dsb-design.md). Depends on `wasm` (the importer
# loads dashc_wasm.wasm). Reads FIGMA_TOKEN from the macOS keychain
# (`security add-generic-password -a "$USER" -s figma-pat -w <token>`); the
# token is read, never printed — only its length. `root` is optional. Public
# Figma files are live-only: the .dsb and .png land in /tmp, never committed;
# the in-scope scratch is cleaned on exit, like reprobe's.
#
# `profile` is the Gfx QA profile preview (story #435): raw, hifi or lite. RAW
# is the null binding and renders the file unchanged, which is what makes it the
# reference arm rather than a fourth thing; hifi and lite pack the document's
# assets under that quality profile in memory, assemble the derived bank, and
# software-decode its block payloads back to RGBA before the painter sees them.
# The painter is unchanged and still only draws RGBA. The output PNG is named
# after the profile, so the three can be compared side by side.
#
# What this view cannot show, so a target bench confirms a short list rather
# than discovering quality: GPU filtering behaviour, driver-level effects
# (vendor bandwidth compression such as UBWC, and the NVIDIA case where ASTC is
# emulated rather than sampled natively), and where in a target pipeline the
# sRGB transfer function is applied.
#
# Epic targets:
#   just render MRk9I5cYY6yJa8JhljzkBn 2411:10795  # first-light
#   just render S30AJmYfnDKGeSQmzuXEUk 1973:6580    # hero
#   just render S30AJmYfnDKGeSQmzuXEUk 1973:6580 lite   # the same file under Lite
render key root="" profile="raw": wasm
    #!/usr/bin/env bash
    set -euo pipefail
    token=$(security find-generic-password -a "$USER" -s figma-pat -w)
    export FIGMA_TOKEN="$token"
    echo "render: FIGMA_TOKEN loaded (${#token} chars)" >&2

    root_flag=""
    if [ -n "{{root}}" ]; then
        root_flag="--root {{root}}"
    fi

    tmp_dsb="importers/figma/.render-tmp.dsb"
    # The importer writes a sidecar next to `-o`'s output
    # (`<out minus .dsb>.vars.json`) — cleaned alongside the .dsb, not read here.
    tmp_vars="importers/figma/.render-tmp.vars.json"
    trap 'rm -f "$tmp_dsb" "$tmp_vars"' EXIT

    (cd importers/figma && deno task import "{{key}}" $root_flag -o .render-tmp.dsb)

    cp "$tmp_dsb" /tmp/render.dsb
    dsb_size=$(wc -c < /tmp/render.dsb | tr -d ' ')
    echo "render: imported /tmp/render.dsb (${dsb_size} bytes)" >&2

    out="/tmp/render-{{profile}}.png"
    cargo run --quiet -p goldens --bin render-dsb -- \
        /tmp/render.dsb "$out" --profile {{profile}}
    png_size=$(wc -c < "$out" | tr -d ' ')
    echo "RENDERED — wrote ${out} (${png_size} bytes, profile {{profile}})"

# The Gfx QA triptych: every corpus scene rendered under RAW, HiFi and LoFi,
# with a diff heatmap per production arm and the banded numbers printed (story
# #435). Runs the profile-preview oracle, which measures every arm against its
# pinned scene band and writes the artifacts as it goes. Each arm also carries
# its SSIMULACRA2, FLIP and PSNR figures (issue #544).
#
# The triptych is written rather than committed: a committed render of a scene
# that exists to show codec loss would have to be re-baselined for every
# unrelated painter change. The durable record is the measured numbers in
# goldens/oracle/profile-manifest.json, which the oracle asserts.
#
# VIEWING CONDITIONS. If a person is shown these images, the viewing conditions
# decide the answer as much as the codec does, so they are stated here rather
# than left to whoever opens the files.
#
#   - Native pixels. No browser zoom, no display scaling, no window that
#     resizes the image. Smooth scaling averages block artifacts away, so a
#     viewer who rescales is reporting on the resampler and not on the codec.
#   - Integer nearest-neighbour if any zoom is needed at all.
#   - Blind and randomised order if the opinion is to mean anything. A reviewer
#     who knows which arm is LoFi is not answering the question being asked.
#   - The full ladder rather than three points. The useful question is where
#     loss becomes visible, not whether three named arms differ; the per-rung
#     figures are in goldens/tooling/tests/perceptual_calibration.rs.
#   - ITU-R BT.500 and ITU-T P.910 are the standard protocols for running this
#     properly. Nothing here implements one.
triptych:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo test -p goldens --test profile_preview_oracle -- --nocapture \
        | grep -E "^PROFILE PREVIEW|^test result"
    echo
    echo "TRIPTYCH — wrote target/profile-preview/<scene>/{raw,hifi,lofi}.png"
    echo "           and {hifi,lofi}-heat.png beside them"
    echo
    echo "VIEWING CONDITIONS — native pixels, no browser or display scaling."
    echo "  Smooth scaling averages block artifacts away, so a rescaled view"
    echo "  reports on the resampler rather than on the codec. Nearest-neighbour"
    echo "  at integer factors if zoom is needed. Blind, randomised order if the"
    echo "  opinion is to mean anything. See the recipe comment for the full"
    echo "  rules and for ITU-R BT.500 / ITU-T P.910."
