#!/usr/bin/env bash
# COMPILER STRUCTURAL COVERAGE (flight-evidence-gaps F2-1): measure which lines
# of the TRUST-SPINE crates (almide-mir) the verification suites actually
# execute. This is the DIRECT LOOK the evidence ladder was missing: a green gate
# says nothing about code the gate never runs (the 2026-07-03 match-linearization
# lived exactly in such a hole). Statement coverage is the DO-178C entry rung —
# MC/DC is the DAL-A rung; this script establishes the measurement, not a target.
#
#   bash proofs/coverage.sh            # measure + enforce the ratchet (= --check)
#   bash proofs/coverage.sh --check    # same, explicit (what CI passes)
#   bash proofs/coverage.sh --update   # additionally RAISE the baseline on gain
#
# Scope: instruments the almide-mir, almide-codegen and almide-wasm test
# suites; a sweep of every runnable spec program through the SHIPPING wasm
# build (`almide build --target wasm`, structural leg forced) and through the
# `almide verify` witness producer (MIR lowering + certificate producers);
# and `almide test spec/` through the CLI (frontend→codegen production path).
# The ratcheted TOTAL spans every workspace crate linked into those binaries
# (#2753 moved this gate off the incumbent's `render_program`, and #2761
# deleted that renderer; the report rows below are filtered by crate for
# readability, the TOTAL is not). Two floors:
# the TOTAL ratchet (proofs/coverage-baseline.txt) and per-file floors for
# the #566 SAFETY SET (proofs/coverage-safety-baseline.txt) — the safety
# files may not rot while the TOTAL holds.
#
# ALMIDE_COVERAGE_CONDITION=1 adds `-Z coverage-options=condition` (per-
# condition branch records incl. the RHS of lazy operators — the measured
# backstop of the MC/DC decision ledger, proofs/mcdc-ledger.sh). Needs a
# toolchain whose rustc accepts the -Z flag (the coverage-condition CI job
# pins a dated nightly; rustc's own MC/DC mode was removed upstream,
# rust-lang/rust#144999).
set -euo pipefail
export LC_ALL=C
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Parse the mode for real (#990: `--check` was accepted and ignored — the
# ratchet happened to run by default, but an unknown flag should be an error,
# not a silently-absorbed fiction).
MODE="${1:---check}"
case "$MODE" in
  --check|--update) ;;
  *) echo "usage: coverage.sh [--check|--update]" >&2; exit 2 ;;
esac

# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

# Resolve llvm-tools from the ACTIVE toolchain's sysroot, not a $HOME glob
# (#990: the `stable-*` glob depends on the runner image's default toolchain
# NAME — one image change and the 58% ratchet silently stops measuring).
SYSROOT="$(rustc --print sysroot)"
LLVM_BIN="$(echo "$SYSROOT"/lib/rustlib/*/bin | awk '{print $1}')"
if [ ! -x "$LLVM_BIN/llvm-profdata" ]; then
    if [ "${CI:-}" = "true" ]; then
        echo "::error::coverage: llvm-tools not found under $SYSROOT — in CI a missing tool is a failure (#990); rustup component add llvm-tools-preview"
        exit 1
    fi
    echo "coverage: llvm-tools not installed (rustup component add llvm-tools-preview) — SKIP"
    exit 0
fi
cd "$ROOT"

# MANUAL llvm-cov pipeline — cargo-llvm-cov's multi-run orchestration silently
# measured the WRONG binary twice (0.00% over 4 stray files reported as data,
# 2026-07-03), so each step here is explicit and its artifact is checked.
COVDIR="$ROOT/target/coverage"
rm -rf "$COVDIR"; mkdir -p "$COVDIR/build"

# `-C instrument-coverage` also instruments BUILD SCRIPTS and proc-macro hosts,
# which then RUN during the build with no LLVM_PROFILE_FILE set — so they drop
# `default_<hash>_<n>.profraw` into their own cwd, i.e. the repo root and the
# crate dirs (~94 files per run, #1361). Two problems, neither about coverage
# numbers: the working tree goes dirty right after a gate run (easy to
# `git add -A` by accident), and `stamp.sh`'s toolchain_fingerprint hashes
# untracked files, so a coverage run makes `receipt.sh` unable to reuse a
# `make verify-trust` result.
#
# Point the build steps' profraw at $COVDIR/build/ — one level DOWN, so the
# merge glob `$COVDIR/*.profraw` below does not pick it up. Build-script
# coverage is not data we want in the report (llvm-cov is scoped by -object
# anyway); we just want it to land somewhere harmless.
if [ "${ALMIDE_COVERAGE_CONDITION:-}" = "1" ]; then
    export RUSTFLAGS="-C instrument-coverage -Z coverage-options=condition"
    echo "coverage: CONDITION mode (per-condition branch records)"
else
    export RUSTFLAGS="-C instrument-coverage"
fi

# Anything that still escapes (a child process that resets the variable) is
# swept on the way out, on every exit path. Scoped to llvm's own default
# profraw naming, never inside target/, and it reports what it removed rather
# than deleting silently.
sweep_stray_profraw() {
    local stray
    stray="$(find "$ROOT" -maxdepth 3 -name 'default_*_*.profraw' -not -path "$ROOT/target/*" 2>/dev/null || true)"
    [ -n "$stray" ] || return 0
    printf '%s\n' "$stray" | while read -r f; do rm -f "$f"; done
    echo "coverage: swept $(printf '%s\n' "$stray" | wc -l | tr -d ' ') stray default_*.profraw (#1361)"
}
trap sweep_stray_profraw EXIT

echo "== 1/4 instrumented build (almide-mir + almide-codegen + almide-wasm + almide-wasm-run + almide-rt-core tests, the almide CLI) =="
# almide-wasm joined the instrumented set at the Stage 2 commissioning: the
# default `--target wasm` leg (and `almide test`'s wasm phase workload) runs
# the structural emitter, so a spine-crate-only measurement halves the TOTAL
# while the system's actual hot path goes unmeasured.
# almide-wasm-run and almide-rt-core joined for the embedded lane's http call
# handle and http.serve (#2661/#2666): their end-to-end fixtures run
# release-only and against sockets, so the only thing that reaches the shared
# cores and the host's op tables here is their own loopback tests. Left out,
# ~500 new branches landed in the TOTAL unmeasured and the per-condition
# ratchet went red on every v0.64.0 soak night.
# The test binaries are taken from cargo's own artifact messages, not from a
# `find` over `<target-dir>/release/deps`: the dated nightly the CONDITION
# mode pins lays its artifacts out under `build/<crate>/<hash>/out/` and the
# `deps` directory does not exist there — from 2026-08-27 every nightly
# per-condition measurement died at step 2/4 with "NO test binaries found"
# (8 red nights) while the stable-toolchain run kept passing on the old
# layout. The JSON `executable` field is the one location that does not
# depend on the layout; the `find` below stays as the fallback.
LLVM_PROFILE_FILE="$COVDIR/build/host-%m-%p.profraw" \
  cargo test -p almide-mir -p almide-codegen -p almide-wasm -p almide-wasm-run -p almide-rt-core --release --no-run --message-format=json --target-dir "$COVDIR/t" \
  >"$COVDIR/build-tests.json" 2>"$COVDIR/build-tests.log" || { tail -20 "$COVDIR/build-tests.log"; exit 1; }
tail -1 "$COVDIR/build-tests.log"
grep -oE '"executable":"[^"]+"' "$COVDIR/build-tests.json" | sed -E 's/^"executable":"//; s/"$//' | LC_ALL=C sort -u >"$COVDIR/testbins.txt" || true
LLVM_PROFILE_FILE="$COVDIR/build/host-%m-%p.profraw" \
  cargo build --release --bin almide --target-dir "$COVDIR/t" 2>&1 | tail -1

echo "== 2/4 run the test suites =="
# `-perm -u+x`, not `-perm /111`: the `/` form is a GNU extension and BSD find
# (macOS) rejects it outright — the second call has no `|| true`, so under
# `set -e` this whole gate died at step 2/4 with "illegal mode string" on every
# non-GNU host, never reaching the ratchet it exists to enforce (#1244 round 5).
TESTBIN_NAMES='/(almide_mir|almide_codegen|almide_wasm|almide_rt_core|backend_parity|section_dump|fuzz_differential|alias_semantics|tail_calls|integration|lower|render)[^/]*$'
TESTBINS="$(grep -E "$TESTBIN_NAMES" "$COVDIR/testbins.txt" 2>/dev/null || true)"
[ -n "$TESTBINS" ] || TESTBINS="$(cat "$COVDIR/testbins.txt" 2>/dev/null || true)"
[ -n "$TESTBINS" ] || TESTBINS="$(find "$COVDIR/t/release/deps" -maxdepth 1 -type f -perm -u+x ! -name '*.d' ! -name '*.dylib' 2>/dev/null | grep -E "$TESTBIN_NAMES" || true)"
[ -n "$TESTBINS" ] || TESTBINS="$(find "$COVDIR/t/release/deps" -maxdepth 1 -type f -perm -u+x ! -name '*.d' ! -name '*.dylib' 2>/dev/null || true)"
# No vacuous measurement: zero test binaries would still produce profraw from
# the step-3 workloads, so the run would report a NUMBER for a suite that never
# ran. That is the #990 failure mode again — fail instead.
[ -n "$TESTBINS" ] || { echo "coverage: NO test binaries found (cargo artifact messages empty, $COVDIR/t/release/deps absent) — the discovery went blind"; exit 1; }
i=0
for tb in $TESTBINS; do
    i=$((i+1))
    LLVM_PROFILE_FILE="$COVDIR/test-$i-%m.profraw" "$tb" >/dev/null 2>&1 || true
done
echo "  test binaries run: $i"

echo "== 3/4 workloads: the shipping wasm build + the witness producer over ALL runnable spec, the v0 CLI over spec =="
CLI="$COVDIR/t/release/almide"
# Every runnable spec program goes through the two paths that SURVIVE the
# incumbent's deletion (#2753; this sweep drove the incumbent's
# `render_program` before):
#   - `almide build --target wasm` with the structural leg forced — the
#     default wasm emitter (almide-wasm) and its route. Forced, so a wall is a
#     wall and never an incumbent fallback that would measure deleted code.
#   - `almide verify --emit` — the untrusted producer only: MIR lowering and
#     the ownership / name / capability witnesses (`certificate*.rs`,
#     `pipeline_witnesses.rs`). With no `almide-verify` beside the binary it
#     exits 127 after writing the bundle; the producer has already run.
# The profile names are per-workload so a single workload's contribution can
# be isolated from the profraw set (the #2753 mutation evidence did that).
# `%4m`, not `%p`: a per-process file made ~1,900 raw profiles (tens of GB,
# and 3 minutes of step 4/4 merging them) — the `%Nm` pool merges online
# across processes, which is also what makes the parallel sweep safe.
SWEEP_JOBS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)"
mkdir -p "$COVDIR/sweep"
find spec -name '*.almd' | LC_ALL=C sort | while read -r f; do
    if grep -q 'fn main' "$f"; then printf '%s\n' "$f"; fi
done > "$COVDIR/sweep.list"
n="$(wc -l < "$COVDIR/sweep.list" | tr -d ' ')"
nw="$(tr '\n' '\0' < "$COVDIR/sweep.list" | COVDIR="$COVDIR" CLI="$CLI" xargs -0 -n 1 -P "$SWEEP_JOBS" sh -c '
    out="$COVDIR/sweep/$$"
    if ALMIDE_WASM_SKIP_STOCK_AUDIT=1 LLVM_PROFILE_FILE="$COVDIR/wasm-%4m.profraw" \
         "$CLI" build "$1" --target wasm -o "$out.wasm" >/dev/null 2>&1; then echo emitted; fi
    LLVM_PROFILE_FILE="$COVDIR/verify-%4m.profraw" \
      "$CLI" verify "$1" --emit "$out.bundle" >/dev/null 2>&1
    rm -f "$out.wasm" "$out.bundle"
    exit 0
' sh | grep -c emitted || true)"
echo "  runnable spec programs: $n (structural wasm build emitted $nw; witness producer ran on all; $SWEEP_JOBS jobs)"
# No vacuous sweep: a CLI that emits nothing (a broken build, a renamed flag)
# would still leave profraw from the other workloads.
[ "$nw" -gt 0 ] || { echo "coverage: the structural wasm sweep emitted NOTHING — the workload went blind"; exit 1; }
# The v0 PRODUCTION path (almide-codegen walker/emit): `almide test` compiles +
# runs every test-block file through the full frontend→codegen pipeline.
LLVM_PROFILE_FILE="$COVDIR/cli-%m-%p.profraw" "$CLI" test spec/ >/dev/null 2>&1 || true
echo "  v0 CLI: almide test spec/ (frontend + codegen production path)"
# The COMPONENT emit paths (almide-wasm-run: the p2 shim and the p3 shim with
# its http / fs / env / io / process op families) are reached only through
# `almide build --target wasm --component`, which no spec test drives — the
# 2026-09-04..06 condition-coverage slide was exactly these lines landing
# unmeasured. Emit-only (no wasmtime): one fixture per host-op family.
c=0
printf 'fn main() -> Unit = {\n  println("Hello, world!")\n}\n' > "$COVDIR/hello.almd"
for f in "$COVDIR/hello.almd" spec/wasm_cross/http_response_headers.almd \
         spec/wasm_cross/env_platform_reporting.almd spec/wasm_cross/args_surface.almd \
         spec/wasm_cross/io_write_ordering.almd spec/wasm_cross/process_args.almd; do
    [ -f "$f" ] || continue
    LLVM_PROFILE_FILE="$COVDIR/cli-%m-%p.profraw" "$CLI" build "$f" --target wasm --component -o "$COVDIR/p2.wasm" >/dev/null 2>&1 || true
    ALMIDE_COMPONENT_P3=1 LLVM_PROFILE_FILE="$COVDIR/cli-%m-%p.profraw" "$CLI" build "$f" --target wasm --component -o "$COVDIR/p3.wasm" >/dev/null 2>&1 || true
    c=$((c+1))
done
echo "  component emit (p2 + p3 shims): $c fixture(s)"
# `almide survive` / `apply --if-survives` (#2147) are reached only through
# their own subcommands, which no spec test drives; their golden-delta
# fixtures (tests/survive/, pinned by tests/survive_test.rs) are the workload.
# Each run gets a scratch copy — `apply` writes. The legs are child `almide`
# processes of this same binary, so their profiles land here too (%p).
s=0
for spec_ in breaks_test:calc.almd:edit.patch breaks_diagnostic:sum.almd:edit.patch \
             fixes_diagnostic:shapes.almd:edit.txt neutral:greet.almd:edit.patch; do
    IFS=: read -r case_ file_ edit_ <<< "$spec_"
    [ -d "tests/survive/$case_" ] || continue
    d_="$COVDIR/survive-$case_"
    rm -rf "$d_"; cp -R "tests/survive/$case_" "$d_"
    ( cd "$d_" && LLVM_PROFILE_FILE="$COVDIR/cli-%m-%p.profraw" "$CLI" survive "$file_" --with "$edit_" --json >/dev/null 2>&1 || true
      LLVM_PROFILE_FILE="$COVDIR/cli-%m-%p.profraw" "$CLI" survive "$file_" --with "$edit_" >/dev/null 2>&1 || true
      LLVM_PROFILE_FILE="$COVDIR/cli-%m-%p.profraw" "$CLI" apply "$file_" --with "$edit_" --if-survives --json >/dev/null 2>&1 || true )
    s=$((s+1))
done
echo "  survive / apply golden deltas: $s case(s)"

echo "== 4/4 merge + report (compiler crate lines) =="
nprof="$(ls "$COVDIR"/*.profraw 2>/dev/null | wc -l | tr -d ' ')"
[ "$nprof" -gt 0 ] || { echo "coverage: NO profraw produced — measurement failed"; exit 1; }
"$LLVM_BIN/llvm-profdata" merge -sparse "$COVDIR"/*.profraw -o "$COVDIR/all.profdata"
OBJS="-object $CLI"
for tb in $TESTBINS; do OBJS="$OBJS -object $tb"; done
REPORT="$("$LLVM_BIN/llvm-cov" report $OBJS \
    -instr-profile="$COVDIR/all.profdata" \
    -ignore-filename-regex="(\\.cargo|rustc|/tests?/|tests_part|examples/|/release/build/)" 2>/dev/null \
  | awk 'NR<=2 || /almide-(mir|codegen|frontend|wasm|wasm-run)\// || /^TOTAL/' | grep -vE 'tests?_part')"
# The full per-file table goes into a collapsed group so a ratchet slide can be
# traced to its files from the log alone; the tail stays as the summary.
echo "::group::per-file coverage (all instrumented compiler crates)"
printf '%s\n' "$REPORT"
echo "::endgroup::"
printf '%s\n' "$REPORT" | tail -40

# ── RATCHET (#566): TOTAL line coverage may only go UP ─────────────────────
# Baseline file holds one number: the floor (integer percent ×100 to avoid
# float compare, e.g. 6589 = 65.89%). `--check` fails when the measured TOTAL
# drops below it; `--update` raises it to the measured value (never lowers).
# CONDITION mode (per-condition branch records, the nightly MC/DC backstop)
# counts differently from line mode — 56.55% against line mode's 67.32% on
# the same tree (2026-09-04) — so it ratchets against its OWN floor file.
# Sharing the line-mode floor made the per-condition job red on every night
# it ever measured (the first measured night after #1892 fixed its
# discovery). Seeded on the first run like the line-mode floor.
if [ "${ALMIDE_COVERAGE_CONDITION:-}" = "1" ]; then
    BASELINE_FILE="$ROOT/proofs/coverage-baseline-condition.txt"
else
    BASELINE_FILE="$ROOT/proofs/coverage-baseline.txt"
fi
total_line_pct="$(printf '%s\n' "$REPORT" | awk '/^TOTAL/ { for (i=1;i<=NF;i++) if ($i ~ /%$/) last=$i } END { gsub(/%/,"",last); print last }')"
total_c="$(printf '%s\n' "$total_line_pct" | awk '{ printf "%d", $1 * 100 }')"
echo
echo "TOTAL line coverage: ${total_line_pct}%"
if [ -f "$BASELINE_FILE" ]; then
    floor="$(cat "$BASELINE_FILE")"
    if [ "$total_c" -lt "$floor" ]; then
        echo "COVERAGE RATCHET FAIL: TOTAL ${total_line_pct}% < baseline $(awk -v f="$floor" 'BEGIN{printf "%.2f", f/100}')%"
        echo "  New code is landing untested. Add tests, or (only with a recorded"
        echo "  justification) lower proofs/coverage-baseline.txt in its own commit."
        exit 1
    fi
    echo "coverage ratchet OK: ${total_line_pct}% >= floor $(awk -v f="$floor" 'BEGIN{printf "%.2f", f/100}')%"
    if [ "$MODE" = "--update" ] && [ "$total_c" -gt "$floor" ]; then
        echo "$total_c" > "$BASELINE_FILE"
        echo "coverage ratchet RAISED to ${total_line_pct}%"
    fi
else
    echo "$total_c" > "$BASELINE_FILE"
    echo "coverage ratchet SEEDED at ${total_line_pct}%"
fi

# ── SAFETY-SET per-file floors (#566 rung 2): the safety files may not rot ──
# proofs/coverage-safety-baseline.txt: `<path> <int100>` per line. A file
# below its floor fails; a file missing from the report fails (renamed =
# stale); `--update` raises floors to the measured value (never lowers).
SAFETY_FILE="$ROOT/proofs/coverage-safety-baseline.txt"
if [ -f "$SAFETY_FILE" ]; then
    FULL_REPORT="$("$LLVM_BIN/llvm-cov" report $OBJS -instr-profile="$COVDIR/all.profdata" 2>/dev/null)"
    fail=0
    updated=""
    while read -r sf floor; do
        [ -n "$sf" ] || continue
        case "$sf" in \#*) continue ;; esac
        # Herestring, not a pipe: `printf | awk '{…exit}'` dies of SIGPIPE
        # under pipefail the moment awk exits early — the #1244 class; the
        # whole block then vanishes under set -e with no output at all
        # (caught by this gate's own red-canary landing run).
        pct="$(awk -v f="$sf" 'index($0, f) { n=0; for (i=1;i<=NF;i++) if ($i ~ /%$/) { n++; if (n==3) { gsub(/%/,"",$i); print $i; exit } } }' <<< "$FULL_REPORT")"
        if [ -z "$pct" ]; then
            echo "SAFETY COVERAGE FAIL: $sf not found in the report (renamed or dropped from the instrumented set)"
            fail=1; continue
        fi
        pc="$(printf '%s' "$pct" | awk '{ printf "%d", $1 * 100 }')"
        if [ "$pc" -lt "$floor" ]; then
            echo "SAFETY COVERAGE FAIL: $sf line ${pct}% < floor $(awk -v x="$floor" 'BEGIN{printf "%.2f", x/100}')%"
            fail=1
        elif [ "$MODE" = "--update" ] && [ "$pc" -gt "$floor" ]; then
            updated="$updated $sf:$pc"
            floor="$pc"
        fi
        printf '%s %s\n' "$sf" "$floor" >> "$SAFETY_FILE.new"
    done < "$SAFETY_FILE"
    if [ "$MODE" = "--update" ] && [ -f "$SAFETY_FILE.new" ]; then
        mv "$SAFETY_FILE.new" "$SAFETY_FILE"
        [ -n "$updated" ] && echo "safety floors RAISED:$updated"
    else
        rm -f "$SAFETY_FILE.new"
    fi
    [ "$fail" -eq 0 ] || exit 1
    echo "safety floors OK ($(grep -vc '^#' "$SAFETY_FILE" | tr -d ' ') file(s))"
fi
