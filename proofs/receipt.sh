#!/usr/bin/env bash
# Emit the RECEIPT (受領書) for the trust chain: run the verification and fold
# the checked facts into named claims, each with its evidence, STATUS, and
# honest scope. This is the tier-1 deliverable the done-definition names — a
# third party reads it, then re-derives every claim with `make verify-trust`.
# Honesty is the point: claims are marked proven / scoped / pending, never
# overclaimed (the hard rail).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# F6-2: identity of the evidence — stamp + verify the toolchain (see proofs/lib/stamp.sh).
source "$ROOT/proofs/lib/stamp.sh"
stamp_toolchain "$ROOT" || exit 1


pass() { "$@" >/dev/null 2>&1 && echo PASS || echo FAIL; }

# SAME-TREE RE-USE. `make verify-trust` runs check.sh + gate.sh + corpus-wall.sh
# + `cargo test -p almide-mir` — a strict superset of the four verdicts below —
# and records the fingerprint of the tree+toolchain it verified. When that
# fingerprint still matches (the CI job's verify-trust step immediately precedes
# its receipt step), re-running them would re-derive the identical verdicts on
# the identical inputs: 232s of the CI job, ~36% of it, spent proving something
# just proven. Fold them in instead.
#
# The honesty rail is the FINGERPRINT, not a timestamp or a flag: it covers the
# compiler binary, every toolchain the gates invoke, the commit, the content of
# tracked modifications, and every untracked file. Edit anything, switch a
# toolchain, or run this standalone on a tree nobody verified, and it does not
# match — so the third-party path (`git clone && make receipt`) verifies in full
# exactly as before. A receipt can therefore never claim PASS for a tree that
# was not actually verified.
VERIFIED_STAMP="$ROOT/proofs/.verified-fingerprint"
REUSED=no
if [ -f "$VERIFIED_STAMP" ] \
    && [ "$(cat "$VERIFIED_STAMP")" = "$(toolchain_fingerprint "$ROOT")" ]; then
    REUSED=yes
    PROOF=PASS; GATE=PASS; CWALL=PASS; VTEST=PASS
else
    PROOF=$(pass "$ROOT/proofs/check.sh")          # kernel + coqchk + axiom audit
    GATE=$(pass "$ROOT/proofs/gate.sh")            # compiler cert ⊳ proven checker
    CWALL=$(pass "$ROOT/proofs/corpus-wall.sh")    # whole v0 corpus ⊳ wall + PCC
    VTEST=$(pass bash -c "cd '$ROOT' && cargo test -q -p almide-mir translation_validation")
fi

cat <<EOF
# Receipt — Almide v1 trust chain

Reproduce every line: \`make verify-trust\` (proof + gate + tests).
Trusted base & known-limitations: proofs/TRUSTED_BASE.md.

Verdicts below were $( [ "$REUSED" = yes ] \
  && echo "folded in from a \`make verify-trust\` of THIS EXACT tree and toolchain (fingerprint match; see proofs/lib/stamp.sh)" \
  || echo "derived by running check.sh, gate.sh, corpus-wall.sh and the verifier tests just now" )\
.

| claim | meaning | status | evidence | scope (honest) |
|---|---|---|---|---|
| C-PROVEN | the checkers' soundness rests only on the Coq kernel | ${PROOF} | proofs/check.sh: the flight-grade property set on the value-semantics subset — RC balance + membership-subset law (name totality + capability bound) + type concretization + memory-model leak-freedom (RuntimeModel) + reuse soundness (\`check_reuse_sound\`: a Reuse acts only on a uniquely-owned object) + free-list reuse-safety (\`FreeList.alloc_not_live\`: a valid allocation never returns a currently-live block — no reuse-after-free) + copy-on-write alias-safety (\`CowSafety.make_unique_yields_unique\`: MakeUnique yields a uniquely-owned block — no aliased in-place mutation) + byte-binding table (Translation) + the emitted \`\$rc_dec\`/\`\$rc_inc\` instruction trees realizing rt_dec/rt_inc (\`WasmRcDec\`) + the rc_inc instruction tree encoding to the REAL wasm bytes (\`WasmEncode\`, grounded against wat2wasm by check-wasm-bytes.sh) + those bytes EXECUTING to rt_inc on a wasm stack machine + the FULL \`\$rc_dec\` bytes' SAFETY — no double-free AND leak-freedom — executed on the renderer's real bytes by a general interpreter with locals/globals/structured-if (\`WasmExec\`, grounded vs wat2wasm) + operand-stack balance (StackBalance) + termination of the loop-free fragment (Termination) + free-list REGION-RESET safety (\`FreeList.region_window_reuse_safe\`: a RegionSave/RegionRestore window preserves the allocator invariant across an arbitrary body, so the frontier reset leaves no free-list entry pointing into the reclaimed region) + PINNED_RC immortality (\`FreeList.pinned_stays_immortal_forever\`: an \`\$alloc8\`/\`__alloc_pinned\` block is never freed, never on the free-list and never returned by \`\$alloc\`, over an arbitrary run) + the COUNT side of reuse (\`FreeListRc.reuse_hands_back_a_zero_count_block\`: a block handed back off the free-list carries no stale reference count, and \`reuse_restores_rc_1\`: the \`\$alloc\`+constructor PAIR leaves it at exactly RC_INITIAL = 1, so \`double_release_traps\` — a re-release of a freed block hits the rc-0 sentinel); 96 audited theorems, \`Print Assumptions\` = Closed under the global context, coqchk re-checked | full (for the proven theorems; subset-scoped) |
| C-SAFE   | no double-free / use-after-free; no dangling reference; no undeclared host effect | ${GATE} / ${VTEST} | THREE properties re-verified PER BUILD by the kernel-proven checker (almide-mir emits a witness → extracted Coq checker accepts/rejects, gate.sh): (1) ownership — \`check_cert\` / \`check_all_sound\`; (2) name totality — \`check_names_cert\` / \`check_names_cert_sound\`; (3) capability bound — \`check_caps_cert\` / \`check_caps_cert_sound\`. PLUS one REAL .almd (return_list.almd) taken through the actual frontend → MIR → proven checker for ownership+names (indicator ① 0→1). The wasm artifact now emits a release per drop (RC regime, A1.1b): \`validate_translation_perceus\` checks it realizes the certified release trace → safe by \`balanced_cert_no_memory_fault\` + cell freed by \`balanced_cert_frees_in_memory\`, with the \`\$rc_dec\` runtime sentinel trapping a double-free (verified firing on wasmtime) | **mostly WITNESS scope**: the reject cases + caps are REPRESENTATIVE MIR (emit_cert.rs); ONE real program now flows end-to-end (ownership+names, value-semantics move-out subset — no calls/control-flow yet, #29). The witness⟷wasm-bytes link is the §3 renderer contract (trusted), not the proven checker. **ownership fragment**; cell-level leak-freedom + double-free trap now REALIZED on the artifact; physical reclamation (free-list, A1.2) + sharing (rc_inc, A1.3) NOT yet; caps-from-source needs a manifest; transitive caps via CallFn now CONSERVATIVELY checked in the corpus gate (\`reaches_capability_or_unknown\`: an unanalyzable callee taints, never over-accepts) — Stdout-only vocabulary |
| C-FAITHFUL | the emitted artifact refines the ALS model | partial | the op→wasm-instruction TABLE is a formal Coq object (Translation.v) + \`validate_translation\` re-checks per build that every op's pattern is present (a drop's is \`call \$rc_dec\`) AND \`validate_translation_perceus\` that one release is emitted per drop (\`balanced_cert_frees_in_memory\`) | SYNTACTIC table-match (presence + release-count) on the RC fragment, the SEMANTIC realization of the release PROVEN at the instruction-tree level (\`WasmRcDec\`), AND for rc_inc the byte ENCODING (\`WasmEncode\`, grounded vs wat2wasm) AND the EXECUTION (\`WasmExec\`: the real bytes run on a wasm stack machine to exactly rt_inc) — so rc_inc is bound END TO END to the real bytes; remaining = the same chain for rc_dec/full-module + that the small interpreter matches the FULL wasm spec (WasmCert-Coq) — the residual heavy track |
| C-WALL   | the lowering boundary is a WALL over the real v0 corpus, not a hole | ${CWALL} | proofs/corpus-wall.sh: the WHOLE v0 spec corpus (465 files, 4195 functions) driven through the real frontend → \`lower_function\` — TOTAL (every function \`Ok\` or explicit \`Unsupported\`, **zero panics / zero undetected refusals** (totality + certificate acceptance — NOT an output-correctness claim; that is output-parity's, on its baseline set)), and the kernel-proven checker ACCEPTs EVERY in-profile function's witness on ALL THREE proven properties: ownership (no double-free/leak), name totality (no dangling MIR ref), capability bound (no undeclared STDOUT effect, checked TRANSITIVELY across CallFn edges) — accept ⟹ all three hold on real programs. The step-4 "continuous corpus verification" mechanism, CI-gated | **measurement + wall**, not the completion definition: today 4083/4195 functions in-profile (97% for ownership+names; caps-VERIFIED 3528 is the lower parity-binding number; value-semantics subset + higher-order pure combinators [list.map/filter/fold/... with a closure, VALUE or EFFECT/statement position — pure combinator invokes-and-discards the closure (no escape); closure arg handled by CAPABILITY (Lambda body calls / ClosureCreate/FnRef callee → effect markers, mir<=ir gate taints a nested-higher-order body) with value DEFERRED + captures BORROWED; OPAQUE function value WALLED; fresh owned result] + for/while loops [PER-ITERATION scope frame = one modeled iteration internally balanced ⟹ N iterations leak-free for any N, NO loop op = checker stays a flat fold; heap iterable borrowed/materialized, loop var aliases container per iteration (Dup) or scalar Const; break/continue over a HEAP frame WALLED (the v0 wasm backend frees AFTER the break branch target → a real early exit would leak a per-iteration heap handle), over a SCALAR-only frame ADMITTED as a no-op (no heap Drop to skip = no leak, either target); heap reassignment (the accumulator) DEFERRED — the var keeps its still-live handle across iterations (memory-safe), scalar reassign admitted] + if/match control flow [statement/scalar-/Unit-/HEAP-tail and heap-bind position, arms LINEARIZED into the flat op stream with a per-arm scope frame, NO branch op = checker stays a flat fold; each arm internally balanced + vacuous on the other path; result is one merged slot = discarded/Const/fresh-Alloc-Opaque-for-heap (memory-safe by construction, value content deferred); fresh-heap-subject/payload-binding-pattern/guard handled; a HEAP arm-reassign is DEFERRED (the var keeps its pre-branch handle, no path-dependent UAF)] + println of any heap arg [materialized, caps-unverified as it reaches Stdout] + in-place place mutation [xs[i]=v/r.field=v via MakeUnique] + tuple destructuring [let (a,b)=(x,y) component-wise or (a,b)=t aliasing the container] + reassignment [x=v rebinds, old rides to scope-end drop] + field/element extraction [xs[i]/r.field — scalar = Const copy; heap = ALIAS the container via Op::Dup, the container-grain field access, reusing the proven a/alias event so the checker + backing gate are unchanged; field-precise + nested-container deferred], incl. references to top-level \`let\` GLOBALS [a value_of miss confirmed against the DECLARED global set is a fresh external value: scalar=Const, heap=owned Alloc{Opaque} dropped at scope end; a non-declared miss still WALLS = a real lowering gap] + heap-tail BLOCKS [\`var x = { stmts; tail }\` lowers the stmts then binds x to the heap tail] + MAP insertion [\`m[k] = v\` → MakeUnique copy-on-write] + NESTED tuple destructuring [\`let (a,(b,c)) = …\` recurses component-wise] + expression-bodied fns + direct heap-literal/named-call-result returns + borrowed-heap-param functions [v1 BORROW-BY-DEFAULT, a synthetic owned-param +1 is gated out] + first-order PURE stdlib Module calls [pure-only admission: an effectful CallFn would omit its capability from \`used\` = accept-but-unsafe, so a purity registry + drift gate wall effectful/impure/higher-order calls] + nested CALL arguments [f(g(x)) materialized into an owned temp, borrowed + dropped] + literal CALL arguments [f("x")/f([1,2,3])/f(3.14) — heap literal via Alloc, scalar literal via Const] + Option/Result constructors [Some/None/Ok/Err — heap variants materialized like a container literal] + BinOp/UnOp [a+b/s1++s2/-n — fresh computed value, heap concat via Alloc / scalar via Const]); the rest are walled with a per-feature reason histogram (the coverage roadmap). CAPS scope is honest: only Stdout is modeled, and the gate verifies the REAL capability-bound property \`reachable ⊆ declared\` (what CapabilityBound.v proves), not "reaches no capability": \`lower_function\` lowers each function's effect signature into a \`declared_caps\` bound (an \`effect fn\` declares {Stdout}, a pure \`fn\` declares ∅), the classifier folds the transitive reachable cap set (\`reachable_caps_or_tainted\`, None on any taint), and the proven checker re-verifies \`reachable ⊆ declared\`. A function is caps-VERIFIED (3528/4083; 555 unverified — incl. functions whose Opaque-elided call is un-materializable (higher-order/effectful-Module/Method/Computed) and so stays tainted, AND functions reaching an UNDECLARED cap such as a non-\`effect fn\` that prints; elided first-order Named + pure Module calls are SURFACED as cert-neutral effect markers by \`record_elided_calls\` and folded, gated by \`mir_calls <= ir_calls\`) only if every reachable cap is within its declared bound — a call to an unanalyzable (walled/cross-file) callee is NOT claimed safe; stderr/abort/fs/net are real host effects not yet named. Soundness invariants are hard; coverage is reported, never overclaimed |
| C-REPRO  | byte-reproducible across hosts | inherited | the v0 wasm_cross byte gate + check-host-determinism.sh (dual oracle) | the differential oracle until v1 parity |

Irreducible base (cannot be proven, named in TRUSTED_BASE.md): Coq kernel,
OCaml extraction (CertiCoq/CompCert will close it), hardware, ALS validity.
Completeness is relative to the declared use; absolute-semantics coverage is
NOT claimed.
EOF

# The receipt is honest only if it can go RED: a rendered FAIL in the verdict
# table must also fail the process that produced it — before this guard the
# script's exit was the heredoc's, so CI's "make receipt" step stayed green
# around a FAIL table (#984). The fingerprint-reuse branch is all-PASS by
# construction; this bites only the derived path.
case "${PROOF}${GATE}${CWALL}${VTEST}" in
  *FAIL*) echo "receipt: a verdict above is FAIL — refusing to exit green (#984)" >&2; exit 1 ;;
esac
