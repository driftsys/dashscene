# Test tiers implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the workspace test suite into three named tiers so the pre-push
gate runs in about 55 seconds instead of over five minutes, without any test
losing its place in a gate.

**Architecture:** The tiers are three `cargo nextest` profiles in one
configuration file, selected by `just` recipes. No test source file changes:
the scheduling decision lives in configuration, not in attributes on the tests.
`regression` is the default profile, so a bare `cargo nextest run` is the gate
rather than something thinner. CI keeps running `regression` on every push and
gains a `calibration` job behind a path filter.

**Tech Stack:** `cargo-nextest` 0.9.87 or newer, `just`, GitHub Actions,
`dorny/paths-filter`.

The design this implements is `docs/wip/2026-08-01-test-tiers-design.md`. Read
it first; it carries the measurements, the alternatives considered, and the
reason the seam falls where it does.

## Global Constraints

- **No test source file changes.** Not one `#[ignore]`, not one `cfg`. If a
  tier boundary cannot be expressed as a nextest filter over binary and test
  names, stop and raise it rather than editing a test.
- **`cargo-nextest` 0.9.87 or newer.** Exact-match filters (`test(=name)`) and
  per-profile `default-filter` were verified against 0.9.87.
- **nextest does not run doctests.** Every recipe that claims to run a tier
  also runs `cargo test --workspace --doc`. The workspace has three doctests.
- **Prose is plain and literal** — no idioms, no figures of speech. Standard
  technical vocabulary stays as-is.
- **Commit messages are conventional commits** with a scope from
  `.git-std.toml`'s list. Allowed types include `build`, `ci`, `docs`, `test`,
  `chore`. The pre-commit hook runs `cargo fmt --all` and `dprint fmt`; the
  pre-push hook runs `just verify`, so push with `--no-verify` while the
  recipes are mid-change and run the gate by hand instead.
- **Tier membership, observed on `main` at `1e45969`:** `all` 1288 tests,
  `regression` 1286, `sanity` 1262, `calibration` 2. These numbers move as
  tests are added. The invariant that does not move, and that the plan asserts,
  is: `regression + calibration = all`, and `sanity` is a subset of
  `regression`.
- **When counting tests from `cargo nextest list`, discard stderr.** Cargo
  writes `Compiling` and `Finished` to stderr and indents them, so counting
  indented lines without `2>/dev/null` inflates the total by exactly the number
  of units compiled. This produced a wrong count once already while the design
  was being measured. From a `run` rather than a `list`, read nextest's
  `Starting N tests` line instead.

---

### Task 1: The nextest profiles

**Files:**

- Create: `.config/nextest.toml`
- Create: `.config/calibration-tier.txt`

**Interfaces:**

- Consumes: nothing.
- Produces: four nextest profiles — `default` (the regression tier),
  `regression` (an alias that inherits `default`), `sanity`, `calibration`,
  and `all`. Later tasks select them with `cargo nextest run -P <name>`.
  Also `.config/calibration-tier.txt`, the byte-for-byte expected stdout of
  `cargo nextest list --workspace -P calibration`. Task 3's `calibrate` recipe
  and Task 4's CI calibration job both diff the live listing against it.

- [ ] **Step 1: Create the configuration file**

Create `.config/nextest.toml`:

```toml
# Test tiers — docs/decisions/test-tiers.md.
#
# The tier lives here rather than in attributes on the tests. An `#[ignore]`
# says "do not run this" at the point the test is defined, which is the wrong
# place to record a schedule that differs per moment: the same test belongs in
# the gate before a push and not in the loop between edits. This file is the
# one place the tiers are defined; `just test`, `just build` and
# `just calibrate` select between them.
#
# The tiers nest. `regression` is a superset of `sanity`; `regression` and
# `calibration` together are the whole suite.
#
# Filters use exact matches (`test(=name)`, `binary(=name)`) rather than
# substring matches, so a test added later cannot join a tier by accident of
# its name.
#
# An exact match has one failure mode of its own: rename a test and its filter
# stops matching, so the test leaves its tier without any error. Counting does
# not catch that — the tiers partition the suite, so a test moving from
# `calibration` into `regression` keeps every total reconciling. What catches it
# is `calibration-tier.txt` beside this file, which pins that tier's membership
# by name; `just calibrate` and the CI calibration job both diff the live
# listing against it. A rename fails there, loudly, instead of quietly
# shrinking the tier. Only the calibration tier is pinned this way: it is two
# names and they change rarely, where pinning 1262 names would churn on every
# test added.

# The minimum version this file was written against and verified on.
# Per-profile `default-filter` and exact-match filters both predate it, but
# pinning the floor turns "runs against an old nextest" into a named error
# rather than a filter that parses to something else.
nextest-version = "0.9.87"

# `default` is the regression tier, so a bare `cargo nextest run` gives the
# same gate `just build` runs rather than a thinner one. The two tests excluded
# here re-derive committed asset tables from the packer alone; they are the
# `calibration` tier.
[profile.default]
default-filter = """
not (
    test(=every_rung_of_every_fixture_is_recorded)
  + test(=the_recorded_contract_table)
)
"""

# An empty profile inherits everything from `default`, including its filter.
# It exists so `-P regression` reads as the tier it selects, rather than
# leaving the gate nameless.
[profile.regression]

# The loop between edits and before every commit. Drops, on top of the
# calibration tier: the profile-preview oracle, the glyph atlas pipeline, the
# bake oracle, and the grid-anchor saturation test. Those cost about 73 s
# together and each is reachable from a narrow part of the tree, so the
# regression tier is where they are caught rather than this one.
[profile.sanity]
default-filter = """
not (
    test(=every_rung_of_every_fixture_is_recorded)
  + test(=the_recorded_contract_table)
  + binary(=profile_preview_oracle)
  + binary(=atlas_pipeline)
  + test(=an_out_of_range_grid_anchor_does_not_panic)
  + binary(=v010_bake_oracle)
)
"""

# The two tests that re-derive a committed table from the packer alone. They
# can only fail when dashpack, its encoder, dashbuf's asset kinds, or the
# corpus payloads change, which is what makes a narrow path filter correct for
# them in CI.
#
# The profile-preview oracle is deliberately NOT here, though it also checks a
# committed table. It renders every scene through Skia, so it reads the whole
# painter stack and almost any Rust change can move it — no path filter could
# honestly cover it. It runs in `regression`, on every push, instead.
[profile.calibration]
default-filter = """
    test(=every_rung_of_every_fixture_is_recorded)
  + test(=the_recorded_contract_table)
"""

# Every tier in one run.
[profile.all]
default-filter = "all()"
```

- [ ] **Step 2: Build the test binaries once, so later counts are not polluted by build output**

Run:

```bash
cargo nextest list --workspace -P all >/dev/null
```

Expected: exits 0. This compiles the test targets; the first run in a fresh
worktree takes several minutes.

- [ ] **Step 3: Assert the tier sizes and the partition invariant**

Run:

```bash
for p in all regression sanity calibration; do
  printf '%-12s %s\n' "$p" \
    "$(cargo nextest list --workspace -P $p 2>/dev/null | grep -cE '^ {4}\S')"
done
```

Expected, on `main` at `1e45969`:

```text
all          1288
regression   1286
sanity       1262
calibration     2
```

The exact numbers move as tests are added. What must hold: `regression` plus
`calibration` equals `all`, and `sanity` is smaller than `regression`. If
`regression` equals `all`, a filter stopped matching — most likely a test was
renamed. Fix the filter, do not adjust the expectation.

- [ ] **Step 4: Pin the calibration tier's membership**

Write the tier's live listing to `.config/calibration-tier.txt`:

```bash
cargo nextest list --workspace -P calibration 2>/dev/null > .config/calibration-tier.txt
```

Then read the file and confirm it is exactly these two tests, and nothing else:

```text
dashpack::band_contract:
    the_recorded_contract_table
goldens::perceptual_calibration:
    every_rung_of_every_fixture_is_recorded
```

If it is not, the filters are wrong. Fix the filters — never the pinned file.

The pinned file is the guard the exact-match filters need. `2>/dev/null`
matters: cargo writes its build progress to stderr, and letting it into the
file would pin build output as though it were tier membership.

Add a header comment? No — the file is compared byte for byte against
`cargo nextest list` output, so it can hold nothing that command does not
print. What it is and who checks it is documented in `.config/nextest.toml`
beside it.

- [ ] **Step 4b: Verify the guard fires on a rename**

Prove the pinned file catches the failure it exists for, rather than assuming
it does:

```bash
cp .config/nextest.toml /tmp/nextest.toml.bak
sed -i '' 's/test(=the_recorded_contract_table)/test(=the_recorded_contract_tabl)/' .config/nextest.toml
cargo nextest list --workspace -P calibration 2>/dev/null \
  | diff -u .config/calibration-tier.txt -
echo "diff exit: $?"
cp /tmp/nextest.toml.bak .config/nextest.toml
```

Expected: the diff prints a removed line and `diff exit: 1`. Then confirm the
restore worked:

```bash
cargo nextest list --workspace -P calibration 2>/dev/null \
  | diff -u .config/calibration-tier.txt - && echo "restored: clean"
```

Expected: `restored: clean`. Do not commit until this second command is clean.

- [ ] **Step 5: Run each tier and record its wall clock**

Run:

```bash
for p in sanity regression calibration; do
  s=$(date +%s)
  cargo nextest run --workspace -P $p > "/tmp/tier-$p.log" 2>&1
  # Capture rc on the line after the run. Reading $? later would report the
  # exit code of whatever command substitution ran in between, which is the
  # same trap as piping a build through `tail`.
  rc=$?
  e=$(date +%s)
  printf '%-12s %ss rc=%s\n' "$p" "$((e - s))" "$rc"
done
```

Expected: all three exit 0. Wall clocks near 7 s, 35 s and 165 s on an idle
8-core machine. Keep the three numbers; Task 5's decision record quotes them.

- [ ] **Step 6: Commit**

```bash
git add .config/nextest.toml
git commit -m "build(repo): define the three test tiers as nextest profiles

The suite takes 320 s because cargo test never overlaps two of its 108
test binaries, and because six tests that re-derive committed asset
tables cost 211 s of that. Neither is a property of the assertions, so
neither needs a test to change.

Three nextest profiles express the tiers as filters over binary and
test names: sanity for the loop between edits, regression as the
default and therefore the gate, calibration for the two re-derivations.
No test source file changes.

Nothing selects these profiles yet — the recipes follow."
```

---

### Task 2: Install cargo-nextest from bootstrap

**Files:**

- Modify: `bootstrap:39-73`

**Interfaces:**

- Consumes: the tiers from Task 1.
- Produces: `cargo-nextest` on `PATH` after `./bootstrap`, so a fresh clone or
  worktree can run the recipes Task 3 adds.

- [ ] **Step 1: Verify the failure this prevents**

Run:

```bash
env PATH="/usr/bin:/bin" cargo nextest --version
```

Expected: fails, because `cargo-nextest` is not a default toolchain component.
This is what a fresh clone hits, and what bootstrap must fix.

- [ ] **Step 2: Add the installer function**

In `bootstrap`, immediately after the closing brace of `install_git_std()`
(line 70), insert:

```bash
# cargo-nextest runs the test tiers (docs/decisions/test-tiers.md). Installed
# from the project's own prebuilt binary rather than `cargo install
# cargo-nextest`, which compiles it from source and would add minutes to every
# fresh clone and every worktree.
install_nextest() {
  if command -v cargo-nextest >/dev/null 2>&1; then
    log "cargo-nextest already installed: $(command -v cargo-nextest)"
    return
  fi

  local platform cargo_bin url asset tmp sha_expected sha_actual
  case "$(uname -s)" in
    # These aliases are gnu-linked on Linux, where detect_platform() picks musl
    # for git-std. The difference is deliberate rather than an oversight:
    # nextest publishes no musl asset under these aliases, and every machine
    # this repository targets is either Darwin or the gnu-linked ubuntu CI
    # runner. A musl-only host would have to build nextest from source.
    Linux)
      case "$(uname -m)" in
        x86_64 | amd64) platform="linux" ;;
        arm64 | aarch64) platform="linux-arm" ;;
        *) die "unsupported architecture for cargo-nextest: $(uname -m)" ;;
      esac
      ;;
    # The mac tarball is a universal binary, so it needs no architecture case.
    Darwin) platform="mac" ;;
    *) die "unsupported OS for cargo-nextest: $(uname -s)" ;;
  esac

  url="https://get.nexte.st/latest/${platform}"
  cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  # get.nexte.st is a redirector, and the checksum is a separate release asset
  # beside the tarball: the same name with .tar.gz replaced by .sha256. Two
  # addresses cannot produce it. The redirector's own address has no checksum
  # beside it, and the fully resolved address is a pre-signed content-delivery
  # URL where a suffix means nothing — asking for that one plus ".sha256"
  # returns the tarball again, so the comparison below would fail on every
  # fresh install rather than only on a corrupted one. The address that works
  # is the first redirect target, taken without following it: that is the
  # release asset itself, and swapping its extension gives the checksum.
  asset="$(curl --fail --silent --show-error --output /dev/null \
    --write-out '%{redirect_url}' "${url}")"
  [ -n "${asset}" ] || die "get.nexte.st did not redirect for ${platform}"

  log "downloading cargo-nextest (${platform})"
  curl --fail --silent --show-error --location "${asset}" \
    --output "${tmp}/nextest.tar.gz"
  curl --fail --silent --show-error --location "${asset%.tar.gz}.sha256" \
    --output "${tmp}/nextest.tar.gz.sha256"
  sha_expected="$(cut -d' ' -f1 "${tmp}/nextest.tar.gz.sha256")"
  # macOS ships shasum, not sha256sum. install_git_std above calls sha256sum
  # unconditionally, which means its own download path cannot work on a Mac —
  # noted here, not fixed here, because it is outside this change.
  if command -v sha256sum >/dev/null 2>&1; then
    sha_actual="$(sha256sum "${tmp}/nextest.tar.gz" | cut -d' ' -f1)"
  else
    sha_actual="$(shasum -a 256 "${tmp}/nextest.tar.gz" | cut -d' ' -f1)"
  fi
  [ "${sha_expected}" = "${sha_actual}" ] || die "sha256 mismatch for cargo-nextest"

  # Extract in the temporary directory and move the finished binary into place,
  # the way install_git_std does. Streaming the download straight into
  # ${cargo_bin} would leave a truncated cargo-nextest there if the transfer
  # failed partway, and the "already installed" check at the top of this
  # function would then skip the repair on every later run — a broken install
  # that reports itself as done.
  tar -xzf "${tmp}/nextest.tar.gz" -C "${tmp}"
  mkdir -p "${cargo_bin}"
  install -m 0755 "${tmp}/cargo-nextest" "${cargo_bin}/cargo-nextest"

  log "installed cargo-nextest to ${cargo_bin}/cargo-nextest"
  case ":${PATH}:" in
    *":${cargo_bin}:"*) ;;
    *) log "note: ${cargo_bin} is not on PATH — add it to your shell profile" ;;
  esac
}
```

- [ ] **Step 3: Call it**

In `bootstrap`, replace the final two lines:

```bash
install_git_std
exec git std bootstrap
```

with:

```bash
install_git_std
install_nextest
exec git std bootstrap
```

- [ ] **Step 4: Run bootstrap and verify it is idempotent**

Run:

```bash
./bootstrap 2>&1 | grep nextest
```

Expected: `[bootstrap] cargo-nextest already installed: ...` on a machine that
has it. On a machine that does not, `[bootstrap] downloading cargo-nextest
(mac)` followed by `[bootstrap] installed cargo-nextest to ...`. Run it twice;
the second run must take the "already installed" path.

- [ ] **Step 4b: Exercise the download path for real**

The machine running this already has `cargo-nextest`, so Step 4 only proves the
skip. Do not settle for verifying the download by reading it. Point
`CARGO_HOME` at a throwaway directory and hide the real binary from
`command -v`, which drives the function down the path that downloads, verifies
the checksum, and installs:

```bash
tmp_home="$(mktemp -d)"
{
  echo 'log() { printf "[probe] %s\n" "$*" >&2; }'
  echo 'die() { printf "[probe] error: %s\n" "$*" >&2; exit 1; }'
  sed -n '/^install_nextest()/,/^}$/p' bootstrap
  echo 'install_nextest'
} > "$tmp_home/probe.sh"
env PATH="/usr/bin:/bin" CARGO_HOME="$tmp_home" bash -euo pipefail "$tmp_home/probe.sh"
"$tmp_home/bin/cargo-nextest" --version
```

Expected: the probe logs `downloading cargo-nextest (mac)` then `installed
cargo-nextest to <tmp>/bin/cargo-nextest`, no checksum error, and the version
line prints. A checksum mismatch, a missing `.sha256`, or a `tar` failure all
surface here rather than on someone's fresh clone.

Then confirm the real installation is untouched and clean up:

```bash
command -v cargo-nextest
rm -rf "$tmp_home"
```

Expected: the real path, not the temporary one.

- [ ] **Step 5: Commit**

```bash
git add bootstrap
git commit -m "build(repo): install cargo-nextest from bootstrap

The test tiers are nextest profiles, so a clone that has not installed
nextest cannot run any of the test recipes. bootstrap already installs
git-std the same way; this follows it.

The prebuilt binary rather than \`cargo install cargo-nextest\`, which
compiles from source and would add minutes to every fresh clone and
every worktree."
```

---

### Task 3: The justfile recipes

**Files:**

- Modify: `justfile:18-20` (the `test` recipe) and `justfile:34-35` (`check`)

**Interfaces:**

- Consumes: the profiles from Task 1, `cargo-nextest` from Task 2.
- Produces: `just test` (sanity), `just test-regression` (regression),
  `just calibrate` (calibration), `just test-all` (every tier). `just check`,
  and therefore `just build` and `just verify`, run the regression tier.

- [ ] **Step 1: Replace the `test` recipe**

In `justfile`, replace:

```just
# Run the Rust test suite.
test:
    cargo test --workspace
```

with:

```just
# Run the sanity tier — the loop between edits and before every commit.
# About 8 s. Tier definitions and the schedule: docs/decisions/test-tiers.md.
#
# `cargo test --doc` rides along in every tier recipe because nextest does not
# run doctests. Leaving it out would mean three recipes that each claim to run
# a tier and each silently skip the same three tests.
test:
    cargo nextest run --workspace -P sanity
    cargo test --workspace --doc

# Run the regression tier — what `check`, `build`, `verify`, the pre-push hook
# and the CI `test` job all run. About 35 s. This is the gate; `just test` is
# not.
test-regression:
    cargo nextest run --workspace -P regression
    cargo test --workspace --doc

# Run the calibration tier — the two tests that re-derive the committed asset
# tables from the packer alone. About 165 s. Run it when the diff touches a
# path in the `packer` filter of .github/workflows/ci.yml, and again at slice
# close. That filter is the list, and docs/decisions/test-tiers.md enumerates
# it with a reason per entry; a copy of the list here would be a fourth one to
# keep in step, and the partial copies have already drifted three times.
#
# The membership check runs first, and it is the reason the tier cannot rot
# quietly. The profiles select by exact test name, so renaming either of the two
# drops it out of the tier with no error — and no count catches that, because
# the tiers partition the suite and the totals still reconcile. Diffing the
# live listing against the pinned .config/calibration-tier.txt does catch it.
# The CI calibration job runs the same two lines; keep them identical.
calibrate:
    cargo nextest list --workspace -P calibration 2>/dev/null \
        | diff -u .config/calibration-tier.txt -
    cargo nextest run --workspace -P calibration

# Every tier in one run.
test-all:
    cargo nextest run --workspace -P all
    cargo test --workspace --doc
```

- [ ] **Step 2: Point `check` at the regression tier**

In `justfile`, replace:

```just
# Full non-build verification: test + lint + audit.
check: test lint audit
```

with:

```just
# Full non-build verification: the regression tier + lint + audit. Not the
# sanity tier — `check` is what `build` and the pre-push hook run, so it takes
# the tier that is the gate (docs/decisions/test-tiers.md).
check: test-regression lint audit
```

- [ ] **Step 3: Verify each recipe selects the tier it names**

Run:

```bash
just test 2>&1 | grep -E 'Starting|Summary|test result'
```

Expected: a `Starting 1262 tests` line, a `Summary [ ~8s] 1262 tests run: 1262
passed` line, and a `test result: ok. 3 passed` line from the doctest run.

Run:

```bash
just test-regression 2>&1 | grep -E 'Starting|Summary|test result'
```

Expected: `Starting 1286 tests`, all passing, plus the same 3 doctests.

Run:

```bash
just calibrate 2>&1 | grep -E 'Starting|Summary'
```

Expected: `Starting 2 tests`, all passing.

- [ ] **Step 4: Verify the whole gate, end to end, and time it**

Run:

```bash
s=$(date +%s); just build > /tmp/build.log 2>&1; echo "REAL_EXIT=$?"; \
  echo "$(( $(date +%s) - s ))s"
```

Expected: `REAL_EXIT=0` and a wall clock near 60 s warm, or near 85 s if clippy
has to re-check. Never pipe this through `tail` — the reported exit code would
be `tail`'s.

- [ ] **Step 5: Commit**

```bash
git add justfile
git commit -m "build(repo): run the test tiers from the recipes

\`test\` becomes the sanity tier, at about 8 s, so there is a rung short
enough to run before every commit. \`check\`, and therefore \`build\`,
\`verify\` and the pre-push hook, take the regression tier at about
55 s rather than the 320 s flat suite. \`calibrate\` and \`test-all\`
are new.

Every tier recipe pairs its nextest run with \`cargo test --doc\`,
because nextest does not run doctests and this workspace has three.

The recipe set is recorded in docs/decisions/house-style.md from
driftsys/git-std, so changing what \`test\` runs is a deviation from
that template; the decision record follows."
```

---

### Task 4: CI runs the regression tier and gains a calibration job

**Files:**

- Modify: `.github/workflows/ci.yml:22-44` (the `changes` job's filters)
- Modify: `.github/workflows/ci.yml:81-98` (the `test` job)
- Modify: `.github/workflows/ci.yml:287-306` (the `ci` aggregate job's `needs`)
- Create: a `calibration` job in the same file, after `render-oracle`

**Interfaces:**

- Consumes: the profiles from Task 1.
- Produces: a `changes` job output named `packer`, and a `calibration` job
  gated on it.

- [ ] **Step 1: Add the `packer` path filter**

In `.github/workflows/ci.yml`, in the `changes` job, add `packer` to `outputs`:

```yaml
outputs:
  figma: ${{ steps.filter.outputs.figma }}
  packer: ${{ steps.filter.outputs.packer }}
```

and add this filter beneath the `figma:` block, inside the `filters: |` string:

```yaml
# The calibration tier re-derives two committed asset tables from
# the packer alone. This list is every input that can move them,
# traced from what those two tests read rather than guessed
# (docs/decisions/test-tiers.md).
#
# The profile-preview oracle is deliberately not covered here. It
# checks a committed table too, but it renders through Skia and
# reads the whole painter stack, so no honest path list could
# bound it — it runs in the regression tier, on every push.
packer:
  - 'crates/dashpack/**'
  - 'crates/dashpack-astcenc-sys/**'
  # AssetKind and its classification decide which rungs the ladder
  # walk considers at all, so a change here moves both tables.
  - 'crates/dashbuf/**'
  - 'corpus/**'
  - 'goldens/tooling/src/metric.rs'
  # The two tests' own helpers: the deterministic stress fixture
  # they both measure, and the repository-root lookup that finds
  # the committed payloads.
  - 'goldens/tooling/tests/common/**'
  # The calibration table is the artifact under test; editing it
  # without re-deriving it is the exact drift this job exists to
  # catch. band_contract.rs needs no entry of its own, because
  # crates/dashpack/** already covers it.
  - 'goldens/tooling/tests/perceptual_calibration.rs'
  # A dependency bump can move an encoder's output without touching
  # any path above. The figma filter includes both for that reason.
  - 'Cargo.toml'
  - 'Cargo.lock'
```

- [ ] **Step 2: Switch the `test` job to the regression tier**

In the `test` job, replace the final step:

```yaml
- run: cargo test --workspace
```

with:

```yaml
# The regression tier (docs/decisions/test-tiers.md). nextest's default
# profile is that tier, so no -P is needed here and a bare run cannot
# silently be the thinner one.
- name: install cargo-nextest
  run: curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
- run: cargo nextest run --workspace
# nextest does not run doctests.
- run: cargo test --workspace --doc
```

- [ ] **Step 3: Add the calibration job**

After the `render-oracle` job and before the `ci` job, insert:

```yaml
calibration:
  name: calibration
  needs: [changes]
  runs-on: ubuntu-latest
  # Path-filtered: the two tests here re-derive the committed asset tables,
  # and the `packer` filter above lists every input that can move them —
  # traced from what those two tests read, not guessed. Running them on
  # every push would put about 3 min of re-derivation behind every
  # unrelated change; running them on nobody's schedule would let the
  # tables drift. The filter is the compromise, and it defines a risky
  # change by what the diff touches rather than by anyone's judgement
  # (docs/decisions/test-tiers.md).
  #
  # The profile-preview oracle is deliberately not here. It checks a
  # committed table too, but it renders through Skia and reads the whole
  # painter stack, so no honest path list could bound it — it runs in the
  # regression tier, on every push.
  if: needs.changes.outputs.packer == 'true'
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
    - name: install flatc
      run: |
        curl -fsSL -o flatc.zip https://github.com/google/flatbuffers/releases/download/v25.12.19/Linux.flatc.binary.g++-13.zip
        unzip -o flatc.zip flatc -d /usr/local/bin
        chmod +x /usr/local/bin/flatc
    # skia-safe's prebuilt Linux binaries link the system fontconfig; the
    # profile-preview oracle renders through the Skia painter.
    - name: install skia system libraries
      run: sudo apt-get update && sudo apt-get install -y libfontconfig1-dev
    - name: install cargo-nextest
      run: curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
    # The same check the `calibrate` recipe runs, inline because this job does
    # not install `just`. It comes first: the profiles select by exact test
    # name, so a rename drops a test out of the tier silently, and no count
    # catches that — the tiers partition the suite, so the totals reconcile
    # either way. Keep this identical to the recipe.
    - name: calibration tier membership
      run: |
        cargo nextest list --workspace -P calibration 2>/dev/null \
          | diff -u .config/calibration-tier.txt -
    - name: calibration tier
      run: cargo nextest run --workspace -P calibration
```

- [ ] **Step 4: Require it in the aggregate**

In the `ci` job, add `calibration` to `needs`, after `render-oracle`:

```yaml
needs:
  [
    changes,
    fmt,
    dprint,
    clippy,
    test,
    demo-build,
    wasm-build,
    convco,
    deno,
    atlas-repro,
    render-oracle,
    calibration,
  ]
```

and extend that job's existing comment so the accepted "skipped" list stays
accurate:

```yaml
# Every job in the workflow, so a red anywhere fails the aggregate
# (docs/decisions/house-style.md). "skipped" is accepted: convco only runs on
# pull requests, deno only when importers/figma/ changed, and calibration
# only when the packer paths changed (docs/decisions/test-tiers.md).
```

- [ ] **Step 5: Verify the workflow parses and the filter selects correctly**

Run:

```bash
python3 -c "import yaml,sys; d=yaml.safe_load(open('.github/workflows/ci.yml')); \
  print(sorted(d['jobs'])); print(d['jobs']['ci']['needs'])"
```

Expected: `calibration` appears in both the job list and the `ci` job's
`needs`.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: run the regression tier, and calibration on a path filter

The test job takes the regression tier, which is nextest's default
profile, so it runs the same gate the pre-push hook runs.

The calibration tier gets its own job, filtered on the paths that can
move the tables it re-derives: the packer, the encoder it links, the
corpus payloads, the perceptual metric, and the three table files
themselves. dorny/paths-filter already drives the deno job the same
way. This is what stops the tier from becoming something nobody runs,
and it defines a risky change by what the diff touches rather than by
judgement.

The aggregate ci job requires it; a skipped calibration is accepted
there, like a skipped deno."
```

---

### Task 5: The decision record

**Files:**

- Create: `docs/decisions/test-tiers.md`
- Modify: `docs/decisions/README.md` (add the index entry)
- Modify: `docs/decisions/house-style.md:26-33` (point the recipe list at it)

**Interfaces:**

- Consumes: the measured wall clocks from Task 1 Step 5.
- Produces: the normative record every later document cites as
  `docs/decisions/test-tiers.md`.

- [ ] **Step 1: Write the record**

Create `docs/decisions/test-tiers.md`. Carry over, from
`docs/wip/2026-08-01-test-tiers-design.md`: the phase timings table, the
slowest-tests table, the "re-derivation is not regression" section, the tier
table with the wall clocks measured in Task 1 Step 5, the "where each tier
runs" list, the mechanism, the "Alternatives considered" section in full, and
the risks. Drop the "What AGENTS.md must say" section — Task 6 makes that real,
and a decision record states what was decided, not what is about to be edited.

Open it with the house header block every other record in
`docs/decisions/` uses — an indented `status` / `scope` / `related` block
directly under the title, not markdown headings. Read
`docs/decisions/compress-raster-only.md` for the exact shape and column
alignment. `status` names the date, that the owner ruled on the tier boundary
and on the trigger, and the commit range; `scope` says which tests run when;
`related` points at `house-style.md` (the recipe set this deviates from),
`asset-quality-profile-bands.md` (the tables the calibration tier re-derives)
and `review-before-ready-not-before-open.md`.

The deviation from `house-style.md`'s recipe list belongs in the body, under
an ordinary heading, not in a `## Supersedes` section — no other record has
one.

- [ ] **Step 2: Add the index entry**

In `docs/decisions/README.md`, add to the list, in the position matching the
file's existing ordering:

```markdown
- [test-tiers.md](test-tiers.md) — the workspace suite runs as three
  nextest tiers: `sanity` before every commit, `regression` as the gate,
  `calibration` on a path filter and at slice close.
```

- [ ] **Step 3: Point house-style at it**

In `docs/decisions/house-style.md`, in the `**justfile**` paragraph, after the
sentence listing the recipes, add:

```markdown
The `test` and `check` recipes deviate from that template: `test` runs the
sanity tier and `check` the regression tier, and `test-regression`,
`calibrate` and `test-all` are additions. The reason, the measurements and
the tier definitions are in [test-tiers.md](test-tiers.md).
```

- [ ] **Step 4: Verify the markdown lints**

Run:

```bash
dprint fmt && markdownlint '**/*.md' --ignore target --ignore node_modules
```

Expected: exits 0.

- [ ] **Step 5: Commit**

```bash
git add docs/decisions/test-tiers.md docs/decisions/README.md docs/decisions/house-style.md
git commit -m "docs(docs): record the test-tier decision

Gardened from docs/wip/2026-08-01-test-tiers-design.md: the phase
timings, the tests that cost 211 of the 320 seconds, the seam
between re-deriving a committed table and guarding against regression,
the three tiers with their measured wall clocks, and the five
alternatives considered.

house-style.md's recipe list comes from driftsys/git-std, and \`test\`
and \`check\` now deviate from it, so that paragraph points here."
```

---

### Task 6: The schedule, in AGENTS.md and the roadmap

**Files:**

- Modify: `AGENTS.md:80-90` (the `## Commands` block)
- Modify: `AGENTS.md:136-138` (the story workflow)
- Modify: `docs/roadmap.md:44-51` (the phase-end revision ritual)
- Modify: `justfile` (three stale wall clocks in the recipe comments)

**Interfaces:**

- Consumes: the recipes from Task 3, the record from Task 5.
- Produces: nothing later tasks read. This is the last substantive change.

- [ ] **Step 1: Update the Commands block**

In `AGENTS.md`, replace the line:

```text
just test        cargo test --workspace
```

with:

```text
just test        sanity tier — 1262 tests, ~8 s. Between edits, and
                  before every commit.
just test-regression  regression tier — 1286 tests, ~35 s. What `build`
                  and the pre-push hook run.
just calibrate    calibration tier — 2 tests, ~165 s. Re-derives the
                  committed asset tables; see the schedule below.
just test-all     every tier in one run.
```

- [ ] **Step 2: Add the schedule to the story workflow**

In `AGENTS.md`, insert this subsection immediately **before** the line
`Story workflow — the definition of done for every story:`, separated from it
by a blank line. It must not go between that line and its own bullet list —
that lead-in ends in a colon and the list directly under it is what it
introduces, so an insertion there separates a sentence from the thing it
promises.

```markdown
**When to run which test tier** (`docs/decisions/test-tiers.md`). The suite
runs as three tiers, so "tests pass" is no longer a claim about all of it:

- **While editing, and before every commit** — `just test`. Seven seconds,
  1262 of 1288 tests. There is no reason to skip it.
- **Before pushing, and before opening a PR** — `just build`, which runs the
  regression tier. The `pre-push` hook runs `just verify` and therefore this
  anyway; running it by hand only buys finding out before the push rather
  than during it.
- **When the diff touches any path in the `packer` filter** — the filter is
  defined in the `changes` job of `.github/workflows/ci.yml`, and enumerated
  with a reason per entry in `docs/decisions/test-tiers.md`. Run
  `just calibrate` before marking the PR ready. The path list is deliberately
  not repeated here: it has already drifted three times as a partial copy,
  most recently omitting `Cargo.lock`. CI runs the tier regardless, and a red
  job on a non-draft PR is what
  `docs/decisions/review-before-ready-not-before-open.md` exists to prevent.
- **At slice close** — `just calibrate`, whatever the slice touched. This is
  the one run not driven by a path, and it is the backstop against a table
  drifting through a change the filter did not predict.
- **Name the tier in the PR body.** Never report a tier as run that was not
  run.
```

- [ ] **Step 3: Add the calibration rung to the slice-close ritual**

In `docs/roadmap.md`, in the `## Staying current — the phase-end revision
ritual` section, after the first paragraph, insert:

```markdown
The ritual has one gate that is not a document edit: run `just calibrate`
before revising anything. It re-derives the committed asset tables, and it is
the only run in the schedule not driven by a path filter — the backstop
against a table that drifted through a change the filter did not predict
(`docs/decisions/test-tiers.md`).
```

- [ ] **Step 3c: Correct the `check` line in the Commands block**

`AGENTS.md`'s `## Commands` block describes `just check` as `test + lint +
audit`. `check` now takes the regression tier rather than `test`, and `test`
now means the sanity tier, so that line reads as though the gate runs the
8-second rung. Replace `test + lint + audit` with
`regression tier + lint + audit`.

- [ ] **Step 3b: Correct the three stale wall clocks in `justfile`**

The recipe comments were written before the tier boundary moved and before the
timings were re-measured on an idle machine. All three are now wrong, and they
are the figures a person reads when deciding which rung to run. Replace, in
`justfile`:

- in the `test` recipe's comment, `About 8 s.` with `About 7 s.`
- in the `test-regression` recipe's comment, `About 55 s.` with `About 35 s.`
- in the `calibrate` recipe's comment, `About 3 min.` with `About 165 s.`

Change nothing else in that file. Then confirm no stale figure survives:

```bash
grep -nE 'About [0-9]+ (s|min)' justfile
```

Expected: exactly `About 7 s.`, `About 35 s.` and `About 165 s.`

- [ ] **Step 4: Verify the counts quoted in AGENTS.md are the counts the recipes produce**

Run:

```bash
grep -E 'just (test|test-regression|calibrate)' AGENTS.md
cargo nextest run --workspace -P sanity 2>&1 | grep Starting
cargo nextest run --workspace 2>&1 | grep Starting
cargo nextest run --workspace -P calibration 2>&1 | grep Starting
```

Expected: the numbers in `AGENTS.md` match the `Starting N tests` lines. If
`main` moved during this work, update `AGENTS.md` to the numbers the run
actually reports — do not leave a stale count in the file every agent reads.

- [ ] **Step 5: Lint and commit**

```bash
dprint fmt && markdownlint '**/*.md' --ignore target --ignore node_modules
git add AGENTS.md docs/roadmap.md
git commit -m "docs(docs): state which test tier to run when

Three tiers are only useful if whoever writes the code knows which one
to run at which moment, and AGENTS.md is where every agent and every
person reads that before starting work. A tier nobody runs at the right
moment is the same failure as a tier that does not exist.

The schedule: sanity before every commit, regression before a push,
calibration when the diff touches the packer paths and again at slice
close. The roadmap's phase-end ritual carries the slice-close rung, so
a slice cannot close without it."
```

---

### Task 7: Garden the working memory and open the PR

**Files:**

- Move: `docs/wip/2026-08-01-test-tiers-design.md` to `docs/archive/`
- Move: `docs/wip/2026-08-01-test-tiers-plan.md` to `docs/archive/`

**Interfaces:**

- Consumes: everything above.
- Produces: a draft PR.

- [ ] **Step 1: Confirm the durable record exists before archiving the raw one**

Run:

```bash
test -f docs/decisions/test-tiers.md && echo "durable record present"
```

Expected: `durable record present`. If it is missing, Task 5 did not land —
stop and finish it. Archiving working memory before the durable record exists
loses the reasoning.

- [ ] **Step 2: Archive both files**

Run:

```bash
git mv docs/wip/2026-08-01-test-tiers-design.md docs/archive/
git mv docs/wip/2026-08-01-test-tiers-plan.md docs/archive/
```

- [ ] **Step 3: Run the full gate one more time**

Run:

```bash
just build > /tmp/final.log 2>&1; echo "REAL_EXIT=$?"
just calibrate > /tmp/final-calibration.log 2>&1; echo "CALIBRATION_EXIT=$?"
```

Expected: both `0`. `just calibrate` is run here because this branch touches
the tier definitions, which is the one change that can move which tests are
in the calibration tier at all.

- [ ] **Step 4: Commit and push**

```bash
git add docs/archive docs/wip
git commit -m "docs(docs): archive the test-tier working memory

The design and the plan are gardened: the decision lives in
docs/decisions/test-tiers.md and the schedule in AGENTS.md, so the raw
session records move to docs/archive/ rather than being deleted."
git push --no-verify -u origin chore/test-tiers
```

`--no-verify` because the pre-push hook runs `just verify`, which is the build
this branch has already run by hand in Step 3.

- [ ] **Step 5: Open the PR as a draft**

```bash
gh pr create --draft --title "build(repo): split the test suite into three tiers" --body "$(cat <<'BODY'
`just build` passed five minutes. Measured phase by phase, the whole cost is
the test phase: 320 s of 325 s warm, with lint and audit at 5 s.

Inside that phase, six tests cost 211 of the 320 seconds, and all six
re-derive a committed asset table rather than guard against regression. Two of
them depend on the packer alone, so they run behind a path filter; the other
four render through Skia and stay in the tier every push runs.

Three nested nextest tiers, each measured:

| tier | tests | wall clock | runs |
| --- | --- | --- | --- |
| sanity | 1262 | ~7 s | `just test`, before every commit |
| regression | 1286 | ~35 s | `just build`, the pre-push hook, the CI test job |
| calibration | 2 | ~165 s | CI path filter, and at slice close |

No test source file changed. The tiers are filters in `.config/nextest.toml`.

Refs #263.

## Review findings

- [ ] (to be filled from the review pass)
BODY
)"
```

- [ ] **Step 6: Review, then mark ready**

Run the review pass on the draft, capture every finding as a checklist item in
the PR description, fix the critical ones, and file one `debt`-labeled issue
per minor one. Mark the PR ready only once CI is green on the commit being
merged and every critical finding is resolved.

---

## Notes for whoever executes this

**`main` is red in CI, and none of it is caused by this branch.** As of
2026-08-01: `clippy` fails with `error: falling back to f32 as the trait bound
f32: From<f64> is not satisfied` in `dashscene-engine`; `atlas-repro` fails
`committed_ascii_bold_fixture_is_reproducible`; `deno task check` fails. Expect
those three to be red on this PR too. Do not try to fix them here.

**There is no `rust-toolchain.toml`.** Local is rustc 1.95.0, CI takes whatever
`dtolnay/rust-toolchain@stable` resolves to, and that is why local clippy is
green while CI clippy is red. Worth filing; out of scope here.

**The follow-on this plan deliberately does not do:** splitting
`every_rung_of_every_fixture_is_recorded` and `the_recorded_contract_table`
into one test per fixture would cut the calibration tier from 165 s to roughly
30 s, at which point the third tier could fold back into `regression`. That is
a refactor of two table-driven tests, where this plan is a configuration file.
File it as a `debt` issue when this lands.
