#!/usr/bin/env bash
# MIRI OVER THE FAN GATE (#4007)
# ==============================
#
# The fan pool (runtime/rs/src/list.rs, almide_fan_deal .. almide_fan_steal)
# hands resident pool threads a borrow of an offer's stack frame through a
# lifetime-erasing transmute — the runtime's `fan-sync` unsafe. Its soundness
# is a protocol: a helper reaches the frame only through a published slot,
# and the caller withdraws the slot and waits until no helper is inside
# before it returns. std::thread::scope would make that safe, but measured
# 13-32% slower on offers of fine and medium elements (#4007), so the gate
# stays and this script checks it.
#
# scripts/miri-generated.sh does NOT reach the gate's cross-thread path: Miri
# reports one CPU by default, so every fan offer has one worker and no helper
# ever enters a gate (the broken variant below passes there). Here the program
# runs with four reported CPUs and the cost model off, so every offer is dealt
# to helpers, over several scheduler seeds.
#
#   bash scripts/miri-fan-gate.sh
#
# ALMIDE_BIN      the compiler (default target/release/almide)
# MIRI_TOOLCHAIN  the nightly with the miri component (default nightly-2026-08-01)
# MIRI_SEEDS      the seed range (default 0..8)
#
# The control: the same crate with the caller's wait for its helpers removed
# MUST come back UB (a helper still inside a gate whose frame is gone). If it
# does not, the run above it proved nothing.
#
# -Zmiri-ignore-leaks: the pool's threads are resident and never joined, so
# the main thread always ends with them alive; that is the design, not a leak.
set -uo pipefail
export LC_ALL=C
cd "$(dirname "$0")/.."
ROOT="$PWD"
ALMIDE_BIN="${ALMIDE_BIN:-$ROOT/target/release/almide}"
MIRI_TOOLCHAIN="${MIRI_TOOLCHAIN:-nightly-2026-08-01}"
MIRI_SEEDS="${MIRI_SEEDS:-0..8}"
[ -x "$ALMIDE_BIN" ] || { echo "::error::miri-fan-gate: $ALMIDE_BIN is not executable (cargo build --release)"; exit 2; }
cargo "+$MIRI_TOOLCHAIN" miri --version >/dev/null 2>&1 || { echo "::error::miri-fan-gate: no miri on $MIRI_TOOLCHAIN"; exit 2; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Flat offers, an offer whose element errs, and offers nested inside a
# `fan { }` block's arms (helpers of one tree taking each other's blocks).
cat > "$WORK/fan.almd" <<'EOF'
fn churn(x: Int, k: Int) -> Int = {
  var acc = x
  var i = 0
  while i < k {
    acc = (acc * 31 + i) % 1000003
    i = i + 1
  }
  acc
}

effect fn inner(x: Int) -> Int = {
  let ys = fan.map(list.range(0, 4), (y) => ok(churn(x + y, 20)))!
  list.fold(ys, 0, (a, b) => a + b)
}

effect fn main() -> Unit = {
  let xs = fan.map(list.range(0, 16), (x) => ok(churn(x, 40)))!
  println("flat=" + int.to_string(list.fold(xs, 0, (a, b) => a + b)))
  let r = fan.map(list.range(0, 8), (x) => if x == 5 then err("bad 5") else ok(churn(x, 10)))
  match r {
    ok(v) => println("unexpected ok " + int.to_string(list.len(v))),
    err(e) => println("err=" + e),
  }
  let (a, b) = fan { inner(1)!, inner(2)! }
  println("nested=" + int.to_string(a + b))
}
EOF

d="$WORK/crate"
mkdir -p "$d"
# No prebuilt runtime rlib: the runtime's source is in the crate Miri interprets.
ALMIDE_NO_RTLIB=1 ALMIDE_RUN_PROJECT_DIR="$d" "$ALMIDE_BIN" build "$WORK/fan.almd" -o "$d/bin" >"$WORK/build.log" 2>&1 \
  || { echo "::error::miri-fan-gate: the fan program does not build"; cat "$WORK/build.log"; exit 1; }
for sym in 'almide_rt_fan_map_par(' 'fn almide_fan_deal' 'fn almide_fan_steal'; do
  grep -qF "$sym" "$d/src/main.rs" || { echo "::error::miri-fan-gate: the crate has no \`$sym\` — the program no longer reaches the fan gate"; exit 1; }
done
expected="$(ALMIDE_FAN_COST_OFF=1 "$d/bin" | sort -u)"

miri() {
  (cd "$1" && ALMIDE_FAN_COST_OFF=1 \
    MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-num-cpus=4 -Zmiri-ignore-leaks -Zmiri-many-seeds=$MIRI_SEEDS" \
    cargo "+$MIRI_TOOLCHAIN" miri run -q </dev/null >"$1/out" 2>"$1/err")
}

# The control: the caller returns as soon as its own taking is done.
c="$WORK/control"
mkdir -p "$c"
cp -R "$d/Cargo.toml" "$d/src" "$c/"
[ -f "$d/Cargo.lock" ] && cp "$d/Cargo.lock" "$c/"
python3 - "$c/src/main.rs" <<'EOF' || { echo "::error::miri-fan-gate: the control's anchor (the caller's wait for its helpers) is gone from almide_fan_deal — update the control"; exit 1; }
import sys
p = sys.argv[1]
s = open(p).read()
anchor = "            if gate.inside.load(Ordering::SeqCst) == 0 {\n                return true;"
if s.count(anchor) != 1:
    sys.exit(1)
open(p, "w").write(s.replace(anchor, "            if true {\n                return true;"))
EOF
miri "$c"
if ! grep -q 'Undefined Behavior' "$c/err"; then
  echo "::error::miri-fan-gate: the control (a caller that does not wait for its helpers) came back without UB — Miri did not see a helper inside a gate, so the run below would prove nothing"
  tail -20 "$c/err"
  exit 1
fi
echo "control: a caller that returns while helpers are inside its gate is UB under Miri, as it must be"

miri "$d"
code=$?
if grep -q 'Undefined Behavior' "$d/err"; then
  echo "::error::miri-fan-gate: undefined behaviour in the fan gate"
  sed -n '/Undefined Behavior/,/^$/p' "$d/err" | head -30
  exit 1
fi
if [ "$code" != 0 ]; then
  echo "::error::miri-fan-gate: Miri exited $code"
  grep -E '^error' "$d/err" | head -10
  exit 1
fi
got="$(sort -u "$d/out")"
if [ "$got" != "$expected" ]; then
  echo "::error::miri-fan-gate: output under Miri differs from the native run"
  diff <(echo "$expected") <(echo "$got")
  exit 1
fi
echo "miri-fan-gate: seeds $MIRI_SEEDS, four CPUs, every offer dealt to helpers — no undefined behaviour, output as native"
