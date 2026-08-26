# dashscene — task runner
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

# About 5 s. Tier definitions and the schedule: docs/decisions/test-tiers.md.
#
# `cargo test --doc` rides along in every tier recipe because nextest does not
# run doctests. Leaving it out would mean three recipes that each claim to run
# a tier and each silently skip the same three tests.
# Run the sanity tier — the loop between edits and before every commit.
test:
    cargo nextest run --workspace -P sanity
    cargo test --workspace --doc

# About 33 s locally when warm, 154 s for the whole workspace suite. This is the
# gate; `just test` is not, and the pre-push hook no longer runs it either
# (see `verify`).
# Run the regression tier — what `check`, `build` and the CI `test` job run.
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
# Run the calibration tier — re-derives the committed asset tables.
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
# Run the exit-gate tier — E1-E5, E7 and E6's native half.
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

# Rust + markdown + Deno lint gate: clippy, cargo fmt check, prim, deno fmt
# check.
# Lint everything: clippy, rustfmt, doc-links, prim and the Deno sources.
lint: deno-fmt-check
    cargo clippy --workspace --all-targets -- -D warnings
    # The `gpu-timing` cfg population, which the line above never compiles.
    # Its own recipe because CI's `clippy` job has to run exactly it — see
    # `gpu-timing-lint`.
    just gpu-timing-lint
    # The wasm32 half, which the line above never sees. Its own recipe rather
    # than four more lines here, because CI has to run exactly it and a second
    # copy in YAML is the drift this repository keeps hitting. Called from the
    # body rather than taken as a dependency so the host pass — the fastest, and
    # the one most changes fail on — still reports first.
    just wasm-lint
    cargo fmt --all -- --check
    # Intra-doc links. Its own recipe for the same reason `wasm-lint` and
    # `prim` are: CI's `clippy` job runs exactly it, and a second copy in YAML
    # is the drift this repository keeps hitting.
    just doc-links
    # Markdown, JSON, YAML and TOML. Its own recipe for the same reason
    # `wasm-lint` is one: CI's `prim` job runs exactly it, and a second copy in
    # YAML is the drift this repository keeps hitting.
    just prim

# Intra-doc links, as a gate — the HOST target.
#
# A doc comment naming an item that does not exist is this repository's most
# common defect, and until v0.16 nothing in `just build` could see one: clippy
# does not resolve doc links, so a link to a deleted function passed the whole
# gate (story #598 shipped one, and a review agent running `cargo doc` is what
# found it). `-D warnings` turns every rustdoc lint into an error here, which
# is more than the two this comment used to name: `redundant_explicit_links`
# reaches the gate as well, and `broken_intra_doc_links` covers an ambiguous
# name ("both a function and a module") as well as an absent one. Both shapes
# are among the twelve corrections issue #1046 needed.
#
# **This recipe is one target's pass, and no target's pass is the whole gate.**
# An item behind a `cfg` this build does not satisfy is absent, not private, so
# rustdoc never reads its doc comment and `--document-private-items` does not
# help. `wasm-lint` and `android-lint` carry the pass for their own triples,
# the same split clippy already has, and issue #1109 is where that came from —
# it was not theoretical, seven broken links were sitting behind those two
# cfgs, six on wasm32 and one on android.
#
# **A fourth population has no pass at all, and it is not a target:**
# `cfg(target_os = "macos")`. This recipe runs on whatever host invokes it, so
# CI resolves macOS-gated doc comments on no runner it has —
# `dashscene-gpu/src/surface.rs` carries four such items — and only a macOS
# developer's pre-push hook reads them. Naming it here rather than implying
# that three passes are a partition.
#
# `--no-deps` so it documents this workspace and not its dependency tree.
#
# `--document-private-items` so the pass reads what rustdoc otherwise strips
# before resolving links. Do not reduce that to "private items": turning it on
# failed on TWELVE links across SEVEN crates that this gate had been passing,
# and they arrived by at least three different routes — a doc comment on a
# genuinely private item; a public item defined in a private module,
# re-exported, whose links are resolved only once that module is documented
# (`Waiver::rule` and `Easing` are both public and both were unchecked); and a
# name that is unambiguous only while the private module sharing it is
# stripped, which is where `emit` and `triage` came from. Issue #1046 carries
# the list.
#
# It was measured to cost nothing on the other half: `private_intra_doc_links`
# still fails a public doc that links to a private item, with the flag exactly
# as without it. Only rustdoc's explanatory note differs.
#
# `--keep-going` so one run reports every crate. Cargo otherwise stops
# scheduling work after the first failure, and both prior measurements of this
# command — issue #1046's own, and the one taken when it was scheduled —
# reported partial and mutually disjoint error sets because of it. A gate that
# under-reports is what sends someone round the loop twice.
#
# **What no target's pass reaches: `#[cfg(test)]`.** rustdoc does not compile
# with `--cfg test`, so a doc comment inside a test module is read by nothing,
# and `--document-private-items` does not help — those items are absent, not
# private. There is no repair available: `RUSTDOCFLAGS='--cfg test'` stops four
# crates compiling outright (E0432/E0433 — a doc unit does not link
# dev-dependencies) and `cargo doc` has no `--tests` selector. Measured under
# issue #1116, which records it rather than leaving it to be rediscovered.

# The flags the three GATE passes share — this recipe, `wasm-lint` and
# `android-lint`, which spells it twice. Written out in full in the first draft
# of this change, two copies had already drifted before it was reviewed, which
# is the same duplication these recipes exist to avoid.
#
# `doc` deliberately does NOT take this: it drops `--keep-going`, because a
# person waiting at a browser is better served by the first failure than by all
# of them. Its other flags must stay in step with these, and the reason is in
# its own comment.
DOC_FLAGS := "--no-deps --document-private-items --keep-going --quiet"

# The intra-doc-link gate, on the host triple.
doc-links:
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace {{ DOC_FLAGS }}

#
# `fmt --check` and `lint` are not redundant. `prim lint` reports format drift
# for JSON, YAML and TOML but NOT for Markdown: it exits 0 on a Markdown file
# that `prim fmt --check` rejects, so dropping the first verb would leave every
# Markdown file in the tree ungated for format. `prim lint` adds the content
# rules — a reference-style link with no definition, an empty link, a bare URL,
# a heading anchor collision — which no formatter can see. It does NOT check
# that a relative link resolves:
# prim passes rumdl no source path, so the rules that would need one are inert.
# markdownlint checked no such thing either, so nothing is lost.
#
# Both read only this repository, so prim is PINNED, in `bootstrap` and in the
# `prim` CI job. See the `audit` job for the opposite case.
# The Markdown/JSON/YAML/TOML gate, in the two verbs it takes.
prim:
    prim fmt --check .
    prim lint .

# Assert every Figma fixture is explicitly link-viewable.
#
# The corpus publishes 32 Figma file keys and a key cannot be rotated, so what
# each file exposes is a published, permanent property. `linkAccess` is the
# field that answers it, and the capture tooling already commits it — but a
# capture is a snapshot, so this asks Figma rather than the fixture.
#
# `inherit` FAILS. It is not a value: it means the answer lives in a project
# setting outside this repository, invisible to the corpus and changeable by
# anyone with project admin without a commit here. Explicit is the only state
# a reader of this repository can verify.
#
# Not part of `check`: it needs a PAT and the network. Run it before
# publication, and after touching fixture sharing.
#
# Needs FIGMA_TOKEN, or the `figma-pat` keychain item — the convention in
# docs/decisions/figma-access-plan-and-pat-policy.md. Paced, because the API
# rate-limits at roughly 20 file reads in quick succession.

# Assert every Figma fixture is explicitly link-viewable (needs a PAT).
figma-sharing:
    #!/usr/bin/env bash
    set -euo pipefail
    # FIGMA_TOKEN and `-a "$USER" -s figma-pat`, the convention every other
    # Figma path here uses (`reprobe`, `render`, `deno-capture`, and
    # importers/figma/src/fetch.ts) — see
    # docs/decisions/figma-access-plan-and-pat-policy.md.
    tok="${FIGMA_TOKEN:-$(security find-generic-password -a "$USER" -s figma-pat -w 2>/dev/null || true)}"
    if [ -z "$tok" ]; then
        echo "figma-sharing: ABORT — no token. Export FIGMA_TOKEN, or add the figma-pat keychain item."
        exit 1
    fi

    # Read the manifest into a file FIRST. A `while read < <(jq ...)` discards
    # jq's exit status, so a missing or renamed manifest yields zero iterations
    # and the recipe reports "all 0 fixtures explicitly link-viewable", exit 0 —
    # the same fail-open `just secrets` closed by refusing an unusable input.
    manifest=corpus/figma-fixtures/manifest.json
    if [ ! -r "$manifest" ]; then
        echo "figma-sharing: ABORT — $manifest is missing or unreadable."
        exit 1
    fi
    rows=$(mktemp) && trap 'rm -f "$rows" "$rows.body"' EXIT
    if ! jq -r '.fixtures[] | "\(.name)\t\(.fileKey)"' "$manifest" > "$rows" 2>/dev/null; then
        echo "figma-sharing: ABORT — $manifest has no readable .fixtures list."
        exit 1
    fi
    expected=$(jq -r '.fixtures | length' "$manifest")
    rows_n=$(wc -l < "$rows" | tr -d ' ')
    if [ "$rows_n" -eq 0 ] || [ "$rows_n" -ne "$expected" ]; then
        echo "figma-sharing: ABORT — the manifest yielded $rows_n rows for $expected fixtures."
        exit 1
    fi
    # Coverage is the manifest, and that is the right scope rather than a
    # shortcut: a key is published only by being listed here, so a fixture
    # dropped from the manifest publishes no key and needs no check. What must
    # not happen is prose elsewhere fixing the count — say "every fixture in
    # the manifest", never "all 32", or the documents drift the moment the
    # corpus grows.
    echo "figma-sharing: checking $expected fixtures (about $((expected * 4))s)…"

    bad=0; checked=0
    while IFS=$'\t' read -r name key; do
        [ "$checked" -gt 0 ] && sleep 4
        code=$(curl -s --fail-with-body --max-time 30 -o "$rows.body" -w '%{http_code}' \
                 -H "X-Figma-Token: $tok" "https://api.figma.com/v1/files/${key}?depth=1" || true)
        if [ "$code" != "200" ]; then
            echo "figma-sharing: ABORT at $name — HTTP $code"
            echo "  $(head -c 200 "$rows.body" 2>/dev/null)"
            echo "  $checked of $expected checked before this; the rest are unknown."
            rm -f "$rows.body"; exit 1
        fi
        read -r la err < <(jq -r '[.linkAccess // "", .err // ""] | @tsv' "$rows.body")
        if [ -n "${err:-}" ]; then
            echo "figma-sharing: ABORT at $name — Figma said: $err"
            rm -f "$rows.body"; exit 1
        fi
        checked=$((checked+1))
        if [ "${la:-}" != "view" ]; then
            printf '  %-26s %s\n' "$name" "${la:-<no linkAccess field>}"
            bad=$((bad+1))
        fi
    done < "$rows"
    rm -f "$rows.body"

    if [ "$checked" -ne "$expected" ]; then
        echo "figma-sharing: ABORT — checked $checked of $expected."
        exit 1
    fi
    if [ "$bad" -gt 0 ]; then
        echo "figma-sharing: $bad of $checked fixtures are not explicitly 'view' (above)."
        echo "  Set each in Figma: Share -> Anyone -> View -> Save."
        exit 1
    fi
    echo "figma-sharing: all $checked fixtures explicitly link-viewable"

#
# Cargo packages only files under a crate's own directory, so the root copies
# reach no `.crate`. Apache-2.0 §4(a) and §4(d) require both to travel with the
# code — §4(d) in particular carries Arm's attribution for the vendored astcenc
# sources. `every_publishable_crate_packages_the_licence_and_notice` fails the
# build if a copy goes missing or drifts.
#
# **The UPM package takes the same pair and is not a crate.** It is distributed
# on its own, by Git URL, so §4 binds it exactly as it binds a `.crate`; it is
# outside `crates/`, so the loop below cannot reach it and neither can the test
# named above, which iterates workspace members. It is copied explicitly rather
# than by widening the glob, because `unity/abi-check` is not distributed and
# must not receive one.
# Copy the root LICENSE and NOTICE into every publishable crate and the UPM package.
licenses:
    #!/usr/bin/env bash
    set -euo pipefail
    n=0
    for c in crates/*/; do
        grep -q '^publish = false' "$c/Cargo.toml" && continue
        cp LICENSE "$c/LICENSE"
        cp NOTICE "$c/NOTICE"
        n=$((n+1))
    done
    cp LICENSE unity/com.driftsys.dashscene/LICENSE
    cp NOTICE unity/com.driftsys.dashscene/NOTICE
    echo "licenses: refreshed LICENSE and NOTICE in $n crates and the UPM package"

# Dependency vulnerability audit.
audit:
    cargo audit

# Secret scan — gitleaks over HEAD and history, plus a pattern-grep backstop.
secrets range="--all":
    #!/usr/bin/env bash
    set -euo pipefail

    # SCOPE. `range` is the history the two history-wide gates read. It defaults
    # to `--all`, the publication gate: every object any ref can reach. CI runs
    # that unscoped, on every pull request and on every push to `main`.
    #
    # The pre-push hook passes `--all --not --remotes`: every object reachable
    # from a local ref that is NOT yet on any remote. That is the set a push can
    # publish, and it is 168 objects here against 10,151 — the whole of the
    # gate's cost was streaming the rest through `git cat-file`, about 18 s of
    # its 21.5 s.
    #
    # NOT `origin/main..HEAD`, which was the first attempt and is fail-open:
    # `git push origin some-other-branch`, or any push while HEAD sits at
    # origin/main, makes that range empty and both gates then pass over zero
    # objects while printing a clean verdict. `--not --remotes` does not depend
    # on which branch is checked out. Verified by planting a Figma PAT on a
    # branch and scanning from a different one.
    #
    # Narrowing cannot weaken either gate's verdict for the objects it does
    # read: both compute "findings minus the triaged baseline" and fail on any
    # remainder, so a smaller range yields a subset and the same predicate. What
    # it gives up is re-detection of something already published, which is CI's
    # job and not a pushing developer's.
    #
    # `$range` is deliberately unquoted at the `git rev-list` call because it
    # carries multiple words (`--all --not --remotes`). It is a recipe argument
    # from a developer's own command line or from the hook, not untrusted input.
    range="{{ range }}"
    if [ "$range" = "--all" ]; then
        scope_label="every object that would be published"
    else
        scope_label="$range — the objects this push adds"
    fi

    # WHY TWO GATES. gitleaks carries the rule set and the triaged ignore
    # files. The grep is a deterministic backstop over every object in
    # history, because gitleaks 8.30.1's defaults were measured on 2026-08-10
    # to miss a correctly-shaped AWS access key ID and to carry no Figma PAT
    # rule at all — both of which this repository can plausibly contain.
    #
    # TWO REPORTING BUGS in 8.30.1: the history scan prints `0 commits
    # scanned` and leaves `.Commit` empty. Neither means the scan did not run.
    #
    # EVERY GATE HERE FAILS CLOSED. That is the whole point of the recipe, and
    # it is the part that was wrong twice. A command substitution takes the
    # status of its *last* command, so `n=$(pipeline; cat f)` hides a failing
    # pipeline from `set -e` entirely; and `|| true` after a scanner turns a
    # crashed scanner into a clean result. Both shapes are banned below. When
    # changing this recipe, test the failure paths — a broken git, a crashed
    # gitleaks, a shallow clone — not only the happy path.

    # ONE trap, one directory, every temporary inside it. Bash *replaces* an
    # EXIT handler rather than appending, so a second `trap` silently discards
    # the first; the previous revision leaked a 19 MB export per run that way,
    # and referenced a variable the early-exit path had not yet assigned.
    work=$(mktemp -d)
    # The history gate below adds a git worktree inside $work, so the handler
    # removes that worktree by name before deleting the directory. Both are in
    # the one EXIT handler, because bash REPLACES an EXIT trap rather than
    # appending, which is the leak the note above records.
    #
    # **`worktree remove`, never `worktree prune`.** Prune is repo-global and
    # defaults to `--expire TIME_MAX`, so it deletes the administrative data of
    # ANY worktree whose directory is momentarily absent — index, HEAD, reflog
    # and refs/worktree, with no grace period. This recipe runs on every push
    # through `just verify`, this machine carries a worktree per lane plus
    # several under a temp directory a reaper can clear, and `prune` resolves
    # against the common git dir shared by all of them. It would also race two
    # concurrent runs, since `worktree add` creates the registration before the
    # directory.
    trap 'git worktree remove --force "$work/hist" >/dev/null 2>&1 || true; rm -rf "$work"' EXIT

    # --- gate 0: is this history complete at all? ---------------------------
    #
    # Runs first, because a truncated history invalidates both history gates
    # below rather than only the last one.
    # A SHALLOW CLONE IS NOT AN INCOMPLETE OBJECT STORE — it is a complete
    # store of a truncated history, so the `missing` check below never fires
    # for it. Measured: a `--depth 1` clone reported "1094 objects read, none
    # missing" and exited 0 over a single commit. Check shallowness directly.
    #
    # A graft is only disqualifying if it truncates *this* history. This
    # repository carries one from a vendored Skia checkout that no ref
    # contains, so test ancestry rather than the flag.
    shallow_file=$(git rev-parse --git-path shallow)
    if [ -f "$shallow_file" ]; then
        while read -r graft; do
            [ -n "$graft" ] || continue
            if git merge-base --is-ancestor "$graft" HEAD 2>/dev/null; then
                echo "ABORT — history is shallow at ${graft:0:12}."
                echo "  A truncated history cannot be scanned. Fix with:"
                echo "      git fetch --unshallow"
                exit 1
            fi
        done < "$shallow_file"
    fi

    # --- gate 1: tracked content at HEAD -----------------------------------
    #
    # Scan committed content, not the working directory. `gitleaks dir` does
    # not honour .gitignore, so pointing it at `.` walks `target/` — 923 MB
    # after a `cargo doc` — and reports findings in build artifacts nobody
    # publishes. Run from *inside* the export and scan `.`, so gitleaks emits
    # repo-relative paths: a .gitleaksignore fingerprint is
    # `<path>:<rule>:<line>`, and an absolute path makes every one miss.
    echo "── gitleaks: tracked content at HEAD ──"
    mkdir -p "$work/head"
    # No `cp` of the config or the ignore file: `git archive` already carries
    # both, because both are tracked. Copying the working-tree versions in
    # would let an uncommitted .gitleaksignore line silence a finding on a scan
    # whose whole point is what is in the commit rather than what is on disk.
    git archive HEAD | tar -x -C "$work/head"
    ( cd "$work/head" && gitleaks dir . --config .gitleaks.toml )

    # --- gate 2: history, against the triaged baseline ----------------------
    #
    # Compare the distinct <rule>:<secret> set against
    # .secrets-history-baseline — anything new fails. History needs a
    # by-value record because the same content sits at many line numbers
    # across history, so a fingerprint written for one of them does not
    # converge on the rest.
    #
    # **The scan runs where .gitleaksignore is not**, which is what makes the
    # baseline the whole adjudication rather than the remainder (issue #987).
    # gitleaks matches a bare <file>:<rule>:<line> in git mode too, so a
    # fingerprint silences that path and line in EVERY commit that carries it.
    # Measured over --all on 2026-08-16, the only variable being whether the
    # file is present:
    #
    #     with    .gitleaksignore : 63 findings, 31 distinct pairs
    #     without .gitleaksignore : 101 findings, 54 distinct pairs
    #
    # 23 pairs were suppressed, and 17 of them appeared in no triage record at
    # all. None was a credential — they are Figma `fileKey` and 40-hex
    # component `key` values, the two classes the baseline header already
    # describes — but nobody had been asked, and a clean run here was being
    # read as "every historical value is triaged".
    #
    # **What git mode still does not read: merge commits.** gitleaks drives
    # `git log -p`, which emits no patch for a merge, so a value introduced
    # only by a conflict resolution never reaches this comparison. Measured on
    # 8.30.1 against a synthetic repository: `gitleaks git --log-opts=--all`
    # reports 0 and `gitleaks dir` reports 1 for the same blob, reachable at
    # HEAD. Gate 1 covers it at HEAD and gate 3 covers it for its patterns over
    # every reachable object, so the hole is this gate's rule set on merge-only
    # content — which is why the line this prints says which scan it was.
    #
    # **A `--no-checkout` worktree is how the file is removed.** `-i <path>`
    # does NOT disable the auto-loaded copy: measured on gitleaks 8.30.1
    # against both an empty directory and an empty file, and the count stayed
    # 63 either way. An earlier reading of this concluded the ignore file had
    # no effect on git mode because of that flag. A worktree shares the object
    # store and every ref, so `--all` and `--all --not --remotes` resolve to
    # exactly the same objects — verified, 880 either side — and `--no-checkout`
    # means nothing is written to disk, so it costs no checkout of a corpus this
    # size. `gitleaks git` reads history rather than a working tree, so an empty
    # one is all it needs.
    echo "── gitleaks: history ($scope_label, against the triaged baseline) ──"
    hist="$work/history.json"
    # **The config comes from the EXPORT**, like the two triage records below
    # and like gate 1's. Reading `.gitleaks.toml` from the working tree would
    # let an uncommitted `[[allowlists]]` entry silence a value that is in
    # history and not at HEAD — which is precisely what this gate, and not gate
    # 1, exists to catch. It would re-open the suppression channel this change
    # closes, one file over.
    config="$work/head/.gitleaks.toml"
    if [ ! -f "$config" ]; then
        echo "history: ABORT — the export carries no .gitleaks.toml."
        exit 1
    fi
    # Not `>/dev/null 2>&1` with an `if` after it: under `set -e` a failing
    # `worktree add` exits here and the guard never runs, so git's reason was
    # discarded for nothing. Captured, then reported.
    wt_err=""
    if ! wt_err=$(git worktree add --no-checkout --detach "$work/hist" HEAD 2>&1); then
        echo "history: ABORT — could not create the scanning worktree."
        printf '%s\n' "$wt_err" | sed 's/^/  /'
        exit 1
    fi
    if [ -e "$work/hist/.gitleaksignore" ]; then
        echo "history: ABORT — the scanning worktree has a .gitleaksignore."
        echo "  This gate reads history the tree scan's fingerprints would hide;"
        echo "  with that file present it would adjudicate only the remainder."
        exit 1
    fi
    # **The `cd` is load-bearing and the subshell keeps it local.** gitleaks
    # resolves `.gitleaksignore` from its working directory, not from the source
    # path it is given: review proposed `gitleaks git "$work/hist"` from here as
    # an equivalent, and it is not — measured, that form reports 63 findings,
    # the suppressed count, because it picks up this checkout's ignore file. The
    # `cd` is what makes the count 101. The config is therefore an absolute path
    # from `$work`.
    #
    # A `cd` that fails would set `rc=1` and read as "findings", which the
    # `jq -e 'type == "array"'` guard below catches: an absent report is not a
    # clean history.
    rc=0
    ( cd "$work/hist" && gitleaks git --log-opts="$range" --config "$config" \
        --report-format json --report-path "$hist" >/dev/null 2>&1 ) || rc=$?
    # 0 = clean, 1 = findings, anything else = the scan itself failed.
    if [ "$rc" -gt 1 ]; then
        echo "history: ABORT — gitleaks exited $rc; the scan did not complete."
        exit 1
    fi
    if ! jq -e 'type == "array"' "$hist" >/dev/null 2>&1; then
        echo "history: ABORT — gitleaks produced no usable report."
        echo "  An empty or malformed report is not a clean history."
        exit 1
    fi
    # Read the triage records from the EXPORT, not the working tree. An
    # uncommitted line in one of them must not silence a finding — the rule
    # gate 1 states, applied to all three gates rather than only the first.
    baseline_file="$work/head/.secrets-history-baseline"
    grep -vE '^#|^[[:space:]]*$' "$baseline_file" > "$work/baseline" || true
    if [ ! -s "$work/baseline" ]; then
        echo "history: ABORT — the committed baseline is empty or unreadable."
        exit 1
    fi
    found=$(jq -r '.[] | "\(.RuleID):\(.Secret)"' "$hist" | sort -u)
    # `-f FILE`, never a pattern argument: a triaged value beginning with `-`
    # is otherwise read as a grep option and grep exits 2, which the old
    # `|| true` reported as clean. grep 1 is "nothing selected" and fine; 2+
    # is an error and must abort.
    set +e
    new=$(printf '%s\n' "$found" | grep -vxF -f "$work/baseline")
    grc=$?
    set -e
    if [ "$grc" -gt 1 ]; then
        echo "history: ABORT — the baseline comparison failed (grep exit $grc)."
        exit 1
    fi
    if [ -n "$new" ]; then
        echo "history: NEW findings not in .secrets-history-baseline — triage before publishing:"
        printf '%s\n' "$new" | sed 's/^/  /'
        exit 1
    fi
    # Says which scan produced the number. "every value" would be wrong twice
    # over: git mode reads no merge patches, and a narrowed range is narrowed on
    # purpose. Issue #987 asked this line to state what it did NOT look at.
    echo "history: clean — $(jq 'length' "$hist") findings from $scope_label, ignore-file suppression off, all in the $(wc -l < "$work/baseline" | tr -d ' ') triaged pairs (git mode reads no merge patches)"

    # --- gate 3: pattern grep over every object -----------------------------
    #
    # Plain commands, never a command substitution wrapping a pipeline, so
    # `set -euo pipefail` actually sees a failure of git, awk or cat-file.
    echo "── pattern grep: $scope_label ──"
    # SCOPE: objects reachable from a ref — which is exactly the set `git push`
    # sends. That is the question a publication gate asks.
    #
    # `git cat-file --batch-all-objects` was tried and rejected. It reaches
    # 20,003 objects here against 9,869 reachable, and the extra half is
    # unreachable: amended-away commits, and residue from a vendored Skia fetch
    # that pulled Chromium's zlib tests into this object store. Scanned, it
    # reports 12 hits, of which eight are a generated alphabet sequence in
    # compression test data and two are this recipe's own test fixtures. None
    # can ever be published, because git does not push unreachable objects — so
    # the wider scan buys permanent false positives and no coverage.
    #
    # THE RESIDUAL RISK IT LEAVES, stated plainly: a credential that was pushed
    # and then removed by a force-push still lives on the remote, which serves
    # it by SHA indefinitely. No local scan can see that, including this one.
    # The mitigation is to rotate the credential, not to scan harder.
    #
    # A partial clone has fewer objects present rather than objects missing, so
    # like a shallow clone it defeats the scan silently. Refuse it.
    if [ -n "$(git config --get extensions.partialclone || true)" ] \
       || [ -n "$(git config --get remote.origin.partialclonefilter || true)" ]; then
        echo "ABORT — this is a partial clone; its object store is incomplete."
        echo "  Re-clone without --filter to scan."
        exit 1
    fi
    # Streamed, not materialized: the previous revision wrote 86 MB to a temp
    # file on every push and grew with history.
    git rev-list --objects $range > "$work/oids"  # UNQUOTED: see the header
    n=$(wc -l < "$work/oids" | tr -d ' ')
    awk '{print $1}' "$work/oids" | git cat-file --batch > "$work/blobs"
    grep -vE '^#|^[[:space:]]*$' "$work/head/.secrets-triaged" > "$work/triaged" || true
    set +e
    grep -aoE "figd_[A-Za-z0-9_-]{20,}|github_pat_[A-Za-z0-9_]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|glpat-[A-Za-z0-9_-]{20,}|npm_[A-Za-z0-9]{30,}|sk-ant-[A-Za-z0-9_-]{20,}|AIza[0-9A-Za-z_-]{35}|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----|(A3T[A-Z0-9]|AKIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA|ASIA)[A-Z0-9]{16}" "$work/blobs" \
        | sort -u > "$work/raw"
    hrc=$?
    set -e
    if [ "$hrc" -gt 1 ]; then
        echo "pattern grep: ABORT — scanning the object stream failed (grep exit $hrc)."
        exit 1
    fi
    set +e
    hits=$(grep -vxF -f "$work/triaged" "$work/raw")
    trc=$?
    set -e
    if [ "$trc" -gt 1 ]; then
        echo "pattern grep: ABORT — the triage comparison failed (grep exit $trc)."
        exit 1
    fi
    if [ -n "$hits" ]; then
        echo "pattern grep: FOUND — triage before publishing, then add to .secrets-triaged:"
        printf '%s\n' "$hits" | sed 's/^/  /'
        exit 1
    fi
    echo "pattern grep: clean — $n publishable objects read"

# Full non-build verification: the regression tier + lint + audit. Not the
# sanity tier — `check` is what `build` runs, so it takes the tier that is the
# gate (docs/decisions/test-tiers.md). The pre-push hook does not run `check`;
# see `verify`.
# Run the regression tier and every gate `build` adds to `assemble`.
check: test-regression lint audit secrets wasm-painter wasm-host c-abi harness-tests

# Everything short of a PR: assemble + check.
build: assemble check

# The pre-push hook: commit-message lint, then the checks that fit in seconds.
# The range is taken against origin/main rather than local main, because local
# main goes stale relative to the remote and would lint commits that are already
# upstream. The range can legitimately be empty (see the recipe) — that is issue
# #110, and it is handled rather than avoided by the choice of ref.
#
# It does NOT run a test tier. `just build` does, and remains the thorough local
# gate; CI runs the tier completely on every push and pull request. See the
# recipe for the measured breakdown and for what that leaves unverified.
#
# Commit lint, then lint + audit + a secret scan of the objects being pushed.
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

    # WHAT THIS GATE IS FOR, and what it deliberately is not.
    #
    # It runs on every push, so it is bounded at seconds. It took 224 s with
    # nothing to rebuild and 513 s when a crate had moved — more than CI's
    # entire run, which is 184-286 s — because it ran the regression tier here,
    # serially, on the machine you are waiting at. CI runs the same tier in one
    # `test` job alongside fifteen other jobs, on runners that became free when
    # the repository went public.
    #
    # The shape, not a table of digits. Warm, every step here is a second or two
    # and the whole gate is seconds; the regression tier alone was 154 s. That
    # ratio is the reason for the split and it does not move.
    #
    # A per-step breakdown used to sit here and was wrong twice in one day —
    # first quoting one quiet machine's run as a range, then replacing it with a
    # figure that did not reproduce. Numbers measured once and pasted into a
    # comment rot silently, and nothing in the gate fails when they do. Time the
    # recipes if you need current figures; `secrets` is the largest step, and
    # `cargo audit` the most variable because it fetches the advisory database.
    #
    # Everything except the tier fits the budget. WHAT IS DROPPED, stated in
    # full rather than as "the tier": `assemble`, `test-regression`,
    # `wasm-painter`, `wasm-host` and `c-abi`.
    #
    # `lint` still TYPE-CHECKS the whole workspace and all four wasm packages —
    # `clippy --all-targets` compiles what it lints — so a compile error still
    # fails here. It does NOT LINK, so a duplicate or undefined symbol in a
    # cdylib passes, and the C ABI header check no longer runs locally at all;
    # CI covers that inside `android-build`.
    #
    # It also resolves intra-doc links on two triples — the host through
    # `doc-links` and wasm32 inside `wasm-lint` — which is why a broken doc
    # link fails a push. The android triple is the one this cannot carry, for
    # the NDK reason `android-lint` gives; CI has it. Measured warm after issue
    # #1109 added the second pass: `lint` 5.5-9.1 s, this gate 14.3 s, the host
    # doc pass 0.42 s of it. Still seconds.
    #
    # WHERE THE DROPPED WORK IS CAUGHT. `.github/workflows/ci.yml` triggers on
    # `pull_request` and on pushes to `main` — NOT on a push to a feature
    # branch. So between pushing a branch and opening its pull request, none of
    # the dropped gates run anywhere. That window is the deliberate cost of this
    # change; `main` itself keeps full coverage, and nothing merges without a
    # pull request.
    #
    # `just build` is unchanged and remains the thorough local gate; run it by
    # hand when you want the tier before pushing. AGENTS.md names it for that.
    #
    # `secrets` is scoped to `--all --not --remotes` — every object a push could
    # publish. The unscoped sweep re-reads 10,151 already-published objects and
    # is about 18 s of its 21.5 s; CI runs it unscoped. The scoped scan still
    # covers the case a push makes irreversible: a credential in the commits
    # about to be published, including one added and then removed within the
    # branch, where the HEAD scan sees nothing.
    #
    # `audit` and the markdown gate are kept even though nothing here can move
    # them, because until the CI jobs added alongside this recipe they ran in no
    # other place — and `cargo audit` in particular fails on a newly published
    # advisory against a dependency that did not change, so no diff predicts it.
    # The markdown half is no longer the expensive one: markdownlint was 7.3 s
    # of this recipe and `just prim` is 0.9 s, both measured on this tree.
    just lint
    just audit
    just secrets "--all --not --remotes"

# Reformat everything in place (Rust, then markdown/JSON/YAML/TOML).
fmt:
    cargo fmt --all
    prim fmt .

# The same flags as `doc-links`, deliberately (issue #1117). Two reasons, and
# the second is the one that costs time. Without `--document-private-items`
# this would not render the private items the gate now reads, so a link the
# gate rejected could not be checked here after it was repaired. And different
# rustdoc flags mean different fingerprints, so alternating this with `lint`,
# `verify` or `build` re-documented every workspace member each way round:
# measured warm on this branch, the aligned pair costs 2.9 s against 35-48 s
# each way for the unaligned one.
#
# `-D warnings` travels with them because the fingerprint is what is being
# aligned. That does mean this refuses to open on a broken link rather than
# opening and hiding it.
#
# `--keep-going` is deliberately absent: this one is for a person waiting at a
# browser, so stopping at the first failure reports sooner.

# Open the rustdoc build in a browser.
doc:
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --document-private-items --quiet --open

# Serve the mdBook docs locally.
book:
    mdbook serve

# Cut a release: git-std bumps versions, writes the changelog, tags.
release:
    git std bump

#
# Measures `measure/web-minimal` — the smallest browser embedder that draws a
# `.dsb` — and `demo-web` beside it, built and post-processed identically so the
# two are comparable. The gap between them is `showcase` and, through it,
# `dashc`: the compiler, which an embedder loading a prebuilt document does not
# link.
#
# This **reports**; it does not gate. A gate has to name a pipeline stage this
# repository produces, and `wasm-opt` is in neither this file nor CI — so the
# numbers below are post-`wasm-bindgen` and nothing more. Pinning a compressor
# the way `dashpack` pins zstd is what a gate would need first
# (`docs/decisions/publishable-and-the-first-version.md`).
#
# Needs `wasm-bindgen-cli` (matching the workspace's `wasm-bindgen`) and
# `brotli`, neither of which `bootstrap` installs.
# What an embedder's runtime actually weighs (issue #776, story #795).
measure-runtime:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p web-minimal -p demo-web --target wasm32-unknown-unknown --release
    out=$(mktemp -d)
    trap 'rm -rf "$out"' EXIT
    printf '%-14s %12s %12s\n' artifact raw brotli
    for pair in "web-minimal:web_minimal" "demo-web:demo_web"; do
      name=${pair%%:*}; file=${pair##*:}
      wasm-bindgen --target web --no-typescript --out-dir "$out/$name" \
        target/wasm32-unknown-unknown/release/$file.wasm
      wasm=$(ls "$out/$name"/*_bg.wasm)
      brotli -q 11 -f -o "$wasm.br" "$wasm"
      printf '%-14s %12s %12s\n' "$name" "$(wc -c < "$wasm")" "$(wc -c < "$wasm.br")"
    done
    echo
    echo "rustc:        $(rustc --version)"
    echo "wasm-bindgen: $(wasm-bindgen --version)"
    echo "brotli:       $(brotli --version)"

#
# Repeatable rather than a one-time audit, which is the point: both registry
# defects this recipe checks for were found by hand once and had already
# recurred by the time they were found again (issue #445, then story #795).
#
# It does not publish, and it does not bump. `cargo package` is the one step
# that answers "what does a consumer actually get" — it builds the .crate a
# consumer downloads and fails on the things only packaging can see: a missing
# build-script input, a path dependency with no version, a file the manifest
# excludes but the build needs.
#
# `--no-verify` is deliberately NOT passed: the compile of the packaged tree is
# most of the value.
# What would have to be true before anything is published (story #795).
package:
    cargo test -p demo --test registry_consistency
    # The publishable members, derived rather than listed: `--workspace` packages
    # `publish = false` members too — that flag stops `cargo publish`, not
    # `cargo package` — and `demo` then fails, because its own `showcase`
    # dependency is declared by path with no version. (`showcase` has a version;
    # the declaration is what lacks one, which is fine for a crate nobody
    # publishes and fatal the moment something packages its consumer.)
    # A hand-written `--exclude` list would be one more registry to drift.
    cargo metadata --format-version 1 --no-deps \
      | jq -r '.packages[] | select(.publish == null) | "-p \(.name)"' \
      | xargs cargo package --allow-dirty

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
    cargo publish -p dashpaint-abi
    cargo publish -p dashscene-gpu
    cargo publish -p dashscene-desktop
    cargo publish -p dashscene-web
    cargo publish -p dashscene-ffi
    cargo publish -p dashscene-android
    cargo publish -p dashscene

# Install local toolchain bits (git hooks, git-std, cargo-nextest, jq, prim).
install:
    ./bootstrap

# Remove build artifacts.
clean:
    cargo clean

#
# --lib on purpose. Without it, cargo also builds the `dashc` bin for wasm,
# producing a second artifact (dashc.wasm) that is the CLI: it reads files and
# reads the environment, and it exports none of the ABI. Two .wasm files where
# one is a decoy is a trap — the importer loads dashc_wasm.wasm.
# Build dashc's cdylib for wasm32 — the module the Deno importer loads.
wasm:
    cargo build -p dashc --lib --release --target wasm32-unknown-unknown

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
# Build the lean painter for wasm32 — the target the web host runs on.
wasm-painter:
    cargo build -p dashscene-gpu --target wasm32-unknown-unknown

#
# Separate from it because they fail for different reasons: that one catches a
# blocking wait reaching the web path, this one catches the host itself, whose
# browser half compiles on no other target and would otherwise be checked by
# nothing until someone opened a page.
# Build the browser host for wasm32 — a gate, like `wasm-painter`.
wasm-host:
    cargo build -p demo-web --target wasm32-unknown-unknown

# Clippy over every crate that has a wasm32 half — the part of `lint` a
# host-target pass cannot see, and what CI's `wasm-gates` job runs.
#
# `crates/dashscene-web` gates host.rs and document.rs on
# `target_arch = "wasm32"`, so a host-target clippy compiles neither — and story
# #741 found two errors sitting in them, carried unchanged from the host they
# were extracted from. A published crate whose main logic is never linted is
# what this exists to prevent. `measure/web-minimal` is here for the same reason
# one crate along: its body is wasm32-only, `assemble` is a host build where the
# crate is empty, and the two build gates name other packages, so without this a
# `dashscene-web` change could break the artifact the payload budget is measured
# over while `just build` stayed green.
#
# `dashscene-gpu` takes `--lib` where the others take `--all-targets`, and the
# asymmetry is not a preference: its test targets use `pollster`, which is a
# native-only dependency, so `--all-targets` cannot resolve on this triple at
# all. `--lib` is the whole of what ships to a browser. Added at issue #903,
# which found the painter built for wasm32 by `wasm-painter` and linted for it
# by nothing — `-- -D warnings` reaches the selected package, not its path
# dependencies, so the three lines below never denied a warning in it.
# Clippy and doc-link every wasm32 half — CI's `wasm-gates` job runs this.
wasm-lint:
    cargo clippy -p dashscene-gpu --target wasm32-unknown-unknown --lib -- -D warnings
    cargo clippy -p dashscene-web --target wasm32-unknown-unknown --all-targets -- -D warnings
    cargo clippy -p demo-web --target wasm32-unknown-unknown --all-targets -- -D warnings
    cargo clippy -p web-minimal --target wasm32-unknown-unknown --all-targets -- -D warnings
    # Intra-doc links on this triple, for the reason the clippy lines above are
    # here: `doc-links` documents the host target, where a
    # `cfg(target_arch = "wasm32")` item does not exist, so nothing had ever
    # resolved a doc link written inside one. Six were broken when issue #1109
    # was measured — five in `dashscene-gpu`, each a link from the async
    # constructor to the blocking `new` it replaces, which is
    # `cfg(not(target_arch = "wasm32"))` and so absent exactly here.
    #
    # **The selection is the whole workspace minus what cannot build here, not
    # a list of the packages with a wasm32 half.** The sixth break was in
    # `dashbuf`, which has no wasm32 half at all — it declares `pub mod map`
    # under `cfg(not(target_arch = "wasm32"))` and links into it from a module
    # that is always compiled, so the link resolves on every triple but this
    # one. A four-package list written from "who has a browser half" missed it,
    # and would have shipped this gate with a hole of exactly the kind it
    # exists to close. Excluding is also the safer direction: a crate added to
    # the workspace is gated by default, and one that cannot build for wasm32
    # fails loudly here instead of being skipped in silence.
    #
    # The nine exclusions are the members that do not compile for this triple:
    # `dashscene-desktop` and `dashscene-ffi` fail outright, `demo-producer`
    # fails because it links the second of those, and the other six carry native
    # build scripts (skia, astcenc, zstd, nv-flip). Warm, this costs 1.1 s — no
    # more than the four-package list it replaces.
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --exclude dashscene-desktop --exclude dashscene-ffi --exclude demo-producer --exclude dashscene-android --exclude dashscene-skia --exclude dashpack --exclude dashpack-astcenc-sys --exclude demo --exclude goldens --target wasm32-unknown-unknown {{ DOC_FLAGS }}

#
# The Rust tests in `dashscene-ffi` call the same functions, but they call them
# as Rust: they see the real enum and a header that was never involved. This is
# the only thing in the workspace that checks the two halves agree — that the
# header declares what the library exports, which a link error catches, and that
# `DS_ABI_VERSION` equals what `ds_abi_version()` returns, which nothing else
# compares.
#
# Links the `cdylib` rather than the `staticlib`: the dynamic library carries
# its own transitive links, where the static one would make this recipe name
# every system framework `wgpu` pulls in and re-name them per platform.
# Exercise the C ABI as a C caller, against its own header.
c-abi:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p dashscene-ffi
    crate=crates/dashscene-ffi
    out=target/debug/c-abi-test
    case "$(uname -s)" in
      Darwin) lib=target/debug/libdashscene_ffi.dylib ;;
      *)      lib=target/debug/libdashscene_ffi.so ;;
    esac
    if [ ! -f "${lib}" ]; then
      echo "c-abi: ${lib} was not built — is crate-type cdylib still set?" >&2
      exit 1
    fi
    "${CC:-cc}" -std=c11 -Wall -Wextra -Werror \
      -I "${crate}/include" \
      "${crate}/tests/abi.c" \
      -o "${out}" \
      -L target/debug -ldashscene_ffi \
      -Wl,-rpath,"$(cd target/debug && pwd)"
    "${out}"

# The host dynamic library — the one an editor-side host loads on this machine.
#
# **The gap is narrower than "nothing builds one", and stating it precisely is
# what says why this recipe exists.** `crates/dashscene-ffi` has declared
# `cdylib` since story #840, and a *debug* library already falls out of two
# recipes: `assemble` builds the whole workspace, and `c-abi` links its C caller
# against exactly that file and fails if it is absent. What no recipe produced
# is the **release** library — the one a host actually loads — and no recipe
# named the path, so a host author read it out of someone's shell history.
#
# `just android` is the other half of the same seam and cross-compiles for
# `aarch64-linux-android` only. That is not a substitute here: an editor-side
# host loads a library for *this* triple into its own process, so the Android
# `.so` is unreachable from it. Story #1230 measured that with a headless
# `-executeMethod` run rather than a play-mode test, and
# `docs/technotes/unity-toolchain.md` says which.
#
# **Not in `check` or `build`, and that leaves a real gap rather than a covered
# one.** `c-abi` links this crate in **debug** on every one of those runs, so
# nothing in any gate links it in release — where `[profile.release]` turns on
# `lto = true` and `codegen-units = 1`. A release-only link failure in the one
# library a host is told to load reaches nobody until someone runs this recipe.
# Adding it would put a full-LTO link into every local `check`, which is a
# scheduling decision rather than this story's to take: issue #1233.
# Build the release host dynamic library and print where it landed.
host-lib:
    #!/usr/bin/env bash
    set -euo pipefail
    # **The path comes from cargo, not from a `uname` mapping plus a test that
    # the file exists.** That is a correctness difference, not a tidier
    # spelling. `cargo build` does not delete the artifacts of a crate type
    # that has been removed — drop `cdylib` from `[lib] crate-type`, rebuild,
    # and the previous `libdashscene_ffi.dylib` is still on disk, which was
    # measured on a throwaway crate rather than assumed. So a `[ ! -f … ]`
    # guard passes over a stale library, and this recipe would print the path
    # of something this run did not produce. That is the shape of issue #1057,
    # where a stale release `.so` was packaged into an APK and announced as the
    # release library. `compiler-artifact.filenames` lists only what this
    # invocation emitted, so a missing cdylib is missing here.
    #
    # `jq` is a bootstrap-installed dependency, so this adds no prerequisite.
    #
    # **Built first, then asked** — two invocations rather than one, and the
    # second is a cache hit costing well under a second. `--message-format=json`
    # puts cargo's diagnostics on stdout as JSON instead of rendering them, so a
    # single json-mode build that failed would print nothing a reader could act
    # on. The plain build below keeps that output; the query below it runs
    # against a tree cargo has already finished with.
    cargo build -p dashscene-ffi --release
    lib=$(cargo build -p dashscene-ffi --release --message-format=json | jq -r '
        select(.reason == "compiler-artifact")
        | select(.target.name == "dashscene_ffi")
        | .filenames[]
        | select(endswith(".dylib") or endswith(".so") or endswith(".dll"))
      ' | tail -n 1)
    if [ -z "${lib}" ]; then
      echo "host-lib: cargo emitted no dynamic library for dashscene-ffi." >&2
      echo "host-lib: is crate-type cdylib still set in its Cargo.toml?" >&2
      exit 1
    fi
    # cargo reports an absolute path already, and that is what a caller needs:
    # a Unity project copies this file into its own `Assets/` from outside this
    # repository, where a path relative to the workspace root is not usable.
    echo "host-lib: ${lib}"

# **The package carries its binaries**, by
# `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D4, so
# refreshing one is a commit rather than a build step a consumer runs. This
# recipe is what produces that commit: a hand copy is how issue #1057 happened,
# where a stale RELEASE `.so` was packaged and announced as the release library
# while the debug one was the artifact being rebuilt.
#
# **Two platforms, because two have a consumer** — macOS arm64 for a developer's
# editor and Android arm64 for a player on the target. D3's Windows and Linux
# rows ship nothing today, and iOS is v1.
#
# After running it, run `just unity-editor WritePluginMeta` if a library is NEW
# at its path, so Unity writes the `.meta` R-E21 requires; an existing library
# replaced in place keeps its `.meta` and its guid, which is the point of
# committing them. Then commit both.
#
# Needs the NDK for the Android half, which bootstrap does not install, so this
# is outside `check` for the reason `just android` is.
#
# Rebuild the native libraries the UPM package ships and place them inside it.
unity-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(git rev-parse --show-toplevel)"
    plugins="${root}/unity/com.driftsys.dashscene/Runtime/Plugins"

    # **Both rows name their triple, and neither trusts the host's own.** An
    # earlier version of this recipe built the macOS row with a plain
    # `cargo build` behind a `uname -s` guard. That guard cannot tell an arm64
    # Mac from an x86_64 one, and D3's macOS row is arm64 — so on an Intel host
    # it installed an x86_64 dylib under a `.meta` declaring `CPU: ARM64`, which
    # every check then passed: the text gate did not open the file, the editor
    # gate reads the `.meta` rather than the binary, and the player that would
    # fail is one an Intel host cannot run. **The text gate opens it now** — it
    # compares each library's header against D3 — so this is the defect that
    # motivated that check rather than one still open. Naming the triple removes the
    # question instead of answering it.
    #
    # The `uname -s` refusal stays for a different reason: linking a Mach-O
    # dylib needs Apple's linker, so this row cannot be produced off macOS at
    # all, whatever triple is named.
    if [ "$(uname -s)" != "Darwin" ]; then
      echo "unity-plugins: D3's macOS row needs Apple's linker, so it can only" >&2
      echo "unity-plugins: be built on macOS. This is $(uname -s)." >&2
      exit 1
    fi

    # `install` does not create the destination directory, and a new platform
    # row is exactly when one does not exist yet.
    mkdir -p "${plugins}/macOS" "${plugins}/Android"

    # **Nothing that identifies the machine that built it may reach a committed
    # binary.** A release build embeds a path per panic location, so an
    # unremapped library carries the builder's home directory — measured at 267
    # `~/.cargo/registry` strings, 27 `~/.rustup/toolchains` and one workspace
    # path in the first `.so` produced here — and this repository is public and
    # its history permanent. The remapping also makes the output independent of
    # WHERE it was built, so two developers produce the same bytes and a future
    # staleness check can compare a hash rather than trusting a date.
    #
    # `--target` is what keeps this off the build scripts: with a triple named,
    # cargo applies RUSTFLAGS to target artifacts only, and a build script
    # compiled for the host keeps its own paths and ships nowhere.
    export RUSTFLAGS="--remap-path-prefix=${HOME}/.cargo/registry=/cargo/registry"
    RUSTFLAGS="${RUSTFLAGS} --remap-path-prefix=${HOME}/.rustup/toolchains=/rustup"
    RUSTFLAGS="${RUSTFLAGS} --remap-path-prefix=${root}=/dashscene"

    # The path comes from cargo, on the rule `host-lib` states in full: cargo
    # does not delete the artifacts of a crate type that has been removed, so a
    # file test over `target/` passes on a stale library.
    emitted() {
      cargo build "$@" --message-format=json | jq -r '
        select(.reason == "compiler-artifact")
        | select(.target.name == "dashscene_ffi")
        | .filenames[]
        | select(endswith(".dylib") or endswith(".so"))
      ' | tail -n 1
    }

    echo "unity-plugins: macOS arm64"
    cargo build -p dashscene-ffi --release --target aarch64-apple-darwin
    host="$(emitted -p dashscene-ffi --release --target aarch64-apple-darwin)"
    if [ -z "${host}" ]; then
      echo "unity-plugins: cargo emitted no dynamic library for the host." >&2
      echo 'unity-plugins: is cdylib still in [lib] crate-type?' >&2
      exit 1
    fi
    install -m 755 "${host}" "${plugins}/macOS/libdashscene_ffi.dylib"

    # **The install name is the one path remapping cannot reach.** rustc emits
    # its own `-install_name` naming the output under `target/`, so the shipped
    # dylib records an absolute path from the machine that built it — measured,
    # after the remapping above had already cleared every other occurrence.
    # A `-Clink-arg` does not win against the flag rustc itself passes, so it is
    # rewritten here instead. `@rpath` is also the correct value: Unity resolves
    # a plugin by the path it copied it to, not by this field.
    install_name_tool -id @rpath/libdashscene_ffi.dylib \
      "${plugins}/macOS/libdashscene_ffi.dylib"

    echo "unity-plugins: android arm64"
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    cargo build --release -p dashscene-ffi --target aarch64-linux-android
    droid="$(emitted --release -p dashscene-ffi --target aarch64-linux-android)"
    if [ -z "${droid}" ]; then
      echo "unity-plugins: cargo emitted no dynamic library for android." >&2
      exit 1
    fi
    install -m 755 "${droid}" "${plugins}/Android/libdashscene_ffi.so"

    echo "unity-plugins: wrote"
    git -C "${root}" status --short -- "${plugins}"

# Two gates over the UPM package, not one (the second added by story #1125).
#
# 1. Compiles `unity/com.driftsys.dashscene/Runtime/BoundaryB.cs` — the
#    package's own file, not a copy — and compares every type on the surface
#    against what `crates/dashpaint-abi` reports for it, member by member and
#    matched by name.
# 2. Compiles the package's whole `Runtime/` against **netstandard2.1**, which
#    is what Unity's default API compatibility level accepts and which the
#    first gate cannot check: `abi-check` targets net10.0, a strict superset.
#    `docs/specification/07-embedding-and-distribution.md` R-E10.
# **No Unity editor is involved**, and none is needed to compare layouts; the
# reason that matters is in
# `docs/decisions/unity-package-sited-in-this-repository.md`.
#
# **The gate crate declares no `crate-type`**, so `cargo build` yields a plain
# rlib and the layout symbols reach no loadable artifact. `cargo rustc
# --crate-type cdylib` produces one for this run alone, with no manifest
# change — so the published crate stays an rlib. Story #859 settled what a
# shipping host loads: not these symbols. It reports each array's row size as
# `DsSlice::stride` instead, and this gate stays the member-by-member check.
#
# **Not in `check`**, on the grounds `gitleaks` and the NDK already set: the
# .NET SDK is not a bootstrap dependency and a clone without it still runs
# `just check`. CI's `unity-abi` job runs exactly this recipe.
#
# Hold the UPM package's C# declarations to the Rust build of boundary B.
unity-abi:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v dotnet >/dev/null 2>&1; then
      echo "unity-abi: the .NET SDK is not installed" >&2
      echo "unity-abi:   brew install dotnet" >&2
      echo "unity-abi:   or https://dotnet.microsoft.com/download" >&2
      exit 1
    fi
    # **Presence is not enough**, and the failure it leaves is unhelpful: an
    # older SDK passes the check above and then `dotnet run` reports NETSDK1045
    # about a target framework, naming neither the version needed nor how to
    # get it. The requirement comes from the project file rather than being
    # restated here, on the rule `web-build` sets for wasm-bindgen.
    want=$(sed -n 's/.*<TargetFramework>net\([0-9][0-9]*\)\..*/\1/p' \
      unity/abi-check/AbiCheck.csproj)
    have=$(dotnet --version | cut -d. -f1)
    if [ "${have}" -lt "${want}" ]; then
      echo "unity-abi: unity/abi-check targets net${want}.0 and the SDK on PATH is ${have}.x" >&2
      echo "unity-abi:   brew upgrade dotnet" >&2
      exit 1
    fi
    # **One invocation, not the two `host-lib` uses.** That recipe builds twice
    # because plain `--message-format=json` would swallow rustc's diagnostics
    # and a failing build would print nothing a reader could act on;
    # `json-render-diagnostics` removes the reason, putting the artifact JSON on
    # stdout and rendered diagnostics on stderr. Checked by forcing a type error
    # and confirming it still renders.
    #
    # **The path comes from cargo, not from a `uname` mapping plus a file test.**
    # `cargo build` does not delete the artifacts of a crate type that has been
    # removed, so a `[ -f … ]` guard passes over a stale library and the check
    # would then report on something this run did not produce — the shape of
    # issue #1057.
    lib=$(cargo rustc -p dashpaint-abi --crate-type cdylib \
      --message-format=json-render-diagnostics | jq -r '
        select(.reason == "compiler-artifact")
        | select(.target.name == "dashpaint_abi")
        | .filenames[]
        | select(endswith(".dylib") or endswith(".so") or endswith(".dll"))
      ' | tail -n 1)
    if [ -z "${lib}" ]; then
      echo "unity-abi: cargo emitted no dynamic library for dashpaint-abi." >&2
      echo 'unity-abi: did cargo rustc --crate-type cdylib stop being accepted?' >&2
      exit 1
    fi
    # The same formatting pass CI's `unity-abi` job runs before this recipe.
    dotnet format unity/abi-check --verify-no-changes
    DASHPAINT_ABI_LIB="${lib}" dotnet run --project unity/abi-check
    # **A second question, and `abi-check` cannot answer it.** That project
    # targets net10.0, a strict superset of the .NET Standard 2.1 Unity
    # defaults to, so it accepts declarations Unity would refuse. This compiles
    # the package's `Runtime/` against netstandard2.1 and is one of the two
    # checks `docs/specification/07-embedding-and-distribution.md` R-E10 names.
    # Measured: a `System.Half` in `BoundaryB.cs` builds clean under net10.0
    # and fails here with CS0234.
    #
    # **It compiles `Runtime/` MINUS `Runtime/Engine/`**, which holds the
    # engine-referencing half — this project has no Unity reference assemblies,
    # so a `UnityEngine` type fails here whatever its API compatibility level
    # actually is (issue #1286). The other half is R-E10's second check,
    # `just unity-editor`, and the ruling is
    # `docs/decisions/r-e10-is-checked-in-two-halves.md`. The project prints
    # what it skipped on every run.
    #
    # The failure is caught rather than left bare because its cheap-looking
    # repair is the wrong one: CS0246 on a new file says a type is missing, and
    # widening this project's exclusion makes it green while narrowing R-E10 to
    # whatever is left.
    # **Twice: the default configuration, and the demonstration one.** The
    # second is what compiles `Runtime/DemoProducer.cs`'s real body against
    # netstandard2.1 — the question this project exists to ask — rather than the
    # empty file the `#if` leaves behind (story #1342).
    if ! dotnet build unity/package-compat -v q --nologo -p:DemoProducer=true; then
      echo "" >&2
      echo "unity-abi: package-compat failed in its DEMONSTRATION configuration." >&2
      echo "unity-abi: Runtime/DemoProducer.cs uses something netstandard2.1 does" >&2
      echo "unity-abi:   not carry. unity/ffi-check compiles it at net10.0 and" >&2
      echo "unity-abi:   would not have caught this." >&2
      exit 1
    fi
    if ! dotnet build unity/package-compat -v q --nologo; then
      echo "" >&2
      echo "unity-abi: package-compat compiles Runtime/ MINUS Runtime/Engine/." >&2
      echo "unity-abi: a CS0246 on a UnityEngine type means the file belongs in" >&2
      echo "unity-abi:   unity/com.driftsys.dashscene/Runtime/Engine/" >&2
      echo "unity-abi: MOVE IT THERE. Do not widen this project's Exclude — that" >&2
      echo "unity-abi: narrows R-E10 to whatever is left, which is issue #1286." >&2
      echo "unity-abi: docs/decisions/r-e10-is-checked-in-two-halves.md" >&2
      exit 1
    fi

# Regenerate the Unity package's HLSL from the WGSL shader library.
#
# **R-T5's mechanism.** `docs/specification/03-target-hardware-rules.md` R-T5
# asks for the SDF shader math to be single-sourced into both product painters'
# shading languages. This compiles `crates/dashscene-gpu/src/shaders/sdf.wgsl`
# to `unity/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl` with `naga` — the
# translator wgpu already runs that same file through for the lean painter — so
# the HLSL is not a port of the WGSL, it is the WGSL compiled.
#
# **Run it after editing the WGSL.** Forgetting is not a silent divergence:
# `the_committed_hlsl_is_what_the_wgsl_compiles_to` in `unity/package-gate`
# re-derives the file on every test run and fails if the committed one differs,
# naming the first line that moved. That test is in the sanity tier, so
# `just test` catches it in seconds.
#
# Never edit `Sdf.hlsl` by hand. An edited file still compiles and still draws,
# and is no longer the arithmetic the other painter evaluates — which is the one
# failure a review does not catch and the reason this is generated at all.
#
# Generate the Unity package's Sdf.hlsl from the WGSL shader library.
sdf-hlsl:
    cargo run -q -p package-gate

# R-E10's second check: the package compiled by a Unity editor.
#
# **`just unity-abi` cannot ask this question and this recipe cannot replace
# it.** That one compiles `Runtime/` against `netstandard.dll` 2.1.0 with no
# Unity reference assemblies, so it cannot compile a type that references
# `UnityEngine` — issue #1286. This one compiles the whole package, engine half
# included, in an editor that has the reference assemblies, under the API
# compatibility level R-E10 is actually about. The ruling that splits them is
# `docs/decisions/r-e10-is-checked-in-two-halves.md`.
#
# **Not in CI, and not in `check`.**
# `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D4
# records that no CI runner here can host a Unity install. So this is a
# developer's gate: run it before opening a pull request that touches
# `Runtime/Engine/`, `Runtime/Shaders/`, `Runtime/Resources/` or
# `Samples~/` — the last because this is the only CHECK that compiles the
# samples; `unity-demo` compiles the Showcase sample while building its player. CI runs the other half on every pull
# request, and `unity/package-gate` runs the parts that need no editor.
#
# **It also writes the `.meta` files R-E2 requires.** A `file:` dependency is a
# MUTABLE package, so the editor writes a `.meta` beside every asset it imports
# — in the working tree, not in the throwaway project. That is how story #1121's
# were made, and `07-embedding-and-distribution.md` R-E2 records that a
# hand-written set would have to guess an importer class per extension. Check
# `git status` after a run that added a file.
#
# **No `-nographics`.** The editor compiles shaders through a real graphics
# device, and a run without one reports no shader errors rather than reporting
# that it could not look. That is the same hazard `unity-painter-uses-brg.md` D4
# names for the `BufferTarget` read.
#
# `method` selects the entry point. `Run` is the gate. `WritePluginMeta`
# is the authoring pass a developer runs ONCE when a native library is
# added: it writes D3's platform data onto each shipped plugin, so Unity
# produces the `.meta` that gets committed. It is a separate entry point
# because a check that writes the values it then reads cannot fail.
# Compile the UPM package, its engine half included, in a Unity editor.
unity-editor method="Run" unity_version="6000.3.22f1":
    #!/usr/bin/env bash
    set -euo pipefail
    # **Refused rather than passed through**, on the grounds the `android`
    # recipe states for its profile: anything but these two names is a typo, and
    # the likely typo is a version string, because that is what this recipe's
    # only positional parameter used to be. Unvalidated it reaches Unity as
    # `-executeMethod DashsceneEditorCompat.6000.3.22f1`, failing after the
    # editor has started; unquoted it splits on whitespace, so `"Run -nographics"`
    # would add the argument this recipe's own comment says must never be present.
    case "{{ method }}" in
      Run|WritePluginMeta) ;;
      *)
        echo "unity-editor: method must be Run or WritePluginMeta, not '{{ method }}'" >&2
        echo "unity-editor:   the version is the SECOND parameter:" >&2
        echo "unity-editor:     just unity-editor Run 6000.3.22f1" >&2
        exit 1
        ;;
    esac


    # **`DASHSCENE_UNITY` overrides the path, and it has to exist.** The default
    # below is the macOS Hub layout, and this recipe is R-E10's ONLY check over
    # `Runtime/Engine/` — so a hardcoded path would put that half of the
    # requirement out of reach for a developer on Linux or Windows entirely.
    editor="${DASHSCENE_UNITY:-/Applications/Unity/Hub/Editor/{{unity_version}}/Unity.app/Contents/MacOS/Unity}"
    if [ ! -x "${editor}" ]; then
      echo "unity-editor: no Unity executable at ${editor}" >&2
      echo "unity-editor:   install {{unity_version}} with the Hub, pass a version:" >&2
      echo "unity-editor:     just unity-editor Run 6000.3.22f1" >&2
      echo "unity-editor:   or point at one directly:" >&2
      echo "unity-editor:     DASHSCENE_UNITY=/path/to/Unity just unity-editor" >&2
      echo "unity-editor: docs/technotes/unity-toolchain.md records what is installed" >&2
      exit 1
    fi
    # **The built-in packages sit at a DIFFERENT offset from the executable on
    # each platform**, so they are searched for rather than derived. macOS puts
    # them under `Unity.app/Contents/Resources/`; Linux and Windows put them
    # under `Editor/Data/`. A first version derived one path arithmetically and
    # was wrong by one directory — caught by this recipe's own URP check, which
    # failed closed rather than resolving nothing quietly.
    editor_dir="$(dirname "${editor}")"
    builtin=""
    for candidate in \
      "${editor_dir}/../Resources/PackageManager/BuiltInPackages" \
      "${editor_dir}/Data/PackageManager/BuiltInPackages" \
      "${editor_dir}/../Data/PackageManager/BuiltInPackages"; do
      if [ -d "${candidate}" ]; then
        builtin="$(cd "${candidate}" && pwd)"
        break
      fi
    done
    if [ -z "${builtin}" ]; then
      echo "unity-editor: no BuiltInPackages directory near ${editor}" >&2
      echo "unity-editor: looked under ../Resources/, Data/ and ../Data/" >&2
      exit 1
    fi

    root="$(git rev-parse --show-toplevel)"
    project="${root}/target/unity-editor-compat"
    package="${root}/unity/com.driftsys.dashscene"

    # The URP version comes from the editor rather than from a literal here:
    # 6.3 ships it as a built-in package, so the manifest must name the version
    # that editor carries or the resolve reaches the network for nothing.
    urp_json="${builtin}/com.unity.render-pipelines.universal/package.json"
    if [ ! -f "${urp_json}" ]; then
      echo "unity-editor: this editor ships no built-in URP at ${urp_json}" >&2
      exit 1
    fi
    urp="$(jq -r .version "${urp_json}")"

    # **The package pins a URP version and this reads one; they must agree.**
    # `package.json`'s `dependencies` entry is what a consumer resolves, and it
    # is a literal — so an editor carrying a different built-in URP would be
    # checked against a version no consumer of this package ever gets.
    pinned="$(jq -r '.dependencies["com.unity.render-pipelines.universal"] // empty' \
      "${package}/package.json")"
    # **A UPM dependency is a MINIMUM, not an exact version.** An editor
    # shipping 17.3.1 against a pin of 17.3.0 is an ordinary and valid consumer
    # configuration, and a first version of this check refused it — which would
    # have removed R-E10's only engine-half check on the next Unity patch bump.
    # Only a pin the editor cannot satisfy is a problem.
    if [ -n "${pinned}" ]; then
      lowest="$(printf '%s\n%s\n' "${pinned}" "${urp}" | sort -V | head -1)"
      if [ "${lowest}" != "${pinned}" ]; then
        echo "unity-editor: package.json pins URP ${pinned} and this editor ships ${urp}," >&2
        echo "unity-editor: which is older — a consumer of this package could not resolve it." >&2
        exit 1
      fi
    fi

    # Rebuilt from scratch each run. A reused Library/ can hold a stale
    # compiled assembly, and this gate's whole answer is whether the CURRENT
    # sources compile — the shape of issue #1057, where a check reported on an
    # artifact its own run had not produced.
    rm -rf "${project}"
    mkdir -p "${project}/Packages" "${project}/Assets/Editor" "${project}/ProjectSettings"

    cat > "${project}/Packages/manifest.json" <<JSON
    {
      "dependencies": {
        "com.driftsys.dashscene": "file:${package}",
        "com.unity.render-pipelines.universal": "${urp}"
      }
    }
    JSON

    cp "${root}/unity/editor-compat/DashsceneEditorCompat.cs" "${project}/Assets/Editor/"

    # **Every sample is copied in so that something compiles them**, and
    # `cp -R` rather than a flat glob: a flat copy puts two samples' files in
    # one directory, where a shared basename silently overwrites, and skips any
    # subdirectory a sample has — which would be compiled by nothing at all. `Samples~` is
    # hidden from Unity's importer by its `~`, and `package-compat` and
    # `ffi-check` both glob `Runtime/**/*.cs` — so nothing in this repository
    # compiled `Samples~/FrameLoop/` at all until issue #1298 put the painter's
    # wiring there. A syntax error in it survived every gate, measured.
    #
    # Into `Assets/` rather than into the package: copying it back under
    # `Samples~` would leave it hidden again, and writing it into the package
    # directory would make this recipe modify the thing it is checking.
    mkdir -p "${project}/Assets/Samples"
    cp -R "${package}"/Samples~/. "${project}/Assets/Samples/"

    # NET_Standard is Unity's default, and this gate asserts it rather than
    # assuming it — the editor script reads the level back and fails if it is
    # anything else. Written here as well so the assertion has something to
    # find on an editor whose default ever changes.
    cat > "${project}/ProjectSettings/ProjectSettings.asset" <<'YAML'
    %YAML 1.1
    %TAG !u! tag:unity3d.com,2011:
    --- !u!129 &1
    PlayerSettings:
      m_ObjectHideFlags: 0
      productName: dashscene-editor-compat
      companyName: driftsys
      apiCompatibilityLevel: 6
      apiCompatibilityLevelPerPlatform: {}
    YAML

    log="${project}/editor.log"
    echo "unity-editor: {{ method }} over ${package} in {{unity_version}} (log: ${log})"
    set +e
    "${editor}" -batchmode -quit -projectPath "${project}" \
      -executeMethod DashsceneEditorCompat.{{ method }} -logFile "${log}"
    status=$?
    set -e

    # The editor's own report, whatever the exit code: a compile error is
    # printed by Unity and not by the script, which never runs when one stops
    # the editor reaching it.
    grep -E "^\[unity-editor\]|error CS|Shader error|Compilation failed" "${log}" || true

    if [ "${status}" -ne 0 ]; then
      echo "unity-editor: FAILED (exit ${status}). Full log: ${log}" >&2
      exit "${status}"
    fi
    echo "unity-editor: OK"

# The package's C# P/Invoke declarations, executed against the library they
# declare.
#
# **A different surface from `unity-abi`, not a second opinion on it.** That
# recipe compares boundary B's value types against `dashpaint-abi`, and
# `package-compat` asks whether the package compiles under netstandard2.1.
# Neither reaches `crates/dashscene-ffi`: until this recipe, nothing compiled a
# C# P/Invoke against `include/dashscene.h` at all, which is item 2 of issue
# #1266.
#
# **Needs no Unity editor and no plugin layout.** The library is the cdylib this
# run built, resolved by an explicit path through
# `NativeLibrary.SetDllImportResolver`, so nothing here depends on where a
# shipped library sits — that is
# `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D2, and
# it is deliberately not exercised here.
#
# **It builds `unity/ffi-check/older-library.c` several ways**, into libraries
# that export less than the package calls, and presents each to its own copy of
# the package assembly in its own `AssemblyLoadContext`. That is what provokes
# issue #1308's failure — a package newer than the library it loads passes the
# version handshake and fails where .NET binds an import — and it is the reason
# this recipe now needs a C toolchain as well. That file enumerates the builds;
# nothing here restates the count.
#
# Needs the .NET SDK and a C compiler, neither of which bootstrap installs, so
# it is outside `check`; CI's `unity-ffi` job runs exactly it.
#
# Execute the UPM package's C# P/Invoke declarations against dashscene-ffi.
unity-ffi:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v dotnet >/dev/null 2>&1; then
      echo "unity-ffi: the .NET SDK is not installed" >&2
      echo "unity-ffi:   brew install dotnet" >&2
      echo "unity-ffi:   or https://dotnet.microsoft.com/download" >&2
      exit 1
    fi
    # **Beside the SDK check rather than beside its use**, because the point of
    # naming a prerequisite is to name it before the build: the recipe compiles
    # `dashscene-ffi` twice and runs `dotnet format` below, and a developer
    # without a C toolchain should not pay for that before being told.
    if ! command -v "${CC:-cc}" >/dev/null 2>&1; then
      echo "unity-ffi: no C compiler (${CC:-cc}); the older-library checks need one" >&2
      echo "unity-ffi:   xcode-select --install, or apt-get install build-essential" >&2
      exit 1
    fi
    # Presence is not enough, and the failure it leaves is unhelpful: an older
    # SDK reports NETSDK1045 about a target framework, naming neither the
    # version needed nor how to get it. The requirement comes from the project
    # file rather than being restated here, as `unity-abi` does.
    want=$(sed -n 's/.*<TargetFramework>net\([0-9][0-9]*\)\..*/\1/p' \
      unity/ffi-check/FfiCheck.csproj)
    have=$(dotnet --version | cut -d. -f1)
    if [ "${have}" -lt "${want}" ]; then
      echo "unity-ffi: unity/ffi-check targets net${want}.0 and the SDK on PATH is ${have}.x" >&2
      echo "unity-ffi:   brew upgrade dotnet" >&2
      exit 1
    fi
    # **Formatting first, because CI runs it and this recipe did not.** The
    # `unity-ffi` job runs `dotnet format unity/ffi-check --verify-no-changes`
    # BEFORE the recipe, so a local `just unity-ffi` could pass while CI failed
    # on whitespace — which is exactly what happened on story #1122, after a
    # clean local build reported nothing. `--verify-no-changes` rather than a
    # rewrite: a gate that silently reformats the tree is not a gate.
    dotnet format unity/ffi-check --verify-no-changes

    # **The path comes from cargo, not from a `uname` mapping plus a file
    # test**, on the rule `host-lib` and `unity-abi` both state: `cargo build`
    # does not delete the artifacts of a crate type that has been removed, so a
    # `[ -f … ]` guard passes over a stale library and the check would report on
    # something this run did not produce. That is the shape of issue #1057.
    #
    # Built first, then asked — two invocations, the second a cache hit — so a
    # failing build still renders its diagnostics.
    cargo build -p dashscene-ffi
    lib=$(cargo build -p dashscene-ffi --message-format=json | jq -r '
        select(.reason == "compiler-artifact")
        | select(.target.name == "dashscene_ffi")
        | .filenames[]
        | select(endswith(".dylib") or endswith(".so") or endswith(".dll"))
      ' | tail -n 1)
    if [ -z "${lib}" ]; then
      echo "unity-ffi: cargo emitted no dynamic library for dashscene-ffi." >&2
      echo 'unity-ffi: is cdylib still in [lib] crate-type?' >&2
      exit 1
    fi
    # A committed document rather than one built here: the gate asserts against
    # a frame with rows in it, and a fixture this repository already goldens is
    # the one least likely to drift under it.
    fixture="goldens/dsb/v03-paint.dsb"
    if [ ! -f "${fixture}" ]; then
      echo "unity-ffi: ${fixture} is missing; the frame checks need a document" >&2
      exit 1
    fi
    # A SECOND fixture, because the first carries no text: the atlas seam's
    # checks need a document that stages glyph runs, and the corpus face and
    # sheet to load it with. Both refused rather than skipped, on the rule the
    # gate itself states — a gate that quietly runs fewer checks reports on less
    # than it claims.
    text_fixture="goldens/dsb/v07-text-hug-in-fill.dsb"
    if [ ! -f "${text_fixture}" ]; then
      echo "unity-ffi: ${text_fixture} is missing; the atlas checks need text" >&2
      exit 1
    fi
    if [ ! -d corpus ]; then
      echo "unity-ffi: corpus/ is missing; the atlas checks need a face and a sheet" >&2
      exit 1
    fi

    # **The older libraries, each exporting less than this package calls** —
    # they are what let the gate provoke issue #1308's failure instead of
    # describing it: a package newer than the library it loads passes the
    # version handshake, because adding a symbol does not move
    # `DS_ABI_VERSION`, and then fails where .NET binds the import. The gate
    # presents each to its own copy of the package assembly in its own
    # `AssemblyLoadContext`; `unity/ffi-check/older-library.c` says what each
    # build reaches that the others cannot, and it is the one place they are
    # enumerated.
    #
    # C rather than Rust, and compiled here rather than declared as a workspace
    # member: it includes `dashscene.h`, so the version it reports is the
    # header's and cannot drift, and this repository already compiles C in a
    # gate (`just c-abi`). A toolchain is the price, and it is reported rather
    # than discovered as a `cc: command not found`.
    case "$(uname -s)" in
      Darwin) ext=dylib ;;
      *)      ext=so ;;
    esac
    mkdir -p target/debug
    older="target/debug/libdashscene_ffi_older.${ext}"
    older_skew="target/debug/libdashscene_ffi_older_skew.${ext}"
    older_silent="target/debug/libdashscene_ffi_older_silent.${ext}"
    older_refuses="target/debug/libdashscene_ffi_older_refuses.${ext}"
    older_lease="target/debug/libdashscene_ffi_older_lease.${ext}"
    build_stub() {
      "${CC:-cc}" -std=c11 -Wall -Wextra -Werror -shared -fPIC ${2} \
        -I crates/dashscene-ffi/include \
        unity/ffi-check/older-library.c \
        -o "${1}"
    }
    build_stub "${older}" ""
    build_stub "${older_skew}" "-DDS_STUB_SKEW=7"
    build_stub "${older_silent}" "-DDS_STUB_SILENT"
    build_stub "${older_refuses}" "-DDS_STUB_REFUSES_FREE"
    build_stub "${older_lease}" "-DDS_STUB_LEASE_REFUSES"

    DASHSCENE_FFI_LIB="${lib}" DASHSCENE_FFI_FIXTURE="${fixture}" \
      DASHSCENE_FFI_TEXT_FIXTURE="${text_fixture}" DASHSCENE_FFI_CORPUS=corpus \
      DASHSCENE_FFI_STUB="${older}" \
      DASHSCENE_FFI_STUB_SKEW="${older_skew}" \
      DASHSCENE_FFI_STUB_SILENT="${older_silent}" \
      DASHSCENE_FFI_STUB_REFUSES_FREE="${older_refuses}" \
      DASHSCENE_FFI_STUB_LEASE_REFUSES="${older_lease}" \
      DASHSCENE_PACKAGE="unity/com.driftsys.dashscene" \
      dotnet run --project unity/ffi-check

    # **The demonstration configuration** (story #1342), the same program over
    # `unity/demo-producer` with `DASHSCENE_DEMO_PRODUCER` defined.
    #
    # Without it the package's `ds_demo_*` declarations are compiled by nothing
    # and bound by nothing: they sit behind a `#if` that the pass above never
    # defines, so a renamed or deleted entry point would reach the demonstration
    # as a `DllNotFoundException` at run time and no gate would have said so.
    # That is issue #1308's class, and story #1342's second condition asks for
    # this pass by name.
    #
    # **It runs every check above a second time**, against a different library,
    # which is not waste: the demo library is `dashscene-ffi` plus an appendix,
    # and a shipped check failing here would mean the appendix changed something
    # it should not have.
    cargo build -p demo-producer
    demo_lib=$(cargo build -p demo-producer --message-format=json | jq -r '
        select(.reason == "compiler-artifact")
        | select(.target.name == "demo_producer")
        | .filenames[]
        | select(endswith(".dylib") or endswith(".so") or endswith(".dll"))
      ' | tail -n 1)
    if [ -z "${demo_lib}" ]; then
      echo "unity-ffi: cargo emitted no dynamic library for demo-producer." >&2
      exit 1
    fi

    DASHSCENE_FFI_LIB="${demo_lib}" DASHSCENE_FFI_FIXTURE="${fixture}" \
      DASHSCENE_FFI_TEXT_FIXTURE="${text_fixture}" DASHSCENE_FFI_CORPUS=corpus \
      DASHSCENE_FFI_STUB="${older}" \
      DASHSCENE_FFI_STUB_SKEW="${older_skew}" \
      DASHSCENE_FFI_STUB_SILENT="${older_silent}" \
      DASHSCENE_FFI_STUB_REFUSES_FREE="${older_refuses}" \
      DASHSCENE_FFI_STUB_LEASE_REFUSES="${older_lease}" \
      DASHSCENE_PACKAGE="unity/com.driftsys.dashscene" \
      DASHSCENE_FFI_EXPECT_DEMO=1 \
      dotnet run --project unity/ffi-check -p:DemoProducer=true

# Draw a document in a built PLAYER and check that ink landed where the
# committed tables put it.
#
# **The only CHECK in this repository that draws a dashscene document through
# the Unity painter.** `just unity-demo` draws as well; its `cycle` action
# asserts that every entry reached the painter, and nothing about what
# landed on the screen. Every other gate over the package compiles, links or
# executes on the CPU; `docs/design/unity-csharp-host.md` says so and this is
# what changes it.
#
# **A player, not a batchmode editor render, and that distinction is the
# reason the recipe exists at all.** Unity strips a shader that no scene or
# material references out of a player build and strips nothing in an editor —
# so `just unity-editor` passed over a package that could not draw as installed
# (issue #1313), and a batchmode editor render would have inherited that
# blindness exactly. This project adds NOTHING to Always Included Shaders: how
# the package's shaders reach a player is the package's problem, and refusing
# to configure it host-side is what makes the run an answer.
#
# **The negative control is in the run, not in this comment.** The gate
# captures a frame the painter deliberately did not draw and evaluates its own
# verdict predicate on it FIRST; a run where that frame passes fails, and says
# why. Issue #1029 is this repository's own case of a "did it draw" check
# passing over a fully black frame, and #1232 and #1191 are two more.
#
# **What a green run does not license.** It measures one graphics API — Metal
# on macOS, whatever the developer's editor targets elsewhere — over one
# document, and it asserts that ink landed where a node is, not that the ink is
# right. Issue #1195 is a measured case of the API mattering; issue #828's
# portable conformance suite is what judges the colour.
#
# Needs a Unity editor, which no CI runner here can host
# (`docs/decisions/the-native-library-ships-inside-the-unity-package.md` D4), so
# it is outside `check` and outside CI — a developer runs it before a PR
# touching `Runtime/Engine/`, `Runtime/Shaders/` or `Runtime/Resources/`.
#
# **It also writes the `.meta` files R-E2 requires**, for `unity-editor`'s
# reason: the package is imported as a `file:` dependency, which is a MUTABLE
# package, so the editor writes a `.meta` beside every asset it imports that
# lacks one — into the working tree. Check `git status` after a run that added
# a file.
#
# Draw a .dsb in a built player and assert the painter inked every node.
unity-render unity_version="6000.3.22f1" timeout="180":
    #!/usr/bin/env bash
    set -euo pipefail
    # **The editor resolution below is `unity-editor`'s, repeated.** Not
    # factored out here: that recipe is R-E10's only engine-half check and this
    # one is a different gate, and a shared helper written from one lane would
    # make a change to either able to break the other silently. **Issue #1316
    # carries factoring the copies out together** rather than any one recipe
    # doing it to the others.
    editor="${DASHSCENE_UNITY:-/Applications/Unity/Hub/Editor/{{unity_version}}/Unity.app/Contents/MacOS/Unity}"
    if [ ! -x "${editor}" ]; then
      echo "unity-render: no Unity executable at ${editor}" >&2
      echo "unity-render:   install {{unity_version}} with the Hub, pass a version:" >&2
      echo "unity-render:     just unity-render 6000.3.22f1" >&2
      echo "unity-render:   or point at one directly:" >&2
      echo "unity-render:     DASHSCENE_UNITY=/path/to/Unity just unity-render" >&2
      exit 1
    fi
    editor_dir="$(dirname "${editor}")"
    builtin=""
    for candidate in \
      "${editor_dir}/../Resources/PackageManager/BuiltInPackages" \
      "${editor_dir}/Data/PackageManager/BuiltInPackages" \
      "${editor_dir}/../Data/PackageManager/BuiltInPackages"; do
      if [ -d "${candidate}" ]; then
        builtin="$(cd "${candidate}" && pwd)"
        break
      fi
    done
    if [ -z "${builtin}" ]; then
      echo "unity-render: no BuiltInPackages directory near ${editor}" >&2
      exit 1
    fi

    root="$(git rev-parse --show-toplevel)"
    project="${root}/target/unity-render-gate"
    package="${root}/unity/com.driftsys.dashscene"
    frames="${project}/frames"

    urp_json="${builtin}/com.unity.render-pipelines.universal/package.json"
    if [ ! -f "${urp_json}" ]; then
      echo "unity-render: this editor ships no built-in URP at ${urp_json}" >&2
      exit 1
    fi
    urp="$(jq -r .version "${urp_json}")"

    # **The package's URP pin has to be one this editor can satisfy**, the check
    # `unity-editor` makes and an earlier version of this recipe dropped. A
    # UPM dependency is a MINIMUM, so only a pin the editor is BELOW is a
    # problem — and without this the whole player build runs before UPM fails
    # to resolve, which is tens of minutes to reach a one-line answer.
    pinned="$(jq -r '.dependencies["com.unity.render-pipelines.universal"] // empty' \
      "${package}/package.json")"
    if [ -n "${pinned}" ]; then
      lowest="$(printf '%s\n%s\n' "${pinned}" "${urp}" | sort -V | head -1)"
      if [ "${lowest}" != "${pinned}" ]; then
        echo "unity-render: package.json pins URP ${pinned} and this editor ships ${urp}," >&2
        echo "unity-render: which is older — a consumer of this package could not resolve it." >&2
        exit 1
      fi
    fi

    # **Nothing is staged into this project any more, and that is the point.**
    # Until story #1334 this recipe built the cdylib and copied it into
    # `Assets/Plugins/`, so the player it built resolved a library this run had
    # produced — and therefore passed whatever the package itself carried. That
    # is the class issue #1313 is an instance of: every gate green while the
    # package could not draw as installed. The library now travels inside the
    # package, by
    # `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D4,
    # and what this gate asks is whether Unity resolves it from there.
    #
    # **A file test is the right shape here, and not for the reason it first
    # seems.** `host-lib` forbids one because it asks cargo where an artifact
    # landed, and `cargo build` does not delete the artifacts of a crate type
    # that has been removed — so `[ -f … ]` over `target/` passes on a stale
    # build, which is issue #1057. There is no cargo invocation to query here:
    # D2 and D3 fix this path, so it is read rather than discovered.
    #
    # **A committed binary is still a build output, and it can still be stale.**
    # That hazard is real and this test does not address it: nothing in this
    # repository compares a shipped library against the sources of the commit
    # carrying it, and this recipe no longer builds `dashscene-ffi` at all, so a
    # green run here says nothing about the Rust sources under review. What
    # catches it is `ds_abi_version` and `DsSlice::stride` at run time, inside
    # the player this gate builds.
    #
    # The test is here rather than left to the player build, which fails deep in
    # an editor log with no path in the error.
    shipped="${package}/Runtime/Plugins/macOS/libdashscene_ffi.dylib"
    if [ ! -f "${shipped}" ]; then
      echo "unity-render: the package ships no macOS library at ${shipped}." >&2
      echo "unity-render: run \`just unity-plugins\` and commit what it writes." >&2
      exit 1
    fi

    # The same committed document `unity-ffi` checks its frame against: the
    # richest compiled fixture in the tree, and one this repository already
    # pins byte for byte.
    fixture="${root}/goldens/dsb/v03-paint.dsb"
    if [ ! -f "${fixture}" ]; then
      echo "unity-render: ${fixture} is missing; the gate needs a document" >&2
      exit 1
    fi

    # Rebuilt from scratch each run, for `unity-editor`'s reason: a reused
    # Library/ can hold a stale compiled assembly or a stale player, and this
    # gate's whole answer is about the CURRENT sources.
    rm -rf "${project}"
    mkdir -p "${project}/Packages" "${project}/ProjectSettings" \
      "${project}/Assets/Editor" \
      "${project}/Assets/StreamingAssets" "${frames}"

    cat > "${project}/Packages/manifest.json" <<JSON
    {
      "dependencies": {
        "com.driftsys.dashscene": "file:${package}",
        "com.unity.render-pipelines.universal": "${urp}"
      }
    }
    JSON

    cp "${root}/unity/render-gate/DashsceneRenderGate.cs" "${project}/Assets/"
    cp "${root}/unity/render-gate/RenderGateBuild.cs" "${project}/Assets/Editor/"
    cp "${fixture}" "${project}/Assets/StreamingAssets/document.dsb"

    cat > "${project}/ProjectSettings/ProjectSettings.asset" <<'YAML'
    %YAML 1.1
    %TAG !u! tag:unity3d.com,2011:
    --- !u!129 &1
    PlayerSettings:
      m_ObjectHideFlags: 0
      productName: RenderGate
      companyName: driftsys
      apiCompatibilityLevel: 6
      apiCompatibilityLevelPerPlatform: {}
    YAML

    log="${project}/editor.log"
    echo "unity-render: building the player in {{unity_version}} (log: ${log})"
    set +e
    "${editor}" -batchmode -quit -projectPath "${project}" \
      -executeMethod RenderGateBuild.Build -logFile "${log}"
    status=$?
    set -e
    grep -E "^\[render-gate-build\]|error CS|Shader error|Compilation failed" "${log}" || true
    if [ "${status}" -ne 0 ]; then
      echo "unity-render: the player build FAILED (exit ${status}). Full log: ${log}" >&2
      exit "${status}"
    fi

    player_path="${project}/Build/player-path.txt"
    if [ ! -f "${player_path}" ]; then
      echo "unity-render: the build wrote no ${player_path}" >&2
      exit 1
    fi
    player="$(cd "${project}" && cat Build/player-path.txt)"
    case "${player}" in /*) ;; *) player="${project}/${player}" ;; esac
    if [ ! -x "${player}" ]; then
      echo "unity-render: ${player} is not executable" >&2
      exit 1
    fi

    # **`-batchmode`, and NOT `-nographics`.** Two measurements decided this,
    # both on 6000.3.22f1, macOS/Metal, 2026-08-23:
    #
    #   - a WINDOWED player launched from a shell the window server never
    #     composites stops making progress within a few frames — its main
    #     thread and UnityGfxDeviceWorker both sit in semaphore_wait_trap
    #     waiting for a drawable — so a gate that opened a window would hang
    #     depending on where that window happened to be stacked;
    #   - `-batchmode` alone keeps the graphics device: this player reports
    #     Metal on an Apple M3 under it, and the gate refuses to run without a
    #     device (R-E14). `-nographics` is what removes the device, and it is
    #     deliberately absent.
    #
    # Batch mode renders no camera by itself, which is why the gate asks for
    # each frame through `RenderPipeline.SubmitRenderRequest` rather than
    # letting Unity draw the camera.
    #
    # **A watchdog, because a player that never quits is the failure this gate
    # is most likely to produce.** `timeout(1)` is GNU coreutils and is not on a
    # stock macOS, so the wait is written out. The player's own exit code is
    # what decides the run — never a grep over its log, which cannot tell a
    # verdict line from a line of prose that quotes one.
    player_log="${project}/player.log"
    echo "unity-render: running ${player}"
    "${player}" -batchmode -ds-out "${frames}" -logFile "${player_log}" &
    pid=$!
    waited=0
    while kill -0 "${pid}" 2>/dev/null; do
      if [ "${waited}" -ge {{timeout}} ]; then
        kill -9 "${pid}" 2>/dev/null || true
        wait "${pid}" 2>/dev/null || true
        echo "unity-render: the player did not exit within {{timeout}}s; killed." >&2
        echo "unity-render: log ${player_log}" >&2
        exit 1
      fi
      sleep 1
      waited=$((waited + 1))
    done
    set +e
    wait "${pid}"
    verdict=$?
    set -e

    if [ -f "${frames}/report.txt" ]; then
      sed 's/^/unity-render: /' "${frames}/report.txt"
    else
      echo "unity-render: the player wrote no report; its log follows" >&2
      grep -E "^\[render-gate\]|Exception|error" "${player_log}" || true
    fi

    if [ "${verdict}" -ne 0 ]; then
      echo "unity-render: FAILED (player exit ${verdict})." >&2
      echo "unity-render: frames ${frames}, log ${player_log}" >&2
      exit "${verdict}"
    fi
    # **Both, and the exit code is the one that decides.** A player killed by a
    # signal, or one that never reached its own verdict, can still leave a
    # report from an earlier run — this project is rebuilt from scratch each
    # run, so it cannot, and the check is here for the day that changes.
    if [ ! -f "${frames}/report.txt" ] || ! grep -qx "PASS" "${frames}/report.txt"; then
      echo "unity-render: the player exited 0 without recording a verdict." >&2
      exit 1
    fi
    echo "unity-render: OK — frames in ${frames}"

# The committed probe table, evaluated through the GENERATED HLSL.
#
# **This is the second consumer of `conformance/layer2-probes.json`, and the
# first that is not WGSL.** Issue #828 produced the table and one consumer,
# `crates/dashscene-gpu/tests/layer2_conformance.rs`, which dispatches
# `sdf.wgsl`. `unity/package-gate`'s
# `the_committed_hlsl_is_what_the_wgsl_compiles_to` re-derives
# `Runtime/Shaders/Sdf.hlsl` and compares the TEXT, which says the generator
# ran. Neither evaluates the generated arithmetic — issue #1312. This recipe
# does: every probe of every function, dispatched as a compute shader that
# `#include`s the package's own `Sdf.hlsl`, compared against the recorded
# expectation within that function's tolerance.
#
# **Say which backend it measured, because a pass does not generalise.** Unity
# translates the HLSL for whatever graphics device the editor obtained, which on
# macOS is Metal — not HLSL on D3D, and not the GLES 3.2 or Vulkan the target
# fleet runs. The harness reads the device back and prints it in the OK line
# rather than assuming it. Issue #1195 is a measured instance of a backend
# changing exactly this class of arithmetic: Metal folded `(o + b) - (o + a)` to
# `b - a` and erased a cancellation the shader depended on. **A pass here is not
# a pass on the fleet.**
#
# **And an editor is not a player.** Issue #1313 is the measured instance: the
# package's shaders are stripped out of a player build while every gate here
# passes. This gate resolves its compute shader through `AssetDatabase`, which
# exists only in an editor, so it says nothing about stripping. What it measures
# is arithmetic.
#
# **Not in CI, and not in `check`.**
# `docs/decisions/the-native-library-ships-inside-the-unity-package.md` D4
# records that no CI runner here can host a Unity install, which is the same
# reason `unity-editor` is outside both. Run it before opening a pull request
# that touches `sdf.wgsl`, `Sdf.hlsl` or `conformance/layer2-probes.json`.
#
# **No `-nographics`.** A compute dispatch needs a real device, and the harness
# fails rather than passing when it finds none — the same hazard `unity-editor`
# names, and the shape issue #1158 measured on an emulator.
#
# Takes the table as a parameter so `unity-conformance-negative` can point it at
# a deliberately corrupted copy. It defaults to the committed file, and nothing
# here writes to it.
#
# Evaluate every probe of the committed table through the generated Sdf.hlsl.
unity-conformance table="conformance/layer2-probes.json" unity_version="6000.3.22f1":
    #!/usr/bin/env bash
    set -euo pipefail
    # **The editor resolution below is `unity-editor`'s, repeated**, for the
    # same reason it gives: this is a developer's gate and a hardcoded macOS
    # path would put it out of reach on Linux or Windows entirely. It is the
    # SECOND copy in this tree; issue #1313's lane adds a third, and factoring
    # all of them is issue #1316 rather than a restructuring of another lane's
    # recipe from this one.
    editor="${DASHSCENE_UNITY:-/Applications/Unity/Hub/Editor/{{unity_version}}/Unity.app/Contents/MacOS/Unity}"
    if [ ! -x "${editor}" ]; then
      echo "unity-conformance: no Unity executable at ${editor}" >&2
      echo "unity-conformance:   install {{unity_version}} with the Hub, pass a version:" >&2
      echo "unity-conformance:     just unity-conformance conformance/layer2-probes.json 6000.3.22f1" >&2
      echo "unity-conformance:   or point at one directly:" >&2
      echo "unity-conformance:     DASHSCENE_UNITY=/path/to/Unity just unity-conformance" >&2
      echo "unity-conformance: docs/technotes/unity-toolchain.md records what is installed" >&2
      exit 1
    fi
    # Searched rather than derived from the executable, because the offset
    # differs per platform — `unity-editor`'s comment carries the measurement.
    editor_dir="$(dirname "${editor}")"
    builtin=""
    for candidate in \
      "${editor_dir}/../Resources/PackageManager/BuiltInPackages" \
      "${editor_dir}/Data/PackageManager/BuiltInPackages" \
      "${editor_dir}/../Data/PackageManager/BuiltInPackages"; do
      if [ -d "${candidate}" ]; then
        builtin="$(cd "${candidate}" && pwd)"
        break
      fi
    done
    if [ -z "${builtin}" ]; then
      echo "unity-conformance: no BuiltInPackages directory near ${editor}" >&2
      echo "unity-conformance: looked under ../Resources/, Data/ and ../Data/" >&2
      exit 1
    fi

    root="$(git rev-parse --show-toplevel)"
    project="${root}/target/unity-hlsl-conformance"
    package="${root}/unity/com.driftsys.dashscene"
    harness="${root}/unity/hlsl-conformance"

    # Absolute, because the editor runs with the throwaway project as its
    # working directory and the harness reads the file by an explicit path
    # rather than guessing one.
    case "{{table}}" in
      /*) table="{{table}}" ;;
      *)  table="${root}/{{table}}" ;;
    esac
    if [ ! -f "${table}" ]; then
      echo "unity-conformance: no probe table at ${table}" >&2
      exit 1
    fi
    # **Refused rather than deleted out from under the editor.** The `rm -rf`
    # below rebuilds `${project}` from scratch, so a table passed from inside
    # that directory disappears between this check and the editor's read, and
    # the run then fails for a missing file with no connection to the recipe
    # that removed it. `unity-conformance-negative` hit exactly that and
    # repaired it in the caller; the mechanism is here.
    case "${table}" in
      "${project}"/*)
        echo "unity-conformance: ${table} is inside ${project}, which this recipe" >&2
        echo "unity-conformance: deletes and rebuilds. Put the table anywhere else." >&2
        exit 1
        ;;
    esac

    # The package depends on URP, so the manifest must name the version this
    # editor ships or the resolve reaches the network for nothing.
    urp_json="${builtin}/com.unity.render-pipelines.universal/package.json"
    if [ ! -f "${urp_json}" ]; then
      echo "unity-conformance: this editor ships no built-in URP at ${urp_json}" >&2
      exit 1
    fi
    urp="$(jq -r .version "${urp_json}")"

    # **The package pins a URP version and this reads one; they must agree.**
    # `unity-editor` carries the same comparison and the same reason: a UPM
    # dependency is a MINIMUM, so an editor shipping a NEWER built-in URP is an
    # ordinary consumer configuration and only an older one is a problem. Left
    # out, the manifest below names a version under the package's own pin, UPM
    # resolves that pin from the registry, and the network round trip reading
    # `urp` exists to avoid happens anyway — offline, as a resolve failure
    # naming neither number.
    pinned="$(jq -r '.dependencies["com.unity.render-pipelines.universal"] // empty' \
      "${package}/package.json")"
    if [ -n "${pinned}" ]; then
      lowest="$(printf '%s\n%s\n' "${pinned}" "${urp}" | sort -V | head -1)"
      if [ "${lowest}" != "${pinned}" ]; then
        echo "unity-conformance: package.json pins URP ${pinned} and this editor ships" >&2
        echo "unity-conformance: ${urp}, which is older — the resolve would reach the" >&2
        echo "unity-conformance: network for the pin, or fail offline." >&2
        exit 1
      fi
    fi

    # **The committed HLSL must be what the WGSL compiles to, before an editor
    # is started over it.** This recipe's own advice is to run it after editing
    # `sdf.wgsl`, and a developer who forgot `just sdf-hlsl` would otherwise buy
    # a multi-minute editor run that evaluated the stale committed file and
    # reported it green. `package-gate`'s test is the same re-derivation the
    # sanity tier runs; it is seconds warm, and it is what makes this gate's
    # answer mean what the comment above says.
    #
    # **A name filter alone is fail-open**, measured: `cargo test <filter>` with
    # a filter that matches nothing prints `0 passed` and exits **0**. So a test
    # renamed or `#[ignore]`d — that file holds six and issue #1307's lane is
    # editing it — would leave this preflight passing over a re-derivation that
    # never ran. The run's own `1 passed` is what says the test executed; a
    # missing target exits 101 and a failing test exits non-zero, so the three
    # ways this can go wrong are covered by the two conditions below.
    probe="the_committed_hlsl_is_what_the_wgsl_compiles_to"
    if ! rederived="$(cargo test -p package-gate --test sdf_hlsl_is_generated \
         -- --exact "${probe}" 2>&1)" \
       || ! printf '%s\n' "${rederived}" | grep -q "^test result: ok\. 1 passed"; then
      printf '%s\n' "${rederived}" | tail -20 >&2
      echo "unity-conformance: the committed Sdf.hlsl is not what sdf.wgsl compiles to," >&2
      echo "unity-conformance: or ${probe} did not run." >&2
      echo "unity-conformance:   just sdf-hlsl" >&2
      echo "unity-conformance: refusing to evaluate a stale generated file." >&2
      exit 1
    fi

    # Rebuilt from scratch each run: a reused Library/ can hold a stale
    # compiled compute shader, and this gate's whole answer is what the CURRENT
    # Sdf.hlsl evaluates to. That is the shape of issue #1057.
    rm -rf "${project}"
    mkdir -p "${project}/Packages" "${project}/Assets/Editor" "${project}/ProjectSettings"

    cat > "${project}/Packages/manifest.json" <<JSON
    {
      "dependencies": {
        "com.driftsys.dashscene": "file:${package}",
        "com.unity.render-pipelines.universal": "${urp}"
      }
    }
    JSON

    # The compute shader sits in Assets/ rather than in the package: it is a
    # test fixture and shipping it would put a conformance harness in a
    # consumer's build. It reaches the arithmetic through the package's own
    # include path, so what it evaluates is the installed file and not a copy.
    cp "${harness}/DashsceneHlslConformance.cs" "${project}/Assets/Editor/"
    cp "${harness}/ProbeJson.cs" "${project}/Assets/Editor/"
    cp "${harness}/SdfConformance.compute" "${project}/Assets/"

    cat > "${project}/ProjectSettings/ProjectSettings.asset" <<'YAML'
    %YAML 1.1
    %TAG !u! tag:unity3d.com,2011:
    --- !u!129 &1
    PlayerSettings:
      m_ObjectHideFlags: 0
      productName: dashscene-hlsl-conformance
      companyName: driftsys
      apiCompatibilityLevel: 6
      apiCompatibilityLevelPerPlatform: {}
    YAML

    log="${project}/editor.log"
    echo "unity-conformance: evaluating ${table} in {{unity_version}} (log: ${log})"
    set +e
    "${editor}" -batchmode -quit -projectPath "${project}" \
      -executeMethod DashsceneHlslConformance.Run \
      -dashsceneProbeTable "${table}" -logFile "${log}"
    status=$?
    set -e

    # The harness's own report, whatever the exit code: a compile error stops
    # the editor reaching the method, and Unity prints that rather than the
    # harness.
    grep -E "^\[unity-conformance\]|error CS|Shader error|Compute shader|Compilation failed" \
      "${log}" || true

    if [ "${status}" -ne 0 ]; then
      echo "unity-conformance: FAILED (exit ${status}). Full log: ${log}" >&2
      exit "${status}"
    fi
    # **A zero exit is not the verdict.** An editor that never reached
    # `-executeMethod` — the argument dropped, `-quit` winning the race, the
    # method renamed — opens the project and exits 0, and this recipe would
    # print OK over a run that evaluated nothing. The harness's own OK line is
    # the only thing that says every probe was dispatched, and it carries the
    # backend the run measured. `ReportDevice` logs that separately before any
    # dispatch, so this is the OK line's own copy rather than the only record.
    if ! grep -q "^\\[unity-conformance\\] OK:" "${log}"; then
      echo "unity-conformance: the editor exited 0 and wrote no OK line." >&2
      echo "unity-conformance: nothing was evaluated. Full log: ${log}" >&2
      exit 1
    fi
    echo "unity-conformance: OK"

# The negative control for `unity-conformance`: a corrupted expectation must
# make it fail, and only that expectation.
#
# **A gate nobody has watched fail is a gate with no measured teeth.** This
# copies the committed table and moves TWO recorded expectations by one unit —
# `erf_approx`'s probe 7, a scalar, and `gradient_ramp`'s probe 3 component 2,
# which is a `vec4f` row. **Every index here is zero-based**, matching the `jq`
# filter, the harness labels the greps require, and the argument above the
# filter, which is where the choice of indices is made. Neither is at index
# zero, and that is the point of the pair.
# Then it requires three things of the run, and each closes a way this control
# could pass over a gate that is not working:
#
#   1. a non-zero exit — but only as the weakest of the three, since a run with
#      no editor, a bad path or a shader that did not compile all exit non-zero;
#   2. BOTH corrupted values named, which pins the index arithmetic that maps a
#      flat value back to a probe and a component. A one-component corruption
#      alone cannot: `at / 1` equals `at`, so a broken mapping is invisible;
#   3. **exactly two of 2555 values differing.** Without this, any mutation that
#      makes the gate reject EVERYTHING passes the control — setting the
#      tolerance to zero, say — because the corrupted probes are then among the
#      failures too. **It is a tight assertion on purpose and it can go red for
#      an honest reason**: `blurred_rounded_box` sits at 89 % of its tolerance,
#      so one probe drifting over on a different adapter reports "failing on
#      more than them". Read that as a backend difference to investigate, not
#      as a broken gate.
#
# It writes the corrupted copy under `target/` and never touches
# `conformance/layer2-probes.json`, which is committed truth
# (`conformance/README.md`, "Re-recording it").
#
# The positive half is `just unity-conformance` — the pair is the evidence, and
# this recipe runs only the negative half because a developer runs the other one
# anyway. Each is a full editor run.
#
# Corrupt two expectations and require `unity-conformance` to catch exactly them.
unity-conformance-negative unity_version="6000.3.22f1":
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(git rev-parse --show-toplevel)"
    # **Outside the directory `unity-conformance` rebuilds.** That recipe
    # `rm -rf`s `target/unity-hlsl-conformance` before it starts, so a
    # corrupted table written in there is deleted between this recipe's
    # `[ -f ]` check and the editor's read — and the run then fails for a
    # missing file, which is not the failure this control is measuring. The
    # first version of this recipe did exactly that, and the grep below is what
    # caught it.
    mkdir -p "${root}/target/unity-hlsl-conformance-negative"
    corrupt="${root}/target/unity-hlsl-conformance-negative/corrupted-probes.json"
    log="${root}/target/unity-hlsl-conformance/editor.log"
    # A log left by an earlier run is what the grep below would read if the
    # editor never started — and an earlier NEGATIVE run's log names the very
    # probe this one is looking for, so a missing editor would read as a pass.
    rm -f "${log}"

    table="${root}/conformance/layer2-probes.json"

    # Two values: one scalar row and one component of a four-component row.
    # `+ 1.0` is a thousand times `erf_approx`'s tolerance, a million times
    # `gradient_ramp`'s, and well inside f32 — so what fails is the comparison
    # and not an overflow.
    #
    # **Neither is at index zero, and that is the point of the pair.** The flat
    # value index maps back as `row = at / components, component = at %
    # components`, and at `at == 0` every wrong mapping agrees with the right
    # one: `0 / 4`, `0 % 4` and `0` are all zero. `gradient_ramp`'s probe 3
    # component 2 is flat index 14, which a dropped divisor reports as probe 14
    # and a swapped mapping as probe 2 component 3 — both distinguishable from
    # `probe 3[2]`, which is what the grep below requires.
    jq '(.functions[] | select(.name == "erf_approx") | .probes[7].expected) += 1.0
        | (.functions[] | select(.name == "gradient_ramp") | .probes[3].expected[2]) += 1.0' \
      "${table}" > "${corrupt}"

    # **The mutation is confirmed by reading the two values back, not by
    # comparing the files.** `jq` re-serialises the whole document — it breaks
    # the committed file's one-line arrays across lines — so `cmp` reports a
    # difference whether or not the filter matched anything, and a guard built
    # on it can never fire. Rename `erf_approx` and that guard would pass an
    # UNCORRUPTED table to the gate, which would then correctly report OK, and
    # this recipe would call a healthy gate toothless.
    for probe in 'select(.name == "erf_approx") | .probes[7].expected' \
                 'select(.name == "gradient_ramp") | .probes[3].expected[2]'; do
      before="$(jq -r ".functions[] | ${probe}" "${table}")"
      after="$(jq -r ".functions[] | ${probe}" "${corrupt}")"
      if [ -z "${before}" ] || [ "${before}" = "${after}" ]; then
        echo "negative: ${probe}" >&2
        echo "negative: reads '${before}' before and '${after}' after, so the filter" >&2
        echo "negative: matched nothing and this control proves nothing." >&2
        exit 1
      fi
    done

    echo "negative: running unity-conformance against ${corrupt}"
    set +e
    just unity-conformance "${corrupt}" "{{unity_version}}"
    status=$?
    set -e

    if [ "${status}" -eq 0 ]; then
      echo "negative: unity-conformance PASSED over a corrupted expectation." >&2
      echo "negative: the gate has no teeth. Full log: ${log}" >&2
      exit 1
    fi
    # **The log has to exist before it can be read.** Its path is written here
    # and composed there, so the two can drift — and a `grep` over a missing
    # file would turn a correct negative result into "failed for some other
    # reason". Issue #1316's factoring is where that duplication goes.
    if [ ! -f "${log}" ]; then
      echo "negative: unity-conformance wrote no log at ${log}, so nothing was read." >&2
      echo "negative: either it refused before starting an editor — read its output" >&2
      echo "negative: above — or the two recipes' log paths have drifted apart." >&2
      exit 1
    fi
    for named in "erf_approx probe 7:" "gradient_ramp probe 3\[2\]:"; do
      if ! grep -q "${named}" "${log}"; then
        echo "negative: unity-conformance failed and did not name '${named}'." >&2
        echo "negative: it exited ${status} for some other reason. Full log: ${log}" >&2
        exit 1
      fi
    done
    # Exactly the two, and nothing else. A gate that rejects everything names
    # them too, so without this the control passes over one with no teeth at
    # all — a zeroed tolerance, a broken readback, a wrong kernel everywhere.
    if ! grep -q "^\\[unity-conformance\\] 2 of 2555 value(s) differ" "${log}"; then
      echo "negative: unity-conformance did not report exactly 2 of 2555 values as" >&2
      echo "negative: differing. It named the corrupted probes, but it is failing on" >&2
      echo "negative: more than them. Full log: ${log}" >&2
      grep -m 1 "value(s) differ" "${log}" >&2 || true
      exit 1
    fi
    echo "negative: OK — the gate reported exactly the two corrupted values:"
    grep -E "probe 7:|probe 3\\[2\\]:|value\\(s\\) differ" "${log}" | head -3

# The Android API level this repository links against.
#
# A floor rather than a target: the NDK ships wrappers from 21 up, and this is
# the oldest device the artifacts will load on.
#
# **33 — Android 13 — ruled 2026-08-09**, on the target fleet rather than on
# Play. Play's requirement is about `targetSdk`, which it gates on being
# recent; it sets no minimum, so it is not what decides this number. The fleet
# is what decides it, and the fleet is Android 13/14.
#
# This is the *link* level, so it is the minimum rather than the target: a
# binary linked at 33 loads on 33 and above and refuses to load below.
#
# What that buys story #841 is worth naming, because it removes work. D6 puts
# vsync on the native side, and the NDK's own `android/choreographer.h`
# annotates each entry point with `__INTRODUCED_IN`:
#
#     AChoreographer_getInstance          24
#     AChoreographer_postFrameCallback    24, __DEPRECATED_IN(29)
#     AChoreographer_postFrameCallback64  29
#     AChoreographer_postVsyncCallback    33
#
# At a floor of 33, `postVsyncCallback` — the one carrying a frame timeline —
# is reachable **unconditionally**. No runtime API-level guard, and no
# `postFrameCallback64` fallback branch for 29-to-32 devices, which a lower
# floor would have required.
ANDROID_API := "33"

# Print the NDK's clang toolchain bin directory, or say what to install.
#
# The NDK is a documented prerequisite rather than something `bootstrap`
# installs, for the reason `web-build` gives about `wasm-bindgen-cli`: it is
# large, it is needed by one target, and every clone paying for it would be the
# wrong trade. Discovered rather than hardcoded, because the path carries a
# version that differs per machine.
[private]
_android-ndk-bin:
    #!/usr/bin/env bash
    set -euo pipefail
    # `ANDROID_NDK_ROOT` is what GitHub's runner images set; `ANDROID_NDK_HOME`
    # is the older name and what a local install usually carries. Both are
    # honoured so the same recipe serves CI and a workstation.
    ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
    if [ -z "${ndk}" ]; then
      sdk=$(just _android-sdk)
      # Highest installed version rather than first, so a machine holding
      # several does not silently pin the oldest by sort order.
      ndk=$(ls -d "${sdk}"/ndk/* 2>/dev/null | sort -V | tail -1 || true)
    fi
    if [ -z "${ndk}" ] || [ ! -d "${ndk}" ]; then
      echo "android: no NDK found. Set ANDROID_NDK_HOME, or install one:" >&2
      echo "android:   sdkmanager --install 'ndk;28.0.12674087'" >&2
      exit 1
    fi
    # One host tag per NDK, and it is the x86_64 build even on Apple silicon.
    bin=$(ls -d "${ndk}"/toolchains/llvm/prebuilt/*/bin 2>/dev/null | head -1 || true)
    if [ -z "${bin}" ]; then
      echo "android: ${ndk} has no llvm prebuilt toolchain" >&2
      exit 1
    fi
    # **Stdout here is the path and nothing else**, which is why both messages
    # above go to stderr. `_android-env` below folds this into its own stdout,
    # and three recipes `eval` that — so an informational line added here is not
    # a cosmetic change: it becomes a command, and they fail with
    # `bash: android:: command not found`. Anything for a human goes to `>&2`.
    echo "${bin}"

# Print the NDK cross-compiling wiring as shell exports, for the caller to eval.
#
# The three recipes that cross-compile — `android`, `android-lint` and
# `android-probe` — each carried their own copy of these five lines, so adding
# `RANLIB_aarch64_linux_android`, or changing the linker name when an NDK
# renames it, was three edits. A partial one fails only on whichever recipe was
# missed, and that is `android-probe`: it needs a device, so it runs least often
# (issue #1101).
#
# **Printing exports rather than running the command** is the shape
# `_android-ndk-bin` establishes one recipe above — a private recipe answers a
# question and the caller decides what to do with the answer. A wrapper that ran
# a command instead would have to carry `android`'s four `cargo build` lines
# through justfile-then-shell quoting for no gain.
#
# **Assign, then eval. Never `eval "$(just _android-env)"`.** A command
# substitution that fails inside `eval`'s argument yields the empty string,
# `eval ""` succeeds, and the recipe would carry on with no exports at all —
# reaching cargo's linker error instead of `_android-ndk-bin`'s "no NDK found".
# Assigning first keeps the `set -e` abort that the inlined copies had, because
# the status of an assignment is the status of the substitution.
#
# `%q` rather than bare interpolation: the path is read off the filesystem and
# goes back through `eval`, so an SDK under a directory with a space in it would
# otherwise be re-split into two words.
#
# **The API level is a parameter, and that is not style.** `just` passes no
# variable override into a nested `just`, so reading `{{ ANDROID_API }}` here
# would silently ignore `just ANDROID_API=34 android`: measured on just 1.38.0,
# the outer recipe sees 34 and a nested one still sees the default. Taking it as
# an argument defaulted to the variable — `api=ANDROID_API` — restores the
# override and keeps `just _android-env` standalone.
#
# It is also the one part of the Android toolchain that can now be checked
# without a cross-compile: `just _android-env` prints what the other three
# recipes will use, in about a second, and fails here rather than minutes later
# inside cargo when the wrapper it names is absent.
[private]
_android-env api=ANDROID_API:
    #!/usr/bin/env bash
    set -euo pipefail
    bin=$(just _android-ndk-bin)
    clang="${bin}/aarch64-linux-android{{ api }}-clang"
    # `_android-ndk-bin` guarantees the directory, not the per-API wrapper in
    # it. An NDK ships one wrapper per level it supports — r28 carries 21 to 35
    # — so an older NDK, or an `ANDROID_API` raised past what this one ships,
    # leaves this path pointing at nothing. Unchecked, that surfaces minutes
    # later as a linker-not-found error inside cargo, with the NDK never named.
    if [ ! -x "${clang}" ]; then
      echo "android: ${bin}" >&2
      echo "android: has no aarch64-linux-android{{ api }}-clang, so ANDROID_API" >&2
      echo "android: {{ api }} is past what this NDK supports. Install a newer NDK," >&2
      echo "android: or lower ANDROID_API in this justfile." >&2
      exit 1
    fi
    printf 'export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=%q\n' "${clang}"
    printf 'export CC_aarch64_linux_android=%q\n' "${clang}"
    printf 'export AR_aarch64_linux_android=%q\n' "${bin}/llvm-ar"

# Print the Android SDK root, honoured in the order every consumer uses.
#
# `ANDROID_HOME`, then `ANDROID_SDK_ROOT`, then the macOS default. Written once
# because a copy of it diverging is not hypothetical: `android-splitscreen`
# carried a two-level version, so on a machine exporting only
# `ANDROID_SDK_ROOT` that recipe failed to find adb while `just android` and
# both `build.sh` scripts succeeded (issue #1006 §7).
#
# It does not check the directory exists. Callers want different things from a
# missing SDK — `_android-ndk-bin` falls through to its own NDK diagnostic,
# `_android-adb` names the adb it could not find — and a check here would
# report "no SDK" for a machine that has one and is merely missing
# platform-tools.
#
# **The two `build.sh` scripts still carry their own copy**, and deliberately:
# reaching this would make `just` a prerequisite of a script that is run
# directly. They are the fourth and fifth copies, and issue #1058 §6 already
# covers what those scripts share with these recipes.
[private]
_android-sdk:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "${ANDROID_HOME:-${ANDROID_SDK_ROOT:-${HOME}/Library/Android/sdk}}"

# Print the path to `adb`, or say what to install.
#
# `android-probe` and `android-splitscreen` both need it, and each carried its
# own copy of this lookup until issue #1007 — a nested `if` with an explanatory
# comment in one, a compressed one-liner in the other. The two had already
# diverged in what they accept, which is the point: this is the shape
# `_android-ndk-bin` establishes for the NDK, applied to the SDK.
#
# **Stdout is the path and nothing else**, for the reason `_android-ndk-bin`
# gives about its own — the callers read it through a command substitution.
#
# The SDK fallback is three levels, `ANDROID_HOME` then `ANDROID_SDK_ROOT` then
# the macOS default, which is what `_android-ndk-bin` and both `build.sh`
# scripts use. `android-splitscreen` had only two of them (issue #1006 §7), so
# on a machine exporting only `ANDROID_SDK_ROOT` that recipe failed to find adb
# while `just android` and `build.sh` both succeeded.
[private]
_android-adb:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v adb >/dev/null 2>&1; then
      command -v adb
      exit 0
    fi
    sdk=$(just _android-sdk)
    adb="${sdk}/platform-tools/adb"
    # Checked, so a missing platform-tools says so. Without it the caller's next
    # step fails as "no device attached", which sends whoever reads it looking
    # for a cable rather than for an install. The NDK alone satisfies
    # `just android`, so having one without the other is the ordinary case.
    if [ ! -x "${adb}" ]; then
      echo "android: no adb on PATH and none at ${adb}" >&2
      echo "android:   sdkmanager --install platform-tools" >&2
      exit 1
    fi
    echo "${adb}"

# The demo producer's exported symbols against the shipped library's.
#
# `unity/demo-producer` is the library the demonstration player loads, and the
# claim that makes it acceptable is that it is `dashscene-ffi` PLUS an appendix
# — the same seventeen entry points, compiled from the same crate linked as an
# rlib, with `ds_demo_*` added beside them. This is what holds that claim.
#
# **It is not a formality, and it is not the `pub use` either.** Measured on
# 2026-08-26: a cdylib that names NOTHING from the `dashscene-ffi` rlib exports
# ZERO `ds_*` symbols, because the linker keeps no object nothing references —
# `#[unsafe(no_mangle)]` does not change that. One that calls into the rlib
# exports all seventeen whether or not it re-exports them, debug and release
# alike. So the shipped set surviving is a property of the link, not of a line
# anyone can read, and asserting it is the only way to know it still holds.
# Without it the demonstration would fail at the first `DllImport` and nothing
# else in this repository would have said why.
#
# **This recipe cannot catch the `pub use` being deleted**, and that is correct
# rather than a gap: the property still holds without it, which the measurement
# above is what establishes.
#
# **Takes a profile, because the link is what it inspects and the link differs
# by profile.** `unity-demo` stages `demo-release` — release plus thin LTO, see
# `[profile.demo-release]` in `Cargo.toml` for why a plain release build of this
# member does not compile at all — and passes `demo-release` here, so the
# artifact this recipe reads is the artifact the player loads. It ran on the
# debug library until the review of PR #1365, which is to say it was answering
# the question about a library nothing loads.
#
# **CI runs the DEBUG link only, and that is a stated gap rather than an
# oversight.** `demo-release` inherits `release`, so running it on a pull
# request would build the whole dependency tree optimized for a demonstration
# library no product ships. The optimized link is checked where it is staged, by
# `unity-demo`, which needs a Unity editor — so a release-only regression in
# what the linker keeps reaches no pull request. `strip`, `codegen-units = 1`
# and thin LTO are what could move it.
#
# Needs no Unity editor and no .NET SDK; CI's `demo-build` job runs it.
demo-exports profile="debug":
    #!/usr/bin/env bash
    set -euo pipefail

    case "{{profile}}" in
      debug)        flag="" ;;
      demo-release) flag="--profile demo-release" ;;
      *) echo "demo-exports: profile must be debug or demo-release, not {{profile}}" >&2
         echo "demo-exports: (there is no plain release build of demo-producer —" >&2
         echo "demo-exports:  see [profile.demo-release] in Cargo.toml for why)" >&2
         exit 1 ;;
    esac

    # The dynamic library each crate emits, from cargo rather than a `uname`
    # mapping plus a file test — `cargo build` does not delete the artifact of a
    # crate type that has been removed, so `[ -f … ]` passes over a stale one.
    # `$3...` are extra cargo arguments, so a feature-varied build reuses this
    # rather than carrying another copy of the artifact-path query. Several
    # recipes here carry one; `grep -c 'compiler-artifact' justfile` counts them.
    library() {
      local pkg="$1" target="$2"
      shift 2
      cargo build -p "${pkg}" ${flag} "$@" --message-format=json \
        | jq -r --arg target "${target}" '
            select(.reason == "compiler-artifact")
            | select(.target.name == $target)
            | .filenames[]
            | select(endswith(".dylib") or endswith(".so") or endswith(".dll"))
          ' | tail -n 1
    }

    # The `ds_*` symbols a library DEFINES.
    #
    # `$NF` and a stripped leading underscore rather than a `grep -o` for the
    # name: Mach-O prefixes every symbol with `_`, ELF does not, and a substring
    # match would also catch a `ds_` in the middle of an unrelated name.
    # **The `|| true` on the grep is what makes the empty-set guard below
    # reachable.** Under `set -euo pipefail` a `grep` that matches nothing exits
    # 1, which kills the recipe at the assignment — so the two diagnostics
    # written for exactly that case could never print, and a broken symbol
    # reader failed with no output at all. Found by the review of PR #1365 and
    # reproduced: `set -euo pipefail; x="$(printf nothing | grep ds_)"` exits
    # before the next statement. The guard below is what turns the empty result
    # into a message; this only stops the shell from exiting first.
    exported() {
      case "$(uname -s)" in
        Darwin) nm -gU "$1" ;;
        *)      nm --dynamic --defined-only "$1" ;;
      esac | awk '{ print $NF }' | sed 's/^_//' | { grep -E '^ds_' || true; } | sort -u
    }

    shipped_lib="$(library dashscene-ffi dashscene_ffi)"
    demo_lib="$(library demo-producer demo_producer)"
    for pair in "dashscene-ffi:${shipped_lib}" "demo-producer:${demo_lib}"; do
      if [ -z "${pair#*:}" ]; then
        echo "demo-exports: cargo emitted no dynamic library for ${pair%%:*}" >&2
        echo 'demo-exports: is cdylib still in its [lib] crate-type?' >&2
        exit 1
      fi
    done

    shipped="$(exported "${shipped_lib}")"
    demo="$(exported "${demo_lib}")"

    # **Checked before the comparison, because an empty set compares equal to
    # everything.** The first hand-run of this check read a stale path, both
    # sides came back empty, and it reported agreement. That is the fail-open
    # shape this refuses.
    if [ -z "${shipped}" ]; then
      echo "demo-exports: ${shipped_lib} exports no ds_* symbol at all." >&2
      echo "demo-exports: the symbol reader is broken, not the library." >&2
      exit 1
    fi

    # **The feature's own claim, checked rather than asserted in prose.**
    # `crates/dashscene-ffi/src/demo.rs` and D3 of the decision record both say
    # the shipped cdylib exports the same set with `demo-seam` on as with it
    # off — which is what makes a feature acceptable on a published crate. Until
    # the review of PR #1365 nothing held it: a `#[unsafe(no_mangle)]` added
    # inside `demo.rs` would grow the PUBLISHED crate's C surface under that
    # feature, and the comparison above would never have seen it, because it
    # builds `dashscene-ffi` with default features only.
    #
    # **Below the empty-set guard, not above it.** The first version of this
    # block sat above it, where a broken symbol reader gives two empty sets that
    # compare equal — the exact fail-open shape that guard exists to refuse.
    seam_lib="$(library dashscene-ffi dashscene_ffi --features demo-seam)"
    if [ -z "${seam_lib}" ]; then
      echo "demo-exports: cargo emitted no dynamic library for dashscene-ffi" >&2
      echo "demo-exports:   under --features demo-seam." >&2
      exit 1
    fi
    seam="$(exported "${seam_lib}")"
    if [ "${seam}" != "${shipped}" ]; then
      echo "demo-exports: --features demo-seam changes the shipped library's C surface." >&2
      echo "demo-exports: that feature is on a PUBLISHED crate and is documented as" >&2
      echo "demo-exports:   adding no exported symbol. Difference:" >&2
      diff <(printf '%s\n' "${shipped}") <(printf '%s\n' "${seam}") >&2 || true
      exit 1
    fi

    missing="$(comm -23 <(printf '%s\n' "${shipped}") <(printf '%s\n' "${demo}"))"
    if [ -n "${missing}" ]; then
      echo "demo-exports: the demo library does not export every shipped entry point." >&2
      echo "demo-exports: missing: ${missing}" >&2
      echo "demo-exports: is 'pub use dashscene_ffi::*;' still in" >&2
      echo "demo-exports:   unity/demo-producer/src/lib.rs? Without it the linker" >&2
      echo "demo-exports:   keeps none of them." >&2
      exit 1
    fi

    added="$(comm -13 <(printf '%s\n' "${shipped}") <(printf '%s\n' "${demo}"))"
    if [ -z "${added}" ]; then
      echo "demo-exports: the demo library adds no entry point to the shipped set," >&2
      echo "demo-exports: so it is the shipped library under another name and the" >&2
      echo "demo-exports: demonstration can drive nothing." >&2
      exit 1
    fi
    stray="$(printf '%s\n' "${added}" | grep -vE '^ds_demo_' || true)"
    if [ -n "${stray}" ]; then
      echo "demo-exports: the demo library adds symbols outside the ds_demo_ prefix:" >&2
      echo "demo-exports:   ${stray}" >&2
      echo "demo-exports: story #1342's first condition is that a demo entry point" >&2
      echo "demo-exports:   carries its own prefix, so a default build can be" >&2
      echo "demo-exports:   asserted not to export one." >&2
      exit 1
    fi

    # **The added count is printed, not asserted, and the difference matters.**
    # This recipe requires the added set to be non-empty and wholly
    # `ds_demo_`-prefixed; it does not pin how many there are, so a seventh
    # entry point would pass here. `unity/ffi-check`'s `expected` set is what
    # names the six, and it names what the PACKAGE declares. Four documents
    # claimed "exactly six" of this recipe until the review of PR #1365.
    echo "demo-exports: OK ({{profile}}) — $(printf '%s\n' "${shipped}" | wc -l | tr -d ' ') shipped entry \
    points present unchanged, $(printf '%s\n' "${added}" | wc -l | tr -d ' ') ds_demo_ added:"
    printf '  %s\n' ${added}

# The Unity showcase — the demonstration a person runs, not a gate. Issue #1329.
#
# Same throwaway-project shape as `unity-render`, and a **fourth** copy of that
# bring-up on purpose: issue #1316 carries factoring the copies out together,
# and a shared helper written from one lane can break the others silently.
#
# **The library is staged here, though the package ships one.** Story #1334 put
# `libdashscene_ffi.dylib` and `libdashscene_ffi.so` inside the package and
# de-staged `unity-render`; this recipe still copies the cdylib into the project.
# Issue #1352 is the follow-up, and it will meet the architecture pin
# `unity-render` carries — a macOS player at Unity's default universal
# architecture gets no library at all. It therefore
# demonstrates the package's C# and its shaders **as installed** — that is
# issue #1313's lesson and the reason the build script refuses the Always
# Included Shaders workaround — and says nothing about a released plugin layout,
# which is issue #1334.
#
# **What it shows, and through what.** Since story #1342 the list is the three
# `corpus/showcase` scenes and then the committed documents. The scenes carry
# their scripted pulse, and the one that declares a variant set carries that
# too — driven by `ds_demo_*`, which `unity/demo-producer` exports and this
# recipe stages in place of the shipped library. `include/dashscene.h` still
# exports no producer-side entry point, and signal binding is still layer 1 and
# `v1` for every host (issues #1261 and #1262), so a DOCUMENT in this list has
# no motion at all. That difference is the point rather than a gap.
#
# Needs a Unity editor, so it is outside `check` and outside CI, like
# `unity-editor` and `unity-render`. Costs tens of minutes for the same two
# reasons: the project is rebuilt from scratch each run and R-E6's `KeepAll`
# makes the player compile a large variant set.
#
# **Three actions.** `run` (the default) opens the window a person drives;
# `build` stops after the build; `cycle` walks every entry once, quits, and
# fails unless the player reported that all of them drew — the one shape that
# reports rather than being watched. Anything else is refused rather than
# treated as "do not run".
#
# **It imports the package as a `file:` dependency**, so the editor WRITES the
# `.meta` files R-E2 requires into the working tree, exactly as `unity-editor`,
# `unity-render` and `unity-conformance` do: check `git status` after a run
# that added a file.
#

# Build the Unity showcase player from this package and run it.
unity-demo unity_version="6000.3.22f1" action="run":
    #!/usr/bin/env bash
    set -euo pipefail
    # The editor resolution `unity-editor` and `unity-render` both use. Issue
    # #1316 is where the three copies are factored out together.
    editor="${DASHSCENE_UNITY:-/Applications/Unity/Hub/Editor/{{unity_version}}/Unity.app/Contents/MacOS/Unity}"
    if [ ! -x "${editor}" ]; then
      echo "unity-demo: no Unity executable at ${editor}" >&2
      echo "unity-demo:   install {{unity_version}} with the Hub, pass a version:" >&2
      echo "unity-demo:     just unity-demo 6000.3.22f1" >&2
      echo "unity-demo:   or point at one directly:" >&2
      echo "unity-demo:     DASHSCENE_UNITY=/path/to/Unity just unity-demo" >&2
      exit 1
    fi
    editor_dir="$(dirname "${editor}")"
    builtin=""
    for candidate in \
      "${editor_dir}/../Resources/PackageManager/BuiltInPackages" \
      "${editor_dir}/Data/PackageManager/BuiltInPackages" \
      "${editor_dir}/../Data/PackageManager/BuiltInPackages"; do
      if [ -d "${candidate}" ]; then
        builtin="$(cd "${candidate}" && pwd)"
        break
      fi
    done
    if [ -z "${builtin}" ]; then
      echo "unity-demo: no BuiltInPackages directory near ${editor}" >&2
      exit 1
    fi

    root="$(git rev-parse --show-toplevel)"
    project="${root}/target/unity-demo"
    package="${root}/unity/com.driftsys.dashscene"

    urp_json="${builtin}/com.unity.render-pipelines.universal/package.json"
    if [ ! -f "${urp_json}" ]; then
      echo "unity-demo: this editor ships no built-in URP at ${urp_json}" >&2
      exit 1
    fi
    urp="$(jq -r .version "${urp_json}")"

    # A UPM dependency is a MINIMUM, so only a pin the editor is BELOW is a
    # problem — and without this the whole player build runs before UPM fails to
    # resolve, which is tens of minutes to reach a one-line answer.
    pinned="$(jq -r '.dependencies["com.unity.render-pipelines.universal"] // empty' \
      "${package}/package.json")"
    if [ -n "${pinned}" ]; then
      lowest="$(printf '%s\n%s\n' "${pinned}" "${urp}" | sort -V | head -1)"
      if [ "${lowest}" != "${pinned}" ]; then
        echo "unity-demo: package.json pins URP ${pinned} and this editor ships ${urp}," >&2
        echo "unity-demo: which is older — a consumer of this package could not resolve it." >&2
        exit 1
      fi
    fi

    # **The demo producer, not the shipped library.** `demo-producer` is
    # `dashscene-ffi` linked as an rlib plus the `ds_demo_*` entry points that
    # build and drive the showcase scenes, and it carries ONE instantiation of
    # the runtime table — which is why the producer is a separate crate rather
    # than a feature of the shipped one
    # (docs/decisions/the-demo-producer-links-the-abi-rather-than-shipping-in-it.md).
    #
    # `just demo-exports` is what holds it to being the shipped library plus an
    # appendix rather than a fork of it, and it runs first so a divergence is
    # reported before tens of minutes of player build.
    just demo-exports demo-release
    #
    # From cargo rather than from a `uname` mapping plus a file test: `cargo
    # build` does not delete the artifacts of a crate type that has been
    # removed, so a `[ -f … ]` guard passes over a stale library. Release,
    # because the player runs it.
    cargo build -p demo-producer --profile demo-release
    lib=$(cargo build -p demo-producer --profile demo-release --message-format=json | jq -r '
        select(.reason == "compiler-artifact")
        | select(.target.name == "demo_producer")
        | .filenames[]
        | select(endswith(".dylib") or endswith(".so") or endswith(".dll"))
      ' | tail -n 1)
    if [ -z "${lib}" ]; then
      echo "unity-demo: cargo emitted no dynamic library for demo-producer." >&2
      echo 'unity-demo: is cdylib still in [lib] crate-type?' >&2
      exit 1
    fi

    # The documents the showcase offers, and the cascade the text one needs —
    # the same font, sheet and metrics the Android harness stages (issue #969).
    # Every input is committed; nothing here is generated at build time.
    documents=(
      "v03-paint.dsb|paint: fills, strokes, corners, and one image fill the painter refuses|0|false"
      "v07-variant-topology.dsb|layout: a variant set, at rest — nothing here can switch it|0|false"
      "v018-variant-shelf.dsb|layout: the variant shelf|0|false"
      "v07-text-hug-in-fill.dsb|text: MSDF glyphs through the cascade|0|true"
    )
    font="${root}/corpus/fonts/inter/Inter-Regular.otf"
    atlas="${root}/corpus/atlas/inter-ascii"
    for input in "${font}" "${atlas}/atlas.png" "${atlas}/atlas.metrics"; do
      if [ ! -f "${input}" ]; then
        echo "unity-demo: ${input} is missing; the text document cannot be shaded" >&2
        exit 1
      fi
    done

    # Rebuilt from scratch each run, for `unity-render`'s reason: a reused
    # Library/ can hold a stale compiled assembly or a stale player, and this
    # demonstration is about the CURRENT sources.
    rm -rf "${project}"
    mkdir -p "${project}/Packages" "${project}/ProjectSettings" \
      "${project}/Assets/Editor" "${project}/Assets/Plugins" \
      "${project}/Assets/StreamingAssets/documents" \
      "${project}/Assets/StreamingAssets/cascade"

    cat > "${project}/Packages/manifest.json" <<JSON
    {
      "dependencies": {
        "com.driftsys.dashscene": "file:${package}",
        "com.unity.render-pipelines.universal": "${urp}"
      }
    }
    JSON

    cp "${root}/unity/demo/DemoBuild.cs" "${project}/Assets/Editor/"
    # **Staged under the shipped library's name, deliberately.** Every
    # `[DllImport]` the package declares names `dashscene_ffi`, and the demo's
    # own imports name it too — which is the point: the player must load ONE
    # library, or `DashsceneRuntime` and `ds_demo_build` would resolve into two
    # instantiations of a `thread_local!` runtime table and no handle minted by
    # one would resolve in the other.
    #
    # It is a rename and not a disguise. `just demo-exports` above has already
    # asserted that this file exports the shipped seventeen unchanged, compiled
    # from the same crate, plus a set carrying only the `ds_demo_` prefix.
    # (That recipe pins the prefix, not the cardinality; `unity/ffi-check`'s
    # demonstration pass is what holds the six by name.)
    staged="${project}/Assets/Plugins/$(basename "${lib}" | sed 's/demo_producer/dashscene_ffi/')"
    cp "${lib}" "${staged}"
    echo "unity-demo: staged $(basename "${lib}") as $(basename "${staged}")"

    # **`Samples~` is hidden from Unity's importer by its `~`**, so the sample
    # is copied in rather than reached inside the package — the same reason
    # `unity-editor` copies it.
    cp "${package}"/Samples~/Showcase/*.cs "${project}/Assets/"

    cp "${font}" "${project}/Assets/StreamingAssets/cascade/Inter-Regular.otf"
    cp "${atlas}/atlas.png" "${project}/Assets/StreamingAssets/cascade/atlas.png"
    cp "${atlas}/atlas.metrics" "${project}/Assets/StreamingAssets/cascade/atlas.metrics"

    manifest="${project}/Assets/StreamingAssets/showcase.json"
    printf '{ "documents": [' > "${manifest}"
    separator=""
    for entry in "${documents[@]}"; do
      # **Checked on the raw row, before the split.** A `|` in a label is
      # taken as a field separator, so the check has to run here: after
      # `read` the label is already truncated and the damage has moved into
      # the fields behind it.
      case "${entry}" in
        *'"'* | *'\'*)
          echo "unity-demo: ${entry}" >&2
          echo "unity-demo: a row carries a quote or a backslash, which the" >&2
          echo "unity-demo: generated showcase.json cannot hold" >&2
          exit 1
          ;;
      esac
      if [ "$(printf '%s' "${entry}" | tr -cd '|' | wc -c | tr -d ' ')" != "3" ]; then
        echo "unity-demo: ${entry}" >&2
        echo "unity-demo: a row must carry exactly four fields separated by |" >&2
        exit 1
      fi

      IFS='|' read -r file label root_ordinal text <<< "${entry}"
      # **The label is written into JSON by `printf`, so it may not carry a
      # quote or a backslash.** A label that did would produce a manifest the
      # sample cannot parse, and the player would come up with no documents.
      # The labels are written a few lines above, so this refuses a mistake
      # rather than escaping around one.
      case "${label}" in
        *'"'* | *'\'*)
          echo "unity-demo: the label for ${file} carries a quote or a backslash," >&2
          echo "unity-demo: which the generated showcase.json cannot hold" >&2
          exit 1
          ;;
      esac
      if [ ! -f "${root}/goldens/dsb/${file}" ]; then
        echo "unity-demo: goldens/dsb/${file} is missing" >&2
        exit 1
      fi
      cp "${root}/goldens/dsb/${file}" "${project}/Assets/StreamingAssets/documents/"
      printf '%s{"path":"documents/%s","label":"%s","shownRoot":%s,"text":%s}' \
        "${separator}" "${file}" "${label}" "${root_ordinal}" "${text}" >> "${manifest}"
      separator=","
    done
    printf '] }\n' >> "${manifest}"

    cat > "${project}/ProjectSettings/ProjectSettings.asset" <<'YAML'
    %YAML 1.1
    %TAG !u! tag:unity3d.com,2011:
    --- !u!129 &1
    PlayerSettings:
      m_ObjectHideFlags: 0
      productName: DashsceneShowcase
      companyName: driftsys
      apiCompatibilityLevel: 6
      apiCompatibilityLevelPerPlatform: {}
    YAML

    log="${project}/editor.log"
    echo "unity-demo: building the player in {{unity_version}} (log: ${log})"
    set +e
    "${editor}" -batchmode -quit -projectPath "${project}" \
      -executeMethod DemoBuild.Build -logFile "${log}"
    status=$?
    set -e
    grep -E "^\[demo-build\]|error CS|Shader error|Compilation failed" "${log}" || true
    if [ "${status}" -ne 0 ]; then
      echo "unity-demo: the player build FAILED (exit ${status}). Full log: ${log}" >&2
      exit "${status}"
    fi

    player_path="${project}/Build/player-path.txt"
    if [ ! -f "${player_path}" ]; then
      echo "unity-demo: the build reported success and wrote no player path" >&2
      exit 1
    fi
    player="$(cat "${player_path}")"
    echo "unity-demo: player ${player}"

    if [ "{{action}}" = "build" ]; then
      echo "unity-demo: built and not run, as asked"
      exit 0
    fi

    player_log="${project}/player.log"

    # **`-logFile`, because every word this demo says goes to a log.** The
    # sample reports each document it drew, and every failure it meets, through
    # `Debug.Log`; without this they land in the platform's default player log,
    # at a path this recipe never prints. `unity-render` passes it for the same
    # reason.
    if [ "{{action}}" = "cycle" ]; then
      # The run that reports rather than the run a person watches: the player
      # walks every entry once and quits, and the log must then carry the
      # line the sample writes when all of them have drawn.
      #
      # **`-batchmode`, and not a window — the shape `unity-render` uses.**
      # A WINDOWED player stalls in `semaphore_wait_trap` waiting for a
      # drawable whenever the window server is not compositing its window, and
      # this action met that twice on 2026-08-24: run in the foreground it drew
      # the first document and then sat for one hour and fifty-four minutes,
      # and handed to `open` it did the same on an unattended machine, failing
      # at its bound after the display had been idle for seventeen minutes.
      # `-batchmode` alone keeps the graphics device — `unity-render`'s own
      # note records that measurement, and `-nographics` is what would take the
      # device away — so the painter still constructs, packs and reports.
      #
      # **What that costs is honest to state**: with no swapchain this action
      # asserts that every entry reached the painter, which is what its
      # output says, and nothing about pixels. `run` is the action with a
      # window, and `unity-render` is what reads pixels back.
      rm -f "${player_log}"

      # **Derived from the path the build reported**, not composed from the
      # product name: `DemoBuild` says in terms that one place knows where the
      # executable is, and a second copy of that name here would fall back to
      # the launch shape measured to stall the moment it drifted.
      "${player}" -batchmode -logFile "${player_log}" -cycle 3 -quit &
      player_pid=$!

      # **The player's own census first, because this recipe cannot know the
      # whole list.** It writes the manifest, so it knows the document count;
      # the scene count belongs to the staged library, which carries whatever
      # `showcase::SCENES` holds. Reading it here is what lets the deadline and
      # the expected total follow the list instead of standing beside it.
      #
      # Until story #1342 this loop grepped for a hard-coded
      # "all N document(s) drew" built from the manifest count alone. The
      # scenes made that line stop matching, the player drew all seven entries,
      # and the run failed reporting that four documents had not drawn. The
      # census is what stops the next such addition being silent.
      census=""
      for _ in $(seq 1 90); do
        if [ -f "${player_log}" ]; then
          census=$(grep -m1 -E \
            '^\[showcase\] entries: [0-9]+ \([0-9]+ scene\(s\), [0-9]+ document\(s\)\)' \
            "${player_log}" || true)
          if [ -n "${census}" ]; then break; fi
        fi
        sleep 1
      done
      if [ -z "${census}" ]; then
        kill "${player_pid}" 2>/dev/null || true
        wait "${player_pid}" 2>/dev/null || true
        grep -E "^\[showcase\]|^\[dashscene\]" "${player_log}" 2>/dev/null || true
        echo "unity-demo: the player never reported its entry census within 90 s." >&2
        echo "unity-demo: it did not reach the end of Awake. Full log at ${player_log}" >&2
        exit 1
      fi
      total=$(printf '%s' "${census}" | sed -E 's/^.*entries: ([0-9]+) .*$/\1/')
      scenes=$(printf '%s' "${census}" | sed -E 's/^.*\(([0-9]+) scene.*$/\1/')
      docs=$(printf '%s' "${census}" | sed -E 's/^.*, ([0-9]+) document.*$/\1/')
      echo "unity-demo: ${census#\[showcase\] }"

      # **The document half is checkable and the scene half is not**, so each
      # gets the strongest assertion available. This recipe wrote the manifest,
      # so a mismatch means an entry was dropped between here and the player.
      if [ "${docs}" != "${#documents[@]}" ]; then
        kill "${player_pid}" 2>/dev/null || true
        echo "unity-demo: the manifest lists ${#documents[@]} documents and the player" >&2
        echo "unity-demo: loaded ${docs}. Full log at ${player_log}" >&2
        exit 1
      fi
      # A player that carries no scene at all is one where the producer did not
      # arrive — a staged library without `ds_demo_*`, or a build that did not
      # define DASHSCENE_DEMO_PRODUCER. Both leave the documents drawing
      # perfectly, which is why this is asserted rather than assumed.
      # **Defaulted before it is compared.** `[ "" -lt 1 ]` is a syntax error
      # that returns 2, and bash reads a non-zero status here as "false" — so a
      # census line this recipe failed to parse would fall straight through the
      # guard below rather than failing it.
      case "${scenes}" in
        ''|*[!0-9]*)
          kill "${player_pid}" 2>/dev/null || true
          echo "unity-demo: could not read a scene count out of the census line:" >&2
          echo "unity-demo:   ${census}" >&2
          exit 1 ;;
      esac
      if [ "${scenes}" -lt 1 ]; then
        kill "${player_pid}" 2>/dev/null || true
        echo "unity-demo: the player carries no showcase scene. The staged library" >&2
        echo "unity-demo: exports no ds_demo_*, or the player build did not define" >&2
        echo "unity-demo: DASHSCENE_DEMO_PRODUCER. Full log at ${player_log}" >&2
        exit 1
      fi

      # Bounded, because the failure this action exists to catch is an entry
      # that never draws — which looks exactly like a player that never exits.
      # The bound follows the count and the interval rather than standing
      # beside them: every entry gets its three seconds, and the rest is slack.
      bound=$(( total * 3 + 30 ))
      drew=""
      for _ in $(seq 1 "${bound}"); do
        if grep -q "^\[showcase\] all ${total} entries drew" "${player_log}"; then
          drew="yes"
          break
        fi
        sleep 1
      done
      # This run's own process, by pid: an unscoped `pkill` would take down a
      # `run` window a developer has open from another worktree.
      kill "${player_pid}" 2>/dev/null || true
      wait "${player_pid}" 2>/dev/null || true

      # **Both prefixes.** Every failure the sample reports is a `[dashscene]`
      # line, so printing only the draws hides the reason exactly when the
      # reason is what is wanted.
      grep -E "^\[showcase\]|^\[dashscene\]" "${player_log}" || true
      if [ -z "${drew}" ]; then
        echo "unity-demo: the player did not report all ${total} entries" >&2
        echo "unity-demo: drawn within ${bound} s. Full log at ${player_log}" >&2
        exit 1
      fi

      # **A document that packed nothing is not a document that drew.** The
      # per-document line carries the count, so refusing a zero costs a grep
      # and closes the gap between reaching the painter and drawing.
      if grep -qE "^\[showcase\] drew .*: 0 instance\(s\)" "${player_log}"; then
        echo "unity-demo: an entry packed no instances at all" >&2
        echo "unity-demo: full log at ${player_log}" >&2
        exit 1
      fi
      # **Distinct scenes drew, not merely that many draws happened.** The
      # count above comes from the player's own census, so a sample that built
      # scene 0 three times would log three `drew scene` lines, none empty, and
      # pass everything up to here — measured during the review of PR #1365.
      # The names come from the library, so comparing distinct ones is the
      # cheapest thing here that can tell three scenes from one drawn thrice.
      # **`|| true`, for the reason `exported()` in `demo-exports` carries
      # one.** A `grep` matching nothing exits 1, `pipefail` propagates it and
      # `set -e` kills the recipe at this assignment — so the diagnostic below,
      # which exists to say that zero distinct scenes drew, could never print.
      # The fix round that added this line fixed that exact defect elsewhere in
      # the same commit and reintroduced it here.
      drew_scenes=$({ grep -E '^\[showcase\] drew scene ' "${player_log}" || true; } \
        | sed -E 's/^\[showcase\] drew scene ([^:]*):.*$/\1/' | sort -u \
        | grep -c . || true)
      if [ "${drew_scenes}" != "${scenes}" ]; then
        echo "unity-demo: the player reported ${scenes} scene(s) and drew ${drew_scenes}" >&2
        echo "unity-demo: distinct one(s). A scene drawn twice under two labels, or one" >&2
        echo "unity-demo: never reached. Full log at ${player_log}" >&2
        exit 1
      fi

      echo "unity-demo: all ${total} entries (${scenes} distinct scene(s), ${docs} document(s))"
      echo "unity-demo: reached the painter, none empty"
      exit 0
    fi

    if [ "{{action}}" != "run" ]; then
      echo "unity-demo: unknown action '{{action}}' — pass run, build or cycle" >&2
      exit 1
    fi

    # Foreground, windowed, and it exits when the person closes it. Left and
    # right switch documents; what the painter refused is on screen.
    #
    # **A windowed player wants a session with a window server**, and so does
    # `cycle`: both hand the bundle to `open`. This repository has measured a
    # windowed player launched from a shell that never composites it stalling
    # in `semaphore_wait_trap` waiting for a drawable — see the note in
    # `unity-render`, which is why that gate is `-batchmode`. Neither action
    # works on a session with no display at all; `cycle` at least fails at its
    # bound rather than hanging.
    echo "unity-demo: left/right switch documents, close the window to finish"
    echo "unity-demo: log at ${player_log}"
    bundle="${player%/Contents/MacOS/*}"
    if [ -d "${bundle}" ]; then
      # `-W` waits for the app to exit, which keeps this action's contract —
      # it returns when the person closes the window — while taking the launch
      # path the `cycle` note above measured. A foreground launch here is what
      # stalled.
      open -W "${bundle}" --args -logFile "${player_log}"
    else
      "${player}" -logFile "${player_log}"
    fi

# Run the Android harness's own tests — the two gates that decide what a device
# run means.
#
# `verdict.sh` decides D4's split-screen verdict from HarnessActivity's markers,
# and `assert-drew.py` is the only witness that the painter drew anything.
# **Both are reachable only through `android-splitscreen`, which needs an
# attached emulator and which nothing schedules**, so until these tests existed
# the only check either had was reading it — and reading missed five false
# verdicts across two review rounds and a black frame passing for months
# (issues #1006, #1029).
#
# Neither test needs a device, an SDK or an NDK. **The recipe as a whole costs
# about 47 s, measured on 2026-08-24** — the two named above are about two
# seconds of it and the four added since are the rest, nearly all of it the
# deliberate waiting the stub-driven tests do inside their script's own poll
# loops. `.github/workflows/ci.yml` carries the same figure; both copies moved
# together, because correcting one and not the other is how the first one drifted.
# So this is in `check`, which means `just build` runs it, and CI's
# `android-build` job runs this recipe rather than the two paths inline — the
# rule that job's own comment gives about `just android`.
#
# Run the Android harness's own gates — the verdict and the drew-anything check.
harness-tests:
    #!/usr/bin/env bash
    set -euo pipefail
    ./crates/dashscene-android/harness/verdict-test.sh
    ./crates/dashscene-android/harness/assert-drew-test.py
    # The frame-sample parser and the attach verdict (story #1229), here for the
    # same reason as the two above: both are reachable only through recipes that
    # need an attached device, so a defect in either is discovered at the device —
    # which is the one place this apparatus exists to keep clear. Neither needs a
    # device, an SDK or an NDK.
    #
    # The attach verdict is the sharper case: five of `ds_attach_outcome`'s six
    # outcomes and three of `ds_capture_state`'s four cannot be produced on an
    # emulator whose painter works, and a short timeout does not
    # produce them either — `am start -W` blocks until the activity is displayed,
    # by which time the marker has been written. Synthetic markers are the only
    # way to reach them.
    ./measure/android/frame-table-test.py
    ./measure/android/attach-outcome-test.sh
    # And the wiring between those two decisions, which neither of them
    # reaches: `attach-timing-test.sh` drives the script itself against a stub
    # `adb` and a stub `just`, so the `case` that maps a capture state onto a
    # verdict, the follower's trap and the suppression of the interval columns
    # execute here rather than only at a device.
    ./measure/android/attach-timing-test.sh
    # The same wiring in `frame-capture.sh`, which issue #1304 gave it. It is a
    # second call site rather than a second opinion: the three unreadable states
    # are decided in `lib.sh` and acted on per script, and none of them is
    # reachable on a device whose painter works — an emulator under memory
    # pressure reaches two of them, which is how they were found. None of the
    # three exits from the `case` there: the scene is degraded and the run's
    # exit comes from the closing guard, so the cases over a single scene assert
    # the guard's text as well as the arm's.
    ./measure/android/frame-capture-test.sh

# Succeed when adb reports at least one attached device.
#
# `android-probe` and `android-splitscreen` both gate on this and carried the
# same three-command test, differing only in the hint each prints after it —
# and those hints legitimately differ, which is why the test is shared and the
# messages are not. That is the other half of issue #1007: the lookup was the
# copy it named, and this is the copy one call site further on.
#
# Silent by design. It answers with its exit status, so a caller writes
# `if ! just _android-has-device; then` and prints its own diagnosis.
# What to tell an operator when adb lists no device, which is NOT the same as no
# emulator running.
#
# `_android-has-device` is false for an emulator sitting at `offline` while its
# process is alive, and every caller used to answer that by telling the operator
# to start one. That is the worst available advice: the AVD lock refuses the
# second emulator on the first one's stderr, so nothing starts — and if the
# first recovers, the run then measures the old emulator believing it is the new
# one. `docs/design/android-toolchain.md` carries the three ways it stops
# answering and what each wants.
#
# One recipe rather than a message per caller, for the reason issue #1101
# records: the copy that diverges is the one in the recipe that needs a device
# and therefore runs least often.
[private]
_android-warn-no-device name:
    #!/usr/bin/env bash
    set -euo pipefail
    n="{{ name }}"
    echo "${n}: adb lists no device, which is not the same as none running." >&2
    echo "${n}: check first — pgrep -f qemu-system" >&2
    echo "${n}:   a process alive -> it is listed 'offline'; do NOT start" >&2
    echo "${n}:      another, the AVD lock refuses it silently. See" >&2
    echo "${n}:      docs/design/android-toolchain.md." >&2
    echo "${n}:   nothing alive   -> start one, or plug a device in." >&2

[private]
_android-has-device:
    #!/usr/bin/env bash
    set -euo pipefail
    adb=$(just _android-adb)
    # The state field matched exactly — `grep -w device` also matches
    # `device.html` in adb's no-permissions line, so a device adb refuses to
    # talk to counted as attached. `measure/android/lib.sh` carries the same
    # fix and the verification.
    [ -n "$("${adb}" devices | sed '1d' | awk -F'\t' '$2 == "device"' || true)" ]

# Build the lean painter for Android — a gate, like `wasm-painter`.
#
# The second platform's compile check, and the cheapest thing that says the
# painter still cross-compiles for it. wgpu selects Vulkan or GLES on the device
# itself, so this gate says nothing about which backend a device offers; that is
# `android-probe`'s question and D3a's.
#
# Plain cargo with the NDK linker wired from the environment, rather than
# `cargo-ndk`. It matches how every other target in this repository is built and
# adds no tool to install.
# **The profile is a parameter, defaulted to `debug`** (story #1229). `just
# android` is unchanged and every existing caller — CI's `android-build`,
# `_apk-harness`, `_apk-demo` — still gets debug.
#
# `just android release` exists because story #1229's attach procedure has to
# build both halves of the comparison issue #960 records: 0.74 s to first frame
# in release against over 218 s in debug, abandoned before it completed. Until
# this parameter there was no recipe for the release half at all, so the one
# measurement in `docs/design/android-toolchain.md` was taken by hand — which is
# how a number gets into a record with no way to re-derive it.
#
# A parameter rather than a second recipe, because the alternative is a second
# copy of the four `cargo build` lines below. Issue #1101 is the record of what
# three copies of a five-line block cost here.
#
# Cross-compile the four Android members for aarch64-linux-android.
android profile="debug":
    #!/usr/bin/env bash
    set -euo pipefail
    # Assigned, then evalled — never `eval "$(just _android-env)"`, which
    # swallows a missing NDK and compiles unwired. See `_android-env`.
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    # **Refused rather than passed through.** cargo takes `--release` and not
    # `--profile debug`, so the two names are translated here; anything else is
    # a typo that would otherwise reach cargo as an unknown profile and build
    # nothing recognisable.
    case "{{ profile }}" in
      debug) flag="" ;;
      release) flag="--release" ;;
      *)
        echo "android: profile must be debug or release, not '{{ profile }}'" >&2
        exit 1
        ;;
    esac
    cargo build ${flag} -p dashscene-gpu --target aarch64-linux-android
    # And the ABI, which is what a platform host actually links. Building the
    # painter alone would leave the crate a host embeds verified on no target
    # but this machine's — story #840's Android build existed only in a
    # developer's shell until this line.
    cargo build ${flag} -p dashscene-ffi --target aarch64-linux-android
    # And the integration crate over it, which is what a Kotlin host loads.
    # It is the only member whose JNI half compiles on no other target, so
    # nothing else in the workspace would catch a break in it (story #841).
    cargo build ${flag} -p dashscene-android --target aarch64-linux-android
    # And the showcase host, which is a second such member: its `Frames`
    # implementation and its four JNI entry points are behind
    # `cfg(target_os = "android")` and compile in no other gate. It was absent
    # from this list when it landed, so several hundred lines cross-compiled
    # nowhere — the same shape as the crate above, one story along (story #842).
    cargo build ${flag} -p demo-android --target aarch64-linux-android

# Clippy the Android members on their own triple (issue #1086) — five
# packages, for the reason the note below gives.
#
# `android` above is `cargo build`, not `cargo clippy`, so until this recipe
# existed **nothing linted the code that compiles only on that triple**. That is
# the platform half of `dashscene-android` (`host.rs`, `loop_.rs`) and
# `demo-android`'s JNI half — the same code those crates' own module docs
# describe as reachable by no test. `just wasm-lint` closed the equivalent gap
# for wasm32 at PR #907; this is the Android half of the same rule, and it is
# its own recipe for the same reason: CI's `android-build` job runs exactly it,
# and a second copy of the package list in YAML is the drift issue #903 keeps
# producing.
#
# `--all-targets` rather than `--lib`, unlike `wasm-lint`'s first line: the
# whole point here is the platform half, and `dashscene-android`'s test module
# is not behind the `cfg` — linting it on this triple is what says the two
# halves of that crate still agree. That choice is what found the two defects
# this recipe's first run caught, both in a test target.
#
# **Five packages, not the four `android` builds.** `-D warnings` reaches the
# selected package and not its path dependencies (issue #903), and `showcase` —
# which `demo-android` links — carries its own `target_os = "android"` arm in
# `resources.rs`. The host `clippy` job compiles that arm out and `wasm-lint`
# does not name it either, so without this line it is denied by nothing.
#
# Needs the NDK, like `android`. It does not depend on `android`, because
# clippy's own check builds what it needs and depending on it would double the
# compile for no gate.
# Clippy and doc-link the five Android members on their own triple.
android-lint:
    #!/usr/bin/env bash
    set -euo pipefail
    # Assigned, then evalled — never `eval "$(just _android-env)"`, which
    # swallows a missing NDK and compiles unwired. See `_android-env`.
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    # One invocation for the four that take the same flags, rather than one
    # each: four would re-resolve the graph four times in what is already the
    # workflow's slowest job.
    cargo clippy \
        -p dashscene-gpu -p dashscene-ffi -p dashscene-android -p demo-android \
        --target aarch64-linux-android --all-targets -- -D warnings
    # **`showcase` is `--lib`, and that is not tidiness.** Its Android arm is in
    # `src/resources.rs`, so the lib is the whole of the coverage; its
    # `examples/still.rs` pulls the `dashscene-skia` dev-dependency, and
    # `--all-targets` therefore drags Skia into the Android graph — measured, a
    # 13.4 MB prebuilt `libskia.a` downloaded for `aarch64-linux-android`, or a
    # from-source Skia build on a release with no prebuilt for the triple. That
    # is minutes in this job for no lint coverage at all. `wasm-lint` splits its
    # first line for the same kind of reason.
    cargo clippy -p showcase --target aarch64-linux-android --lib -- -D warnings
    # Intra-doc links on this triple, for the reason `wasm-lint` carries the
    # same line: `doc-links` documents the host target, where a
    # `cfg(target_os = "android")` item does not exist, so nothing had ever
    # resolved a doc link written inside one (issue #1109). It found one — a
    # public `dashscene-android::loop_` linking into the private `frames`.
    #
    # **This is an explicit list where `wasm-lint` excludes instead, and the
    # difference is deliberate.** These five are exactly the members carrying
    # `target_os = "android"`; re-derive with
    #
    #     grep -rl 'target_os = "android"' crates/ corpus/ demo-android/
    #
    # and if that returns a member not named here, this line is stale. The
    # whole-workspace shape is the safer default and is what `wasm-lint` uses,
    # but this is the workflow's slowest job and `--all-targets` here drags
    # Skia into the Android graph for no coverage, which the clippy comment
    # above measures. `showcase` keeps its `--lib` selection for that reason.
    RUSTDOCFLAGS='-D warnings' cargo doc \
        -p dashscene-gpu -p dashscene-ffi -p dashscene-android -p demo-android \
        --target aarch64-linux-android {{ DOC_FLAGS }}
    RUSTDOCFLAGS='-D warnings' cargo doc -p showcase --lib --target aarch64-linux-android {{ DOC_FLAGS }}

# `android` above cross-compiles the four Android **Rust** members and stops
# there, so until this recipe existed **no gate compiled any Java in this
# repository**. That is issue #1030, and it is how the false handshake marker
# PR #1032 corrected could sit in `HarnessActivity` unnoticed.
#
# "No gate" rather than "no compiler": `android-splitscreen` runs the harness
# script before its device check, deliberately — see its own "Build before
# requiring a device" note — so anyone who ran that recipe did compile the two
# harness files. Nothing scheduled it. `demo-android`'s script had no caller at
# all, so its two files are the ones no one had compiled.
#
# **No device and no emulator.** Both scripts package an APK from a cross-built
# `.so` and committed inputs: the harness stages
# `goldens/dsb/v07-text-hug-in-fill.dsb` together with the cascade its text
# needs — a font file from `corpus/fonts/` and the committed MSDF sheet and
# metrics from `corpus/atlas/` (issue #969) — and the showcase ships no `.dsb`
# at all because its scenes are built in code.
# Nothing here is generated at build time, which is what makes it a runner-safe
# gate rather than a second place for the corpus to be rebuilt.
#
# It depends on `android` so a clean checkout has the libraries the scripts
# need, and it packages what that dependency built. Both scripts used to prefer
# a `release` library over a `debug` one when both existed, so a machine that
# had ever built `--release` for this target packaged the older artifact and
# said so ("using the release library"). The profile is named now, not searched
# for: `DASHSCENE_ANDROID_PROFILE`, defaulted to `debug` because that is what
# `android` builds (issue #1057).
#
# That also removes the reason to keep `--release` steps out of this job.
# Folding in `android-probe`, which builds release for this triple, no longer
# changes what gets packaged.
#
# The prerequisites are the SDK's build-tools and platform — aapt2, d8,
# zipalign, apksigner — a JDK for javac and keytool, and `zip`, which comes
# from neither: it is a system utility, and a slim image without it gets
# through aapt2, javac and d8 before failing. `bootstrap` installs none of the
# three, the same trade `android` makes for the NDK.
# **The profile threads through to both halves** (story #1229), so
# `just android-apk release` cross-compiles release and packages the release
# library. `just android-apk` is unchanged and is what CI runs.
#
# **The cross-compile happens once, here, and the halves are told so.** It cannot
# be a `just` dependency for the reason `_apk-harness` gives — a dependency's
# arguments are evaluated before the body that resolves the profile — and it must
# not be left to both halves either: a warm no-op `just android` was **measured at
# 10.2 s** on 2026-08-17, so letting each half call it added about twenty seconds
# to what `android-lint`'s own comment calls the workflow's slowest job, for two
# cargo invocations that compile nothing.
#
# `DASHSCENE_ANDROID_BUILT` is what carries that fact, and it names the profile
# rather than being a bare flag, so a mismatch still builds. A developer who
# exports it by hand is opting out of the cross-compile, which is why it is
# compared rather than merely tested for presence.
#
# Package both Android hosts into APKs — the gate that compiles their Java.
android-apk profile="debug":
    #!/usr/bin/env bash
    set -euo pipefail
    export DASHSCENE_ANDROID_PROFILE="${DASHSCENE_ANDROID_PROFILE:-{{ profile }}}"
    just android "${DASHSCENE_ANDROID_PROFILE}"
    export DASHSCENE_ANDROID_BUILT="${DASHSCENE_ANDROID_PROFILE}"
    # **Both halves run, and one run reports both** (issue #1058 §1). Under
    # `set -euo pipefail` a harness failure meant `demo-android`'s Java was
    # never compiled in that run, so breaking both hosts at once had the
    # developer fix what the log named, rerun, and fail again on the other —
    # for a gate whose whole purpose is compiling both. PR #1053's own
    # verification relied on that ordering, so the masking was observed rather
    # than hypothetical.
    #
    # Each half is `|| failed=...` rather than a `just` dependency, because a
    # failing dependency aborts the chain, which is the masking itself. The
    # message names which half went wrong: a red step whose last line is the
    # other half's success is the ambiguity this section set out to remove.
    failed=""
    just _apk-harness {{ profile }} || failed="the harness host"
    just _apk-demo {{ profile }} || failed="${failed:+${failed} and }the showcase host"
    if [ -n "${failed}" ]; then
      echo "android-apk: FAILED — ${failed}. Both halves ran; each reported above." >&2
      exit 1
    fi

# The harness APK.
#
# Split out so `android-splitscreen` can depend on **this** rather than on
# `android-apk` — issue #1058 §6 removed that recipe's inlined copy of the
# invocation, and it installs only the harness APK, so a compile error in
# `demo-android`'s Java should not stop D4's split-screen check.
#
# `DASHSCENE_ANDROID_PROFILE` is **defaulted, not assigned**: a caller who sets
# it is honoured, where an unconditional `export` would silently package
# something other than what was asked — issue #1057 inverted. The default is
# `debug` because that is what `android` builds.
#
# **The parameter sets that default, and the environment still wins** (story
# #1229). Both orders were written and this is the one that keeps issue #1057's
# ruling: a caller who exports the variable is honoured. What the parameter adds
# is that `just android-apk release` cross-compiles release *and* packages it.
#
# **The cross-compile is called from the body rather than declared as a
# dependency**, and the reason is the precedence above: a `just` dependency's
# arguments are evaluated before the body runs, so a dependency cannot be given a
# profile that the body is what resolves. Declaring `(android profile)` and
# exporting the resolved value instead is the shape that lets cargo build one
# profile while `build.sh` packages the other — issue #1057's defect with the two
# profiles swapped, and an APK shipping a library that is not the one under test
# reads as a successful build.
[private]
_apk-harness profile="debug":
    #!/usr/bin/env bash
    set -euo pipefail
    export DASHSCENE_ANDROID_PROFILE="${DASHSCENE_ANDROID_PROFILE:-{{ profile }}}"
    # Skipped only when `android-apk` above has already cross-compiled *this*
    # profile in this process tree; called alone, this still builds.
    if [ "${DASHSCENE_ANDROID_BUILT:-}" != "${DASHSCENE_ANDROID_PROFILE}" ]; then
      just android "${DASHSCENE_ANDROID_PROFILE}"
    fi
    ./crates/dashscene-android/harness/build.sh

# The showcase APK, on the same terms as `_apk-harness` above.
[private]
_apk-demo profile="debug":
    #!/usr/bin/env bash
    set -euo pipefail
    export DASHSCENE_ANDROID_PROFILE="${DASHSCENE_ANDROID_PROFILE:-{{ profile }}}"
    # Skipped only when `android-apk` above has already cross-compiled *this*
    # profile in this process tree; called alone, this still builds.
    if [ "${DASHSCENE_ANDROID_BUILT:-}" != "${DASHSCENE_ANDROID_PROFILE}" ]; then
      just android "${DASHSCENE_ANDROID_PROFILE}"
    fi
    ./demo-android/android/build.sh

# Build the D3a probe, push it to an attached device and run it.
#
# D3a of `docs/decisions/host-integration-in-three-layers.md` is recorded as **a
# risk to check rather than a measured fact**, and this is what checks it: the
# example replicates the painter's own `request_device`, so the verdict is the
# painter's rather than a comparison of two numbers that might not be the ones
# that bind.
#
# **It covers that request and no more (issue #890).** It does not cover which
# adapter a host would pick, whether a surface would offer a format the painter
# can blend in, or anything after the device request. The probe prints all three
# on every run, and `docs/design/android-toolchain.md`'s "What is not measured"
# carries them.
#
# A plain executable pushed to `/data/local/tmp` rather than an APK, because
# adapter enumeration needs no window and no Java. That keeps the probe
# available before any of the Android host exists.
#
# **An emulator result describes the host machine's GPU and is not the D3a
# measurement.** Record it as an emulator result or not at all.
# Build the D3a adapter probe, push it to an attached device and run it.
android-probe:
    #!/usr/bin/env bash
    set -euo pipefail
    # Assigned, then evalled — never `eval "$(just _android-env)"`, which
    # swallows a missing NDK and compiles unwired. See `_android-env`.
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    cargo build -p dashscene-gpu --example adapter_report --release \
      --target aarch64-linux-android
    adb=$(just _android-adb)
    if ! just _android-has-device; then
      just _android-warn-no-device android-probe
      exit 1
    fi
    "${adb}" push target/aarch64-linux-android/release/examples/adapter_report \
      /data/local/tmp/adapter_report
    "${adb}" shell chmod 755 /data/local/tmp/adapter_report
    "${adb}" shell /data/local/tmp/adapter_report

# Q-6: what one more mid-frame render-target switch costs on the attached device
# (issue #1128, story #1229).
#
# `dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER` is 8 and is a stand-in
# for a number nobody has measured — which is why `paint.render-target-budget` is
# a warning. R-T1 makes every mid-frame render-target switch a tile-memory flush,
# and the cost of that flush is a property of the target GPU, so a desktop cannot
# answer it.
#
# **Windowless, and the same shape as `android-probe` above** for the same
# reason: a plain executable pushed to `/data/local/tmp` needs no APK, no Java
# and no Activity, so it runs before anything else is installed. The two recipes
# differ only in which example they carry, and both are release builds because a
# debug one measures rustc's optimizer rather than the device.
#
# **Read the slope and never the absolute.** Every frame is read back through a
# staging buffer — the same cost at every layer count — so the per-frame figures
# carry a constant term no device frame pays. The probe fits a line over the
# sweep and prints "BELOW THIS PROBE'S RESOLUTION" rather than a number when the
# slope does not clear its own residual, which is what keeps a noise figure from
# being recorded as the Q-6 measurement. It was measured doing exactly that: at
# 30 frames per point the marginal column swung ±1.3 ms with no trend in it.
#
# **An emulator result describes the host machine's GPU** behind a translation
# layer, and is not the Q-6 measurement.
# Build the render-target cost probe, push it to a device and run it.
android-layer-cost:
    #!/usr/bin/env bash
    set -euo pipefail
    # Assigned, then evalled — never `eval "$(just _android-env)"`, which
    # swallows a missing NDK and compiles unwired. See `_android-env`.
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    cargo build -p dashscene-gpu --example layer_cost --release \
      --target aarch64-linux-android
    adb=$(just _android-adb)
    if ! just _android-has-device; then
      just _android-warn-no-device android-layer-cost
      echo "android-layer-cost: on a handheld image, start it with -gpu host —" >&2
      echo "android-layer-cost: in the default mode the painter obtains no device" >&2
      echo "android-layer-cost: and this probe reports that, not a cost (#1158)." >&2
      exit 1
    fi
    "${adb}" push target/aarch64-linux-android/release/examples/layer_cost \
      /data/local/tmp/layer_cost
    "${adb}" shell chmod 755 /data/local/tmp/layer_cost
    # **The two sweep bounds are forwarded explicitly**, because `adb shell` does
    # not carry the host's environment. Without this the probe's own
    # `DS_LAYER_MAX` and `DS_LAYER_FRAMES` overrides are unreachable through the
    # only recipe that runs it, which makes them documentation for a bound nobody
    # can apply — and they are the sole limit on this probe's duration, since every
    # other step of `android-measure` takes a timeout and a plain executable under
    # `adb shell` cannot. Empty is passed as empty, which the probe reads as "use
    # the default" rather than as zero.
    "${adb}" shell "DS_LAYER_MAX='${DS_LAYER_MAX:-}' \
      DS_LAYER_FRAMES='${DS_LAYER_FRAMES:-}' /data/local/tmp/layer_cost"

# The `gpu-timing` feature's own pass: clippy and intra-doc links over the code
# behind that cfg.
#
# **Its own recipe for the reason `doc-links`, `wasm-lint` and `prim` are** —
# CI's `clippy` job runs exactly it, and a second copy of the command in YAML is
# the drift this repository keeps hitting. Added inline to `lint` first, which
# was issue #1108 reproduced: CI runs the literal
# `cargo clippy --workspace --all-targets`, not `just lint`, and
# `--all-targets` does not imply `--all-features` — so nothing in CI compiled
# either the query-set code in `dashscene-gpu` or `examples/gpu_time.rs`, and a
# rename behind the cfg would have landed green.
#
# **The doc pass is the half clippy cannot cover**, and the same half issue
# #1109 added to `wasm-lint` and `android-lint`: an item behind an unsatisfied
# cfg is *absent* rather than private, so `just doc-links` — which runs on
# default features — resolves no link written on `GpuTiming`,
# `timing_features` or `last_gpu_time`.
#
# Clippy and the intra-doc-link pass for the `gpu-timing` feature's own code.
gpu-timing-lint:
    cargo clippy -p dashscene-gpu --all-targets --features gpu-timing -- -D warnings
    RUSTDOCFLAGS='-D warnings' cargo doc -p dashscene-gpu --features gpu-timing --no-deps --document-private-items --quiet

# GPU execution time on the attached device, from its own timestamps (epic
# #1107).
#
# **The only route to a GPU number on a retail Android device**, established by
# eliminating the others on a Pixel 5: `perfetto --query` registers no
# `gpu.counters`, the `kgsl` and `dma_fence` ftrace tracepoints exist and will
# not enable under `traced_probes` — a 20 s trace with the painter drawing
# recorded zero of them against 75 000 `sched_switch` — and `/sys/class/kgsl` is
# refused to `shell` with no `su`. Timestamp queries are what is left.
#
# **`--features gpu-timing`, which is off everywhere else.** It adds two feature
# bits to the device request — `TIMESTAMP_QUERY` and
# `TIMESTAMP_QUERY_INSIDE_ENCODERS`, both of which the encoder bracketing needs
# — where the shipped painter requests no features beyond the ASTC bit it bakes
# with. The limits are a separate axis and are untouched. The feature's comment
# in `crates/dashscene-gpu/Cargo.toml` carries why that must not depend on a
# build flag. `just android-probe` prints whether an adapter offers the queries at
# all, so a device that forecloses this says so first.
#
# Offscreen and windowless, like `android-layer-cost`: no swapchain, so the
# figure excludes the acquire and present that dominate a windowed frame's
# wall-clock. That is the point — those were measured and this is the term they
# did not contain.
#
# Build the GPU-timing probe, push it to a device and run it.
android-gpu-time:
    #!/usr/bin/env bash
    set -euo pipefail
    # Assigned, then evalled — never `eval "$(just _android-env)"`, which
    # swallows a missing NDK and compiles unwired. See `_android-env`.
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    cargo build -p dashscene-gpu --example gpu_time --features gpu-timing --release \
      --target aarch64-linux-android
    adb=$(just _android-adb)
    if ! just _android-has-device; then
      just _android-warn-no-device android-gpu-time
      exit 1
    fi
    "${adb}" push target/aarch64-linux-android/release/examples/gpu_time \
      /data/local/tmp/gpu_time
    "${adb}" shell chmod 755 /data/local/tmp/gpu_time
    # **Forwarded explicitly**, because `adb shell` does not carry the host's
    # environment — so without this the probe's own `DS_GPU_FRAMES` and
    # `DS_GPU_WARMUP` overrides are unreachable through the only recipe that
    # runs it, and they are its sole duration bound: a plain executable under
    # `adb shell` takes no timeout. `android-layer-cost` forwards its two the
    # same way and for the same reason. Empty is passed as empty, which the
    # probe reads as "use the default" rather than as zero.
    "${adb}" shell "DS_GPU_FRAMES='${DS_GPU_FRAMES:-}' \
      DS_GPU_WARMUP='${DS_GPU_WARMUP:-}' /data/local/tmp/gpu_time"

# The whole Android measurement apparatus, into one evidence bundle (story
# #1229).
#
# Five things in an order that requires the operator to decide nothing: the
# adapter probe (#885), the render-target probe (#1128), the showcase frame
# capture with its CPU sampler (#842), the vendor-neutral GPU pass, and the
# attach procedure (#960). `measure/android/run.sh` carries why that order.
#
# **It takes no measurement this repository may record.** Every number it
# produces belongs to one of those issues, and each closes when a **device** has
# run this — an emulator result describes the machine running the emulator, and
# the bundle says so in its own README rather than leaving it to whoever reads
# the directory later.
#
# **Start the emulator with `-gpu host`** (issue #1158). On the images this has
# been measured against, under the default mode the painter obtains no device,
# every frame is black, and the frame capture reports no samples after minutes.
# **Not every image**: the automotive one pins its Vulkan ICD to SwiftShader and
# gives the painter a device in the default mode, which is a CPU rasteriser and
# so answers "does it draw" and not "how fast" — see
# `docs/design/android-toolchain.md`, "The debug attach on the automotive image,
# bounded". The adapter probe runs first precisely so
# that failure surfaces in seconds.
#
# Not in `check` and not in CI: it needs an attached device, and a runner has
# none. `harness-tests` is what CI runs of this apparatus — the parser's own
# cases, which need no device.
#
# Run the whole Android measurement apparatus and write one evidence bundle.
android-measure:
    #!/usr/bin/env bash
    set -euo pipefail
    # The one adb lookup, exported for the scripts. They deliberately have no
    # lookup of their own: issue #1007 is the record of a second one honouring a
    # variable the first did not, and failing to find adb while everything else
    # succeeded.
    ADB=$(just _android-adb) ./measure/android/run.sh

# `docs/decisions/host-integration-in-three-layers.md` D4 names three cases that
# get the surface handshake wrong: rotation, backgrounding and split-screen. The
# first two have been exercised; the third had not, and
# `crates/dashscene-android/src/handshake.rs` carries six host-side unit tests
# and no instrumented one (issue #874).
#
# **This needs no target hardware and no hand gesture**, both of which #874
# assumed. Measured on 2026-08-14 against a `medium_tablet` API 35 emulator:
# `am start --windowingMode 6` is accepted and the activity lands in
# `multi-window`, which is what the issue reports as removed in Android 12. The
# catch is that it only takes effect on a **cold** launch — against a running
# activity the request is swallowed as `onActivityRestartAttempt` and the
# activity is merely brought forward, which is what makes the path look dead.
# So this force-stops first.
#
# **Start the emulator with `-gpu host`** (issue #1158). Under the default GPU
# mode the painter cannot obtain a device, the harness draws a black frame, and
# this recipe fails at `assert-drew` after about ten minutes with
# `Failed to open rendernode`. Same AVD, same APK and same commit pass end to
# end with the flag. `just android-probe` reports what the painter's own
# `request_device` gets on the attached adapter, so it is the cheap check that
# the mode is right before spending those ten minutes.
#
# The assertion is `HarnessActivity`'s own markers. `surfaceDestroyed —
# entering the handshake` is logged unconditionally on entry; exactly one of
# three lines follows it, and which one is the whole verdict:
#
#     handshake complete, returning        blocked for the loop to stop and the
#                                          surface to be dropped — D4's case
#     no drawable extent was ever reported nothing was started, so there was
#                                          nothing to hand back (issue #1094)
#     no runtime handle, nothing to hand   nativeSurfaceCreated could not get
#     back                                 the window or spawn the thread
#
# **The markers are counted and paired, not matched for presence** (issue #1006
# and its follow-up comment). The scoped log routinely holds more than one
# create/destroy cycle, because the cold `--windowingMode 6` launch and the
# Settings launch each resize the harness window. Presence alone passed when
# cycle 1 completed and cycle 2 — the split transition, the one this recipe
# exists to measure — entered `surfaceDestroyed` and never returned. Since every
# entry logs exactly one of the three exits, `entering == complete + no-drawable
# + no-handle` says every cycle finished, and a shortfall is precisely the
# use-after-free window D4 names.
#
# It is a narrow case, and deliberately not the one issue #960 describes: a GPU
# device that cannot be obtained returns a NON-ZERO handle — #960's own log
# records `runtime handle -5476376631205712496` — so it takes the handshake
# branch, and this recipe cannot tell a hung handshake from a loop that had
# already ended.
#
# **`nativeIsRunning` is not what would tell them apart**, and this comment said
# it was until issue #1080. It reports `Handshake::is_running`, which is true for
# `Starting` as well as `Running`, and the render thread reports `started()` only
# once its attach has returned — so a thread wedged *inside* an attach answers
# `true`, the same answer a drawing loop gives. Adding the call would assert the
# loop was live at the exact moment it was not.
#
# What does distinguish them is the marker pair PR #1077 put around the attach,
# read together with the failure line. `attaching a WxH surface` is written
# before every acquisition and `attached a WxH surface` after every one that
# succeeded; both of this crate's acquisitions write the pair since PR #1092.
# So, after an `attaching` line:
#
#   attached …                          the acquisition finished
#   attach failed: …                    it finished and failed, and the loop
#   could not rebuild the surface: …    stopped — NOT a wedge
#   neither, and nothing after          still inside the call: the wedge
#
# The last row — `attaching` with none of the other three after it — is the only
# one that is issue #960. Reading `attaching` with no `attached` as a wedge on
# its own would call every failed attach one, which is the same shape of wrong
# advice issue #1080 was filed to remove.
#
# A tablet profile rather than a phone: split-screen is most reliable on a large
# screen, and a panel form factor is closer to this project's target anyway.
#
# Exercise D4's split-screen case against an emulator and assert the handshake.
android-splitscreen: harness-tests _apk-harness
    #!/usr/bin/env bash
    set -euo pipefail
    adb=$(just _android-adb)
    img="system-images;android-35;google_apis_playstore;arm64-v8a"
    pkg=dev.driftsys.dashscene.harness
    act="${pkg}/dev.driftsys.dashscene.HarnessActivity"
    script="crates/dashscene-android/harness/assert-drew.py"

    # **The verdict is a file, and `harness-tests` — the first dependency above
    # — has already exercised it.** `verdict.sh` holds the logic that decides
    # PASS or FAIL and `assert-drew.py` is the only witness that the painter
    # drew. `just` runs dependencies in order and to completion before this
    # body, so a broken gate fails in about a second rather than after the
    # cross-compile, the APK build, an install and ten minutes on an emulator.
    #
    # Reading is what those tests replace, and reading has a record: it missed
    # five distinct false verdicts across two review rounds, and it missed
    # `assert-drew.py` passing a black frame for months (issue #1029).
    . crates/dashscene-android/harness/verdict.sh

    # **Dependencies, not two inlined calls** (issue #1058 §6). This ran
    # `just android` and the harness `build.sh` directly, so there were two
    # copies of that invocation and the second was exercised only by whoever had
    # an emulator attached.
    #
    # `_apk-harness` rather than `android-apk`, because this installs only the
    # harness APK and a compile error in `demo-android`'s Java should not stop
    # D4's split-screen check. `harness-tests` is named first because `just`
    # runs dependencies in order and to completion before the body, so the
    # cheap gate that needs no toolchain fails before the cross-compile rather
    # than after it.
    #
    # The build still happens before the device check, which is what the
    # inlined version was written for: a cold cross-compile needs no emulator,
    # and checking for a device minutes earlier only widens the window in which
    # it can go away.
    apk="target/android-harness/harness.apk"
    if ! just _android-has-device; then
      # `sdk` is resolved here rather than at the top: it is read only in this
      # branch, and a nested `just` per run for a string used once in an error
      # hint is waste.
      sdk=$(just _android-sdk)
      echo "android-splitscreen: the APK is built at ${apk}, but no device is attached." >&2
      echo "android-splitscreen: create the emulator once —" >&2
      echo "android-splitscreen:   avdmanager create avd -n dashscene-splitscreen \\" >&2
      echo "android-splitscreen:     -k '${img}' -d medium_tablet" >&2
      echo "android-splitscreen: then start it with a host GPU and re-run —" >&2
      echo "android-splitscreen:   ${sdk}/emulator/emulator -avd dashscene-splitscreen -gpu host &" >&2
      echo "android-splitscreen: -gpu host is required (issue #1158): the default mode" >&2
      echo "android-splitscreen: draws a black frame and fails at assert-drew." >&2
      echo "android-splitscreen: the automotive image does not offer split-screen." >&2
      exit 1
    fi

    # **The wide filter, and it is not the one the markers are counted with.**
    # The harness logs `dashscene`/`harness: …`; the native side logs the same
    # tag with no `harness:` prefix — `crates/dashscene-android/src/logging.rs`
    # — so `attaching a WxH surface`, `attached`, `attach failed:` and
    # `could not rebuild the surface:` carry no prefix at all. Those four are
    # exactly what this recipe's header names as the evidence separating a
    # wedged acquisition from a failed one, so a diagnostic dump that dropped
    # them would print a failure with none of the evidence for it (issue #1006
    # comment f).
    dump_evidence() {
      "${adb}" logcat -d 2>/dev/null \
        | grep -E "[IEW] dashscene:|Failed to open rendernode" | tail -14 || true
    }

    # **Poll for the frame rather than sleeping a fixed time**, and treat the
    # script's exit status as three cases rather than two (issue #1006 §4, §8).
    #
    # A `sleep 8` gated this: on a cold emulator the first frame can take
    # longer, and the screenshot then catches the launch animation, which reads
    # as "the painter drew nothing". Polling turns that false FAIL into a
    # slower PASS.
    #
    # **Four statuses, not three.** 1 is "the painter drew nothing", 3 is "it
    # drew and the text did not" (issue #1100), 2 is a screenshot the script
    # could not read and 127 is no python3 on PATH. The remedies have nothing in
    # common, which is why 3 is separate from 1: 1 points at the GPU device, 3
    # at the JNI text path.
    #
    # The script honours that contract since issue #1029 §2 — a truncated PNG
    # used to raise `zlib.error` past its `except (OSError, ValueError)` and
    # exit 1, which this branch reported as a painter fault.
    # `assert-drew-test.py` covers the corrupt-file statuses, though not every
    # internal handler that can produce one.
    #
    # 1 and 2 are both retried, 127 is not. A screenshot taken during the launch
    # animation reads as "drew nothing" and an interrupted `exec-out screencap`
    # is a truncated PNG — both transient. A missing interpreter cannot improve
    # by waiting.
    # **The painter's own window, from the harness's own log line** (issue
    # #1191). `screencap` captures the whole display; in multi-window the painter
    # owns about half of it and another window owns the rest, so a verdict over
    # the display is a verdict about both. `HarnessActivity.logWindowBounds`
    # writes `window bounds X,Y WxH` on every `surfaceChanged`, and the **last**
    # one is the current layout — the cold `--windowingMode 6` launch and the
    # Settings launch each resize the window, so an earlier line describes a
    # layout that is gone.
    #
    # Empty when the harness could not report them, which is what the script's
    # whole-display path is still there for: this must degrade to the old
    # behaviour rather than become unrunnable.
    # **`sed -n ... p`, never a bare substitution.** sed prints a line it did not
    # match, so a substitution here passes anything through: the harness logs
    # `window bounds unavailable — the view is not laid out` when the view is not
    # laid out yet, that line matches the grep, and the substitution then emitted
    # it verbatim as the rect. `assert-drew.py` exits 2 on it, which this
    # recipe's poll does not treat as terminal, so the run spent twenty
    # screenshots and then failed — instead of falling back to the whole display
    # the way the comment above promises. Verified against that exact line.
    #
    # The numeric shape is required by the grep as well, so a negative origin —
    # which `getLocationOnScreen` reports for a partly-offscreen window, and
    # which `[0-9]+` cannot match — takes the fallback rather than the failure.
    painter_rect() {
      "${adb}" logcat -d 2>/dev/null | tr -d '\r' \
        | grep -E "I dashscene: harness: window bounds [0-9]+,[0-9]+ [0-9]+x[0-9]+" \
        | tail -1 \
        | sed -nE 's/.*window bounds ([0-9]+),([0-9]+) ([0-9]+)x([0-9]+).*/\1,\2,\3,\4/p' \
        || true
    }

    assert_drew() {
      local shot out rc rect
      shot="target/android-harness/screen-$1.png"
      out=""
      rc=1
      for _ in $(seq 1 20); do
        "${adb}" exec-out screencap -p > "${shot}" || true
        # Re-read per iteration rather than once: the window can still be
        # settling while this polls, and a rect from before the transition would
        # survey where the window used to be.
        rect="$(painter_rect)"
        set +e
        if [ -n "${rect}" ]; then
          out=$(python3 "${script}" "${shot}" --rect="${rect}" 2>&1)
        else
          out=$(python3 "${script}" "${shot}" 2>&1)
        fi
        rc=$?
        set -e
        # Only 0 and 127 end the loop. 3 looks final — "the text did not
        # draw" — but it is exactly what the frame between the window
        # appearing and the painter's first frame looks like, so breaking on
        # it reintroduced the false FAIL this polling exists to remove. A
        # frame whose text is genuinely missing costs the full wait, which is
        # the right trade for a gate.
        if [ "${rc}" -eq 0 ] || [ "${rc}" -eq 127 ]; then break; fi
        sleep 1
      done
      printf '%s\n' "${out}"
      return "${rc}"
    }

    # One reporter for both call sites. The fullscreen call had the
    # environment-versus-painter split and the multi-window one did not, which
    # reintroduced issue #1006 §4 at the newer of the two.
    drew_or_die() {
      local phase rc
      phase="$1"
      rc=0
      assert_drew "${phase}" || rc=$?
      if [ "${rc}" -eq 0 ]; then return 0; fi
      if [ "${rc}" -eq 3 ]; then
        # **Not the painter, and not the GPU device.** Something drew, so the
        # frame loop started and the D4 reasoning below holds; what did not draw
        # is the text. Printing the no-device diagnosis here would overwrite the
        # script's correct one (issue #1100).
        echo "android-splitscreen: the ${phase} frame drew, but its text did not." >&2
        echo "android-splitscreen: the JNI text entry point, the face cascade and the" >&2
        echo "android-splitscreen: committed MSDF sheet are on that path, and nothing" >&2
        echo "android-splitscreen: else in this repository exercises them." >&2
        dump_evidence >&2
        exit 1
      fi
      if [ "${rc}" -gt 1 ]; then
        echo "android-splitscreen: assert-drew could not run on the ${phase} frame" >&2
        echo "android-splitscreen: (exit ${rc}). That is an environment failure, not a" >&2
        echo "android-splitscreen: painter verdict: 127 is no python3 on PATH, 2 is a" >&2
        echo "android-splitscreen: screenshot it could not read. Nothing is known" >&2
        echo "android-splitscreen: about the painter." >&2
        exit 1
      fi
      echo "android-splitscreen: the ${phase} process drew nothing, so a handshake" >&2
      echo "android-splitscreen: result would say nothing about D4: surfaceDestroyed" >&2
      echo "android-splitscreen: blocks for the frame loop to stop, and a loop that" >&2
      echo "android-splitscreen: never started may never signal." >&2
      echo "android-splitscreen: if the log below says 'Failed to open rendernode'," >&2
      echo "android-splitscreen: restart the emulator with -gpu host (issue #1158)." >&2
      dump_evidence >&2
      exit 1
    }

    # Uninstall rather than `install -r`. A signing key that changed makes the
    # latter fail with INSTALL_FAILED_UPDATE_INCOMPATIBLE while the device goes
    # on running the previous build — build.sh's own comment calls that the
    # worst failure it could have, and this makes it unreachable.
    "${adb}" uninstall "${pkg}" >/dev/null 2>&1 || true
    "${adb}" install "${apk}" >/dev/null
    "${adb}" shell am force-stop "${pkg}" || true
    # `|| true` on every logcat -c, like every other fallible adb call here.
    # On Android 11 and later this routinely fails with "failed to clear the
    # 'main' log" and returns non-zero; under `set -e` that aborted the run
    # with no message of its own, after the cross-compile, the APK build and
    # the install (issue #1006 §5).
    "${adb}" logcat -c || true

    # **A smoke check on the fullscreen launch**, so a black screen fails here
    # rather than after the split transition.
    echo "android-splitscreen: launching, then checking it actually drew"
    "${adb}" shell am start -W -n "${act}" >/dev/null
    drew_or_die fullscreen

    # Only now is the split transition worth measuring.
    "${adb}" shell am force-stop "${pkg}" || true
    "${adb}" logcat -c || true
    echo "android-splitscreen: relaunching cold into multi-window"
    start_out=$("${adb}" shell am start -W -n "${act}" --windowingMode 6 2>&1 || true)
    # **Matched in bash, never `printf | grep -q`.** Under `pipefail` grep -q
    # exits at its first match, printf dies on SIGPIPE, and the pipeline
    # reports 141 — measured, with the match on the first of 40000 lines — so
    # a match inverts into a miss. The rest of this recipe matches in bash for
    # the same reason.
    #
    # **`am start` exits 0 when it refuses the launch** and prints `Error:` on
    # stdout, so `>/dev/null` threw away the only evidence (issue #1006 §3).
    # `error:` in lower case is adb's own, not `am`'s — a dead connection, a
    # device that went away — and it lands in the same capture, so both cases
    # are named rather than one being reported as the other.
    if [[ "${start_out}" == *"Error:"* ]]; then
      echo "android-splitscreen: am start refused the launch —" >&2
      printf '%s\n' "${start_out}" | sed 's/^/android-splitscreen:   /' >&2
      echo "android-splitscreen: an image with no multi-window feature refuses" >&2
      echo "android-splitscreen: --windowingMode 6. The automotive image is one." >&2
      exit 1
    fi
    if [[ "${start_out}" == *"error:"* ]]; then
      echo "android-splitscreen: adb failed rather than am —" >&2
      printf '%s\n' "${start_out}" | sed 's/^/android-splitscreen:   /' >&2
      echo "android-splitscreen: the device was there a moment ago, so this is a" >&2
      echo "android-splitscreen: connection that dropped rather than anything about" >&2
      echo "android-splitscreen: windowing." >&2
      exit 1
    fi
    # **A warm start is a Warning, not an Error**, so the check above does not
    # see it: `am` prints "Activity not started, its current task has been
    # brought to the front" and exits 0, having ignored `--windowingMode`
    # entirely. The force-stop above is what should prevent it, and that call
    # is `|| true`, so this case stays reachable.
    if [[ "${start_out}" == *"brought to the front"* ]]; then
      echo "android-splitscreen: am start was swallowed as a warm start, so" >&2
      echo "android-splitscreen: --windowingMode was ignored and the activity was" >&2
      echo "android-splitscreen: merely brought forward — onActivityRestartAttempt." >&2
      echo "android-splitscreen: The force-stop above did not take effect; check" >&2
      echo "android-splitscreen: that ${pkg} actually stopped." >&2
      exit 1
    fi

    # **Scope the log to this process, by pid.** Taking the log from the
    # relaunch's own `surfaceCreated` line onward was the previous scoping, and
    # it has a failure mode of its own: that line is written at relaunch and the
    # verdict is read after a dumpsys, a second `am start` and up to 30 s of
    # polling, so on the emulator's 256 KB `main` ring the anchor can rotate out
    # while the later markers survive (issue #1006 comment a). A pid cannot
    # rotate out of the filter.
    #
    # `head -1`, because `pidof` prints every process of the package on one or
    # more lines and a two-line `--pid=` argument is rejected — which
    # `2>/dev/null` would then hide as an empty log and a confident FAIL.
    #
    # **Two fallbacks, and neither is "read the whole buffer".** An unscoped
    # read admits the fullscreen run's own markers, which is the stale-pair
    # false PASS issue #1006 §1 exists to prevent and is reachable because
    # `logcat -c` above is `|| true`. If `pidof` says nothing, or if the
    # pid-scoped read comes back empty — `--pid` unsupported, or the pid already
    # gone — fall back to the `surfaceCreated` anchor, which is what this
    # replaced rather than something weaker.
    pid=$("${adb}" shell pidof "${pkg}" 2>/dev/null | tr -d '\r' | tr ' ' '\n' \
      | grep -E '^[0-9]+$' | head -1 || true)
    anchored_run() {
      "${adb}" logcat -d 2>/dev/null | tr -d '\r' \
        | grep -E "[IEW] dashscene: harness:" \
        | awk '/surfaceCreated/{seen=1} seen' || true
    }
    if [ -n "${pid}" ]; then
      this_run() {
        "${adb}" logcat -d --pid="${pid}" 2>/dev/null | tr -d '\r' \
          | grep -E "[IEW] dashscene: harness:" || true
      }
      if [ -z "$(this_run)" ]; then
        echo "android-splitscreen: logcat --pid=${pid} returned nothing; anchoring" >&2
        echo "android-splitscreen: on surfaceCreated instead." >&2
        this_run() { anchored_run; }
      fi
    else
      echo "android-splitscreen: no pid for ${pkg}; anchoring on surfaceCreated" >&2
      this_run() { anchored_run; }
    fi

    sleep 5
    # **The Task line is the anchor, and that is deliberate rather than left
    # over.** `mWindowingMode` is printed on the ConfigurationContainer dump
    # that precedes the ActivityRecords, and this anchor is the one that has
    # actually produced a correct reading on a device — 2026-08-14, the
    # measurement this recipe's header records. An ActivityRecord anchor was
    # written here and reverted: it is unverifiable without an emulator, and if
    # the mode is not printed inside that block it resolves empty and every run
    # fails at the gate below. What #1006 §6 is unambiguously right about is
    # the interpolation: `${pkg}` went into an ERE, so every `.` in
    # `dev.driftsys.dashscene.harness` matched any character. `-F` fixes that
    # without changing what is read.
    #
    # **Verified on a device on 2026-08-16**, which is what settled it: this
    # reads `mWindowingMode=multi-window` on an API 35 `medium_tablet` emulator
    # in split-screen, across three consecutive runs of this recipe. The
    # ActivityRecord anchor was written, could not be checked, and was reverted
    # before that run — so the version that ships is the one with evidence.
    mode=$("${adb}" shell dumpsys activity activities 2>/dev/null | tr -d '\r' \
      | grep -F -A12 ":${pkg}" | grep -oE "mWindowingMode=[a-z-]+" | head -1 || true)
    if [ "${mode}" != "mWindowingMode=multi-window" ]; then
      echo "android-splitscreen: expected multi-window, got '${mode:-nothing}'" >&2
      echo "android-splitscreen: am start neither refused the launch nor reported a" >&2
      echo "android-splitscreen: warm start, so this is the mode the window manager" >&2
      echo "android-splitscreen: gave it — or the dump did not name the task." >&2
      exit 1
    fi

    # **Assert this process drew, before Settings joins the split.** The verdict
    # below comes from this process and nothing checked that it had drawn
    # (issue #1006 §2) — but the check has to happen *now*: `screencap` captures
    # the whole display, so once Settings occupies the other half its pixels
    # alone clear `assert-drew.py`'s threshold and a black harness half would
    # pass. The activity is already in multi-window, which the gate above just
    # asserted, so this is the right process in the right mode.
    drew_or_die multiwindow

    # **The baseline, and it is what makes the verdict about the split.** The
    # cold `--windowingMode 6` launch resizes the harness window itself, so a
    # complete create/destroy cycle can already be in the log here. Counting
    # absolutely would let that cycle satisfy the verdict without the
    # transition this recipe exists to measure ever being observed.
    ds_tally "$(this_run)"
    base_entering=${ds_entering}
    base_complete=${ds_complete}
    base_nohandle=${ds_nohandle}
    echo "android-splitscreen: ${mode}; putting a second app in the other half"
    # Checked, for the reason the `am start` above is: `am` exits 0 and prints
    # `Error: Activity class {...} does not exist.` on an image without
    # Settings, and discarding that reported "the split transition destroyed no
    # surface" — a use-after-free diagnosis for a missing app.
    settings_out=$("${adb}" shell am start -n com.android.settings/.Settings \
      --windowingMode 6 2>&1 || true)
    if [[ "${settings_out}" == *"Error:"* ]]; then
      echo "android-splitscreen: could not put Settings in the other half —" >&2
      printf '%s\n' "${settings_out}" | sed 's/^/android-splitscreen:   /' >&2
      echo "android-splitscreen: without a second app there is no split transition" >&2
      echo "android-splitscreen: to measure, so this is not a D4 result." >&2
      exit 1
    fi

    # One dump per iteration, assigned once. The break test used to evaluate
    # the whole pipeline and throw the output away, then the next line ran it
    # again — an extra adb round trip per iteration, and a window in which the
    # buffer could change between the test and the read (issue #1006 comment e).
    #
    # It ends when the split has produced an entry AND every entry has reached
    # one of its three exits, rather than only on the completion marker. A
    # no-handle run logs no completion marker at all, so it used to run all 30
    # iterations before the check that names it fired (comment b).
    log=""
    for _ in $(seq 1 30); do
      log=$(this_run)
      ds_tally "${log}"
      if ds_settled "${base_entering}"; then break; fi
      sleep 1
    done
    # **A settle window, because balanced is not the same as finished.** The
    # split can produce two cycles; if the first completes before the second
    # enters, the loop breaks on a snapshot in which every entry has returned
    # and a later hang is never seen. Three more seconds and a re-count close
    # most of that window — not all of it, which is why it is a re-count rather
    # than a claim.
    sleep 3
    log=$(this_run)
    ds_tally "${log}"
    printf '%s\n' "${log}"
    echo "android-splitscreen: ${ds_entering} entered, ${ds_complete} completed, ${ds_nohandle} with no handle, ${ds_nodrawable} with no drawable (before the split: ${base_entering}, ${base_complete}, ${base_nohandle})"

    case "$(ds_verdict "${base_entering}" "${base_complete}" "${base_nohandle}")" in
      PASS)
        echo "android-splitscreen: PASS — drew a frame in multi-window, the split destroyed $((ds_entering - base_entering)) surface(s), and every one returned ($((ds_complete - base_complete)) ran the handshake)"
        ;;
      FAIL:split-destroyed-nothing)
        echo "android-splitscreen: FAIL — the split transition destroyed no surface," >&2
        echo "android-splitscreen: so D4's split-screen case did not run. Everything" >&2
        echo "android-splitscreen: counted predates putting Settings in the other half." >&2
        exit 1
        ;;
      FAIL:entered-never-returned)
        stuck=$((ds_entering - ds_complete - ds_nohandle - ds_nodrawable))
        echo "android-splitscreen: FAIL — ${stuck} of ${ds_entering} entries never returned." >&2
        echo "android-splitscreen: That is the use-after-free window D4 names: the" >&2
        echo "android-splitscreen: callback must block until the surface is dropped and" >&2
        echo "android-splitscreen: then return." >&2
        dump_evidence >&2
        exit 1
        ;;
      FAIL:split-had-no-handle)
        echo "android-splitscreen: FAIL — $((ds_nohandle - base_nohandle)) of the split's" >&2
        echo "android-splitscreen: own surfaceDestroyed calls found no runtime handle, so" >&2
        echo "android-splitscreen: nativeSurfaceCreated could not get the window or spawn" >&2
        echo "android-splitscreen: the thread. This is NOT the no-GPU-device case, which" >&2
        echo "android-splitscreen: returns a non-zero handle (issue #960)." >&2
        dump_evidence >&2
        exit 1
        ;;
      FAIL:split-ran-no-handshake)
        echo "android-splitscreen: FAIL — the split transition's own surfaceDestroyed" >&2
        echo "android-splitscreen: returned without running a handshake, so D4's case" >&2
        echo "android-splitscreen: never executed for it. An earlier cycle completing is" >&2
        echo "android-splitscreen: not that measurement. ${ds_nodrawable} of the ${ds_entering}" >&2
        echo "android-splitscreen: entries never carried a drawable extent (issue #1094)." >&2
        dump_evidence >&2
        exit 1
        ;;
    esac

#
# `wasm-bindgen` post-processes the module cargo produced into the JS glue a
# page imports. The CLI's version and the `wasm-bindgen` crate's are two halves
# of one ABI: a mismatch fails in the browser rather than at build time, so the
# pair is checked here instead of being discovered there.
#
# The CLI is not installed by `bootstrap`. It builds from source in minutes and
# is needed only by this demonstration, so every clone paying for it would be
# the wrong trade; the check below prints the exact command instead.
# Assemble the browser host into `target/web`, ready to serve.
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

#
# The server is `demo-web/serve.py` rather than `python3 -m http.server`, which
# does not implement `Range`. Without ranges the host still draws — it notices
# the whole file arrived — but the prefix loading this story exists to
# demonstrate never happens.
# Serve the browser host on 127.0.0.1, with byte ranges honoured.
web port="8787": web-build
    python3 demo-web/serve.py target/web {{ port }}

# Type-check the Deno importer's entry points.
deno-check:
    cd importers/figma && deno task check

# Run the Deno importer's test suite. Depends on `wasm`: the suite loads
# dashc_wasm.wasm and asserts its output against the golden .dsb.
# Run the Deno importer's tests, against a freshly built `dashc_wasm.wasm`.
deno-test: wasm
    cd importers/figma && deno task test

# Format the Deno importer sources.
deno-fmt:
    cd importers/figma && deno task fmt

# Check the Deno importer sources are already formatted, without rewriting
# them. Matches the CI deno job's formatting gate (.github/workflows/ci.yml);
# `deno-fmt` alone cannot fail, so this is the recipe that actually gates it.
# Check the Deno importer's formatting — the recipe that actually gates it.
deno-fmt-check:
    cd importers/figma && deno task fmt --check

# Capture the Figma fixture corpus, image-fill bytes included. Needs
# FIGMA_TOKEN (docs/decisions/figma-access-plan-and-pat-policy.md). Never commit the token.
# Capture the Figma fixture corpus from live Figma. Needs a PAT.
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
# Import one live Figma file and print the diagnostics it raised.
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
# Import one live Figma node and render it under a Gfx QA profile.
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
# Render the Gfx QA triptych, and state the viewing conditions it needs.
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
