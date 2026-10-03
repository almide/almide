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
#                        contracts in the ledger. Every cell is a TOTAL, so the
#                        rows render from the stamped record in
#                        proofs/ledger-counts.toml (a dated `counts:generated`
#                        block inside this one) — a fixture or contract PR never
#                        rewrites them; `--counts` (or scripts/gen-ledger-counts.sh)
#                        re-measures and restamps
#   wasm-size:generated  the Hello, world size table, rendered from the COMMITTED,
#                        stamped baseline docs/benchmarks/wasm-size.txt —
#                        measuring and publishing are separate acts (the
#                        build-speed block's rule): `--measure` rebuilds Hello,
#                        world on the wasm leg and restamps the baseline.
#
#   bash scripts/gen-readme-stats.sh            # rewrite README.md in place
#   bash scripts/gen-readme-stats.sh --check    # exit 1 if a block is stale; with a
#                                               # compiler at hand, also rebuild Hello,
#                                               # world and demand the baseline's bytes
#   bash scripts/gen-readme-stats.sh --measure  # rebuild Hello, world, restamp, rewrite
#   bash scripts/gen-readme-stats.sh --counts   # restamp proofs/ledger-counts.toml, rewrite
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
VIC_START="<!-- native-victory:generated:start — rendered from docs/benchmarks/native-victory.txt by scripts/gen-readme-stats.sh; DO NOT EDIT between the markers -->"
VIC_END="<!-- native-victory:generated:end -->"
VIC_LEDGER="docs/benchmarks/native-victory.txt"
. scripts/lib/ledger-counts.sh
[ "$MODE" = "--counts" ] && counts_stamp

[ -f "$README" ] || { echo "::error::$README not found (run from repo root)"; exit 2; }
[ -f "$LEDGER" ] || { echo "::error::$LEDGER not found"; exit 2; }
for m in "$STATS_START" "$STATS_END" "$SIZE_START" "$SIZE_END" "$RT_START" "$RT_END" "$VIC_START" "$VIC_END"; do
  grep -qxF "$m" "$README" || { echo "::error::marker missing from $README: $m"; exit 2; }
done

almide_bin() {
  if [ -n "${ALMIDE_BIN:-}" ]; then echo "$ALMIDE_BIN"; return; fi
  if [ -x target/release/almide ]; then echo "target/release/almide"; return; fi
  command -v almide 2>/dev/null || true
}

# Build Hello, world and print "<bytes> <leg>" exactly as the compiler's own
# `Built …` line names them — a size that does not name the leg that produced
# it is the ambiguity the Built line was added to remove. One wasm leg since
# #2752.
measure_leg() {
  local bin="$1" dir out line
  dir="$(mktemp -d)"
  printf 'fn main() -> Unit = {\n  println("Hello, world!")\n}\n' > "$dir/hello.almd"
  out="$("$bin" build "$dir/hello.almd" --target wasm -o "$dir/hello.wasm" 2>&1 || true)"
  rm -rf "$dir"
  line="$(printf '%s\n' "$out" | grep -oE '\(([0-9]+) bytes, structural leg' || true)"
  [ -n "$line" ] || { echo "::error::could not read the Built line from the compiler; output was: $out" >&2; return 1; }
  printf '%s %s\n' "$(printf '%s' "$line" | grep -oE '[0-9]+' | head -1)" structural
}

kv() { grep -E "^$1[[:space:]]*=" "$BASELINE" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//'; }

measure_both() { # sets S_BYTES from a fresh build
  local bin s
  bin="$(almide_bin)"
  [ -n "$bin" ] || { echo "::error::no compiler binary — set ALMIDE_BIN or build target/release/almide"; return 2; }
  s="$(measure_leg "$bin")"
  S_BYTES="${s%% *}"
  BIN_VERSION="$("$bin" --version 2>/dev/null | head -1)"
}

# Which build a version line names, in words a reader cannot misread (#2384).
# A develop build carries the number of the last Cargo.toml bump, so
# "almide 0.66.0 (dev)" is NOT the 0.66.0 release — it may be days of commits
# after it (or, between a bump and its tag, before it). Stamped as bare
# "almide 0.66.0 (dev)", the README read as "the 0.66.0 release ships 325 B"
# while the release shipped 953 B. So the stamp records the build's identity:
#   release → "almide X (release, sha)", as `almide --version` prints it
#   dev     → "the develop build at <sha>, after|before the vX release"
# and a version line that names neither (a pre-#2384 binary) is refused: a
# number that cannot say which compiler produced it is not a measurement.
describe_build() { # $1 = `almide --version` line → one phrase on stdout
  local line="$1" num sha tag p
  num="$(printf '%s' "$line" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
  case "$line" in
    *"(release"*) printf '%s' "$line"; return 0 ;;
    *"(dev"*) ;;
    *) echo "::error::'$line' names no build provenance (release/dev) — measure with a binary built by this tree's Makefile or the release workflow" >&2; return 1 ;;
  esac
  sha="$(printf '%s' "$line" | grep -oE '\(dev, [0-9a-f]+\)' | grep -oE '[0-9a-f]{7,}' || true)"
  # A dev binary built without ALMIDE_BUILD_SHA (plain `cargo build`) has no
  # sha; --check still re-verifies the committed bytes against this tree's own
  # build on every CI run, so the tree's HEAD is the build being stamped.
  [ -n "$sha" ] || sha="$(git rev-parse --short=9 HEAD)"
  tag="v$num"
  # The release tag sits on main's merge commit, so "after" means: the tag's
  # commit or one of its parents (the develop tip that was merged) is an
  # ancestor of this build.
  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    for p in $(git rev-parse "$tag^{commit}") $(git rev-parse "$tag^{commit}^@"); do
      if git merge-base --is-ancestor "$p" "$sha" 2>/dev/null; then
        printf 'the develop build at %s, after the %s release' "$sha" "$tag"; return 0
      fi
    done
    printf 'a develop build at %s that does not contain the %s release' "$sha" "$tag"
  else
    printf 'the develop build at %s, before %s is tagged' "$sha" "$tag"
  fi
}

if [ "$MODE" = "--measure" ]; then
  measure_both
  BUILD_DESC="$(describe_build "$BIN_VERSION")" || exit 2
  # The released row: README_RELEASE_BIN names a binary from a release asset
  # (`gh release download vX.Y.Z -R almide/almide`); without it the previous
  # stamp is carried over — a release's bytes are immutable once shipped.
  if [ -n "${README_RELEASE_BIN:-}" ]; then
    R_VERSION="$("$README_RELEASE_BIN" --version 2>/dev/null | head -1)"
    case "$R_VERSION" in *"(release"*) ;; *) echo "::error::README_RELEASE_BIN is '$R_VERSION', not a release build"; exit 2 ;; esac
    r="$(measure_leg "$README_RELEASE_BIN")"; R_BYTES="${r%% *}"; R_DATE="$(date +%F)"
  else
    R_VERSION="$(kv release_version 2>/dev/null || true)"; R_BYTES="$(kv release_bytes 2>/dev/null || true)"; R_DATE="$(kv release_date 2>/dev/null || true)"
  fi
  cat > "$BASELINE" <<EOF
# Hello, world wasm size — the SOURCE for the README's wasm-size block.
# Regenerate: bash scripts/gen-readme-stats.sh --measure
#             (README_RELEASE_BIN=<binary from a release asset> also restamps
#             the released row; without it the released row is carried over)
# Checked:    bash scripts/gen-readme-stats.sh --check rebuilds Hello, world and
#             demands these exact bytes — a changed preamble is re-stamped HERE,
#             never edited by hand in README.md. The bytes are machine-independent:
#             the emitters are pure Rust and the structural leg's build artifact is
#             the #1588 WASI form.
# build is derived from the binary's \`almide --version\` line (#2384): a develop
# build is named by its sha and the release it follows, never by the bare
# version number it carries.
version          = $BIN_VERSION
build            = $BUILD_DESC
date             = $(date +%F)
program          = fn main() -> Unit = { println("Hello, world!") }
structural_bytes = $S_BYTES
release_version  = $R_VERSION
release_bytes    = $R_BYTES
release_date     = $R_DATE
EOF
  echo "wasm-size: baseline restamped in $BASELINE (structural $S_BYTES B, $BUILD_DESC)"
fi

[ -f "$BASELINE" ] || { echo "::error::$BASELINE not found — run: bash scripts/gen-readme-stats.sh --measure"; exit 2; }

thousands() { printf '%s' "$1" | awk '{ n=$1; s=""; while (length(n) > 3) { s="," substr(n, length(n)-2) s; n=substr(n, 1, length(n)-3) } print n s }'; }

# The stamped totals, as recorded (the measurement recipes: scripts/lib/ledger-counts.sh).
stdlib_fns="$(counts_get stdlib_functions)"
stdlib_mods="$(counts_get stdlib_modules)"
test_files="$(counts_get spec_test_files)"
contracts="$(counts_get contracts)"
size_version="$(kv version)"; size_date="$(kv date)"
size_struct="$(thousands "$(kv structural_bytes)")"

stats_body="$(mktemp)"; size_body="$(mktemp)"; rt_body="$(mktemp)"; rendered="$(mktemp)"
trap 'rm -f "$stats_body" "$size_body" "$rt_body" "$rendered"' EXIT

counts_render_stats > "$stats_body"

# The develop and released columns come from ONE stamp each; the develop
# column's label is the stamped `build` phrase, never the bare version number a
# develop binary carries (describe_build above). A baseline stamped before the
# `build` key existed is refused rather than rendered as if it were a release.
size_build="$(kv build || true)"
[ -n "$size_build" ] || { echo "::error::$BASELINE has no 'build' line — restamp with: bash scripts/gen-readme-stats.sh --measure"; exit 2; }
rel_version="$(kv release_version || true)"; rel_bytes="$(kv release_bytes || true)"; rel_date="$(kv release_date || true)"
if [ -n "$rel_bytes" ]; then
  rel_num="$(printf '%s' "$rel_version" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
  rel_head=" released v${rel_num} |"; rel_rule="---:|"; rel_cell=" **$(thousands "$rel_bytes") B** |"
  rel_line=" The released column is the compiler from the v${rel_num} release asset (\`${rel_version}\`, \`gh release download v${rel_num} -R almide/almide\`), measured ${rel_date}."
else
  rel_head=""; rel_rule=""; rel_cell=""; rel_line=""
fi
cat > "$size_body" <<EOF
| Program (\`almide build --target wasm\`, as shipped) | develop |${rel_head}
|---|---:|${rel_rule}
| Hello, world | **${size_struct} B** |${rel_cell}

The develop column is ${size_build} (\`almide --version\`: \`${size_version}\`), measured ${size_date}; CI rebuilds it on every push and fails if the bytes move without a restamp of \`docs/benchmarks/wasm-size.txt\`.${rel_line} No post-hoc optimizer touches the shipped bytes (\`--wasm-opt\` is opt-in and its output is not the renderer's own module).
EOF

# A ledger stamped by a develop binary records `almide X (dev)`, which reads as
# the X release to anyone who does not know #2384. Say what it is.
say_version() { # $1 = a recorded `almide --version` line
  local num
  num="$(printf '%s' "$1" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
  case "$1" in
    *"(dev"*) printf 'a develop build carrying version %s, not the %s release' "$num" "$num" ;;
    *) printf '%s' "$1" ;;
  esac
}

# The wasm-runtime block (#1701), rendered from the committed ledger — the
# gate (scripts/check-wasm-runtime-ratio.sh) re-measures the ratios; this
# renderer only formats what is committed, same rule as the size block.
rt_version="$(grep -E '^version' "$RT_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//')"
rt_date="$(grep -E '^date' "$RT_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//')"
{
  echo '| Benchmark (`almide bench`, verify-then-time, min of 2×5 interleaved) | wasm/native, `main` only | cold start (spawn vs compile + instantiate) |'
  echo "|---|---:|---:|"
  grep -E '^[a-z].*\| measured' "$RT_LEDGER" | while IFS='|' read -r n _ _ _ r _ _ cr; do
    printf '| %s | **%s×** | %s× |\n' "$(echo "$n" | xargs)" "$(echo "$r" | xargs | cut -d' ' -f1)" "$(echo "$cr" | xargs | cut -d' ' -f1)"
  done
  walled=$(grep -cE '^[a-z].*\| walled' "$RT_LEDGER" || true)
  oom=$(grep -cE '^[a-z].*\| oom-embedded' "$RT_LEDGER" || true)
  echo
  printf '%s%s%s\n' \
    'Embedded wasm host (Perceus RC in linear memory) against the native binary, same machine, same run. The ratio times the program'"'"'s own `main`, entry to return, on both legs (native in-process, wasm around the host call): process spawn and module compile/instantiate are outside it, and the cold-start column shows them (#2980). Small workloads run at a ledger-fixed size (`args=`) so `main` is long enough to time. Cross-engine ratios do NOT cancel hardware (a 2-core CI runner measures nbody ~10x worse), so the stamped ratio verdict runs on the stamping machine class; CI gates the STATUS taxonomy below and judges the wasm leg by a same-runner A/B against the latest release binary (interleaved, min-of-runs, `ab_band` in the ledger — #2143) (`scripts/check-wasm-runtime-ratio.sh`). The wasm leg runs `fan` arms and callbacks one at a time. Of these rows only fannkuchredux'"'"'s native leg runs its `fan` on threads (#2044), which is most of its gap; binarytrees'"'"' and mandelbrot'"'"'s `fan.map` callbacks run sequentially on both legs (user time equals wall time on both, measured 2026-10-03). The unmeasured corpus cells stay honest instead of estimated: ' \
    "${walled} wall on the wasm build path, ${oom} exhaust the embedded heap (#1729)" \
    ' — each re-measured every gate run, so a cell that starts benching fails the gate until its row is promoted. Ledger: `docs/benchmarks/wasm-runtime.txt` ('"$(say_version "${rt_version}"), ${rt_date}"').' 
} > "$rt_body"

# The native-victory block (#1330), rendered from the committed ledger — the
# gate (scripts/check-perf-ratio.sh, VICTORY rows) re-measures the claim
# (ratio < 1.0) and the ablation every CI round; this renderer only formats
# what is committed, same rule as the two blocks above.
vic_version="$(grep -E '^version' "$VIC_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//')"
vic_date="$(grep -E '^date' "$VIC_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//')"
vic_kv() { grep -E "^$1[[:space:]]*=" "$VIC_LEDGER" | head -1 | sed -E 's/^[^=]*=[[:space:]]*//'; }
vic_run="$(vic_kv runner_run)"; vic_commit="$(vic_kv runner_commit)"
vic_rversion="$(vic_kv runner_version)"; vic_rdate="$(vic_kv runner_date)"
[ -n "$vic_run" ] && [ -n "$vic_rdate" ] || { echo "::error::$VIC_LEDGER has no runner_run/runner_date stamp for its CI column"; exit 2; }
vic_body=$(mktemp -t readme-vic.XXXXXX)
{
  echo '| Workload (`bench.py`, median of 9, interleaved) | optimization | Almide / ordinary Rust, M4 Pro | without it (`ALMIDE_REGION_OFF=1` / `ALMIDE_FAN_SEQUENTIAL=1`), M4 Pro | ubuntu-latest CI runner |'
  echo "|---|---|---:|---:|---:|"
  grep -E '^[a-z][a-z_-]* *\|' "$VIC_LEDGER" | while IFS='|' read -r n opt small large abl runner; do
    printf '| %s | %s | **%s** / **%s** | %s | %s |\n' "$(echo "$n" | xargs)" "$(echo "$opt" | xargs)" \
      "$(echo "$small" | xargs)" "$(echo "$large" | xargs)" "$(echo "$abl" | xargs)" "$(echo "$runner" | xargs)"
  done
  echo
  printf '%s\n' \
    'Two ratios per row are the two input sizes (the win holds at both); the Rust side is the ordinary program a person writes for it — a `Box` per node, one thread, no arena, no `unsafe`, no SIMD — compiled with the same `rustc` flags, and the "without it" column is the same Almide source with the region window turned off, so the whole gap is that one optimization. The absolute ratio is allocator-dependent (the CI runner frees a `Box` cheaper), the direction is not: the `perf-ratchet` job fails if either row reaches 1.0 or the ablation stops paying. Declaration and methodology: [docs/project/BENCHMARKS.md](./docs/project/BENCHMARKS.md#faster-than-ordinary-rust-1330). Ledger: `docs/benchmarks/native-victory.txt`. The M4 Pro columns were measured on '"${vic_version}, ${vic_date}"' and have not been re-measured since; the runner column is what the `perf-ratchet` job of develop CI run ['"${vic_run}"'](https://github.com/almide/almide/actions/runs/'"${vic_run}"') printed at `'"${vic_commit}"'` ('"$(say_version "${vic_rversion}")"'), '"${vic_rdate}"' (the size is in each cell).'
} > "$vic_body"

splice() { # $1 start marker, $2 end marker, $3 body file; stdin → stdout
  awk -v S="$1" -v E="$2" -v B="$3" '
    $0 == S { print; while ((getline l < B) > 0) print l; close(B); skip = 1; next }
    $0 == E { skip = 0 }
    !skip { print }'
}
splice "$STATS_START" "$STATS_END" "$stats_body" < "$README" | splice "$SIZE_START" "$SIZE_END" "$size_body" | splice "$RT_START" "$RT_END" "$rt_body" | splice "$VIC_START" "$VIC_END" "$vic_body" > "$rendered"

if [ "$MODE" = "--check" ]; then
  if ! cmp -s "$rendered" "$README"; then
    echo "::error::README.md stats blocks are stale — run: bash scripts/gen-readme-stats.sh"
    diff -u "$README" "$rendered" | head -40 || true
    exit 1
  fi
  echo "readme-stats: blocks are fresh (stdlib ${stdlib_fns}/${stdlib_mods}, tests ${test_files}, contracts ${contracts} — as stamped $(counts_date))."
  if [ -n "$(almide_bin)" ]; then
    measure_both
    if [ "$S_BYTES" != "$(kv structural_bytes)" ]; then
      echo "::error::Hello, world no longer matches $BASELINE (structural $S_BYTES vs $(kv structural_bytes)) — run: bash scripts/gen-readme-stats.sh --measure"
      exit 1
    fi
    echo "readme-stats: Hello, world rebuilt, bytes match the baseline."
  else
    echo "::warning::no compiler binary — Hello, world not rebuilt; the baseline's bytes were not re-verified."
  fi
  exit 0
fi

if cmp -s "$rendered" "$README"; then
  echo "readme-stats: blocks already fresh."
else
  cp "$rendered" "$README"
  echo "readme-stats: README.md rewritten (stdlib ${stdlib_fns}/${stdlib_mods}, tests ${test_files}, contracts ${contracts}, Hello, world ${size_struct} B)."
fi
