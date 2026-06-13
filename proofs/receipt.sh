#!/usr/bin/env bash
# Emit the RECEIPT (受領書) for the trust chain: run the verification and fold
# the checked facts into named claims, each with its evidence, STATUS, and
# honest scope. This is the tier-1 deliverable the done-definition names — a
# third party reads it, then re-derives every claim with `make verify-trust`.
# Honesty is the point: claims are marked proven / scoped / pending, never
# overclaimed (the hard rail).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

pass() { "$@" >/dev/null 2>&1 && echo PASS || echo FAIL; }

PROOF=$(pass "$ROOT/proofs/check.sh")          # kernel + coqchk + axiom audit
GATE=$(pass "$ROOT/proofs/gate.sh")            # compiler cert ⊳ proven checker
VTEST=$(pass bash -c "cd '$ROOT' && cargo test -q -p almide-mir translation_validation")

cat <<EOF
# Receipt — Almide v1 trust chain

Reproduce every line: \`make verify-trust\` (proof + gate + tests).
Trusted base & known-limitations: proofs/TRUSTED_BASE.md.

| claim | meaning | status | evidence | scope (honest) |
|---|---|---|---|---|
| C-PROVEN | the checkers' soundness rests only on the Coq kernel | ${PROOF} | proofs/check.sh: 6 theorems (RC balance + name totality + type concretization), \`Print Assumptions\` = Closed under the global context, coqchk re-checked | full (for the proven theorems) |
| C-SAFE   | no double-free / use-after-free; no dangling reference | ${GATE} / ${VTEST} | (1) the ownership certificate is re-verified by the kernel-proven \`check_cert\` (gate.sh) + the EMITTED wasm is per-build validated Dec-free → safe by \`eager_copy_refines_safety\` (V); (2) name totality \`check_names_sound\` — every used MIR value is defined | **ownership fragment, eager-copy**; leak-freedom NOT yet (eager-copy leaks) |
| C-FAITHFUL | the emitted artifact refines the ALS model | partial | \`eager_copy_refines_safety\` (ALS.v) + V on real bytes | safety core only; full V (emitted bytes ⊒ ALS value semantics) is the brick-4 body |
| C-REPRO  | byte-reproducible across hosts | inherited | the v0 wasm_cross byte gate + check-host-determinism.sh (dual oracle) | the differential oracle until v1 parity |

Irreducible base (cannot be proven, named in TRUSTED_BASE.md): Coq kernel,
OCaml extraction (CertiCoq/CompCert will close it), hardware, ALS validity.
Completeness is relative to the declared use; absolute-semantics coverage is
NOT claimed.
EOF
