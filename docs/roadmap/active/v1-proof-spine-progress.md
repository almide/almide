<!-- description: v1 proof-spine progress — what of task #31 (全V / leak-freedom / 推移的caps / 抽出穴) is PROVEN vs the honest remaining. Records the CapabilityReach.v transitive-caps theorem (2026-06-21). -->
# v1 Proof Spine — #31 progress (全V / leak-freedom / 推移的caps / 抽出穴)

The spine is mature: `proofs/check.sh` builds all theorems with `coqc`, **independently
re-verifies** with `coqchk` (De Bruijn criterion), audits axioms (`Print Assumptions` =
*Closed under the global context* on every theorem), and a claim-drift gate enforces
public claims ⊆ proven. Status of task #31's four named gaps, audited 2026-06-21:

## ✅ leak-freedom — ALREADY PROVEN
`OwnershipChecker.check_sound : check ops = true -> no_double_free ops /\ no_leak ops`,
where `no_leak ops := run ops = Some 0` (every acquired ref released). The extracted
end-to-end `check_cert_sound` carries it, and `OwnershipLoop.v` extends it to loop-carried
slots (leak/double-free-free for any iteration count). So leak-freedom is in the kernel base
at cert level. (The self-hosted recursive-drop ROUTINES — `__drop_value`, `__vdrop_obj`,
the rendered `DropListStr`/`DropListListStr` loops — emit a single balanced `d` the cert
sees; their INTERNAL per-element correctness is the trusted floor, leak-loop-verified.)

## ✅ 抽出穴 (extraction hole) — CLOSED
`checker.ml` is **extracted from the proven `check_cert`** (Extract.v), and `gate.sh` builds
the checker from that extraction. The tokenizer is now INSIDE the proof (`check_cert` parses
bytes AND checks), so the whole "cert bytes ⟶ accept/reject" pipeline is the extracted proven
function — no hand-written checker divergence.

## ✅ 推移的caps — THEOREM NOW PROVEN (CapabilityReach.v, commit a28ccf8a)
`CapabilityBound.v` proved the PER-FUNCTION check (direct used caps ⊆ declared). The
TRANSITIVE reach (a function reaches its callees' caps — "no network even via a helper") was
computed only in the UNTRUSTED Rust classifier (corpus-wall's `reaches_capability_or_unknown`
fold). **`CapabilityReach.reaches_sound`** now proves the composition that justifies it:
- model: a program = function nodes `{fallowed; fdirect; fcallees}` (callees = indices);
- `reaches` = direct caps ∪ ⋃ callees' transitive caps (fuel-bounded);
- `fn_ok` (per-function + per-edge): `direct ⊆ allowed` AND each callee `allowed(g) ⊆ allowed(f)`;
- **theorem**: `prog_ok prog -> ∀ fuel i, reaches prog fuel i ⊆ allowed(i)`.
Built on `Subset.v`'s shared law; axiom-clean; coqc + coqchk in `check.sh`; non-vacuous demos
(helper-calling main accepted + reach bounded; helper reaching undeclared network rejected).

REMAINING for transitive caps end-to-end (a SMALLER wiring follow-up, not a new theorem): the
classifier must EMIT the call-graph witness (`fallowed|fdirect|callees` per function) and an
extracted `check_prog` must re-verify it per build, so the gate CONSUMES the proof instead of
trusting the Rust fold. Today the gate wires the per-function `caps.cert` (CapabilityBound);
the composition is proven but not yet emitted as a witness.

## 全V — cert-level COMPLETE; the heavy remainder is #40
The flight-grade properties are all proven at cert level: ownership (double-free + leak),
names (no dangling), caps (per-function + now transitive composition), stack balance,
termination, COW safety, loop ownership, and the rc_inc/rc_dec **byte-binding** (grounded in
wat2wasm, non-circular). The remaining heavy track is **#40**: the full memory machine bound
to wasm BYTES via a WasmCert-Coq ISA (a complete verified wasm execution semantics) — proofs
flag it as "the deferred heavy track". That is the next big proof-spine brick.
