#!/usr/bin/env bash
# PASS-ISOLATED FIXTURE GATE (#1487).
#
# rustc's MIR goldens pin each test to ONE pass (`//@ test-mir-pass: GVN`) so a
# golden for pass X is stable under any change to pass Y. This is that
# property for Almide, in the house style: the fixtures assert RUNTIME OUTPUT,
# not emitted code, and the invariant is per-pass SEMANTIC PRESERVATION —
# every `spec/pass_isolated/*.almd` must print byte-identically under
#
#   full pipeline / ALMIDE_DISABLE_OPT=1 (no perf passes) /
#   ALMIDE_ONLY_PASS=<its declared pass> (exactly one perf pass)
#
# and the full-pipeline wasm leg must match the native output. A `// @pass:`
# header names the axis point: `fold`, `dce`, or `propagate` adds the
# only-that-pass leg; `none` marks a fixture that isolates an EMITTER path
# (the five conformance-mutation paths each have one, named for their patch)
# and runs the ablated/full identity alone.
#
# ADDITIVE by design (DoD item 4): the end-to-end corpus is untouched — this
# gate catches a pass changing semantics in isolation, end-to-end catches the
# interactions isolation structurally cannot.
#
# Division of labor with proofs/conformance-mutations: mutants whose damage is
# OUTPUT-visible die here (verified at introduction: m1/m4/m5 each turn this
# gate red); mutants whose damage is emitted-code-visible or degrades to the
# silent classic-codegen fallback (m2, m3) die in the mutation gate, which
# watches the emitted artifacts. An output-identity runner structurally cannot
# see a graceful fallback — that is the mutation gate's job, kept there.
set -uo pipefail
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"

ALMIDE="${ALMIDE:-almide}"
fail=0
count=0

for f in spec/pass_isolated/*.almd; do
  count=$((count + 1))
  pass=$(grep -m1 '^// @pass:' "$f" | sed 's|^// @pass:[[:space:]]*||')
  case "$pass" in
    fold|dce|propagate|none) ;;
    *) echo "::error::pass-isolated: $f declares no valid // @pass: header (fold|dce|propagate|none)"; fail=1; continue ;;
  esac

  full=$("$ALMIDE" run "$f" 2>/dev/null); full_rc=$?
  if [ $full_rc -ne 0 ]; then
    echo "::error::pass-isolated: $f fails under the FULL pipeline (rc=$full_rc):"
    echo "$full" | head -5
    fail=1
    continue
  fi

  abl=$(ALMIDE_DISABLE_OPT=1 "$ALMIDE" run "$f" 2>/dev/null); abl_rc=$?
  if [ $abl_rc -ne $full_rc ] || [ "$abl" != "$full" ]; then
    echo "::error::pass-isolated: $f diverges under ABLATION (a perf pass is load-bearing for semantics):"
    diff <(echo "$full") <(echo "$abl") | head -6
    fail=1
  fi

  if [ "$pass" != "none" ]; then
    iso=$(ALMIDE_ONLY_PASS="$pass" "$ALMIDE" run "$f" 2>/dev/null); iso_rc=$?
    if [ $iso_rc -ne $full_rc ] || [ "$iso" != "$full" ]; then
      echo "::error::pass-isolated: $f diverges under ONLY-$pass (the pass changes semantics in isolation):"
      diff <(echo "$full") <(echo "$iso") | head -6
      fail=1
    fi
  fi

  wasm=$("$ALMIDE" run --target wasm "$f" 2>/dev/null); wasm_rc=$?
  if [ $wasm_rc -ne 0 ] || [ "$wasm" != "$full" ]; then
    echo "::error::pass-isolated: $f wasm leg diverges from native (rc=$wasm_rc):"
    diff <(echo "$full") <(echo "$wasm") | head -6
    fail=1
  fi
done

if [ $fail -ne 0 ]; then
  echo "::error::pass-isolated gate FAILED"
  exit 1
fi
echo "pass-isolated: $count fixture(s), each byte-identical across full/ablated/only-pass and native==wasm"
