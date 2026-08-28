#!/usr/bin/env bash
# #1628 stage 4 — the WASI/Component-Model pin gate.
#
# proofs/wasi-pin-policy.toml is the ledger; this gate re-derives every
# fact it states from the tree, so none of them can drift silently:
#   1. the vendored p3 WIT interfaces are at exactly [wasi].minor;
#   2. the p3 shim imports interfaces at that same version;
#   3. the embedded host's wasmtime Cargo pin is [runtime].crate_major;
#   4. CI installs [runtime].ci for the execution legs;
#   5. the p3 test harness passes [runtime].flags verbatim;
#   6. the wasm-tools family pins (wasmparser / wit-component) agree
#      with [wasm-tools].family AND with each other (lockstep doctrine).
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
err() { echo "::error::check-wasi-pins: $1"; fail=1; }

policy="proofs/wasi-pin-policy.toml"
get() { grep -E "^$2 *= *" "$1" | head -1 | sed -E 's/^[^=]*= *"?([^"]*)"?.*/\1/'; }

minor="$(get "$policy" minor)"
crate_major="$(get "$policy" crate_major)"
ci_ver="$(get "$policy" ci)"
flags="$(get "$policy" flags)"
family="$(get "$policy" family)"

# 1. vendored WIT versions.
while IFS= read -r v; do
  [ "$v" = "$minor" ] || err "vendored WIT at wasi:*@$v, policy says $minor"
done < <(grep -rhoE "^package wasi:[a-z-]+@[0-9.]+" crates/almide-wasm-run/wit/p3/deps/*/package.wit | sed -E 's/.*@//' | sort -u)

# 2. the p3 shim's import interface versions.
while IFS= read -r v; do
  [ "$v" = "$minor" ] || err "wasi_p3.rs imports wasi:*@$v, policy says $minor"
done < <(grep -ohE "wasi:[a-z/-]+@[0-9.]+" crates/almide-wasm-run/src/wasi_p3.rs | sed -E 's/.*@//' | sort -u)

# 3. the embedded host's wasmtime pin.
got="$(grep -E '^wasmtime *= *' crates/almide-wasm-run/Cargo.toml | sed -E 's/[^0-9]*([0-9]+).*/\1/')"
[ "$got" = "$crate_major" ] || err "wasmtime Cargo pin is $got, policy says $crate_major"

# 4. the CI-installed runtime.
grep -q "wasmtime-${ci_ver}-" .github/workflows/ci.yml \
  || err "ci.yml does not install wasmtime ${ci_ver} (policy [runtime].ci)"

# 5. the p3 harness flag surface.
python3 - "$flags" <<'PY'
import re, sys
flags = sys.argv[1]
src = open("tests/component_p3_test.rs").read()
# The harness passes the same flags as one -W value and -S p3=y args.
w = re.search(r'"-W",\s*\n?\s*"([^"]+)"', src)
ok = w and w.group(1) in flags and '"-S"' in src and '"p3=y"' in src
sys.exit(0 if ok else 1)
PY
[ $? -eq 0 ] || err "component_p3_test.rs flag surface drifted from policy [runtime].flags"

# 6. wasm-tools family lockstep.
for dep in wasmparser wit-component; do
  got="$(grep -E "^$dep *= *" Cargo.toml | head -1 | sed -E 's/[^0-9]*([0-9]+\.[0-9]+).*/\1/')"
  [ "$got" = "$family" ] || err "$dep pinned $got, policy family $family"
done

[ "$fail" -eq 0 ] && echo "wasi-pins OK: WASI $minor, wasmtime crate $crate_major / CI $ci_ver, wasm-tools $family — every stated pin re-derived from the tree."
exit "$fail"
