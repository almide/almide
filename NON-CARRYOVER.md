# The Non-Carryover Ledger

> Last updated: 2026-08-25 (stage 90, burn-up **599/599 — COMPLETE**).

## 0. How this ledger CLOSES (the mechanism, not a sample)

"Every incumbent weakness" is only checkable against a CLOSED
enumeration, and the incumbent maintains exactly one: the 599-fixture
conformance corpus. Its practice (enforced by the als gates) is that
every shipped defect distills into a fixture citing its issue and its
contract (`// @contract: C-NNN` headers; fixture prose cites #NNN) —
the corpus IS the executable union of every ruled behavior and every
regression ever caught. The GitHub label taxonomy is NOT that closure
(defect issues ship unlabeled — #1542 carries no label), so this ledger
does not pretend to enumerate the tracker.

Therefore the closure mechanism is: **the burn-up gate replays the
whole corpus against this engine and byte-verifies every claim; a row
it cannot claim yet is NAMED in the histogram, never silent.** The
ledger below is the narrative index of the interesting classes; the
closed verification is `backend_parity.rs` at its current floor, and
the ledger is COMPLETE exactly when the burn-up reaches 599/599 (the
remaining rows are enumerated by reason-label at every run).

**CLOSED 2026-08-25**: the burn-up stands at **599/599, divergence
zero** — every fixture in the incumbent's closed conformance
enumeration byte-matches on this engine (stdout hash + exit code), and
the grow-only floor (`SUPPORTED_FLOOR: 599`) makes any future
regression a test failure, not a drift. The port-vs-rebuild record is
simultaneously closed: PORT-MATRIX.md (regenerated at 599) records
435 linked admissions (each with its audit tier), 9 explicit
rejections with their layout-coupling reasons, and 792 impls whose
surfaces are covered by native arms in this emitter or sit outside
the corpus behind honest walls — nothing links silently, and nothing
in the corpus is unverified.

The port-vs-rebuild record has the same shape: `PORT-MATRIX.md` is
GENERATED (scripts/gen-port-matrix.py) over every one of the 1236
self-host registry impls — linked / rejected-with-reason / unreached —
so no admission decision can be sampled or forgotten.

The standing goal forbids carrying forward any weakness of the incumbent
`develop` compiler, requires every port to be justified, and requires the
rest to be rebuilt. This ledger is the CHECKABLE form of that promise:
one row per incumbent defect class, the greenfield mechanism that
excludes it, and the evidence. A row without evidence is a TODO, not a
claim.

Comparative grounding: every architectural decision below traces to the
nine-compiler reference corpus (`../almide-references`, SHA-pinned) —
the specific canon entries are cited as `W-n` (RESEARCH-wasm-backends),
`R-*` (the other RESEARCH files), or §n of ARCHITECTURE.md (each
ratified ○× against that corpus).

## 1. The five architectural diseases (the 2026-08 incumbent diagnosis)

| # | Incumbent disease | Greenfield exclusion | Evidence |
|---|---|---|---|
| D1 | Layout = f(producer): each emitter picks its own block shapes, equivalence patched per-pair | ONE ratified layout (`almide-layout` crate: RC/LEN/CAP header, PAYLOAD=12, SUM_TAG/SUM_FIELD) consumed by every lowering; no second producer exists | `crates/almide-layout/src/lib.rs`; every `slot_memarg` call site |
| D2 | Name-keyed fail-open ABI registry + fixpoint | The fn table is INDEX-keyed from one `collect_program_fns` pass; the self-host registry resolves through an explicit closed WHITELIST (fail-closed: unlisted = honest wall) | `calls.rs resolve_qualified` (VERIFIED/…); PORTLOG stages 70–73 |
| D3 | Per-target pass ordering (nanopass zoo diverging per backend) | One lowering (`s5::lower_to_ir`) feeds BOTH legs; the interp and the wasm emitter consume the SAME IR with no per-target rewrite phases | `crates/almide-spine/src/s5.rs`; ARCHITECTURE §1 law 2 |
| D4 | Desugar zoo (surface forms rewritten into diverging cores) | Never paid: greenfield lowers surface forms DIRECTLY (guard/guard-let land as their own statement kinds; the raise leaf handles the desugared else-arm without a rewrite pass) | `stmts.rs lower_stmt_guard`; `data.rs lower_err_raise` |
| D5 | Grammar scars (parser-level irregularities) | CARRIED — honestly: the shared frontend grammar is upstream of this tree and is the one inherited surface. Tracked, not hidden | ARCHITECTURE §0 (the language ports; the engine does not) |

## 2. Incumbent defect classes found DURING this arc, with exclusion proof

Each of these was discovered by greenfield's own gates while building —
i.e. the incumbent still HAS the defect and greenfield measurably does
not.

| Incumbent defect | Where it lives there | Greenfield state | Evidence |
|---|---|---|---|
| `x + 0.0 → x` float fold (IEEE-invalid: wrong for -0.0) | incumbent folder | No such fold exists; float folds restricted to exact transforms | #1542 filed; fixture `float_no_contraction.almd` claimed |
| Stale fuel verdict + cross-region poisoning on cut | incumbent meter | C-320 exit bookkeeping on BOTH legs (region repair guarded by depth-at-entry) | #1572 filed; `fuel.rs` region_repair; ruled sequence fixture |
| Native leg double-evaluates side-effectful map callbacks | incumbent native HOF | HOF callbacks lower ONCE per element by construction (inline loop bodies) | als report pending (C-321 candidate); host-oracle jurisdiction rule |
| Anon-record shuffled by-name fields misplaced | incumbent wasm leg | By-name slot resolution in the record lowering; mutant 015 pins the class | `data.rs` record lowering; `ci/mutations/015` |
| `len`-as-tag Value layout (tag stored in the len slot) | incumbent Value blocks | REBUILT: tag @SUM_TAG, payload @SUM_FIELD — ratified 2026-08-20 ○ "rebuild, do not adopt" | `value.rs` module doc; value_eq/merge helpers over the native layout |
| List len header holds ELEMENT COUNT (vs bytes), 8-byte slots for all classes | incumbent list layout | REBUILT: byte-length header, 4/8-byte slots by value class; the coupling is fenced by the RAW-HEADER-READ whitelist rule (any self-host impl reading `load32(h+4)` stays unlinkable) | `calls.rs` whitelist comment (from_bytes rejection); PORTLOG stage 71 |
| `bytes` read/set partially trap-form + sum-form room tests that wrap | incumbent early forms | C-229 TOTAL matrix (OOB read = default, OOB set = no-op) on SUBTRACT-form room tests, every width both endiannesses | `bytes.rs`; fixture `bytes_negative_offset_family.almd` claimed |
| `list.slice` end clamp signed (`0..-1` = empty; native = whole) | greenfield's own first draft — caught by C-054 fixture before any release | Unsigned-saturating clamp; mutants 010 (refreshed) + 042 pin it | `list.rs` slice arm; `list_count_index_truncation.almd` claimed |
| Division-by-zero abort has NO isolated claimed observer | both trees (coverage gap) | FINDING, not yet fixed: a zero-guard-only mutant survives the incremental nets | PORTLOG stage 72; als fixture candidate at next pin advance |

## 3. Port vs. rebuild — the decision record

The rule ratified in ARCHITECTURE §0: instruments port, propulsion is
rebuilt. Concretely:

**Ported unmodified (the crown jewels):**
- The language surface, dialect epochs, `spec/` corpus, CHEATSHEET,
  llms.txt — the assets the mission (MSR) is measured on.
- The contract ledger discipline and the 3-way-oracle method (now with
  the a877-pinned wasm-leg third oracle and jurisdiction rules).
- Self-host stdlib BODIES, but only through the audited whitelist: an
  impl links iff its body is layout-clean (pure scalar prims, read-only
  loads on digest-shared layouts, prim-MEDIATED allocs, ctor-built
  sums). Admitted classes so far: Dragon4/float formatting, string
  trim/upper/lower/take_end/drop_end, json_parse, list_repeat/range,
  the 233-cell sized-conversion family, the scalar/text tail batch.
  Every claim is byte-verified by the burn-up before it counts.

**Rebuilt (the propulsion):**
- The wasm emitter entirely (`crates/almide-wasm`): layout, calls,
  control flow, display, equality, collections, Value model, bytes,
  the deterministic meter — zero lines shared with the incumbent
  renderer.
- The interpreter as referee (`crates/almide-interp`) on the same IR.
- The gates: burn-up grow-only floor + byte-exact claims, the 40-patch
  mutant fleet (numbered through 042) with observer-first discipline,
  hold-balance invariant,
  surface-matrix golden, aviation-quality (codopsy A) in CI,
  file-discipline cap, wrong-desk guard.

**Rejected ports (audited and refused):**
- `string_from_bytes` self-host (raw list-header read — the rule's
  founding counterexample).
- Every incumbent list combinator self-host (same header coupling) —
  hand-emitted natively in `list_comb.rs` instead.
- The incumbent's magic-division constants path was implemented, PROVEN
  exact, measured 60% slower on aarch64/cranelift, and retired — the
  guards-dropped literal-division form stayed (PORTLOG, perf war).

## 4. What "world's best" is measured against (the references)

The comparative claims are per-mechanism, each grounded in
`../almide-references` (nine compilers, SHA-pinned) at decision time:

- Fn values / closures: zig's +1-biased table + grain's closure elision
  + roc's uniform signature — W-1/W-2; greenfield adopted the inline-
  first form with the table only for fn-as-data.
- Safety checks live in the emitter's PRE-stage, not the backend (zig's
  Sema doctrine, W-3) — greenfield's checker types drive the C-180/
  C-179/C-002 sized-integer lowering with no backend re-inference.
- Sort: measured to zig-pdqsort parity; pipeline fusion beats hand-
  fused zig on the measured corpus; strings at incumbent parity;
  recursion at 1.08× the loop ceiling (PORTLOG perf stages).
- The deterministic meter and NaN canonicalization (C-210) follow the
  als normative rulings — the judge repo is the oracle, never another
  implementation (F1: no oracle circularity).

Open comparative fronts (not yet claimable, tracked): vendored libm
bit-parity (C-305), the matrix/f32 kernels, regex, WASI host surface.
