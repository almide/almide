#!/usr/bin/env bash
# gen-readme-stats.sh — regenerate the machine-derived stats blocks in README.md.
#
# Hand-written counts fossilize: the README carried "164-contract" while the
# ledger held 311, "310 test files" while spec/ held 421, and a 703 B Hello,
# world measured on a compiler four releases back. Nothing read those numbers,
# so nothing noticed. Everything between the stats markers is DERIVED here:
#
#   stats:generated      the derived-count rows under Project Status — stdlib
#                        functions/modules (summed from the signature indexes
#                        tools/gen-stdlib-doc-index.py regenerates from the
#                        compiler and CI checks), `.almd` test files under spec/,
#                        contracts in the ledger (parsed as gen-claims.sh parses it)
#   wasm-size:generated  the Hello, world size table, rendered from the COMMITTED,
#                        stamped baseline docs/benchmarks/wasm-size.txt —
#                        measuring and publishing are separate acts (the
#                        build-speed block's rule): `--measure` rebuilds Hello,
#                        world on both wasm legs and restamps the baseline.
#
#   bash scripts/gen-readme-stats.sh            # rewrite README.md in place
#   bash scripts/gen-readme-stats.sh --check    # exit 1 if a block is stale; with a
#                                               # compiler at hand, also rebuild Hello,
#                                               # world and demand the baseline's bytes
#   bash scripts/gen-readme-stats.sh --measure  # rebuild Hello, world, restamp, rewrite
#
# ALMIDE_BIN names the compiler (default target/release/almide, then PATH).
# scripts/check-readme-numbers.sh is the other half: it fails on any count the
# README still writes by hand outside these blocks without a measurement date.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 2

README="README.md"
LEDGER="docs/contracts/contracts.toml"
BASELINE="docs/benchmarks/wasm-size.txt"
MODE="${1:-}"

STATS_START="<!-- stats:generated:start — derived from docs/stdlib/*.md, spec/, and docs/contracts/contracts.toml by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->"
STATS_END="<!-- stats:generated:end -->"
SIZE_START="<!-- wasm-size:generated:start — rendered from docs/benchmarks/wasm-size.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->"
SIZE_END="<!-- wasm-size:generated:end -->"
RT_START="<!-- wasm-runtime:generated:start — rendered from docs/benchmarks/wasm-runtime.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->"
RT_END="<!-- wasm-runtime:generated:end -->"
RT_LEDGER="docs/benchmarks/wasm-runtime.txt"

[ -f "$README" ] || { echo "::error::$README not found (run from repo root)"; exit 2; }
[ -f "$LEDGER" ] || { echo "::error::$LEDGER not found"; exit 2; }
for m in "$STATS_START" "$STATS_END" "$SIZE_START" "$SIZE_END" "$RT_START" "$RT_END"; do
  grep -qxF "$m" "$README" || { echo "::error::marker missing from $README: $m"; exit 2; }
done

almide_bin() {
  if [ -n "${ALMIDE_BIN:-}" ]; then echo "$ALMIDE_BIN"; return; fi
  if [ -x target/release/almide ]; then echo "target/release/almide"; return; fi
  command -v almide 2>/dev/null || true
}

# Build Hello, world on one leg and print "<bytes> <leg>" exactly as the
# compiler's own `Built …` line names them — a size that does not name the leg
# that produced it is the ambiguity the Built line was added to remove.
measure_leg() {
  local bin="$1" force_incumbent="$2" dir out line
  dir="$(mktemp -d)"
  printf 'fn main() -> Unit = {\n  println("Hello, world!")\n}\n' > "$dir/hello.almd"
  if [ "$force_incumbent" = 1 ]; then
    out="$(ALMIDE_WASM_INCUMBENT=1 "$bin" build "$dir/hello.almd" --target wasm -o "$dir/hello.wasm" 2>&1 || true)"
  else
    out="$("$bin" build "$dir/hello.almd" --target wasm -o "$dir/hello.wasm" 2>&1 || true)"
  fi
  rm -rf "$dir"
  line="$(printf '%s\n' "$out" | grep -oE '\(([0-9]+) bytes, (structural|incumbent v1) leg' || true)"
  [ -n "$line" ] || { echo "::error::could not read the Built line from the compiler; output was: $out" >&2; return 1; }
  printf '%s %s\n' "$(printf '%s' "$line" | grep -oE '[0-9]+' | head -1)" \
    "$(printf '%s' "$line" | grep -oE 'structural|incumbent')"
}

measure_both() { # sets S_BYTES / I_BYTES from a fresh build on each leg
  local bin s i
  bin="$(almide_bin)"
  [ -n "$bin" ] || { echo "::error::no compiler binary — set ALMIDE_BIN or build target/release/almide"; return 2; }
  s="$(measure_leg "$bin" 0)"
  i="$(measure_leg "$bin" 1)"
  [ "${s#* }" = structural ] || { echo "::error::default routing did not take the structural leg (got: $s)"; return 1; }
  [ "${i#* }" = incumbent ]  || { echo "::error::ALMIDE_WASM_INCUMBENT=1 did not take the incumbent leg (got: $i)"; return 1; }
  S_BYTES="${s%% *}"; I_BYTES="${i%% *}"
  BIN_VERSION="$("$bin" --version 2>/dev/null | head -1)"
}

if [ "$MODE" = "--measure" ]; then
  measure_both
  cat > "$BASELINE" <<EOF
# Hello, world wasm size — the SOURCE for the README's wasm-size block.
# Regenerate: bash scripts/gen-readme-stats.sh --measure
# Checked:    bash scripts/gen-readme-stats.sh --check rebuilds Hello, world and
#             demands these exact bytes — a changed preamble is re-stamped HERE,
#             never edited by hand in README.md. The bytes are machine-independent:
#             the emitters are pure Rust and the structural leg's build artifact is
#             the #1588 WASI form.
version          = $BIN_VERSION
date             = $(date +%F)
program          = fn main() -> Unit = { println("Hello, world!") }
structural_bytes = $S_BYTES
incumbent_bytes  = $I_BYTES
EOF
  echo "wasm-size: baseline restamped in $BASELINE (structural $S_BYTES B, incumbent $I_BYTES B)"
fi

[ -f "$BASELINE" ] || { echo "::error::$BASELINE not found — run: bash scripts/gen-readme-stats.sh --measure"; exit 2; }

kv() { grep -E "^$1[[:space:]]*=" "$BASELINE" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//'; }
thousands() { printf '%s' "$1" | awk '{ n=$1; s=""; while (length(n) > 3) { s="," substr(n, length(n)-2) s; n=substr(n, 1, length(n)-3) } print n s }'; }

stdlib_fns="$(grep -h '^## Signature index (' docs/stdlib/*.md | grep -oE '[0-9]+' | awk '{ s += $1 } END { print s + 0 }')"
stdlib_mods="$(grep -l '^## Signature index (' docs/stdlib/*.md | wc -l | tr -d ' ')"
test_files="$(grep -rlE '^[[:space:]]*test "' spec --include='*.almd' | wc -l | tr -d ' ')"
contracts="$(awk '/'"'"''"'"''"'"'/ { s = !s; next } !s && /^\[\[contract\]\]/ { n++ } END { print n + 0 }' "$LEDGER")"
size_version="$(kv version)"; size_date="$(kv date)"
size_struct="$(thousands "$(kv structural_bytes)")"; size_incumb="$(thousands "$(kv incumbent_bytes)")"

stats_body="$(mktemp)"; size_body="$(mktemp)"; rt_body="$(mktemp)"; rendered="$(mktemp)"
trap 'rm -f "$stats_body" "$size_body" "$rt_body" "$rendered"' EXIT

cat > "$stats_body" <<EOF
| Derived count | Value |
|---|---|
| Stdlib | ${stdlib_fns} functions across ${stdlib_mods} modules — self-hosted \`.almd\`, signature indexes regenerated from the compiler by \`tools/gen-stdlib-doc-index.py\` |
| Tests | ${test_files} \`.almd\` test files under \`spec/\` (\`almide test spec/\`) + the ${contracts}-contract cross-target ledger |
EOF

cat > "$size_body" <<EOF
| Program (\`almide build --target wasm\`, verified, as shipped) | incumbent v1 leg | structural leg |
|---|---:|---:|
| Hello, world | **${size_incumb} B** | **${size_struct} B** |

Measured on ${size_version}, ${size_date}, from \`docs/benchmarks/wasm-size.txt\`; no post-hoc optimizer touches the shipped bytes (\`--wasm-opt\` is opt-in and its output is not the verified module).
EOF

# The wasm-runtime block (#1701), rendered from the committed ledger — the
# gate (scripts/check-wasm-runtime-ratio.sh) re-measures the ratios; this
# renderer only formats what is committed, same rule as the size block.
rt_version="$(grep -E '^version' "$RT_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//')"
rt_date="$(grep -E '^date' "$RT_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//')"
{
  echo '| Benchmark (`almide bench`, verify-then-time, median of 5) | wasm/native ratio |'
  echo "|---|---:|"
  grep -E '^[a-z].*\| measured' "$RT_LEDGER" | while IFS='|' read -r n _ _ _ r; do
    printf '| %s | **%s×** |\n' "$(echo "$n" | xargs)" "$(echo "$r" | xargs)"
  done
  routed=$(grep -cE '^[a-z].*\| routed-incumbent' "$RT_LEDGER" || true)
  walled=$(grep -cE '^[a-z].*\| walled' "$RT_LEDGER" || true)
  oom=$(grep -cE '^[a-z].*\| oom-embedded' "$RT_LEDGER" || true)
  echo
  printf '%s%s%s\n' \
    'Embedded wasm host (Perceus RC in linear memory) against the native binary, same machine, same run. Cross-engine ratios do NOT cancel hardware (a 2-core CI runner measures nbody ~10x worse), so the ratio verdict runs on the stamping machine class and CI gates the STATUS taxonomy below (`scripts/check-wasm-runtime-ratio.sh`). binarytrees runs its fan arms on the embedded host'"'"'s thread pool, which is why wasm WINS there. The unmeasured corpus cells stay honest instead of estimated: ' \
    "${routed} route to the incumbent artifact, ${walled} wall on the wasm build path, ${oom} exhaust the embedded heap (#1729)" \
    ' — each re-measured every gate run, so a cell that starts benching fails the gate until its row is promoted. Ledger: `docs/benchmarks/wasm-runtime.txt` ('"${rt_version}, ${rt_date}"').' 
} > "$rt_body"

splice() { # $1 start marker, $2 end marker, $3 body file; stdin → stdout
  awk -v S="$1" -v E="$2" -v B="$3" '
    $0 == S { print; while ((getline l < B) > 0) print l; close(B); skip = 1; next }
    $0 == E { skip = 0 }
    !skip { print }'
}
splice "$STATS_START" "$STATS_END" "$stats_body" < "$README" | splice "$SIZE_START" "$SIZE_END" "$size_body" | splice "$RT_START" "$RT_END" "$rt_body" > "$rendered"

if [ "$MODE" = "--check" ]; then
  if ! cmp -s "$rendered" "$README"; then
    echo "::error::README.md stats blocks are stale — run: bash scripts/gen-readme-stats.sh"
    diff -u "$README" "$rendered" | head -40 || true
    exit 1
  fi
  echo "readme-stats: blocks are fresh (stdlib ${stdlib_fns}/${stdlib_mods}, tests ${test_files}, contracts ${contracts})."
  if [ -n "$(almide_bin)" ]; then
    measure_both
    if [ "$S_BYTES" != "$(kv structural_bytes)" ] || [ "$I_BYTES" != "$(kv incumbent_bytes)" ]; then
      echo "::error::Hello, world no longer matches $BASELINE (structural $S_BYTES vs $(kv structural_bytes), incumbent $I_BYTES vs $(kv incumbent_bytes)) — run: bash scripts/gen-readme-stats.sh --measure"
      exit 1
    fi
    echo "readme-stats: Hello, world rebuilt on both legs, bytes match the baseline."
  else
    echo "::warning::no compiler binary — Hello, world not rebuilt; the baseline's bytes were not re-verified."
  fi
  exit 0
fi

if cmp -s "$rendered" "$README"; then
  echo "readme-stats: blocks already fresh."
else
  cp "$rendered" "$README"
  echo "readme-stats: README.md rewritten (stdlib ${stdlib_fns}/${stdlib_mods}, tests ${test_files}, contracts ${contracts}, Hello, world ${size_incumb}/${size_struct} B)."
fi
