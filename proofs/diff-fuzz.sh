#!/usr/bin/env bash
# DIFFERENTIAL FUZZ GATE — the generative complement to the fixture-based output-parity.sh.
#
# output-parity.sh byte-diffs a FIXED corpus (spec/*.almd): it only catches a miscompile in a
# shape that already has a fixture. The `var v = w` scalar-aliasing bug shipped because NO fixture
# exercised "init a mutable var from another var, then reassign it in a loop". This gate closes that
# blind spot GENERATIVELY: it synthesizes random programs over the v1-renderable subset (var/let
# binds + reassignment, scalar/string/list accumulators, for-loops, if/match, recursion, the sha1
# rotation shape) and byte-diffs v0 (native) vs v1 (wasm). A new lowering brick is auto-covered the
# moment its shape appears in a generated program — no hand-written fixture required.
#
#   v0 oracle : almide run <f>                                   (native)
#   v1        : examples/render_program <f> -> wat -> wasmtime   (trust-spine path)
# Per program: MATCH (v0==v1) / WALL (v1 Unsupported — fine) / v0fail (skip) / MISMATCH (FAIL — a
# silent miscompile) / RUNERR (v1 renders but traps — FAIL). Any MISMATCH/RUNERR fails the gate.
#
#   bash proofs/diff-fuzz.sh [N] [SEED]    # default N=120, SEED from $EPOCHSECONDS
# Deterministic: the SEED is printed; a failing case prints its generated source for exact repro.
# Skips gracefully if almide (v0) or wasmtime is absent.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
N="${1:-120}"
SEED="${2:-${EPOCHSECONDS:-$(date +%s)}}"

command -v wasmtime >/dev/null || { echo "diff-fuzz: wasmtime not found — SKIP"; exit 0; }
# v0 and v1 MUST come from the SAME, FRESH build of THIS tree, or the diff is meaningless. ALWAYS
# rebuild (cargo is incremental — a no-op if current) rather than trust an existing binary, which may
# be stale and produce a phantom mismatch. Build the variant matching the tree (release if a release
# tree exists — CI builds --release; else debug). Never a PATH `almide`.
if [ -d "$ROOT/target/release/.fingerprint" ]; then
  PROF=release; FLAG=--release
else
  PROF=debug; FLAG=
fi
( cd "$ROOT" && cargo build -q $FLAG --bin almide 2>/dev/null && cargo build -q $FLAG -p almide-mir --example render_program 2>/dev/null ) \
  || { echo "diff-fuzz: build failed"; exit 1; }
ALM="$ROOT/target/$PROF/almide"; RP="$ROOT/target/$PROF/examples/render_program"
{ [ -x "$ALM" ] && [ -x "$RP" ]; } || { echo "diff-fuzz: almide / render_program not built — SKIP"; exit 0; }

TMP="${TMPDIR:-/tmp}/almide-diff-fuzz.$$"; mkdir -p "$TMP"; trap 'rm -rf "$TMP"' EXIT
RANDOM=$SEED
echo "diff-fuzz: N=$N SEED=$SEED"

# Deterministic pseudo-random helpers (seeded $RANDOM).
ri() { echo $(( RANDOM % $1 )); }                 # 0..$1-1
pick() { local a=("$@"); echo "${a[$((RANDOM % ${#a[@]}))]}"; }

gen() {  # echo a self-contained program for template id $1
  local t=$1 n op a b c d
  case $t in
    0) # SCALAR VAR ALIASING (the shipped bug): a aliases h, reassigned in a loop — h must NOT change.
       a=$((1 + $(ri 90))); n=$((1 + $(ri 8))); op="$(pick + - '*')"
       cat <<EOF
fn f() -> Int = {
  var h = $a
  var a = h
  for i in 0..$n { a = a $op (i + 1) }
  h * 100000 + a
}
fn main() -> Unit = println(int.to_string(f()))
EOF
       ;;
    1) # SHA1-style rotation: e<-d<-c<-b<-a<-t, each from the previous var (aliasing if mis-lowered).
       a=$((1+$(ri 50))); b=$((1+$(ri 50))); c=$((1+$(ri 50))); n=$((1+$(ri 6)))
       cat <<EOF
fn f() -> Int = {
  var a = $a
  var b = $b
  var c = $c
  for i in 0..$n {
    let t = a + b + c + i
    c = b
    b = a
    a = t
  }
  a * 10000 + b * 100 + c
}
fn main() -> Unit = println(int.to_string(f()))
EOF
       ;;
    2) # SCALAR for-accumulator (used OR unused loop var).
       a=$((1+$(ri 9))); n=$((1+$(ri 20))); op="$(pick + - '*')"; b="$(pick 'i' '1')"
       cat <<EOF
fn f() -> Int = { var s = $a; for i in 0..$n { s = s $op $b }; s }
fn main() -> Unit = println(int.to_string(f()))
EOF
       ;;
    3) # STRING accumulator (concat / interp).
       n=$((1+$(ri 12))); c="$(pick x ab Q '-')"
       if [ $(( RANDOM % 2 )) = 0 ]; then body="s = s + \"$c\""; else body="s = \"\${s}$c\""; fi
       cat <<EOF
fn f() -> String = { var s = ""; for i in 0..$n { $body }; s }
fn main() -> Unit = println(f())
EOF
       ;;
    4) # LIST[Int] accumulator + length.
       n=$((1+$(ri 15)))
       cat <<EOF
fn f() -> Int = { var xs: List[Int] = []; for i in 0..$n { xs = xs + [i * 2] }; list.len(xs) + list.sum(xs) }
fn main() -> Unit = println(int.to_string(f()))
EOF
       ;;
    5) # if returning a scalar, with a var reassigned in one branch.
       a=$((1+$(ri 50))); b=$((1+$(ri 50))); c=$((1+$(ri 50)))
       cat <<EOF
fn f(x: Int) -> Int = {
  var r = $a
  if x > $b then { r = r + $c } else { r = r * 2 }
  r
}
fn main() -> Unit = println(int.to_string(f($((1+$(ri 100)))) + f($((1+$(ri 100))))))
EOF
       ;;
    6) # NESTED loop with a per-iteration heap accumulator + a carried scalar (sha1 block shape).
       n=$((1+$(ri 4))); op="$(pick + '*')"
       cat <<EOF
fn f() -> Int = {
  var acc = 0
  for blk in 0..$n {
    var w = bytes.new(8)
    for i in 0..8 { w = bytes.set(w, i, blk $op (i + 1)) }
    acc = acc + bytes.read_u8(w, 3) + bytes.read_u8(w, 7)
  }
  acc
}
fn main() -> Unit = println(int.to_string(f()))
EOF
       ;;
    7) # int match returning a scalar (tag dispatch).
       a=$((1+$(ri 5)))
       cat <<EOF
fn classify(n: Int) -> Int = match n { 0 => 100, 1 => 200, 2 => 300, _ => n * 7 }
fn main() -> Unit = println(int.to_string(classify($a) + classify($((RANDOM % 6)))))
EOF
       ;;
  esac
}

mismatch=0; match=0; wall=0; skip=0; runerr=0
for k in $(seq 1 "$N"); do
  t=$(( RANDOM % 8 ))
  src="$TMP/p$k.almd"; gen "$t" > "$src"
  o0="$("$ALM" run "$src" 2>/dev/null)" || { skip=$((skip+1)); continue; }   # v0 must run (else skip)
  if ! "$RP" "$src" > "$src.wat" 2>/dev/null; then wall=$((wall+1)); continue; fi   # v1 walls = fine
  if ! o1="$(wasmtime "$src.wat" 2>/dev/null)"; then
    runerr=$((runerr+1)); echo "RUNERR (v1 traps) — tmpl $t:"; cat "$src"; continue
  fi
  if [ "$o0" = "$o1" ]; then match=$((match+1)); else
    mismatch=$((mismatch+1))
    echo "MISMATCH — tmpl $t  v0=[$o0]  v1=[$o1]"; echo "--- source ---"; cat "$src"; echo "--------------"
  fi
done

echo "diff-fuzz: match=$match wall=$wall skip=$skip mismatch=$mismatch runerr=$runerr (SEED=$SEED)"
[ "$mismatch" = 0 ] && [ "$runerr" = 0 ] || { echo "diff-fuzz: FAIL — re-run with SEED=$SEED to reproduce"; exit 1; }
echo "diff-fuzz: OK"
