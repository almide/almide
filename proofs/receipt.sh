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
| C-PROVEN | the checkers' soundness rests only on the Coq kernel | ${PROOF} | proofs/check.sh: the flight-grade property set on the value-semantics subset — RC balance + membership-subset law (name totality + capability bound) + type concretization + memory-model leak-freedom (RuntimeModel) + reuse soundness (\`check_reuse_sound\`: a Reuse acts only on a uniquely-owned object) + free-list reuse-safety (\`FreeList.alloc_not_live\`: a valid allocation never returns a currently-live block — no reuse-after-free) + copy-on-write alias-safety (\`CowSafety.make_unique_yields_unique\`: MakeUnique yields a uniquely-owned block — no aliased in-place mutation) + byte-binding table (Translation) + the emitted \`\$rc_dec\`/\`\$rc_inc\` instruction trees realizing rt_dec/rt_inc (\`WasmRcDec\`) + the rc_inc instruction tree encoding to the REAL wasm bytes (\`WasmEncode\`, grounded against wat2wasm by check-wasm-bytes.sh) + those bytes EXECUTING to rt_inc on a wasm stack machine + the FULL \`\$rc_dec\` bytes' SAFETY — no double-free AND leak-freedom — executed on the renderer's real bytes by a general interpreter with locals/globals/structured-if (\`WasmExec\`, grounded vs wat2wasm) + operand-stack balance (StackBalance) + termination of the loop-free fragment (Termination); 37 audited theorems, \`Print Assumptions\` = Closed under the global context, coqchk re-checked | full (for the proven theorems; subset-scoped) |
| C-SAFE   | no double-free / use-after-free; no dangling reference; no undeclared host effect | ${GATE} / ${VTEST} | THREE properties re-verified PER BUILD by the kernel-proven checker (almide-mir emits a witness → extracted Coq checker accepts/rejects, gate.sh): (1) ownership — \`check_cert\` / \`check_all_sound\`; (2) name totality — \`check_names_cert\` / \`check_names_cert_sound\`; (3) capability bound — \`check_caps_cert\` / \`check_caps_cert_sound\`. PLUS one REAL .almd (return_list.almd) taken through the actual frontend → MIR → proven checker for ownership+names (indicator ① 0→1). The wasm artifact now emits a release per drop (RC regime, A1.1b): \`validate_translation_perceus\` checks it realizes the certified release trace → safe by \`balanced_cert_no_memory_fault\` + cell freed by \`balanced_cert_frees_in_memory\`, with the \`\$rc_dec\` runtime sentinel trapping a double-free (verified firing on wasmtime) | **mostly WITNESS scope**: the reject cases + caps are REPRESENTATIVE MIR (emit_cert.rs); ONE real program now flows end-to-end (ownership+names, value-semantics move-out subset — no calls/control-flow yet, #29). The witness⟷wasm-bytes link is the §3 renderer contract (trusted), not the proven checker. **ownership fragment**; cell-level leak-freedom + double-free trap now REALIZED on the artifact; physical reclamation (free-list, A1.2) + sharing (rc_inc, A1.3) NOT yet; caps-from-source needs a manifest; transitive caps via CallFn = later brick |
| C-FAITHFUL | the emitted artifact refines the ALS model | partial | the op→wasm-instruction TABLE is a formal Coq object (Translation.v) + \`validate_translation\` re-checks per build that every op's pattern is present (a drop's is \`call \$rc_dec\`) AND \`validate_translation_perceus\` that one release is emitted per drop (\`balanced_cert_frees_in_memory\`) | SYNTACTIC table-match (presence + release-count) on the RC fragment, the SEMANTIC realization of the release PROVEN at the instruction-tree level (\`WasmRcDec\`), AND for rc_inc the byte ENCODING (\`WasmEncode\`, grounded vs wat2wasm) AND the EXECUTION (\`WasmExec\`: the real bytes run on a wasm stack machine to exactly rt_inc) — so rc_inc is bound END TO END to the real bytes; remaining = the same chain for rc_dec/full-module + that the small interpreter matches the FULL wasm spec (WasmCert-Coq) — the residual heavy track |
| C-REPRO  | byte-reproducible across hosts | inherited | the v0 wasm_cross byte gate + check-host-determinism.sh (dual oracle) | the differential oracle until v1 parity |

Irreducible base (cannot be proven, named in TRUSTED_BASE.md): Coq kernel,
OCaml extraction (CertiCoq/CompCert will close it), hardware, ALS validity.
Completeness is relative to the declared use; absolute-semantics coverage is
NOT claimed.
EOF
