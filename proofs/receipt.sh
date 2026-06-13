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
| C-PROVEN | the checkers' soundness rests only on the Coq kernel | ${PROOF} | proofs/check.sh: RC balance + the shared membership-subset law (name totality + capability bound) + type concretization, \`Print Assumptions\` = Closed under the global context, coqchk re-checked | full (for the proven theorems) |
| C-SAFE   | no double-free / use-after-free; no dangling reference; no undeclared host effect | ${GATE} / ${VTEST} | THREE properties re-verified PER BUILD by the kernel-proven checker (almide-mir emits a witness → extracted Coq checker accepts/rejects, gate.sh): (1) ownership — \`check_cert\` / \`check_all_sound\`; (2) name totality — \`check_names_cert\` / \`check_names_cert_sound\`; (3) capability bound — \`check_caps_cert\` / \`check_caps_cert_sound\`. PLUS one REAL .almd (return_list.almd) taken through the actual frontend → MIR → proven checker for ownership+names (indicator ① 0→1). The eager-copy wasm is separately validated Dec-free → safe by \`eager_copy_refines_safety\` | **mostly WITNESS scope**: the reject cases + caps are REPRESENTATIVE MIR (emit_cert.rs); ONE real program now flows end-to-end (ownership+names, value-semantics move-out subset — no calls/control-flow yet, #29). The witness⟷wasm-bytes link is the §3 renderer contract (trusted), not the proven checker. **ownership fragment, eager-copy**; leak-freedom NOT yet (eager-copy leaks); caps-from-source needs a manifest; transitive caps via CallFn = later brick |
| C-FAITHFUL | the emitted artifact refines the ALS model | partial | \`eager_copy_refines_safety\` (ALS.v) + V on real bytes | safety core only; full V (emitted bytes ⊒ ALS value semantics) is the brick-4 body |
| C-REPRO  | byte-reproducible across hosts | inherited | the v0 wasm_cross byte gate + check-host-determinism.sh (dual oracle) | the differential oracle until v1 parity |

Irreducible base (cannot be proven, named in TRUSTED_BASE.md): Coq kernel,
OCaml extraction (CertiCoq/CompCert will close it), hardware, ALS validity.
Completeness is relative to the declared use; absolute-semantics coverage is
NOT claimed.
EOF
