#!/usr/bin/env bash
# STDLIB PURITY DRIFT GATE — the non-recurring gate for brick #47 (sound stdlib
# Module-call lowering). almide-mir lowers a `<module>.<func>` stdlib call into
# the value-semantics subset ONLY when the callee is provably PURE (reaches no
# host capability), because the proven capability checker derives `used` caps
# only from `Op::Call`'s typed RtFn — an effectful stdlib call lowered as a bare
# `Op::CallFn` would silently omit its capability = accept-but-unsafe. The set of
# pure modules is `crates/almide-mir/src/purity.rs::PURE_MODULES`.
#
# This gate makes the DANGEROUS DRIFT structurally impossible to ship:
#
#   (1) PURITY: every module in PURE_MODULES declares ZERO `effect fn` in its
#       stdlib/<module>.almd source. If a currently-pure module ever GAINS an
#       `effect fn`, this fails — so a pure→effectful transition cannot silently
#       keep the module on the admit-list (the exact accept-but-unsafe hazard).
#   (2) EXISTENCE: every PURE_MODULES entry names a real stdlib/<module>.almd
#       (no typo / stale entry that would wall nothing or, worse, admit a name
#       the frontend resolves elsewhere).
#   (3) COMPLETENESS: every stdlib/*.almd module is CLASSIFIED — PURE (in the
#       registry), EFFECTFUL (declares `effect fn`), or IMPURE-PLAIN (listed here
#       with a justification: reaches the host WITHOUT the keyword, which
#       under-approximates). A new, unclassified module FAILS — forcing an
#       explicit pure-or-walled decision rather than a silent default-to-pure.
#
# Mirrors the spirit of scripts/check-rt-oracle-registry.sh and the #34 claim-
# drift gate: a soundness-relevant registry is mechanically re-derived from its
# single source (the `effect` keyword) and any divergence is a hard failure.
set -euo pipefail
cd "$(dirname "$0")"
ROOT="$(cd .. && pwd)"
# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1

PURITY_RS="$ROOT/crates/almide-mir/src/purity.rs"
STDLIB="$ROOT/stdlib"

fail=0
note() { echo "  $*"; }
err() { echo "FAIL: $*" >&2; fail=1; }

# IMPURE-PLAIN: modules with ZERO `effect fn` that nonetheless reach the host (the
# `effect` keyword under-approximates). Walled wholesale in purity.rs by simple
# absence from PURE_MODULES; enumerated HERE only so the completeness check can
# tell "intentionally walled, justified" from "unclassified new module".
declare -A IMPURE_PLAIN=(
  [args]="raw() delegates to env.args() — reads process arguments"
  [mem]="save/restore the allocator arena — mutates global allocator state"
  [testing]="assert_* print failures / abort the process — Stdout + process effect"
  [datetime]="now()/monotonic_ns() read the wall clock — a nondeterministic host source"
  [prim]="the v1 primitive floor — raw memory load/store + the fd_write host call (unsafe, reaches Stdout). v1 intercepts prim.* to Op::Prim BEFORE the purity gate, and Op::Prim{FdWrite} carries Capability::Stdout in cap_witness, so caps stay counted; this entry just keeps the registry complete (prim is never admitted as pure)."
  [print_str]="the self-hosted println implementation (v1 runtime) — writes to stdout via prim.fd_write, so it reaches Stdout. Not a user-facing module; auto-linked when a program prints."
  [io_print]="the self-hosted io.print implementation (v1 runtime) — writes a string's bytes to stdout via prim.fd_write (no trailing newline, unlike print_str), so it reaches Capability::Stdout (the SAME cap as println, NOT a new one). A PLAIN fn (not 'effect fn', like print_str) so its bare Unit return is not effect-monad/Result wrapped. v1 intercepts io.print to this linked impl whose Op::Prim{FdWrite} carries Stdout in cap_witness (counted transitively), so a caller is caps-verified only if it declares Stdout (an effect fn). Not a user-facing module; auto-linked when a program calls io.print."
  [random_int]="the self-hosted random.int implementation (v1 runtime) — reads host entropy via prim.random_get, so it reaches Capability::Entropy. v1 intercepts random.int to this linked impl whose Op::Prim{RandomGet} carries Entropy in cap_witness (counted transitively), so a caller is caps-verified only if it declares Entropy (an effect fn). Not a pure module; auto-linked when a program calls random.int."
  [env_unix_timestamp]="the self-hosted env.unix_timestamp implementation (v1 runtime) — reads the host wall clock via prim.clock_time_get, so it reaches Capability::Clock. A PLAIN fn (not 'effect fn') so its bare i64 scalar return is not effect-monad/Result wrapped (like random_int). v1 intercepts env.unix_timestamp to this linked impl whose Op::Prim{ClockTimeGet} carries Clock in cap_witness (counted transitively), so a caller is caps-verified only if it declares Clock (an effect fn). Not a pure module; auto-linked when a program calls env.unix_timestamp."
  [io_read_n_bytes]="the self-hosted io.read_n_bytes implementation (v1 runtime) — reads up to n bytes of standard input via prim.read_n_bytes, so it reaches Capability::Stdin (the SAME cap as io.read_line, NOT a new one). A PLAIN fn (not 'effect fn') matching io.almd's 'fn read_n_bytes(n: Int) -> List[Int]' — its bare List[Int] return is not effect-monad/Result wrapped (so callers use it without '!'), like io_print's bare Unit. v1 intercepts io.read_n_bytes to this linked impl whose Op::Prim{ReadNBytes} carries Stdin in cap_witness (counted transitively), so a caller is caps-verified only if it declares Stdin (an effect fn). Not a user-facing module; auto-linked when a program calls io.read_n_bytes."
)

# (A) Extract PURE_MODULES from the Rust const (the single source of the admit-set).
pure_modules=$(
  awk '/pub const PURE_MODULES/{f=1} f{print} f&&/\];/{exit}' "$PURITY_RS" \
    | grep -oE '"[a-z0-9_]+"' | tr -d '"' | sort -u
)
if [ -z "$pure_modules" ]; then
  err "could not extract PURE_MODULES from $PURITY_RS"
  echo; echo "STDLIB PURITY GATE: FAILED (registry unreadable)"; exit 1
fi
n_pure=$(echo "$pure_modules" | wc -l | tr -d ' ')
note "PURE_MODULES: $n_pure modules extracted from purity.rs"

has_effect_fn() { grep -qE '^[[:space:]]*effect[[:space:]]+fn[[:space:]]' "$1"; }

# (1)+(2) Every PURE module exists and declares zero `effect fn`.
for m in $pure_modules; do
  src="$STDLIB/$m.almd"
  if [ ! -f "$src" ]; then
    err "PURE_MODULES lists '$m' but $src does not exist (stale/typo)"
    continue
  fi
  if has_effect_fn "$src"; then
    n=$(grep -cE '^[[:space:]]*effect[[:space:]]+fn[[:space:]]' "$src")
    err "PURE module '$m' declares $n 'effect fn' — it reaches a host capability and must NOT be admitted (drop it from PURE_MODULES or wall the effectful fns)"
  fi
  # A module cannot be both pure AND on the impure-plain wall list.
  if [ -n "${IMPURE_PLAIN[$m]+x}" ]; then
    err "module '$m' is in BOTH PURE_MODULES and IMPURE_PLAIN — contradictory classification"
  fi
done

# (3) Completeness: every stdlib module is PURE, EFFECTFUL, or justified IMPURE-PLAIN.
n_effectful=0
n_unclassified=0
for src in "$STDLIB"/*.almd; do
  m=$(basename "$src" .almd)
  if echo "$pure_modules" | grep -qx "$m"; then
    continue # PURE (checked above)
  elif has_effect_fn "$src"; then
    n_effectful=$((n_effectful + 1)) # EFFECTFUL (walled by the keyword)
  elif [ -n "${IMPURE_PLAIN[$m]+x}" ]; then
    : # IMPURE-PLAIN (walled, justified)
  else
    err "stdlib module '$m' is UNCLASSIFIED — add it to PURE_MODULES (if every fn is a pure data transform) or to IMPURE_PLAIN with a justification (it reaches the host without the 'effect' keyword)"
    n_unclassified=$((n_unclassified + 1))
  fi
done
note "EFFECTFUL (keyword) modules walled: $n_effectful"
note "IMPURE-PLAIN modules walled (justified): ${#IMPURE_PLAIN[@]}"

# Every justified IMPURE-PLAIN entry must still exist (no stale rows).
for m in "${!IMPURE_PLAIN[@]}"; do
  [ -f "$STDLIB/$m.almd" ] || err "IMPURE_PLAIN lists '$m' but $STDLIB/$m.almd no longer exists (stale)"
done

echo
if [ "$fail" -eq 0 ]; then
  echo "STDLIB PURITY GATE OK: every PURE_MODULES entry is keyword-pure and real;"
  echo "every stdlib module is classified (pure / effectful / justified impure-plain)."
else
  echo "STDLIB PURITY GATE: FAILED — a pure module reaches the host or a module is"
  echo "unclassified. Admitting it would emit an under-counted capability witness"
  echo "(accept-but-unsafe). Fix purity.rs / IMPURE_PLAIN above."
  exit 1
fi
