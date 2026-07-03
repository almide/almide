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
# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

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
    8) # CUSTOM VARIANT (user ADT): scalar-field constructors + tag-dispatch match (the v1
       # value model — tag@slot0 + i64 field slots). Exercises ctor construct in arg + let
       # positions AND a multi-arm scalar match (ADT bricks 2+3). Arms are emitted out of tag
       # order on purpose (the match tests each arm's own tag, not declaration order).
       a=$((1+$(ri 40))); b=$((1+$(ri 40))); c=$((1+$(ri 40)))
       cat <<EOF
type Tok = Num(Int) | Pair(Int, Int) | Eof
type Word = Tok2(String) | Blank
fn val(t: Tok) -> Int = match t {
  Pair(x, y) => x * y,
  Num(n)     => n,
  Eof        => -1,
}
fn show(t: Tok) -> Unit = match t {
  Num(n)     => println(int.to_string(n)),
  Pair(x, y) => println(int.to_string(x + y)),
  Eof        => println("eof"),
}
fn name(t: Tok) -> String = match t {
  Num(n)     => "num:" + int.to_string(n),
  Pair(x, y) => "pair",
  Eof        => "eof",
}
fn wtag(w: Word) -> String = match w {
  Tok2(s) => s,
  Blank   => "_",
}
fn wlen(w: Word) -> Int = match w {
  Tok2(s) => string.len(s),
  Blank   => 0,
}
fn main() -> Unit = {
  let mid = Num($c)
  println(int.to_string(val(Num($a))))
  println(int.to_string(val(Pair($a, $b))))
  println(int.to_string(val(Eof)))
  println(int.to_string(val(mid)))
  show(Num($b))
  show(Pair($a, $c))
  show(Eof)
  println(name(Num($a)))
  println(name(Pair($b, $c)))
  println(name(Eof))
  println(wtag(Tok2("hi")))
  println(wtag(Blank))
  println(int.to_string(wlen(Tok2("abcd")) + wlen(Blank)))
  println(tos(Add(Lit($a), Neg(Lit($b)))))
  println(tos(Add(Neg(Lit($c)), Lit($a))))
}
type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr)
fn tos(e: Expr) -> String = match e {
  Lit(n)    => int.to_string(n),
  Add(l, r) => "(" + tos(l) + " + " + tos(r) + ")",
  Neg(x)    => "-" + tos(x),
}
EOF
       ;;
    9) # SCALAR COMPARISON across the type matrix — ==/!=/</<=/>/>= over Int/String/Bool/Float, in
       # BOTH an if-condition AND a value position (`let r = a OP b`). This is the class that shipped
       # silently: String/Bool ordering compared the i64 handle (arbitrary order), and a heap `==` ran
       # BOTH arms. No template generated a comparison, so the generative net was blind to the WHOLE
       # class — this template closes that hole.
       local cty cop cav cbv
       cty="$(pick int str bool float)"; cop="$(pick '==' '!=' '<' '<=' '>' '>=')"
       case $cty in
         int)   cav="$(ri 100)"; cbv="$(ri 100)" ;;
         str)   cav="\"$(pick apple banana cat ab abc x)\""; cbv="\"$(pick apple banana cat ab abc x)\"" ;;
         bool)  cav="$(pick true false)"; cbv="$(pick true false)" ;;
         float) cav="$(ri 10).$(ri 9)"; cbv="$(ri 10).$(ri 9)" ;;
       esac
       cat <<EOF
fn cmp_if() -> String = if $cav $cop $cbv then "T" else "F"
fn cmp_val() -> String = { let r = $cav $cop $cbv; if r then "T" else "F" }
fn main() -> Unit = { println(cmp_if()); println(cmp_val()) }
EOF
       ;;
    10) # LIST COMPARISON — ==/!= over List[Int|String|Float|Bool], if-condition AND value position.
        # Element-wise deep equality; was both-arms (silently ran both branches of the `if`).
       local lty lop lav lbv
       lty="$(pick int str float bool)"; lop="$(pick '==' '!=')"
       case $lty in
         int)   lav="[$(ri 9), $(ri 9), $(ri 9)]"; lbv="[$(ri 9), $(ri 9), $(ri 9)]" ;;
         str)   lav="[\"$(pick a b c)\", \"$(pick a b c)\"]"; lbv="[\"$(pick a b c)\", \"$(pick a b c)\"]" ;;
         float) lav="[$(ri 5).$(ri 9), $(ri 5).$(ri 9)]"; lbv="[$(ri 5).$(ri 9), $(ri 5).$(ri 9)]" ;;
         bool)  lav="[$(pick true false), $(pick true false)]"; lbv="[$(pick true false), $(pick true false)]" ;;
       esac
       cat <<EOF
fn cmp_if() -> String = { let a = $lav; let b = $lbv; if a $lop b then "T" else "F" }
fn cmp_val() -> String = { let a = $lav; let b = $lbv; let r = a $lop b; if r then "T" else "F" }
fn main() -> Unit = { println(cmp_if()); println(cmp_val()) }
EOF
       ;;
  esac
}

mismatch=0; match=0; wall=0; skip=0; runerr=0
for k in $(seq 1 "$N"); do
  t=$(( RANDOM % 11 ))
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
