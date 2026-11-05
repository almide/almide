#!/usr/bin/env bash
# LAYOUT-RANDOMIZED A/B OF TWO RELEASE PROFILES (#4459). A measurement, not a gate.
#
# What it measures: for each perf row, the runtime of the `almide build
# --release` binary under profile A and profile B, each built with N
# different function orders (lld `--shuffle-sections=.text*=<seed>`), timed
# in interleaved rounds (every round runs every (row, profile, layout) cell
# once, in a shuffled order). Per row it prints the mean over layouts of each
# layout's median time, the B/A ratio of those means, and a two-sided
# rank-sum p-value over the 2N per-layout medians.
#
# Why layouts: on the layout-sensitive rows (fib, regexcycle, strbuild,
# decode, treealloc) ONE build's function placement — where `.text` starts
# mod 64, how the std functions after the program pack — moves the runtime
# by 10-13% with the code unchanged. A single-layout A/B of a change that
# moves the layout (any change to what the binary links) reads that as a
# win or a loss. Sampling layouts on both sides measures the code instead.
#
#   A = ALMIDE_RELEASE_STD_UNSPLIT=1 (the plain fat-LTO profile)
#   B = the default profile (the backtrace split, src/cli/std_split.rs)
#
# Usage: ALMIDE_BIN=target/release/almide scripts/perf-layout-ab.sh [row ...]
#   env: LAYOUTS (16) ROUNDS (5) OUT (a temp dir; keeps the binaries and the
#        per-run times in $OUT/times.json)
#        SECTIONS (".text*"): the output-section globs shuffled, space-separated;
#        ".text* .rodata* .data* .bss*" randomizes where the data lands too
# Linux x86_64 with rust-lld (the rustc default there) only.
set -euo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"
ALMIDE="$(realpath "${ALMIDE_BIN:-$(command -v almide)}")"
LAYOUTS="${LAYOUTS:-16}"
ROUNDS="${ROUNDS:-5}"
OUT="${OUT:-$(mktemp -d)}"
SECTIONS="${SECTIONS:-.text*}"
mkdir -p "$OUT"
REAL_RUSTC="$(command -v rustc)"

# name source arg ablation-env (bench.py's QUICK_ARGS; `-` = none)
ROWS_TABLE='
nbody research/benchmark/perf/nbody/nbody.almd 10000000 -
spectralnorm research/benchmark/perf/spectralnorm/spectralnorm.almd 2500 -
fasta research/benchmark/perf/fasta/fasta.almd 2500000 -
fft research/benchmark/perf/fft/fft.almd 22 -
fannkuchredux research/benchmark/perf/fannkuchredux/fannkuchredux.almd 11 -
fannkuchredux-abl research/benchmark/perf/fannkuchredux/fannkuchredux.almd 11 ALMIDE_FAN_SEQUENTIAL
onebrc research/benchmark/perf/onebrc/onebrc.almd 1000000 -
binarytrees research/benchmark/perf/binarytrees/binarytrees.almd 17 -
binarytrees-abl research/benchmark/perf/binarytrees/binarytrees.almd 17 ALMIDE_REGION_OFF
treealloc research/benchmark/perf/treealloc/treealloc.almd 20 -
treealloc-abl research/benchmark/perf/treealloc/treealloc.almd 20 ALMIDE_REGION_OFF
mandelbrot research/benchmark/perf/mandelbrot/mandelbrot.almd 2000 -
strbuild-append research/benchmark/perf/strbuild/strbuild_append.almd 200000 -
strbuild-push research/benchmark/perf/strbuild/strbuild_push.almd 200000 -
listbuild research/benchmark/perf/listbuild/listbuild_prealloc.almd 23 -
listbuild-append research/benchmark/perf/listbuild/listbuild_append.almd 23 -
listbuild-comb research/benchmark/perf/listbuild/listbuild_combinator.almd 23 -
strchurn research/benchmark/perf/strchurn/strchurn.almd 2000000 -
decode research/benchmark/perf/decode/decode.almd 1000000 -
wordfreq research/benchmark/perf/wordfreq/wordfreq.almd 8000000 -
wordfreq-group research/benchmark/perf/wordfreq/wordfreq_group.almd 8000000 -
jsonround research/benchmark/perf/jsonround/jsonround.almd 100000 -
fib research/benchmark/perf/fib/fib.almd 31:300 -
fib-abl research/benchmark/perf/fib/fib.almd 31:300 ALMIDE_SELF_INLINE_OFF
regexcycle-8 research/benchmark/perf/regexcycle/regexcycle.almd 8:400000 -
regexcycle-32 research/benchmark/perf/regexcycle/regexcycle.almd 32:400000 -
'
if [ "$#" -gt 0 ]; then
  rows="$(for r in "$@"; do echo "$ROWS_TABLE" | awk -v r="$r" '$1 == r'; done)"
else
  rows="$(echo "$ROWS_TABLE" | sed '/^$/d')"
fi
[ -n "$rows" ] || { echo "perf-layout-ab: no such row: $*"; exit 2; }
echo "$rows" > "$OUT/rows.txt"

# The shuffle seed reaches the fat-LTO link through a `rustc` shim first on
# PATH (almide calls `rustc`; cargo finds the same one): it appends the lld
# flags to the one rustc call that links with LTO and passes every other call
# through untouched.
mkdir -p "$OUT/shim"
cat > "$OUT/shim/rustc" <<EOF
#!/usr/bin/env bash
set -f
for a in "\$@"; do
  if [ "\$a" = "lto=fat" ] || [ "\$a" = "lto" ]; then
    extra=()
    for g in $SECTIONS; do extra+=("-Clink-arg=-Wl,--shuffle-sections=\$g=\$LAYOUT_SEED"); done
    exec "$REAL_RUSTC" "\$@" "\${extra[@]}"
  fi
done
exec "$REAL_RUSTC" "\$@"
EOF
chmod +x "$OUT/shim/rustc"

for side in A B; do
  for k in $(seq 1 "$LAYOUTS"); do
    cell="$side$k"; mkdir -p "$OUT/bin/$cell" "$OUT/tmp/$cell"
    while read -r name src arg abl; do
      envs=(LAYOUT_SEED=$((k * 7919)) TMPDIR="$OUT/tmp/$cell" PATH="$OUT/shim:$PATH")
      [ "$side" = A ] && envs+=(ALMIDE_RELEASE_STD_UNSPLIT=1)
      [ "$abl" != "-" ] && envs+=("$abl=1")
      if ! env "${envs[@]}" "$ALMIDE" build --release "$src" -o "$OUT/bin/$cell/$name" > "$OUT/bin/$cell/$name.log" 2>&1; then
        cat "$OUT/bin/$cell/$name.log"; echo "perf-layout-ab: build failed: $cell $name"; exit 1
      fi
    done < "$OUT/rows.txt"
    # A cache shared across layouts would hand every layout the same binary.
    rm -rf "$OUT/tmp/$cell"
  done
done

python3 -I - "$OUT" "$LAYOUTS" "$ROUNDS" <<'PY'
import json, math, os, random, statistics, subprocess, sys, time
out, layouts, rounds = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
rows = [l.split() for l in open(f"{out}/rows.txt") if l.strip()]
cells = [f"{s}{k}" for s in "AB" for k in range(1, layouts + 1)]
def argv(cell, r): return [f"{out}/bin/{cell}/{r[0]}"] + ([] if r[2] == "-" else [r[2]])
def env(r):
    e = dict(os.environ)
    if r[3] != "-": e[r[3]] = "1"
    return e
for r in rows:  # every layout of both profiles prints the same thing
    outs = {subprocess.run(argv(c, r), capture_output=True, env=env(r), timeout=600).stdout for c in cells}
    if len(outs) != 1: sys.exit(f"perf-layout-ab: {r[0]} prints differently across builds")
t = {(r[0], c): [] for r in rows for c in cells}
for k in range(rounds):
    order = [(r, c) for r in rows for c in cells]; random.shuffle(order)
    for r, c in order:
        t0 = time.perf_counter()
        subprocess.run(argv(c, r), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env(r))
        t[(r[0], c)].append(time.perf_counter() - t0)
    print(f"round {k + 1}/{rounds}", file=sys.stderr, flush=True)
json.dump({f"{a}|{b}": v for (a, b), v in t.items()}, open(f"{out}/times.json", "w"))
def ranksum_p(a, b):
    pooled = sorted((x, g) for g, xs in enumerate((a, b)) for x in xs)
    ranks, i = [0.0] * len(pooled), 0
    while i < len(pooled):
        j = i
        while j < len(pooled) and pooled[j][0] == pooled[i][0]: j += 1
        for q in range(i, j): ranks[q] = (i + j + 1) / 2
        i = j
    n1, n2 = len(a), len(b)
    u = sum(rk for rk, (_, g) in zip(ranks, pooled) if g == 0) - n1 * (n1 + 1) / 2
    z = (u - n1 * n2 / 2) / math.sqrt(n1 * n2 * (n1 + n2 + 1) / 12)
    return math.erfc(abs(z) / math.sqrt(2))
print(f"{'row':18s} {'A mean s':>9s} {'B mean s':>9s} {'B/A':>6s} {'p':>6s}  {'A spread':>8s}  {'B spread':>8s}")
for r in rows:
    a = [statistics.median(t[(r[0], f"A{k}")]) for k in range(1, layouts + 1)]
    b = [statistics.median(t[(r[0], f"B{k}")]) for k in range(1, layouts + 1)]
    ma, mb = statistics.mean(a), statistics.mean(b)
    print(f"{r[0]:18s} {ma:9.4f} {mb:9.4f} {mb / ma:6.3f} {ranksum_p(a, b):6.3f}  {max(a) / min(a) - 1:8.1%}  {max(b) / min(b) - 1:8.1%}")
PY
