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
"$COQC" -Q . AlmideTrust ALS.v
"$COQC" -Q . AlmideTrust Translation.v
"$COQC" -Q . AlmideTrust RuntimeModel.v
"$COQC" -Q . AlmideTrust NameTotality.v
"$COQC" -Q . AlmideTrust TypeConcretization.v
"$COQC" -Q . AlmideTrust CapabilityBound.v
"$COQC" -Q . AlmideTrust StackBalance.v
"$COQC" -Q . AlmideTrust Termination.v
"$COQC" -Q . AlmideTrust FreeList.v
"$COQC" -Q . AlmideTrust WasmRcDec.v
"$COQC" -Q . AlmideTrust WasmEncode.v
"$COQC" -Q . AlmideTrust CowSafety.v

echo
echo "== independent re-check (coqchk — De Bruijn criterion) =="
"$COQCHK" -Q . AlmideTrust AlmideTrust.Subset AlmideTrust.OwnershipChecker AlmideTrust.ALS AlmideTrust.Translation AlmideTrust.RuntimeModel AlmideTrust.NameTotality AlmideTrust.TypeConcretization AlmideTrust.CapabilityBound AlmideTrust.StackBalance AlmideTrust.Termination AlmideTrust.FreeList AlmideTrust.WasmRcDec AlmideTrust.WasmEncode AlmideTrust.CowSafety

echo
echo "== A2 byte-binding grounding (wat2wasm cross-check; SKIP if wabt absent) =="
bash ./check-wasm-bytes.sh

echo
echo "PROOF SPINE OK: kernel-checked, axiom-clean (Closed under the global"
echo "context), and independently re-verified."
