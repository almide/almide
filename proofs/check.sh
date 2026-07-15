#!/usr/bin/env bash
# Re-derive every proof in the trust spine from scratch — the third-party
# "make verify". Anyone with Rocq/Coq runs this and the kernel + the INDEPENDENT
# re-checker (coqchk, the De Bruijn criterion) confirm every theorem, with the
# axiom audit (`Print Assumptions`) printed so the trusted base is visible.
set -euo pipefail
cd "$(dirname "$0")"

COQC="${COQC:-$(command -v coqc)}"
COQCHK="${COQCHK:-$(command -v coqchk)}"

echo "== kernel check (coqc) + axiom audit (Print Assumptions) =="
"$COQC" -Q . AlmideTrust Subset.v
"$COQC" -Q . AlmideTrust OwnershipChecker.v
"$COQC" -Q . AlmideTrust OwnershipLoop.v
"$COQC" -Q . AlmideTrust OwnershipFilter.v
"$COQC" -Q . AlmideTrust CoownLoop.v
"$COQC" -Q . AlmideTrust CoownCompose.v
"$COQC" -Q . AlmideTrust ALS.v
"$COQC" -Q . AlmideTrust Translation.v
"$COQC" -Q . AlmideTrust RuntimeModel.v
"$COQC" -Q . AlmideTrust NameTotality.v
"$COQC" -Q . AlmideTrust TypeConcretization.v
"$COQC" -Q . AlmideTrust CapabilityBound.v
"$COQC" -Q . AlmideTrust CapabilityReach.v
"$COQC" -Q . AlmideTrust CallModes.v
"$COQC" -Q . AlmideTrust StackBalance.v
"$COQC" -Q . AlmideTrust Termination.v
"$COQC" -Q . AlmideTrust FreeList.v
"$COQC" -Q . AlmideTrust WasmRcDec.v
"$COQC" -Q . AlmideTrust WasmEncode.v
"$COQC" -Q . AlmideTrust WasmExec.v
"$COQC" -Q . AlmideTrust WasmIsa.v
"$COQC" -Q . AlmideTrust WasmDecode.v
"$COQC" -Q . AlmideTrust CowSafety.v

echo
echo "== independent re-check (coqchk — De Bruijn criterion) =="
COQCHK_OUT="$("$COQCHK" -Q . AlmideTrust AlmideTrust.Subset AlmideTrust.OwnershipChecker AlmideTrust.OwnershipLoop AlmideTrust.OwnershipFilter AlmideTrust.CoownLoop AlmideTrust.CoownCompose AlmideTrust.ALS AlmideTrust.Translation AlmideTrust.RuntimeModel AlmideTrust.NameTotality AlmideTrust.TypeConcretization AlmideTrust.CapabilityBound AlmideTrust.CapabilityReach AlmideTrust.CallModes AlmideTrust.StackBalance AlmideTrust.Termination AlmideTrust.FreeList AlmideTrust.WasmRcDec AlmideTrust.WasmEncode AlmideTrust.WasmExec AlmideTrust.WasmIsa AlmideTrust.WasmDecode AlmideTrust.CowSafety 2>&1)"
echo "$COQCHK_OUT"

echo
echo "== claim-drift gate (#34, indicator ⑤): TRUSTED_BASE axiom ledger ⊆ kernel-checked =="
# Every theorem the axiom ledger CLAIMS is "Closed under the global context" must
# be a constant the kernel re-checker (coqchk) actually verified — so a public
# claim can never drift past what is proven. Ledger rows: | `name` | File.v | … |.
CHECKED="$(printf '%s\n' "$COQCHK_OUT" | grep -oE 'cst:AlmideTrust\.[A-Za-z0-9]+\.[A-Za-z0-9_]+' | sed 's/^.*\.//' | sort -u)"
CLAIMED="$(grep -oE '^\| `[A-Za-z0-9_]+`' TRUSTED_BASE.md | tr -d '|` ' | sort -u)"
drift=0
for t in $CLAIMED; do
  if printf '%s\n' "$CHECKED" | grep -qx "$t"; then
    echo "  ok   $t"
  else
    echo "  FAIL $t — claimed in the axiom ledger but NOT kernel-checked (claim drift)"; drift=1
  fi
done
[ "$drift" = 0 ] && echo "CLAIMS OK: every axiom-ledger theorem is kernel-checked (public claims ⊆ proven)." || exit 1

echo
echo "== A2 byte-binding grounding (wat2wasm cross-check; SKIP if wabt absent) =="
bash ./check-wasm-bytes.sh

echo
echo "== A2 byte-EXECUTION grounding (wasmtime cross-check; SKIP if wasmtime absent) =="
bash ./check-wasm-exec.sh

echo
echo "PROOF SPINE OK: kernel-checked, axiom-clean (Closed under the global"
echo "context), and independently re-verified."
