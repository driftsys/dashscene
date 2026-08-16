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

# Run the sanity tier — the loop between edits and before every commit.
# About 5 s. Tier definitions and the schedule: docs/decisions/test-tiers.md.
#
# `cargo test --doc` rides along in every tier recipe because nextest does not
# run doctests. Leaving it out would mean three recipes that each claim to run
# a tier and each silently skip the same three tests.
test:
    cargo nextest run --workspace -P sanity
    cargo test --workspace --doc

# Run the regression tier — what `check`, `build` and the CI `test` job run.
# About 33 s locally when warm, 154 s for the whole workspace suite. This is the
# gate; `just test` is not, and the pre-push hook no longer runs it either
# (see `verify`).
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

# Rust + markdown + Deno lint gate: clippy, cargo fmt check, prim, deno fmt
# check.
lint: deno-fmt-check
    cargo clippy --workspace --all-targets -- -D warnings
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

# The Markdown/JSON/YAML/TOML gate, in the two verbs it takes.
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

# Copy the root LICENSE and NOTICE into every publishable crate.
#
# Cargo packages only files under a crate's own directory, so the root copies
# reach no `.crate`. Apache-2.0 §4(a) and §4(d) require both to travel with the
# code — §4(d) in particular carries Arm's attribution for the vendored astcenc
# sources. `every_publishable_crate_packages_the_licence_and_notice` fails the
# build if a copy goes missing or drifts.
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
    echo "licenses: refreshed LICENSE and NOTICE in $n crates"

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
    trap 'rm -rf "$work"' EXIT

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
    # It does NOT follow that .gitleaksignore is inert here, which this
    # comment claimed until 2026-08-15. gitleaks also matches a bare
    # <file>:<rule>:<line> in git mode, so a fingerprint silences that path
    # and line in EVERY commit that carries it. Measured over --all on that
    # date: 101 findings with the file removed, 63 with it in place. The
    # baseline therefore adjudicates only what the fingerprints leave, and a
    # value pinned at a line it also occupies in history never reaches this
    # comparison at all. Issue #987 carries the measurement and the options;
    # until it is settled, do not read a clean run here as "every historical
    # value is triaged". Note also that `-i <dir>` does not disable the file
    # — remove it to reproduce.
    echo "── gitleaks: history ($scope_label, against the triaged baseline) ──"
    hist="$work/history.json"
    rc=0
    gitleaks git --log-opts="$range" --config .gitleaks.toml \
        --report-format json --report-path "$hist" >/dev/null 2>&1 || rc=$?
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
    echo "history: clean — $(jq 'length' "$hist") findings, all in the $(wc -l < "$work/baseline" | tr -d ' ') triaged pairs"

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
check: test-regression lint audit secrets wasm-painter wasm-host c-abi

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

# What an embedder's runtime actually weighs (issue #776, story #795).
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

# What would have to be true before anything is published (story #795).
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
    cargo publish -p dashscene-unity
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
    # The eight exclusions are the members that do not compile for this triple:
    # `dashscene-desktop` and `dashscene-ffi` fail outright, and the other six
    # carry native build scripts (skia, astcenc, zstd, nv-flip). Warm, this
    # costs 1.1 s — no more than the four-package list it replaces.
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --exclude dashscene-desktop --exclude dashscene-ffi --exclude dashscene-android --exclude dashscene-skia --exclude dashpack --exclude dashpack-astcenc-sys --exclude demo --exclude goldens --target wasm32-unknown-unknown {{ DOC_FLAGS }}

# Exercise the C ABI as a C caller, against its own header.
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
      sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-${HOME}/Library/Android/sdk}}"
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
android:
    #!/usr/bin/env bash
    set -euo pipefail
    # Assigned, then evalled — never `eval "$(just _android-env)"`, which
    # swallows a missing NDK and compiles unwired. See `_android-env`.
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    cargo build -p dashscene-gpu --target aarch64-linux-android
    # And the ABI, which is what a platform host actually links. Building the
    # painter alone would leave the crate a host embeds verified on no target
    # but this machine's — story #840's Android build existed only in a
    # developer's shell until this line.
    cargo build -p dashscene-ffi --target aarch64-linux-android
    # And the integration crate over it, which is what a Kotlin host loads.
    # It is the only member whose JNI half compiles on no other target, so
    # nothing else in the workspace would catch a break in it (story #841).
    cargo build -p dashscene-android --target aarch64-linux-android
    # And the showcase host, which is a second such member: its `Frames`
    # implementation and its four JNI entry points are behind
    # `cfg(target_os = "android")` and compile in no other gate. It was absent
    # from this list when it landed, so several hundred lines cross-compiled
    # nowhere — the same shape as the crate above, one story along (story #842).
    cargo build -p demo-android --target aarch64-linux-android

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

# Package both Android hosts into APKs — the gate that compiles their Java.
#
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
# need. It does **not** guarantee they package what that dependency just built:
# both scripts prefer a `release` library over a `debug` one when both exist,
# and `android` builds debug, so a machine that has ever built `--release` for
# this target packages the older artifact and says so ("using the release
# library"). Issue #1057 carries it.
#
# What keeps that off CI is **the step list, not the checkout**: `android-build`
# restores `target/` from `Swatinem/rust-cache`, so it is not clean, and no step
# in it builds `--release` for this triple. Adding one — folding in
# `android-probe`, say, which does — would carry a release tree forward in the
# cache and make #1057 reachable there.
#
# The prerequisites are the SDK's build-tools and platform — aapt2, d8,
# zipalign, apksigner — a JDK for javac and keytool, and `zip`, which comes
# from neither: it is a system utility, and a slim image without it gets
# through aapt2, javac and d8 before failing. `bootstrap` installs none of the
# three, the same trade `android` makes for the NDK.
android-apk: android
    #!/usr/bin/env bash
    set -euo pipefail
    ./crates/dashscene-android/harness/build.sh
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
android-probe:
    #!/usr/bin/env bash
    set -euo pipefail
    # Assigned, then evalled — never `eval "$(just _android-env)"`, which
    # swallows a missing NDK and compiles unwired. See `_android-env`.
    android_env=$(just _android-env {{ ANDROID_API }})
    eval "${android_env}"
    cargo build -p dashscene-gpu --example adapter_report --release \
      --target aarch64-linux-android
    # An `x && y` one-liner would rely on the `set -e` exemption for non-final
    # commands in an `&&` list, which is exactly the kind of thing that is true
    # today and breaks when someone reorders the line.
    if command -v adb >/dev/null 2>&1; then
      adb=adb
    else
      adb="${ANDROID_HOME:-${HOME}/Library/Android/sdk}/platform-tools/adb"
      # Checked, so a missing platform-tools says so. Without this the next
      # step's failure is reported as "no device attached", which sends whoever
      # reads it looking for a cable rather than for an install. The NDK alone
      # satisfies `just android`, so having one without the other is the
      # ordinary case rather than a corner.
      if [ ! -x "${adb}" ]; then
        echo "android-probe: no adb on PATH and none at ${adb}" >&2
        echo "android-probe:   sdkmanager --install platform-tools" >&2
        exit 1
      fi
    fi
    if [ -z "$("${adb}" devices | sed '1d' | grep -w device || true)" ]; then
      echo "android-probe: no device attached — start an emulator or plug one in" >&2
      exit 1
    fi
    "${adb}" push target/aarch64-linux-android/release/examples/adapter_report \
      /data/local/tmp/adapter_report
    "${adb}" shell chmod 755 /data/local/tmp/adapter_report
    "${adb}" shell /data/local/tmp/adapter_report

# Exercise D4's split-screen case against an emulator, end to end, and assert
# the handshake completed.
#
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
# The assertion is `HarnessActivity`'s own markers. `surfaceDestroyed —
# entering the handshake` says the callback was reached; `surfaceDestroyed —
# handshake complete, returning` is logged only after it has blocked for the
# frame loop to stop and the surface to be dropped. Reaching the first without
# the second is precisely the use-after-free D4 exists to prevent, so both are
# required. Both are checked for presence, not for order.
#
# A third marker, `no runtime handle, nothing to hand back`, is the case where
# `nativeSurfaceCreated` could not obtain the window or spawn the thread. Until
# 2026-08-15 the completion marker was logged outside its guard, so that case
# emitted it having handed nothing back and this recipe reported a PASS for a
# teardown that never happened (issue #1006).
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
android-splitscreen:
    #!/usr/bin/env bash
    set -euo pipefail
    sdk="${ANDROID_HOME:-${HOME}/Library/Android/sdk}"
    img="system-images;android-35;google_apis_playstore;arm64-v8a"
    pkg=dev.driftsys.dashscene.harness
    act="${pkg}/dev.driftsys.dashscene.HarnessActivity"
    if command -v adb >/dev/null 2>&1; then adb=adb; else adb="${sdk}/platform-tools/adb"; fi
    if ! command -v adb >/dev/null 2>&1 && [ ! -x "${adb}" ]; then
      echo "android-splitscreen: no adb on PATH and none at ${adb}" >&2
      echo "android-splitscreen:   sdkmanager --install platform-tools" >&2
      exit 1
    fi
    # **Build before requiring a device.** A cold `just android` is a
    # multi-minute cross-compile and needs no emulator; checking for a device
    # first and using it minutes later only widens the window in which it can
    # go away. build.sh reports a missing libdashscene_android.so by name, so
    # without `just android` this stops one step short.
    just android
    ./crates/dashscene-android/harness/build.sh
    apk="target/android-harness/harness.apk"
    if [ -z "$("${adb}" devices | sed '1d' | grep -w device || true)" ]; then
      echo "android-splitscreen: the APK is built at ${apk}, but no device is attached." >&2
      echo "android-splitscreen: create the emulator once —" >&2
      echo "android-splitscreen:   avdmanager create avd -n dashscene-splitscreen \\" >&2
      echo "android-splitscreen:     -k '${img}' -d medium_tablet" >&2
      echo "android-splitscreen: then start it and re-run —" >&2
      echo "android-splitscreen:   ${sdk}/emulator/emulator -avd dashscene-splitscreen &" >&2
      echo "android-splitscreen: the automotive image does not offer split-screen." >&2
      exit 1
    fi
    # Uninstall rather than `install -r`. A signing key that changed makes the
    # latter fail with INSTALL_FAILED_UPDATE_INCOMPATIBLE while the device goes
    # on running the previous build — build.sh's own comment calls that the
    # worst failure it could have, and this makes it unreachable.
    "${adb}" uninstall "${pkg}" >/dev/null 2>&1 || true
    "${adb}" install "${apk}" >/dev/null
    "${adb}" shell am force-stop "${pkg}" || true
    "${adb}" logcat -c
    # **Assert it draws before asserting anything else.** The harness logs
    # surfaceCreated, surfaceChanged and a runtime handle whether or not the
    # painter obtained a device, and logs nothing when it did not (issue #960),
    # so every log-only check passes against a black screen. That is how a black
    # frame survived a full run here on 2026-08-14 and was found by a human
    # looking at the emulator. The screenshot is the only witness.
    echo "android-splitscreen: launching, then checking it actually drew"
    "${adb}" shell am start -W -n "${act}" >/dev/null
    sleep 8
    shot="target/android-harness/screen.png"
    "${adb}" exec-out screencap -p > "${shot}"
    if ! python3 crates/dashscene-android/harness/assert-drew.py "${shot}"; then
      echo "android-splitscreen: stopping here. A handshake result is meaningless" >&2
      echo "android-splitscreen: while nothing is being drawn: surfaceDestroyed" >&2
      echo "android-splitscreen: blocks for the frame loop to stop, and a loop" >&2
      echo "android-splitscreen: that never started may never signal." >&2
      "${adb}" logcat -d | grep -E "Failed to open rendernode|I dashscene:" | tail -8 >&2 || true
      exit 1
    fi
    # Only now is the split transition worth measuring.
    "${adb}" shell am force-stop "${pkg}" || true
    "${adb}" logcat -c
    echo "android-splitscreen: relaunching cold into multi-window"
    "${adb}" shell am start -W -n "${act}" --windowingMode 6 >/dev/null
    sleep 5
    mode=$("${adb}" shell dumpsys activity activities 2>/dev/null \
      | grep -A12 "A=[0-9]*:${pkg}" | grep -oE "mWindowingMode=[a-z-]+" | head -1 || true)
    if [ "${mode}" != "mWindowingMode=multi-window" ]; then
      echo "android-splitscreen: expected multi-window, got '${mode:-nothing}'" >&2
      echo "android-splitscreen: a warm start swallows --windowingMode as" >&2
      echo "android-splitscreen: onActivityRestartAttempt — force-stop first." >&2
      exit 1
    fi
    echo "android-splitscreen: ${mode}; putting a second app in the other half"
    "${adb}" shell am start -n com.android.settings/.Settings --windowingMode 6 >/dev/null 2>&1 || true
    # Everything THIS instance logged, and nothing the previous one did. The
    # force-stop above can leave the dying instance's teardown markers in the
    # buffer: entries written in the millisecond window around a `logcat -c`
    # are not removed, and a stale `entering`/`complete` pair is otherwise
    # indistinguishable from this run's, which would report a PASS for a
    # teardown the split transition never produced (issue #1006).
    #
    # The relaunch's own `surfaceCreated` is the boundary. A process being torn
    # down logs no such line, so taking the log from the first one onward drops
    # exactly the stale pair and keeps everything this run emitted. A timestamp
    # cutoff was tried first and is worse: `adb shell` joins argv with spaces
    # unescaped, so the format string needs a second level of quoting, and
    # second-precision rounds the cutoff down into the very window it excludes.
    this_run() {
      "${adb}" logcat -d 2>/dev/null | grep -E "I dashscene: harness:" | awk '/surfaceCreated/{seen=1} seen'
    }
    for _ in $(seq 1 30); do
      # Captured and matched in bash rather than piped into `grep -q`. Under
      # `pipefail`, `grep -q` exits at its first match and closes the pipe,
      # logcat dies on SIGPIPE, and the pipeline reports that failure — so the
      # break would never fire and this loop would always run its full 30 s.
      if [[ "$(this_run || true)" == *"handshake complete, returning"* ]]; then break; fi
      sleep 1
    done
    log="$(this_run || true)"
    echo "${log}"
    # `handle == 0` means nativeSurfaceCreated could not obtain the window or
    # spawn the thread. HarnessActivity logs that case distinctly rather than
    # falling through to the completion marker, which is what let a callback
    # that handed nothing back still claim a handshake.
    #
    # This is NOT the no-GPU-device case. That returns a non-zero handle and
    # takes the handshake branch — issue #960. `nativeIsRunning` would not
    # answer it either, for the reason this recipe's own header now gives
    # (issue #1080): a thread wedged inside an attach reports `true`. The
    # `attaching`/`attached` marker pair in logcat is what does.
    if [[ "${log}" == *"no runtime handle, nothing to hand back"* ]]; then
      echo "android-splitscreen: FAIL — reached surfaceDestroyed with no runtime" >&2
      echo "android-splitscreen: handle, so no handshake ran. nativeSurfaceCreated" >&2
      echo "android-splitscreen: could not get the window or spawn the thread;" >&2
      echo "android-splitscreen: check logcat for E lines this filter drops." >&2
      exit 1
    fi
    # Matched in bash for the reason the wait loop above gives: a `printf |
    # grep -q` pipeline reports SIGPIPE under `pipefail` once the log exceeds
    # the pipe buffer, which here would invert into a FAIL on a log that
    # carries the marker.
    if [[ "${log}" != *"entering the handshake"* ]]; then
      echo "android-splitscreen: FAIL — the surface was never destroyed, so D4's" >&2
      echo "android-splitscreen: split-screen case did not run." >&2
      exit 1
    fi
    if [[ "${log}" != *"handshake complete, returning"* ]]; then
      echo "android-splitscreen: FAIL — entered the handshake and never returned." >&2
      echo "android-splitscreen: That is the use-after-free window D4 names: the" >&2
      echo "android-splitscreen: callback must block until the surface is dropped" >&2
      echo "android-splitscreen: and then return." >&2
      exit 1
    fi
    echo "android-splitscreen: PASS — drew a frame, surface destroyed, handshake completed"

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
