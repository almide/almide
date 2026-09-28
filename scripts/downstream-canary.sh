#!/usr/bin/env bash
# One leg of the downstream canary (#2839): run a real downstream project's
# commands with the BASELINE compiler (the previous release) and with the
# CANDIDATE compiler (what is about to be tagged), and classify each command.
#
#   scripts/downstream-canary.sh <owner/repo> <out-dir>
#
# Environment:
#   BASELINE_BIN   path to the baseline `almide`
#   CANDIDATE_BIN  path to the candidate `almide`
#   CANARY_COMMANDS  newline-separated almide argument lists (e.g. "check\ntest")
#   CANARY_REF     optional ref to check out (default: the default branch HEAD)
#   CANARY_TIMEOUT per-command seconds (default 900)
#
# Writes <out-dir>/result.tsv (read by scripts/downstream-canary.py report)
# and one log per command per side. The leg itself exits 0 whenever it
# produced a result: the verdict belongs to the report step, which also
# treats a MISSING result as an infra failure — never as a pass.
#
# Per command, with b/c the baseline/candidate outcome:
#   ok/ok        both pass (for `test`: the candidate also ran at least as
#                many test files and fell back to native no more often)
#   fail/fail    both fail — pre-existing, reported, not a regression
#   ok/FAIL      REGRESSION — confirmed by one candidate re-run
#   ok/flaky     the candidate failed once and passed on the re-run
#   FAIL/ok      fixed
#   infra        the project or a dependency could not be fetched — no
#                compiler verdict exists for it
#
# Isolation: the two compilers each get their own copy of the project and
# their own TMPDIR, so no build cache, lock file, or scratch dir written by
# one can answer for the other (a shared cache once made two binaries report
# the same stale result). The dependency SOURCE cache (~/.almide/cache) is
# shared deliberately: it holds git checkouts, not compiler output.
set -uo pipefail

repo=${1:?usage: downstream-canary.sh <owner/repo> <out-dir>}
out=${2:?usage: downstream-canary.sh <owner/repo> <out-dir>}
: "${BASELINE_BIN:?}" "${CANDIDATE_BIN:?}" "${CANARY_COMMANDS:?}"
timeout_s=${CANARY_TIMEOUT:-900}
ref=${CANARY_REF:-}

mkdir -p "$out"
out=$(cd "$out" && pwd)
result="$out/result.tsv"
work=$(mktemp -d -t canary-XXXXXX)
: > "$result"

emit() { printf '%s\n' "$(IFS=$'\t'; echo "$*")" >> "$result"; }

# ── fetch the project (infra, never a verdict) ──────────────────────────────
fetched=""
for i in 1 2 3; do
  rm -rf "$work/src"
  if git clone -q --filter=blob:none "https://github.com/$repo" "$work/src" 2>"$out/clone.log"; then
    if [ -z "$ref" ] || git -C "$work/src" checkout -q "$ref" 2>>"$out/clone.log"; then
      fetched=yes; break
    fi
  fi
  sleep $((i * 10))
done
if [ -z "$fetched" ]; then
  emit project "$repo" "-" "infra" "clone failed: $(tail -1 "$out/clone.log")"
  exit 0
fi
sha=$(git -C "$work/src" rev-parse HEAD)
emit project "$repo" "$sha" "fetched" ""

cp -a "$work/src" "$work/baseline"
cp -a "$work/src" "$work/candidate"
mkdir -p "$work/tmp-baseline" "$work/tmp-candidate"

# Warm the dependency source cache with the baseline compiler, so a
# network failure shows up here as infra instead of as a compile verdict
# later. `dep-path` fetches every dependency before it looks the name up.
first_dep=$(cd "$work/src" && python3 - <<'PY'
import tomllib
with open("almide.toml", "rb") as f:
    deps = tomllib.load(f).get("dependencies", {})
print(next(iter(deps), ""))
PY
)
if [ -n "$first_dep" ]; then
  deps_ok=""
  for i in 1 2 3; do
    if (cd "$work/baseline" && TMPDIR="$work/tmp-baseline" "$BASELINE_BIN" dep-path "$first_dep") >"$out/deps.log" 2>&1; then
      deps_ok=yes; break
    fi
    sleep $((i * 10))
  done
  if [ -z "$deps_ok" ]; then
    emit deps "-" "-" "infra" "dependency fetch failed: $(grep -m1 -i 'fail' "$out/deps.log" | cut -c1-200)"
    exit 0
  fi
fi

# ── run one command with one compiler ───────────────────────────────────────
# Prints the exit code (124 = timed out).
run_side() {  # side bin log args...
  local side=$1 bin=$2 log=$3; shift 3
  (cd "$work/$side" && TMPDIR="$work/tmp-$side" timeout -k 30 "$timeout_s" "$bin" "$@") >"$log" 2>&1
  echo $?
}

# "3 via WASM, 1 via native fallback, 0 failed (of 4 files)" -> "4 1"
test_counts() {
  local s
  s=$(grep -E 'via WASM.*via native fallback.*failed' "$1" | tail -1)
  [ -z "$s" ] && { echo "- -"; return; }
  echo "$(sed -E 's/.*\(of ([0-9]+) files?\).*/\1/' <<<"$s") $(sed -E 's/.*, ([0-9]+) via native fallback.*/\1/' <<<"$s")"
}

is_fetch_failure() { grep -qE 'Failed to (fetch|checkout) |Failed to move fetched dependency' "$1"; }

n=0
while IFS= read -r cmd; do
  [ -z "$cmd" ] && continue
  n=$((n + 1))
  read -r -a args <<<"$cmd"
  blog="$out/cmd$n.baseline.log"; clog="$out/cmd$n.candidate.log"
  b=$(run_side baseline "$BASELINE_BIN" "$blog" "${args[@]}")
  c=$(run_side candidate "$CANDIDATE_BIN" "$clog" "${args[@]}")
  detail="exit $b/$c"
  if is_fetch_failure "$blog" || is_fetch_failure "$clog"; then
    cls=infra; detail="$detail; dependency fetch failed mid-run"
  elif [ "$b" = 0 ] && [ "$c" = 0 ]; then
    cls=ok/ok
    if [ "${args[0]}" = test ]; then
      read -r bt bf <<<"$(test_counts "$blog")"
      read -r ct cf <<<"$(test_counts "$clog")"
      detail="$detail; test files $bt/$ct, native fallback $bf/$cf"
      if [ "$bt" != - ] && { [ "$ct" = - ] || [ "$ct" -lt "$bt" ]; }; then
        cls=ok/FAIL; detail="$detail; the candidate ran fewer test files (a shrunken run is not a green run)"
      elif [ "$bf" != - ] && [ "$cf" != - ] && [ "$cf" -gt "$bf" ]; then
        cls=ok/FAIL; detail="$detail; more files fell back to native (the wasm leg retreated)"
      fi
    fi
  elif [ "$b" != 0 ] && [ "$c" != 0 ]; then
    cls=fail/fail
  elif [ "$b" != 0 ]; then
    cls=FAIL/ok
    [ "$b" = 124 ] && detail="$detail; baseline timed out after ${timeout_s}s"
  else
    # Baseline passed, candidate failed: confirm with one re-run so a flaky
    # test or a runner hiccup is reported as such, not as a regression.
    c2=$(run_side candidate "$CANDIDATE_BIN" "$clog.rerun" "${args[@]}")
    if [ "$c2" = 0 ]; then
      cls=ok/flaky; detail="$detail; candidate passed on re-run"
    else
      cls=ok/FAIL; detail="exit $b/$c (re-run $c2)"
      [ "$c" = 124 ] && detail="$detail; candidate timed out after ${timeout_s}s (hang?)"
    fi
  fi
  emit cmd "$cmd" "$n" "$cls" "$detail"
  echo "[$repo] almide $cmd: $cls ($detail)"
done <<<"$CANARY_COMMANDS"

rm -rf "$work"
exit 0
