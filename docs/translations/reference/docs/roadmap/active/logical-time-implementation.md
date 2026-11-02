Description: Implementation blueprint: Op::Charge, fuel ABI, metered clones, race lowering, gates.
<a id="logical-time-async--the-implementation-blueprint"></a>

# Logical-Time Async — the implementation blueprint

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](logical-time-implementation.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](logical-time-implementation.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](logical-time-implementation.zh-CN.md)

> Non-normative historical implementation blueprint pinned to the original. No automatic synchronization, current implementation verification or independent measurement. The dated 2026-08-02 status and later-in-document future stages coexist; preserve the remaining strict cap/unwind, metered clone and Dyn charge. Historical race/bounded syntax, proof and zero-overhead claims are source claims, not current availability or whole-system assurance. The source alternates ticks/fuel names and says no new unwinding while listing unwind as remaining; these are not silently reconciled. Commands are quoted plans, not executed. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-implementation.md)

> **Implementation status (worktree-stage1-charge-probe, 2026-08-02)**: Implemented through Stage 1 probe / Stage 2 bounded / Stage 3 race / Wave 1 surface unification / all of T2 / T1-3 native Res carrier (CM-1 v0.3 = 3ns/unit, unit-exact boundary fixtures). Remaining = strict cap + unwind, metered clone, Dyn charge. Ledger: research/spike/charge-probe/BURNDOWN.md (this ledger is authoritative in a discrepancy).

> Charter: [async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md). Semantics: [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md); proofs: [logical-time-proofs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md). This document specifies where in the compiler, with which data structures, and how to carry the design through: engineering grounded in the actual codebase.

<a id="0-接地--コードベースが既に持っているもの"></a>

## 0. Grounding — what the codebase already has

Translating the design vocabulary into implementation vocabulary gives:

| Design term | Implementation entity |
|---|---|
| Shared MIR | `MirFunction { ops: Vec<Op> }` — a **flat op stream**. No basic blocks; structured markers `IfThen` / `LoopStart` / `LoopBreakUnless` / `LoopEnd` express control flow (WAT-shaped) |
| Both renderers | `render_native.rs` (v1 native → Rust source) and `render_wasm/` (→ WAT). Native retains a v0 codegen fallback |
| Proof-shaped validator | The `certificate.rs` family: the existing framework projects MIR ops into an event sequence and rechecks it with a **Coq-proved checker** (`proofs/`, `check_all` / `check_names`) |
| Exhaustion propagation path | Lifted effect fn ABI (can-err = always wrapped, already unified). `Err(String)` is the sole error channel |
| Specialization mechanism | Mono in almide-optimize (add fuel to the existing cloning dimensions) |
| Foundation for three-point comparison | `spec/wasm_cross` fixtures + the existing evidence class of emit-time Σ-probes |

Everything in the right column actually exists; this grounds the design that follows rather than leaving it on paper.

<a id="1-opcharge--時計の最小単位"></a>

## 1. `Op::Charge` — the smallest clock unit

Add just one op to MIR.

```rust
/// 論理時計の 1 事象。site は CM-1 の charge site 台帳上の安定 id。
/// cost は静的（Const）か、実行時値に線形（Dyn: base + per_unit × len(v)）。
Charge { site: ChargeSiteId, cost: ChargeCost /* Const(u32) | Dyn { base, per_unit, len_of: ValueId } */ },
```

The code comments identify one logical-clock event: site is a stable ID in the CM-1 charge-site ledger; cost is static (Const) or linear in a runtime value (Dyn: base + per_unit × len(v)).

**Insertion points** (this placement satisfies W1/W2):

- One at function entry (cost = the number of serial ops from entry).
- One immediately after `LoopStart` (cost = serial ops in one iteration): every cycle necessarily passes through it (W1).
- A `Dyn` charge immediately before variable-cost intrinsics (list concatenation, string construction, etc.).
- Additional charges on branches of `IfThen` if their costs are substantially asymmetric (start with entries and loops only; choose granularity in the same PR that fixes the CM constants).

Insert at **IR→MIR lowering, when MIR is born**. Every subsequent MIR pass (`pipeline*.rs`, including peepholes in `render_wasm_fuse.rs` and elsewhere) must **not fuse across Charge**.

**Refinement (the precise implementation form of charter §4 pillar 1)**: “assigned before optimization” means assignment at the IR→MIR boundary. Upstream IR passes (optimize → mono → ir_link) are **inside the definition** of CM-1. If a change there alters the charge count, fixed-fixture `consumed_fuel` goes red: the ratchet catches compiler-version semantic changes and forces a CM-version decision. There are only two user-facing optimization knobs, both covered already: native `--release` is rustc optimization, and Charge is rendered as counter operations so it does not disappear. `--wasm-opt` is similarly covered because counters enter the observations of the existing differential parity gate.

<a id="2-fuel-の実行機構--新しい-abi-を作らない"></a>

## 2. Fuel execution — do not invent a new ABI

- **wasm**: module global `$__ticks: i64`. Charge renders as `global.get → i64.const cost → i64.sub → global.set` plus an exhaustion branch. `Dyn` reads len and computes a multiply-add.
- **native**: runtime thread-local `FUEL: Cell<i64>`. Render the analogous subtraction and branch.
- **Propagate exhaustion through the existing effect ABI**. Metered clones (§3) render in can-err form; exhaustion is an `Err` with the ledger's constant message, carried by the ordinary `?` chain to the `fan.bounded` / `fan.race` boundary. **Do not build** new unwinding, a new return convention, or a new wrapper network: the ABI unified in #840/#841 works unchanged.
- For charge-trace observation in fixtures, add a trace accumulator (a hash of the site-ID sequence) alongside `$__fuel` **only in probe builds**. Normal race / bounded builds carry no hash (winner and Err suffice as observations).

<a id="3-計測の特殊化--mono-の追加次元"></a>

## 3. Metering specialization — another mono dimension

The design requires metering only within bounded regions. Implement it as another mono cloning dimension.

1. The checker takes the transitive closure of functions reachable from `fan.bounded` / `fan.race` bodies (closure literals are trackable through the existing closure-table path).
2. Clone that set with a `metered` flag (the same thing mono does for generics; names follow a stable convention such as the `__fuel$` prefix).
3. Only metered clones receive Charge insertion at IR→MIR and render with the can-err ABI. The unmetered side stays byte-for-byte identical to today: the claim of **zero global overhead** follows from this structure.
4. **v1-only restriction**: native v0 codegen fallback has no Charge concept. Thus syntax that hits a v1 native-render wall inside a bounded region is a **compile error** (diagnostics in the same style as the wasm hard wall). Build fuel only on the trust spine.

Race branches additionally carry a `speculative` flag: render potentially trapping ops (div/rem, array bounds) in checked form and convert traps into “branch termination records” (the semantics' delayed-trap decision). Bounded is only metered (traps still terminate immediately as usual: it is not speculative).

<a id="4-stage-1----fuel-probe-表面ゼロで反証器を先に立てる"></a>

## 4. Stage 1 — `--fuel-probe`: build the falsifier before any surface

Specialization (§3) is Stage 2 heavy machinery; do not build it in Stage 1. The hidden `--fuel-probe` flag renders **every function** as if metered and prints `(consumed_fuel, trace_hash)` in a conventional stderr line at program termination.

- Fixtures: run `spec/wasm_cross/fuel_probe_*.almd` on both targets and compare **all three of result / consumed_fuel / trace_hash**. A divergence of one unit or one position directly falsifies charge-trace preservation.
- This probe build becomes the user-facing calibrator **`--ticks-report`**: print consumed ticks per region / program **alongside measured time on this machine** (`52,000 ticks (≈0.4ms here)` — the tool connects numerical intuition, like EVM estimateGas). From Stage 2, also provide **`almide run --ticks n` / `almide test --ticks n`**, a host-boundary budget that merely wraps main in bounded, through the same mechanism. The harness / CI owns the number, making a deterministic hang killer that is not flaky like wall time ([ticks-interface-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md), correction 2). In the same PR, guard the prime suspects, the `render_wasm_fuse.rs` peepholes, against fusion across Charge.
- **Charge certificate**: add a parallel form to the `certificate.rs` family. `charge_certificate(f)` projects the MIR Charge sequence (ordered site IDs) and checks equality with reprojection after rendering. It has the same shape as ownership certificate → Coq checker, so the extension point for a future `ChargeTotality.v` in `proofs/` (equal sequence length and order) opens in the same place.
- Add property tests on generated MIR to `tests/` (random op sequence → both renderers → matching traces).

<a id="5-stage-2--fanbounded-表面と-cm-1-定数の確定"></a>

## 5. Stage 2 — `fan.bounded`: fix the surface and CM-1 constants

- Parser: fan v2 head grammar (`fan.bounded(<expr>) { body }`). Simply extend the member-access branch of `parse_fan_primary` into a head-args + block branch. **No labels or literal suffixes**: the budget is just an expression (`compute.ms(100)`); lexer unchanged (ADR-0001).
- Checker: add `bounded` to the fan-arm table in `static_dispatch.rs`. Require a pure body (existing purity mechanism). Its type is `Result[T, String]`; auto-unwrap follows the existing race/any/settle contract.
- Lowering: register the body closure in the metered set → clone as in §3 → at region entry, `FUEL = min(n, 外側remaining)` (EIP-150-style min-cap takes two ops: assign on entry, subtract consumption from the outer region on exit; this explains why no region tag is needed). In that expression, 外側remaining denotes the outer remaining budget.
- **Fix CM-1 constants in this PR**, recording them as a versioned object in the contract ledger. Fixtures pin representative programs' consumed_fuel to absolute values (ratchet).

<a id="6-stage-3--fanrace-まず逐次で出荷する"></a>

## 6. Stage 3 — `fan.race`: ship sequentially first

The semantic confluence theorem guarantees the same observation for sequential scan and parallel execution. In implementation-order terms, **the first race can use sequential scan on both targets**:

1. Run branches in list order with cap = `min(n, 最良決定事象.time − 1)` (use the Charge exhaustion branch directly: no separate cap mechanism, just put cap in `FUEL`). Here 最良決定事象 is the best decisive event.
2. Record branch termination (Complete / Err / trap record / exhaustion), then decide using the least decisive event in merge order after scanning. If a trap lies in the visible window, **rethrow** the recorded trap there.
3. Separate native thread parallelism into a **later performance lane**. Semantics already allows parallelism (proved); leaving it out initially loses nothing beyond postponing an observation-preserving optimization. Wasm is sequential anyway.

This confines Stage 3 implementation risk to checked-trap rendering and the scan driver. Revise E027 into a signature-migration hint in the same PR.

<a id="7-fan-v2-wave-1--既存-desugar-の簡略化として実装する"></a>

## 7. fan v2 Wave 1 — implement as a simplification of existing desugaring

Block-form any/settle is actually **simpler** than today's `desugar_fan.rs`. Currently, it tracks thunk-list literals/let bindings and recovers bodies inline (#599 path). With blocks, arm expressions are **available from the start**: the tracking mechanism disappears, while desugaring to match chains is reusable. Mapper-form `fan.any(xs, f)` is the same “data + one closure” as `fan.map`, so it avoids the `List[funcref]` wall (implementation support for fan-v2.md's claim).

Touch points: `parser/primary.rs` (head branch), `static_dispatch.rs` (arm table + E0xx tombstone for thunk-list form), `desugar_fan.rs` (simplification), `fmt`, interp bridge, migration of 8 spec files, removal of dead `ExprKind::Await` / `r#async`.

<a id="8-interp第三の審級の扱い"></a>

## 8. Handling interp (the third adjudicator)

almide-interp walks IR, upstream of MIR. Since fuel is defined on MIR, explicitly mark interp as **abstaining on fuel syntax** during Stages 1–3 (programs containing race/bounded reduce the 3-way oracle to 2-way). Do not mirror CM-1 onto IR: two definitions would create drift. Make the abstention scope machine-readable in fixture notes; retire it if interp later changes to walk MIR.

<a id="9-ゲート配線の一覧"></a>

## 9. Gate wiring inventory

| Gate | What it pins | Location |
|---|---|---|
| Three-point fixtures | Agreement on result + consumed_fuel + trace_hash across both targets | `spec/wasm_cross/fuel_*` |
| Charge certificate | Rendering preserves Charge sequence length and order | `certificate.rs` family + future `proofs/` |
| Consumed ratchet | Representative programs' absolute fuel values (detect semantic changes across compiler revisions) | Pins within fixtures |
| Confluence gate | Exhaustive semantic-model checking (design-side regression) | `research/spike/logical-time-race/run-gate.sh` |
| Lean belt | 7 selection-algebra theorems | CI `lean-proofs` (wired) |
| wasm-opt parity | `-Oz` does not break counters | Existing parity gate (only expand observations) |
| Contract ledger | CM-1 versioned object + new C-NNN family | `contracts.toml` + `check-contracts.sh` |

<a id="10-未決実装が答えを出す点"></a>

## 10. Open questions (implementation supplies the answers)

- Charge granularity (start with entry + loop only, or where to set the branch-asymmetry threshold): decide through Stage 1 probe measurement and fix it in the CM-1-constants PR.
- The per_unit constant table for `Dyn` charge: same as above.
- Naming for metered clones and representation of metered slots in the closure table: fix in the Stage 2 implementation PR (the design answer is that closures which may flow into a region have a metered variant).
- The trace_hash hash function: order-sensitive, not a mere sum, for fixture diagnostics; provide a sequential probe dump mode for locating positions.

A corresponding implementation PR may overwrite any section of this document, but must do so by revising this document: do not let design and implementation have separate truths.
