#!/usr/bin/env bash
# perf-probe: the wasm/native ratio rows of docs/benchmarks/wasm-runtime.txt
# plus ablation variants, on a GitHub runner instead of a loaded dev box.
#
# Three tables:
#   1. TIME   — `almide bench` (the ledger's own instrument: main() only,
#               embedded host on the wasm leg), ROUNDS interleaved rounds of
#               native, wasm; min-of-mins and median-of-medians per leg.
#   2. IR     — callgrind instruction counts (deterministic): the native
#               binary, and the WASI artifact precompiled by stock wasmtime
#               (default flags, and `-C inlining=y`), each minus an empty
#               program's count on the same leg (process start-up / runtime
#               init). Ir counts work, not cycles: a divide and an add are
#               one Ir each. Read it next to the TIME table.
#   3. ALLOC  — allocation counters on both legs (ALMIDE_ALLOC_COUNT /
#               ALMIDE_WASM_ALLOC_COUNT), deterministic.
set -uo pipefail
B="${ALMIDE_BIN:-$PWD/target/release/almide}"
P=research/benchmark/perf
D=.github/perf-probe
ROUNDS="${PROBE_ROUNDS:-3}"
RUNS="${PROBE_RUNS:-9}"
OUT="${PROBE_OUT:-/tmp/perf-probe}"
mkdir -p "$OUT"

# name | source | args
ROWS=(
  "listbuild_prealloc|$P/listbuild/listbuild_prealloc.almd|"
  "listbuild_append|$P/listbuild/listbuild_append.almd|"
  "listbuild_notrig|$D/listbuild_notrig.almd|"
  "trig_only|$D/trig_only.almd|"
  "fft|$P/fft/fft.almd|"
  "fasta|$P/fasta/fasta.almd|150000"
  "fasta_constmod|$D/fasta_constmod.almd|150000"
  "fasta_setat|$D/fasta_setat.almd|150000"
  "fasta_nowrite|$D/fasta_nowrite.almd|150000"
  "strchurn|$P/strchurn/strchurn.almd|"
  "itoa_only|$D/itoa_only.almd|"
  "mapbuild|$P/mapbuild/mapbuild.almd|100000"
  "mapbuild_int|$D/mapbuild_int.almd|100000"
  "mapbuild_str|$D/mapbuild_str.almd|100000"
)
ONLY="${PROBE_ONLY:-}"

cat > "$OUT/empty.almd" <<'EOF'
effect fn main() -> Unit = {
  println("x")
}
EOF

# ── 1. TIME ──────────────────────────────────────────────────────────────
# The FIRST "(min" / "median" on the line is main()'s; the cold-start column repeats both words.
min_of() { sed -n 's/^[^(]*(min \([0-9.]*\),.*/\1/p' | head -1; }
med_of() { sed -n 's/^[^(]*: median \([0-9.]*\) ms.*/\1/p' | head -1; }
echo "## TIME (almide bench, ms, main() only; $ROUNDS interleaved rounds x $RUNS runs)"
printf '%-22s %10s %10s %10s %10s %8s %8s\n' row nat_min wasm_min nat_med wasm_med r_min r_med
for row in "${ROWS[@]}"; do
  IFS='|' read -r name src args <<<"$row"
  [ -n "$ONLY" ] && [[ ",$ONLY," != *",$name,"* ]] && continue
  nmins=(); wmins=(); nmeds=(); wmeds=()
  for _ in $(seq "$ROUNDS"); do
    o=$("$B" bench "$src" --runs "$RUNS" ${args:+-- $args} 2>&1 </dev/null | grep '^bench ' | head -1)
    nmins+=("$(min_of <<<"$o")"); nmeds+=("$(med_of <<<"$o")")
    o=$("$B" bench "$src" --target wasm --runs "$RUNS" ${args:+-- $args} 2>&1 </dev/null | grep '^bench ' | head -1)
    wmins+=("$(min_of <<<"$o")"); wmeds+=("$(med_of <<<"$o")")
  done
  python3 - "$name" "${nmins[*]}" "${wmins[*]}" "${nmeds[*]}" "${wmeds[*]}" <<'PY'
import sys, statistics as st
name = sys.argv[1]
def nums(s):
    try: return [float(x) for x in s.split()]
    except ValueError: return []
nm, wm, nd, wd = (nums(a) for a in sys.argv[2:6])
if not (nm and wm and nd and wd) or len(nm) != len(wm):
    print(f"{name:<22} FAILED (a leg did not bench: nat={sys.argv[2]!r} wasm={sys.argv[3]!r})"); sys.exit()
a, b, c, d = min(nm), min(wm), st.median(nd), st.median(wd)
spread = lambda xs: f"{min(xs):.2f}-{max(xs):.2f}"
print(f"{name:<22} {a:10.2f} {b:10.2f} {c:10.2f} {d:10.2f} {b/a:8.2f} {d/c:8.2f}   (med spread nat {spread(nd)} wasm {spread(wd)})")
PY
done

# ── 2. IR ────────────────────────────────────────────────────────────────
ir_of() { # callgrind Ir total of a command, or empty
  local f="$OUT/cg.$$.$RANDOM"
  valgrind --tool=callgrind --callgrind-out-file="$f" "$@" >/dev/null 2>"$f.err" || { echo ""; return; }
  sed -n 's/^summary: *\([0-9]*\).*/\1/p;s/^totals: *\([0-9]*\).*/\1/p' "$f" | head -1
  rm -f "$f" "$f.err"
}
build_legs() { # name src -> $OUT/name.native, name.cwasm, name.inl.cwasm
  local name=$1 src=$2
  "$B" build "$src" -o "$OUT/$name.native" >/dev/null 2>&1 || echo "native build failed: $name" >&2
  "$B" build "$src" --target wasm -o "$OUT/$name.wasm" >/dev/null 2>&1 || echo "wasm build failed: $name" >&2
  wasmtime compile "$OUT/$name.wasm" -o "$OUT/$name.cwasm" 2>/dev/null || echo "wasmtime compile failed: $name" >&2
  wasmtime compile -C inlining=y "$OUT/$name.wasm" -o "$OUT/$name.inl.cwasm" 2>"$OUT/$name.inl.err" || { echo "wasmtime compile -C inlining=y failed: $name: $(head -2 "$OUT/$name.inl.err")" >&2; rm -f "$OUT/$name.inl.cwasm"; }
}
if command -v valgrind >/dev/null && command -v wasmtime >/dev/null; then
  build_legs empty "$OUT/empty.almd"
  base_n=$(ir_of "$OUT/empty.native"); base_w=$(ir_of wasmtime run --allow-precompiled "$OUT/empty.cwasm")
  base_i=""; [ -f "$OUT/empty.inl.cwasm" ] && base_i=$(ir_of wasmtime run -C inlining=y --allow-precompiled "$OUT/empty.inl.cwasm")
  echo
  echo "## IR (callgrind, millions of instructions, minus empty-program base: native ${base_n:-?}, wasm ${base_w:-?}, wasm+inl ${base_i:-?})"
  printf '%-22s %10s %10s %10s %8s %8s\n' row native wasm wasm_inl w/n inl/n
  for row in "${ROWS[@]}"; do
    IFS='|' read -r name src args <<<"$row"
    [ -n "$ONLY" ] && [[ ",$ONLY," != *",$name,"* ]] && continue
    build_legs "$name" "$src"
    n=$(ir_of "$OUT/$name.native" $args)
    w=$(ir_of wasmtime run --allow-precompiled "$OUT/$name.cwasm" $args)
    i=""; [ -f "$OUT/$name.inl.cwasm" ] && i=$(ir_of wasmtime run -C inlining=y --allow-precompiled "$OUT/$name.inl.cwasm" $args)
    python3 - "$name" "${n:-}" "${w:-}" "${i:-}" "${base_n:-0}" "${base_w:-0}" "${base_i:-0}" <<'PY'
import sys
name, n, w, i, bn, bw, bi = sys.argv[1:8]
def m(x, b):
    return (int(x) - int(b or 0)) / 1e6 if x else None
N, W, I = m(n, bn), m(w, bw), m(i, bi)
f = lambda v: f"{v:10.1f}" if v is not None else f"{'FAILED':>10}"
r = lambda a, b: f"{a/b:8.2f}" if a is not None and b else f"{'-':>8}"
print(f"{name:<22} {f(N)} {f(W)} {f(I)} {r(W, N)} {r(I, N)}")
PY
  done
  # Stock-wasmtime wall time, default vs `-C inlining=y`, interleaved, whole
  # process (start-up included, identical on both columns), min/median of N.
  echo
  echo "## STOCK WASMTIME wall ms (precompiled; default vs -C inlining=y; interleaved x $RUNS)"
  printf '%-22s %10s %10s %10s %10s %8s\n' row def_min inl_min def_med inl_med inl/def
  for row in "${ROWS[@]}"; do
    IFS='|' read -r name src args <<<"$row"
    [ -n "$ONLY" ] && [[ ",$ONLY," != *",$name,"* ]] && continue
    [ -f "$OUT/$name.cwasm" ] && [ -f "$OUT/$name.inl.cwasm" ] || { echo "$name: no cwasm pair"; continue; }
    python3 - "$name" "$RUNS" "$OUT/$name.cwasm" "$OUT/$name.inl.cwasm" $args <<'PY'
import subprocess, sys, time, statistics as st
name, runs, d, i = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
args = sys.argv[5:]
cmd_d = ["wasmtime", "run", "--allow-precompiled", d, *args]
cmd_i = ["wasmtime", "run", "-C", "inlining=y", "--allow-precompiled", i, *args]
td, ti = [], []
ref = None
for _ in range(runs + 1):
    for cmd, acc in ((cmd_d, td), (cmd_i, ti)):
        t = time.perf_counter(); out = subprocess.run(cmd, capture_output=True).stdout; acc.append((time.perf_counter() - t) * 1000)
        if ref is None: ref = out
        elif out != ref: print(f"{name:<22} OUTPUT MISMATCH"); sys.exit()
td, ti = td[1:], ti[1:]
print(f"{name:<22} {min(td):10.2f} {min(ti):10.2f} {st.median(td):10.2f} {st.median(ti):10.2f} {min(ti)/min(td):8.2f}")
PY
  done
else
  echo "IR table skipped: valgrind or wasmtime missing"
fi

# ── 3. ALLOC ─────────────────────────────────────────────────────────────
echo
echo "## ALLOC (deterministic counters)"
for row in "${ROWS[@]}"; do
  IFS='|' read -r name src args <<<"$row"
  [ -n "$ONLY" ] && [[ ",$ONLY," != *",$name,"* ]] && continue
  n=$(ALMIDE_ALLOC_COUNT=1 "$B" run "$src" ${args:+-- $args} 2>&1 >/dev/null </dev/null | grep '__ALMD_ALLOC' | tail -1)
  w=$(ALMIDE_WASM_ALLOC_COUNT=1 "$B" run "$src" --target wasm ${args:+-- $args} 2>&1 >/dev/null </dev/null | grep '__ALMD_WASM_ALLOC' | tail -1)
  printf '%-22s native: %s\n%-22s wasm:   %s\n' "$name" "${n#__ALMD_ALLOC }" "" "${w#__ALMD_WASM_ALLOC }"
done
