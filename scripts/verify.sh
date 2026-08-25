#!/usr/bin/env bash
#
# verify.sh — this repo's verification lanes, as a thing you run instead of a list you remember.
#
#   ./scripts/verify.sh quick [crate …]   per-task: fmt, clippy + tests for what you touched,
#                                         ALWAYS plus the root package (where the guards live)
#   ./scripts/verify.sh snapshots         the geometric RTL references (needs a Vulkan rasterizer)
#   ./scripts/verify.sh sweep             every lane, including the crates --workspace cannot see
#   ./scripts/verify.sh lanes             show the lanes and their commands
#
# WHY THIS FILE EXISTS. Until now verification here was prose in CLAUDE.md, and
# `crates/pos-contract` was red through FIVE consecutive task verifications, every one of them
# reporting green, because `cargo test --workspace` cannot see an excluded crate. A rule in a
# document is a rule nobody runs. `tests/guards.rs` now derives from `Cargo.toml`'s own `exclude`
# array that every excluded crate is named in THIS file, so a fourth one fails the build until
# somebody wires it in.
#
# ---------------------------------------------------------------------------------------
# THIS SCRIPT TAKES THE `cargo` LOCK ITSELF. DO NOT WRAP IT IN `lane-lock`.
#
# It re-execs itself under `lane-lock cargo --` once, for the whole run. Wrapping it from
# outside deadlocks: `lane-lock` opens a fresh descriptor per invocation and has no reentrancy
# detection, so the inner acquire blocks on a lock its own ancestor holds. That is why the
# timeout below is 900 and not the 1800 default — a deadlock should be DIAGNOSED in fifteen
# minutes, not hung for thirty, and the trap on exit 75 says what to check.
#
# WHY LOCK AT ALL, given the sibling `abdu-egui-ui` repo measured a lock as the WRONG fix for
# its own ci.sh. Both halves of that are true and they are different problems:
#
#   * There, concurrent runs needed DISJOINT STATE, not mutual exclusion — two overlapping
#     sweeps wrote the same per-lane result files and one read the other's success as its own.
#     Cargo's target-dir lock already serialized their builds; a lock would only have queued a
#     1-minute `quick` behind a 10-minute `sweep`.
#   * Here, the declared contended resource is MEMORY. CLAUDE.md records that
#     `cargo test --workspace` needs `CARGO_BUILD_JOBS=1` on this 15 GB box or the linker OOMs,
#     and `.lane-lock` names `cargo` for that reason. Four till lanes share one target dir.
#
# The queueing cost is real and is accepted rather than denied: a `quick` WILL wait behind a
# `sweep`. `sweep` is rare — it runs before a push, and push is Abdu's lever. If that stops
# being true, revisit this, and measure rather than reason.
#
# Because the run holds the lock end to end, two runs cannot overlap, so this script needs none
# of the sibling's per-run log-directory machinery. That is a consequence of the lock, not an
# oversight — if the lock ever goes, the disjoint-state problem arrives with its departure.
# ---------------------------------------------------------------------------------------

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

readonly ROOT_PACKAGE="e2manage-pos-terminal"
# The snapshot lane's target and filter. Named here rather than inline so `lanes` can print
# them and a rename has one place to happen.
readonly SNAPSHOT_TARGET="sign_in_both_directions"
readonly SNAPSHOT_FILTER="snapshot_"
readonly LOCK_TIMEOUT=900

# ── The lock, taken once ────────────────────────────────────────────────────────────────
# Only the lanes that actually run cargo take the lock. `lanes` is pure text, and a help
# command that queues behind another lane's ten-minute sweep is a help command nobody runs.
if [[ -z "${VERIFY_SH_HOLDS_CARGO_LOCK:-}" && ( "${1:-}" == "quick" || "${1:-}" == "sweep" || "${1:-}" == "snapshots" ) ]]; then
    export VERIFY_SH_HOLDS_CARGO_LOCK=1
    set +e
    lane-lock cargo --timeout "$LOCK_TIMEOUT" -- "$0" "$@"
    status=$?
    set -e
    if (( status == 75 )); then
        echo "verify.sh: gave up waiting ${LOCK_TIMEOUT}s for the 'cargo' lock." >&2
        echo "  If you wrapped this script in lane-lock yourself, that is the deadlock:" >&2
        echo "  it locks internally. Run it bare. Otherwise: lane-lock --status" >&2
    fi
    exit $status
fi

# ── Outcome ledger ──────────────────────────────────────────────────────────────────────
# Names rather than counters: the summary has to say WHICH lane skipped and why, because a
# lane that vanishes silently is the defect this script exists to close.
declare -a PASSED=() SKIPPED=()

run() {
    local label="$1"; shift
    echo "── ${label}"
    echo "   \$ $*"
    "$@"
    PASSED+=("$label")
}

# ── Lanes ───────────────────────────────────────────────────────────────────────────────

lane_fmt() { run "fmt" cargo fmt --all -- --check; }

lane_root_package() {
    # `-p <crate>` does NOT build the root package's tests/, which is where tests/guards.rs
    # lives and which scans crates/. A green per-crate run can bless a change the guards
    # refuse; that has fired twice. So this lane is unconditional in `quick`.
    run "root package (guards)" cargo test -p "$ROOT_PACKAGE"
}

lane_crate() {
    local crate="$1"
    run "clippy ${crate}" cargo clippy -p "$crate" --lib --tests -- -D warnings
    run "test ${crate}" cargo test -p "$crate"
}

lane_workspace() {
    run "clippy workspace" cargo clippy --workspace --all-targets -- -D warnings
    run "test workspace" cargo test --workspace
}

# An excluded crate, run in its own directory because no workspace command reaches it.
#
# `skip_if` is a regex naming the ONE environmental failure this lane is allowed to have.
# Anything else is a real failure and propagates. An unconditional `|| true` here would
# recreate, in the script meant to close it, exactly the hole it closes.
lane_excluded() {
    local dir="$1" skip_if="$2" skip_reason="$3"; shift 3
    local label="excluded: ${dir}"
    echo "── ${label}"
    echo "   \$ (cd ${dir} && $*)"

    local output status
    set +e
    output="$( cd "$dir" && "$@" 2>&1 )"
    status=$?
    set -e

    if (( status == 0 )); then
        PASSED+=("$label")
        return 0
    fi

    if [[ -n "$skip_if" ]] && grep -Eq -- "$skip_if" <<<"$output"; then
        echo "   skipped: ${skip_reason}"
        SKIPPED+=("${label} — ${skip_reason}")
        return 0
    fi

    echo "$output" >&2
    echo "verify.sh: ${label} FAILED (exit ${status}), and not for the one allowed reason." >&2
    return "$status"
}

# The geometric snapshot lane.
#
# Separate from `quick` because it costs what the others do not: the `image-snapshots` feature
# pulls a wgpu stack into the test build and needs a Vulkan rasterizer present (lavapipe here).
# Layer 1 in the same file needs neither, and runs in `quick` with everything else.
#
# THE COUNT ASSERTION IS THE POINT OF THIS FUNCTION, not decoration. A cargo test filter is a
# literal substring with no alternation, and one that selects nothing **exits 0 and prints
# `ok`** — so a renamed test, or a `#[cfg(feature)]` that stopped matching, silently converts
# this lane into a green that ran nothing. That is the exact failure `pos-contract` spent five
# task verifications in, one layer down.
lane_snapshots() {
    local label="snapshots"
    echo "── ${label}"
    echo "   \$ cargo test --features image-snapshots --test ${SNAPSHOT_TARGET} ${SNAPSHOT_FILTER}"

    local output status
    set +e
    output="$( cargo test --features image-snapshots --test "$SNAPSHOT_TARGET" \
                   -- "$SNAPSHOT_FILTER" 2>&1 )"
    status=$?
    set -e

    # A host with no Vulkan ICD cannot render, and that is an environment fact rather than a
    # regression — the one tolerated failure here, named, exactly as pos-updater's OpenSSL is.
    if (( status != 0 )) && grep -Eq -- "no Vulkan|No adapter|Failed to create render state|VK_ERROR_INCOMPATIBLE_DRIVER" <<<"$output"; then
        echo "   skipped: no Vulkan rasterizer on this host (install mesa-vulkan-drivers for lavapipe)"
        SKIPPED+=("${label} — no Vulkan rasterizer on this host")
        return 0
    fi

    if (( status != 0 )); then
        echo "$output" >&2
        echo "verify.sh: ${label} FAILED (exit ${status})." >&2
        return "$status"
    fi

    # Sum the `N passed` across every `test result:` line rather than trusting the exit code.
    local ran
    ran="$( grep -oE '^test result: ok\. [0-9]+ passed' <<<"$output" \
            | grep -oE '[0-9]+' | paste -sd+ - | bc )"
    ran="${ran:-0}"

    if (( ran == 0 )); then
        echo "$output" >&2
        echo "verify.sh: ${label} exited 0 having run NO test." >&2
        echo "  The filter '${SNAPSHOT_FILTER}' selected nothing in --test ${SNAPSHOT_TARGET}." >&2
        echo "  A cargo filter that matches nothing still exits 0; that is why this is checked." >&2
        return 1
    fi

    echo "   ${ran} snapshot test(s) ran"
    PASSED+=("${label} (${ran} test(s))")
}

lane_excluded_crates() {
    lane_excluded "crates/pos-contract" "" "" cargo test
    # pos-updater pulls reqwest 0.11 with default features, so it links native-tls and needs
    # system OpenSSL headers nothing else here requires. That is the ONE tolerated failure.
    lane_excluded "crates/pos-updater" \
        "openssl-sys|OPENSSL_DIR|Could not find directory of OpenSSL" \
        "openssl-sys unavailable on this host" \
        cargo check
}

# The `exclude` array from Cargo.toml, and ONLY that array.
#
# awk, not a sed range: sed searches its END address from the line AFTER the start, so a
# single-line `exclude = [...]` runs on and swallows the next `]` it finds — here
# `default-members = ["."]` — silently adding "." to the list of excluded crates. A
# plausible-looking wrong answer is worse than no answer. `tests/guards.rs` parses this same
# array; both sides read the tree, neither restates it.
excluded_crates() {
    awk '
        /^exclude[[:space:]]*=[[:space:]]*\[/ { collecting = 1 }
        collecting                            { buffer = buffer $0 }
        collecting && /\]/                    { print buffer; exit }
    ' Cargo.toml | grep -o '"[^"]*"' | tr -d '"'
}

# ── Summary ─────────────────────────────────────────────────────────────────────────────
# A run that prints nothing and exits 0 is indistinguishable from a run that did nothing, so
# the totals are the point, not decoration.
summarise() {
    echo
    echo "── verify.sh summary"
    printf '   passed:  %d lane(s)\n' "${#PASSED[@]}"
    local lane
    for lane in "${PASSED[@]}"; do printf '     ✓ %s\n' "$lane"; done
    printf '   skipped: %d lane(s)\n' "${#SKIPPED[@]}"
    for lane in "${SKIPPED[@]}"; do printf '     ~ %s\n' "$lane"; done
    if (( ${#PASSED[@]} == 0 )); then
        echo "verify.sh: no lane ran. That is a failure, not a pass." >&2
        return 1
    fi
}

# ── Commands ────────────────────────────────────────────────────────────────────────────

cmd_quick() {
    lane_fmt
    local crate
    for crate in "$@"; do
        [[ "$crate" == "$ROOT_PACKAGE" ]] && continue   # the root lane below covers it
        lane_crate "$crate"
    done
    lane_root_package
    summarise
}

cmd_sweep() {
    lane_fmt
    lane_workspace
    lane_root_package
    lane_excluded_crates
    lane_snapshots
    summarise
}

cmd_snapshots() {
    lane_snapshots
    summarise
}


cmd_lanes() {
    sed -n '3,9p' "$0"
    echo
    echo "  quick lanes:  fmt · clippy/test per named crate · root package (guards, always)"
    echo "  sweep lanes:  fmt · clippy+test --workspace · root package · each excluded crate · snapshots"
    echo "  snapshots:    cargo test --features image-snapshots --test '"$SNAPSHOT_TARGET"' -- '"$SNAPSHOT_FILTER"'"
    echo
    echo "  Excluded crates, which no --workspace command can see:"
    excluded_crates | sed 's/^/    /'
}

case "${1:-}" in
    quick)      shift; cmd_quick "$@" ;;
    sweep)      cmd_sweep ;;
    snapshots)  cmd_snapshots ;;
    lanes)  cmd_lanes ;;
    *)      sed -n '3,9p' "$0" >&2; exit 2 ;;
esac
