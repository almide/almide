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
# MIRI_PREEMPTION_RATES  the scheduler preemption rates every program runs
#                 under, each over all of MIRI_SEEDS (default "0.01 0.2":
#                 Miri's default, and one that switches threads twenty times
#                 as often — two schedules per seed instead of one)
#
# THE SHAPES. Each is its own program, so a control that comes back UB says
# that THIS shape crosses threads under Miri, not that some other one does:
#   gate        flat `fan.map` offers, an offer whose element errs, offers
#               nested inside a `fan { }` block's arms
#   par_filter  `list.filter` / `list.any` under `fan { }` arms — the
#               `list.par_filter` / `list.par_any` twins (almide_fan_search)
#   min_bounded gate offers made inside `fan.min(compute…)` arms and a
#               `fan.bounded` body (the metered race fold, once `fan.race`)
#   fold        `fs.fold_lines_chunked`'s line-fold workers, which start
#               through the fan arms' bounded spawn (`almide_fan_spawn`)
#
# The controls: for the gate shapes, the same crate with the caller's wait
# for its helpers removed MUST come back UB (a helper still inside a gate
# whose frame is gone). The line-fold workers hold no gate (they are scoped
# threads), so their control writes one unsynchronised `static mut` from
# every worker, which MUST come back UB as a data race — proof that Miri ran
# two workers at once. If a control does not, the run after it proved nothing.
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
MIRI_PREEMPTION_RATES="${MIRI_PREEMPTION_RATES:-0.01 0.2}"

CHURN='fn churn(x: Int, k: Int) -> Int = {
  var acc = x
  var i = 0
  while i < k {
    acc = (acc * 31 + i) % 1000003
    i = i + 1
  }
  acc
}
'

mkdir -p "$WORK/gate" "$WORK/par_filter" "$WORK/min_bounded" "$WORK/fold"
{ echo "$CHURN"; cat <<'EOF'; } > "$WORK/gate/main.almd"
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
{ echo "$CHURN"; cat <<'EOF'; } > "$WORK/par_filter/main.almd"
effect fn main() -> Unit = {
  let (kept, hit) = fan {
    list.filter(list.range(0, 16), (x) => churn(x, 30) % 2 == 0),
    list.any(list.range(0, 16), (x) => churn(x, 20) % 7 == 3),
  }
  println("kept=" + int.to_string(list.len(kept)) + " any=" + (if hit then "yes" else "no"))
}
EOF
{ echo "$CHURN"; cat <<'EOF'; } > "$WORK/min_bounded/main.almd"
fn inner(x: Int) -> Int = {
  let (a, b) = fan {
    list.filter(list.range(0, 12), (y) => churn(x + y, 20) % 2 == 0),
    list.filter(list.range(0, 12), (y) => churn(x + y, 10) % 3 == 0),
  }
  list.len(a) + list.len(b)
}

effect fn main() -> Unit = {
  let w = fan.min(compute.ms(50)) { inner(1), inner(2) } ?? -1
  let r = fan.bounded(compute.ms(50)) { inner(3) } ?? -1
  println("min=" + int.to_string(w) + " bounded=" + int.to_string(r))
}
EOF
cat > "$WORK/fold/main.almd" <<'EOF'
import fs

effect fn main() -> Unit = {
  let text = list.range(0, 40) |> list.map((i) => "line " + int.to_string(i)) |> list.join("\n")
  fs.write_text("fold.txt", text + "\n")!
  let parts = fs.fold_lines_chunked("fold.txt", 4, 0, (acc, line) => acc + string.len(line))!
  println("parts=" + int.to_string(list.len(parts)) + " total=" + int.to_string(list.fold(parts, 0, (a, b) => a + b)))
}
EOF

# shape -> the symbols its crate must contain (the shape still reaches the
# path the gate judges) and its control
declare -A NEEDS=(
  [gate]='almide_rt_fan_map_par(|fn almide_fan_deal|fn almide_fan_steal'
  [par_filter]='almide_rt_list_par_filter(|almide_rt_list_par_any(|fn almide_fan_deal'
  [min_bounded]='almide_rt_list_par_filter(|__almd_bounded_|fn almide_fan_deal'
  [fold]='almide_rt_fs_fold_lines_chunked(|fn almide_fan_spawn'
)
declare -A CONTROL=([gate]=wait [par_filter]=wait [min_bounded]=wait [fold]=race)
SHAPES="${MIRI_SHAPES:-gate par_filter min_bounded fold}"

miri() { # dir rate
  (cd "$1" && ALMIDE_FAN_COST_OFF=1 \
    MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-num-cpus=4 -Zmiri-ignore-leaks -Zmiri-many-seeds=$MIRI_SEEDS -Zmiri-preemption-rate=$2" \
    cargo "+$MIRI_TOOLCHAIN" miri run -q </dev/null >"$1/out" 2>"$1/err")
}

break_crate() { # main.rs kind
  python3 - "$1" "$2" <<'EOF'
import sys
p, kind = sys.argv[1], sys.argv[2]
s = open(p).read()
if kind == "wait":
    # the caller returns as soon as its own taking is done
    anchor = "            if gate.inside.load(Ordering::SeqCst) == 0 {\n                return true;"
    repl = "            if true {\n                return true;"
else:
    # every line-fold worker bumps one unsynchronised counter
    anchor = "almide_stack_guard_register();\n                    ranges.iter()"
    repl = "{ static mut ALMIDE_MIRI_RACE: u64 = 0; unsafe { *(&raw mut ALMIDE_MIRI_RACE) += 1; } }\n                    almide_stack_guard_register();\n                    ranges.iter()"
if s.count(anchor) != 1:
    sys.exit(1)
open(p, "w").write(s.replace(anchor, repl))
EOF
}

for shape in $SHAPES; do
  d="$WORK/$shape/crate"
  mkdir -p "$d"
  # No prebuilt runtime rlib: the runtime's source is in the crate Miri interprets.
  (cd "$WORK/$shape" && ALMIDE_NO_RTLIB=1 ALMIDE_RUN_PROJECT_DIR="$d" "$ALMIDE_BIN" build main.almd -o "$d/bin") >"$WORK/$shape/build.log" 2>&1 \
    || { echo "::error::miri-fan-gate: the $shape program does not build"; cat "$WORK/$shape/build.log"; exit 1; }
  IFS='|' read -r -a syms <<<"${NEEDS[$shape]}"
  for sym in "${syms[@]}"; do
    grep -qF "$sym" "$d/src/main.rs" || { echo "::error::miri-fan-gate: the $shape crate has no \`$sym\` — the program no longer reaches the path this shape judges"; exit 1; }
  done
  expected="$(cd "$d" && ALMIDE_FAN_COST_OFF=1 ./bin | sort -u)"

  c="$WORK/$shape/control"
  mkdir -p "$c"
  cp -R "$d/Cargo.toml" "$d/src" "$c/"
  [ -f "$d/Cargo.lock" ] && cp "$d/Cargo.lock" "$c/"
  break_crate "$c/src/main.rs" "${CONTROL[$shape]}" \
    || { echo "::error::miri-fan-gate: the $shape control's anchor (${CONTROL[$shape]}) is gone from the generated crate — update the control"; exit 1; }
  miri "$c" 0.01
  if ! grep -q 'Undefined Behavior' "$c/err"; then
    echo "::error::miri-fan-gate: the $shape control (${CONTROL[$shape]}) came back without UB — Miri did not see this shape cross threads, so the run below would prove nothing"
    hew "$c/err" --tail 20
    exit 1
  fi
  echo "control $shape (${CONTROL[$shape]}): UB under Miri, as it must be — $(grep -m1 -o 'Undefined Behavior: [^`]*' "$c/err" | head -c 120)"

  for rate in $MIRI_PREEMPTION_RATES; do
    miri "$d" "$rate"
    code=$?
    if grep -q 'Undefined Behavior' "$d/err"; then
      echo "::error::miri-fan-gate: undefined behaviour in the $shape shape (preemption rate $rate)"
      sed -n '/Undefined Behavior/,/^$/p' "$d/err" | head -30
      exit 1
    fi
    if [ "$code" != 0 ]; then
      echo "::error::miri-fan-gate: Miri exited $code on the $shape shape (preemption rate $rate)"
      hew grep '^error' "$d/err" | head -10
      exit 1
    fi
    got="$(sort -u "$d/out")"
    if [ "$got" != "$expected" ]; then
      echo "::error::miri-fan-gate: the $shape output under Miri (preemption rate $rate) differs from the native run"
      diff <(echo "$expected") <(echo "$got")
      exit 1
    fi
    echo "miri-fan-gate: $shape, seeds $MIRI_SEEDS, preemption rate $rate, four CPUs — no undefined behaviour, output as native"
  done
done
