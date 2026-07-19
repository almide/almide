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

END-TO-END WIRING — DONE (commits be98af34, 1485a2bc, a774500d): the fold now lives in the
proof, consumed per build.
- `check_prog_cert` (CapabilityReach) parses the call-graph witness INSIDE the proof (functions
  `;`-separated, each `declared|direct|callee-indices`) and `prog_within` COMPUTES the transitive
  reach + checks `reach ⊆ declared` per function — soundness `check_prog_cert_sound`, axiom-clean.
  (The `prog_ok`/`reaches_sound` per-edge composition law is kept as a separate result; the gate
  uses the direct `prog_within`, which accepts a callee that over-declares.)
- Extracted to OCaml (`Extract.v` → `checker.ml`) + a `caps-transitive` driver mode;
  `build-checker.sh` demos a bounded graph ACCEPT + an undeclared-network REJECT.
- `program_cap_graph_witness` (certificate.rs) EMITS the graph from MIR; an unknown/cross-file/
  elided callee routes to a sentinel UNIVERSE node so any caller reaching it is REJECTED
  (conservative, now decided by the proof). Unit-tested + verified end-to-end through `./checker`.
- `classify_corpus` writes `caps_graph.cert` (one line per fully-analyzable+within-bound file);
  `corpus-wall.sh` runs `check_prog_cert` over it. **Gate green over the whole v0 corpus: 198
  program witnesses ACCEPT** beside ownership/names/caps. So for fully-analyzable programs the
  reachability fold is re-derived by the kernel-proven checker, not trusted from Rust.

REMAINING (smaller): partially-analyzable files still use the per-function `caps.cert`
(reachable computed by the Rust fold) — extending the graph witness to them needs the UNIVERSE
taint to be the gate's honest-scope "unverified" rather than a hard reject (a policy refinement).

## 全V — cert-level COMPLETE; #40 byte-binding now grounded on BOTH sides
The flight-grade properties are all proven at cert level: ownership (double-free + leak),
names (no dangling), caps (per-function + now transitive composition), stack balance,
termination, COW safety, loop ownership, and the rc_inc/rc_dec **byte-binding**.

## #40 byte-binding — ENCODER + EXECUTOR both grounded (commit 9a82b61d)
The rc byte-binding has two model-vs-reality gaps; both are now grounded against real tools,
re-checked every build by `check.sh`:
- **ENCODER** (`WasmEncode.v`: instruction tree → bytes, `encode_body = rc_inc_bytes`): grounded
  by `check-wasm-bytes.sh` — wat2wasm re-assembles the rc primitives to the SAME bytes.
- **EXECUTOR** (`WasmExec.v`: a bespoke interpreter `run_g` proving the bytes' memory effects):
  NOW grounded by `check-wasm-exec.sh` — wasmtime executes the SAME bytes to EXACTLY the proven
  effects: rc_inc 4→5, rc_dec rc=1 frees to 0 AND reclaims to `$freelist` (leak-freedom
  reclamation), rc_dec rc=0 TRAPS (double-free sentinel). So `run_g` is faithful to a real
  engine for the rc ops — the byte-EXECUTION binding is non-circular.

## #40 WasmCert-Coq port — the ARCHITECTURE brought in-tree (WasmIsa.v, commit 6f2dfda8)
Importing the external WasmCert-Coq library is infeasible here (no opam; the library targets
older Coq, not Rocq 9.1). What it BUYS is an architecture — a relational ISA SPEC + an
executable interpreter proven to refine it — and that we now have natively:
- `istep` — a relational small-step semantics, one rule per opcode in the wasm-spec style (the
  trusted ISA SPEC), with `irun` sequencing it over an instruction list.
- `estep`/`erun` — an executable evaluator proven to REFINE the spec: `erun_sound` (every result
  is a real reduction), `erun_complete`, `istep_det` (deterministic).
- `rc_inc`'s effect (cell → cell+1) proven THROUGH the relation via the refinement.
This replaces WasmExec.v's bespoke `run_g` (no spec to refine) with a real semantics relation
for the rc subset. Axiom-clean, in `check.sh` (coqc + coqchk).

STRUCTURED CONTROL — DONE (commit e1d2112c): `istep`/`irun` now cover `IIf` (block `if/end`, runs
its body) and `IUnreachable` (the trap = a stuck state, no rule). Coq's structural guard rejects
the nested single/mutual fixpoint (list-inside-instr), so the executable `erun` is FUEL-bounded,
proven SOUND w.r.t. the relation by fuel induction. **rc_dec carried THROUGH the relation**:
- `rc_dec_isa_traps_on_zero`: a double-free never completes (the interpreter returns None for
  EVERY fuel — the sentinel fires), the safety direction.
- `rc_dec_isa_frees_when_one`: rc=1 ⟹ decremented to 0 AND reclaimed onto `$freelist` (leak-
  freedom), reachable in the spec (via `erun_sound`).
So the double-free trap + reclamation that `check-wasm-exec.sh` grounds EMPIRICALLY are now also
relational ISA THEOREMS.

FORALL UPGRADE — DONE, every rc theorem is now `forall`/relational (commits 8aaaba53, cf438a37):
- `isa_det`: the ISA relation is DETERMINISTIC (a config steps to ≤1 successor; combined mutual
  induction). So a program reduction has a UNIQUE final state.
- `erun_complete`: irun ⟹ ∃ fuel, erun = Some (combined induction; `erun_S_cons` one-step-unfold
  keeping inner `erun` folded + `erun_mono_add` fuel monotonicity align the IIf body/continuation
  at a common fuel — the `cbn`-unfolds-`erun` snag, solved).
- `rc_inc_isa_effect`, `rc_dec_isa_frees_when_one`: **`forall`** ("EVERY reduction reaches the
  effect") — the interpreter computes one (`erun_sound`) and `isa_det` makes it THE one.
- `rc_dec_isa_traps_rel`: **`~ irun`** — NO reduction of a double-free completes (the spec itself
  is stuck), via `erun_complete` + the executable `forall`-fuel trap. The relational safety theorem.
So `erun` is a verified interpreter REFINING the spec (sound + complete + deterministic), and all
three rc facts (inc effect, free/reclaim, double-free trap) hold for EVERY reduction. Axiom-clean.

REMAINING heavy track (#40's maximal form): the FULL canonical WasmCert-Coq ISA (complete
verified wasm execution semantics) so the binding holds for ALL wasm programs abstractly, not
just the rc subset. A multi-week library port — proofs flag it "the deferred heavy track".
The PRACTICAL byte-binding (emitted rc bytes ↔ real encode ↔ real execute) is closed + grounded;
WasmIsa.v now gives the rc subset a spec-relation-backed semantics, the architectural foundation
a full port would extend.

**Since this doc's scope was written**, the spine has grown further: `proofs/check.sh`'s
`coqc`/`coqchk` module list now also includes `CoownLoop`, `CoownCompose`, `CallModes`,
`WasmDecode`, `CowSafety`, and `FreeList` (verified: `grep -n "coqchk\|\.vo" proofs/check.sh`).
