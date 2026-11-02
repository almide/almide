Original description metadata: Flight gate G-F4 — the reference application (a fixed-point Q16.16 PID control-law kernel over a counted sim loop) that passes `make verify` end-to-end, the 7-stage verify pipeline (exist vs gated on keystones), the receipt it emits (C-SAFE/C-PROVEN green, C-WCET/C-FAITHFUL pending), and the de-risking order (Slice 0 scalar-no-print green now → Slice 1 print=G-F0 frontier → Slice 2 keystone-あ unlocks C-WCET → Slice 3 keystone-い unlocks Ferrocene).
<a id="flight-reference-app--pid-制御則カーネル--make-verify--receiptg-f4"></a>

# Flight Reference App — PID control-law kernel + make verify + receipt (G-F4)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flight-reference-app.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-reference-app.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flight-reference-app.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md)

> Source caveat: this retains a historical application plan and its later status corrections, not a flight-ready controller or accepted qualification. The original code sketch has known syntax and state-update errors and is deliberately not repaired or executed. Later entrypoints supersede the old print/renderer/app-receipt backlog; the historical receipt remains an example, not freshly generated evidence. Its allocation-bound label C-WCET does not establish physical worst-case execution time, and certificate safety does not prove control-law stability, gains or functional requirements. Stdout-only capability scope is not all host effects. The initial whole-spine kernel-only wording and clean-compile/object-trust slogan do not erase extraction, renderer, host or toolchain trust. The Ferrocene lane uses stock rustc pending access, not demonstrated qualified-toolchain use or regulatory credit. Numbers, votes and bounds are recorded source-time results; no implementation, proof, measurement or receipt command was rerun.

> **STATUS 2026-08-12 — application landed (PR: flight-pid-reference)**: `spec/wasm_cross/flight_pid_control.almd` pinned as contract C-230. Measured: native == wasm **byte identity** (16-step trace), **interp also votes** (3-way agreement, voting 254/374), corpus wall **TOTAL** (6319 fn, walled 0 — all fn carry certificates). Stages 1, 2a-c, 3, 4, 5 green for this application (Coq revalidation for 2a-c in CI trust-spine). Differences from the design sketch: `err` is reserved for a Result constructor → `e`/`prev_e`; `..` → `..<`; local clamp → delegated to `int.clamp` (E015); **the sketch forgot to update integral/prev_err** — corrected as scalar `var` updates in the driver (as noted in §1's constraints). Remaining: stage 2d (keystone あ), 6-7 (keystone い), and the app-scoped receipt block (this line records remaining work as of 2026-08-12).

<a id="current-evidence-entrypoints"></a>

## Current evidence entrypoints

- `spec/wasm_cross/flight_pid_control.almd` is the implemented application;
  the original sketch below contains the already-recorded state-update/syntax
  mistakes and is retained as design history, not a copyable current example.
- `proofs/receipt.sh` now emits an app-scoped G-F4 block. Its actual verdicts
  and scope—not the original PENDING list below—describe the checked artifact.
- `crates/almide-mir/src/render_native.rs` and `src/cli/emit.rs` provide the
  native renderer/trace-map path; it is not merely an unconnected 368-line demo.
- `.github/workflows/ferrocene-lane.yml` explicitly uses stock rustc pending
  Ferrocene access. Passing that lane is not proof of qualified-toolchain use,
  complete Almide→Rust faithfulness or regulatory acceptance.
- The per-loop witness/physical-WCET proposal remains separate; do not mark it
  complete just because the reference application or bounded profile exists.

> **Goal**: concrete design for [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) ladder **G-F4** — a **reference application** written in the flight profile with verification/receipt pathways (= the “ammunition” turning slides into artifacts). Runs → oracle byte identity → per-build certificates → readable Rust → Ferrocene. Must fit the flight subset ([flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md)).
> **Related**: [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2 / [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) / [flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md).

---

<a id="1-リファレンスアプリ--採用fixed-point-pid-制御則カーネル"></a>

## 1. Reference application — selected: fixed-point PID control-law kernel

`pid_control.almd` — run a single-axis PID controller using integer fixed-point (Q16.16) in a **bounded counted simulation loop** (N steps), printing the saturated actuator command each step. Exactly the “control-law kernel” the ladder gives as G-F4's example ([flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2).

**Reasons for selection, by criterion**:
- **(a) Looks safety-critical**: PID + saturation + anti-windup is the most universal pattern in aviation/automotive control (FADEC, control-surface actuators, ECU torque). Certification engineers immediately map “control law” to their DO-178C/ISO 26262 mental model. Stronger than CRC (= looks like a utility) or a ring buffer (= plumbing).
- **(b) Fits the flight subset**: all scalar Int (fixed-point), no dynamic allocation in the hot path, no recursion in the loop body; the driver is counted range `for t in 0..N` — precisely the shape lowered by `try_lower_scalar_for_range` (`crates/almide-mir/src/lower/control.rs:766`). PID `step` is a pure arithmetic `fn` (no heap/caps); only the driver's `println(int.to_string(cmd))` reaches Stdout.
- **(c) Small but real**: ~60-80 lines. Q16.16 multiplication (rescale with `/ 65536`), saturation (`math.max`/`math.min`, already self-hosted and pure), integral anti-windup (accumulator clamp). A control engineer recognizes every term.
- **(d) Exercises certificates end to end**: touches every proved property — **ownership** (each `int.to_string` creates 1 `Alloc` `i`, move-out to `println`, drop), **name totality** (driver→`pid_step`→`clamp` call graph), **capability bound** (driver **declares** Stdout and reaches it ⇒ ACCEPT; integral/derivative math is pure ∅), **counted loop** = first real witness target for the WCET keystone.

**Structural sketch** (grounded in `docs/CHEATSHEET.md` + the self-hosted surface):

```almide
// Q16.16: 1.0 == 65536。全スカラ Int ── hot path に heap なし。
let SCALE: Int = 65536
let OUT_MIN: Int = 0 - 6553600
let OUT_MAX: Int = 6553600
let I_MIN: Int = 0 - 3276800
let I_MAX: Int = 3276800

fn fmul(a: Int, b: Int) -> Int = (a * b) / SCALE          // (a*b)>>16
fn clamp(x: Int, lo: Int, hi: Int) -> Int = math.max(lo, math.min(hi, x))

// 1 PID ステップ。PURE: capability 宣言なし(∅)。
fn pid_step(kp: Int, ki: Int, kd: Int, setpoint: Int, measured: Int,
            integral: Int, prev_err: Int) -> Int = {
  let err   = setpoint - measured
  let integ = clamp(integral + err, I_MIN, I_MAX)         // anti-windup
  let deriv = err - prev_err
  let raw   = fmul(kp, err) + fmul(ki, integ) + fmul(kd, deriv)
  clamp(raw, OUT_MIN, OUT_MAX)                            // アクチュエータ飽和
}

// COUNTED シミュレーション driver。effect fn({Stdout} 宣言)。counted range。
effect fn main() -> Unit = {
  var integral = 0
  var prev_err = 0
  var measured = 0
  let setpoint = 65536                                    // 1.0 ステップ入力
  for t in 0..16 {                                       // counted、リテラル bound = WCET witness
    let cmd  = pid_step(13107, 6553, 3276, setpoint, measured, integral, prev_err)
    measured = measured + (cmd / 256)                    // 簡易プラント
    println(int.to_string(cmd))                          // 唯一の Stdout 出口
  }
}
```

Full translation of the preserved sketch's comments: Q16.16: 1.0 == 65536; all scalar Int, no heap on the hot path. Multiplication corresponds to (a*b)>>16. One PID step is PURE, with no capability declaration (∅); the integral clamp provides anti-windup and the final clamp actuator saturation. The counted simulation driver is an effect fn (declares {Stdout}) using a counted range. The setpoint is a 1.0 step input; the literal loop bound is the WCET witness. Updating measured is a simplified plant, and printing is the only Stdout exit. The original code, including its known missing state updates, is retained, not repaired here.

*(Scalar `var` reassignment inside a counted scalar loop is allowed by `scalar_loop_depth` (`control.rs:816`). Per-iteration heap-tuple co-return `(cmd, integral)` is in the DEFERRED subset, so keep integral in a scalar `var` — see §5.)*

**Runner-up: CRC-16/CCITT** (`crc16.almd`) — shift/XOR fold the CRC register in a counted loop over a fixed byte table. **More trivially in-subset** (the exact iteration shape of `list.sum`) and **a seed for the minimum slice** (§4), but looks like a “utility” to certification engineers. Retain for **minimum-first work**; lead the demo with PID.

---

<a id="2-原設計の検証パイプライン当時の-existgated-記録"></a>

## 2. Original verification pipeline (exist/gated record at the time)

CI Trust Spine is `make verify-trust`→`make receipt` (`Makefile:77-84`). Stages for this application:

| # | Stage | Command/tool | Output | "pass" | Status |
|---|---|---|---|---|---|
| 1 | source→IR→MIR | `render_program` example (`examples/render_program.rs:28-50`) | per-fn MIR | All fn `Ok` (in-subset), zero walled | **EXISTS** |
| 2a | Ownership certificate + ACCEPT | `gate.sh`: `emit_cert_from_source`→`checker ownership` | `ownership.cert` | `check_all_sound` 0 | **EXISTS** |
| 2b | Name-totality + ACCEPT | `checker names` | `names.cert` | `check_names_cert_sound` 0 | **EXISTS** |
| 2c | Capability + ACCEPT | `checker caps`/`tcaps`. `main`={Stdout}, `pid_step`=∅ | `caps.cert` | reachable⊆declared: main's Stdout declared ⇒ **ACCEPT** | **EXISTS** (application's novel positive case: *declared* effect ACCEPTs) |
| 2d | Alloc-bound/counted-loop witness | `checker allocbound` ([flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)) | `<B>\|L16{…}` | `lrun` rederives `total≤B`; alloc-free body ⇒ trip-independent | **GATED keystone (あ)** (`CountedLoopStart` absent; `lib.rs:254` only has generic `LoopStart`) |
| 3 | Corpus-wall membership | `corpus-wall.sh` | wall report | `lower_function` total; all in-profile witnesses ACCEPT | **EXISTS** |
| 4 | Wasmtime execution | `render_program`→`.wat`→wasmtime | command sequence stdout | Runs, exit 0 | **PARTIAL GATED**: scalar+heap execution exists, actual `println`→`$print_str` hits discipline test `handwritten_wasm_runtime_does_not_grow` (baseline 11) = “v0 trap,” G-F0 frontier |
| 5 | Oracle byte identity (dual oracle vs v0) | v0 (`almide run --target wasm`) and `diff` (C-REPRO “until v1 parity”) | byte identity | v1 stdout == v0 stdout | **Gate mechanism EXISTS**; covers this application after stage 4 print path lands |
| 6 | Readable Rust render | `render_rust_program` ([flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)) | review-grade `.rs` | `cargo fmt` applied and human review passed | **GATED keystone (い)** (currently only a 368-line `Vec<i64>` demo) |
| 7 | Ferrocene compile | `ferrocene` rustc | Compiled with qualified toolchain | clean compile = object-code trust (bypass Gap-1) | **GATED keystone (い)** + Ferrocene access |

**Exist/gated summary**: stages **1, 2a-c, 3** are **green now** for the value-semantics + scalar-counted-loop subset. Stage **2d** (WCET claim) gated on keystone (あ). Stages **6-7** (Ferrocene leg) gated on keystone (い). Stages **4-5** = active **G-F0 frontier** (the *only* thing between “scalar/heap executes” and “print in the loop and match v0 bytes” is self-hosting `$print_str`).

---

<a id="3-原設計の-receipt-形現行出力は-receiptsh-を参照"></a>

## 3. Original receipt shape (see receipt.sh for current output)

`make receipt` (`receipt.sh`) folds checked facts into a C-SAFE/C-REPRO/C-FAITHFUL/C-PROVEN bundle. For `pid_control.almd` (extend the overall receipt with an app-scoped block):

```
# Receipt — Almide v1 flight reference app: pid_control.almd
# 再現: make verify-trust && make receipt
# 信頼基底と限界: proofs/TRUSTED_BASE.md

App: pid_control.almd — 単軸 Q16.16 PID 制御則、16 ステップ counted sim。
Subset: scalar fixed-point + counted range + 1 宣言 {Stdout} effect。

| claim       | meaning(this app)                                  | status  | evidence(段)                                              |
|-------------|----------------------------------------------------|---------|------------------------------------------------------------|
| C-PROVEN    | checker soundness は Coq kernel のみに依存          | PASS    | 45 定理・Print Assumptions closed・coqchk(段 2)           |
| C-SAFE/own  | pid_control に double-free/UAF なし                 | PASS    | 全 fn ownership witness ACCEPT(段 2a/3)                   |
| C-SAFE/name | call graph に dangling MIR 参照なし                 | PASS    | checker names ACCEPT(段 2b)                               |
| C-SAFE/cap  | main は宣言した {Stdout} のみ到達、PID 数学は pure  | PASS    | reachable⊆declared ACCEPT(段 2c)                          |
| C-WCET      | 総確保が静的有界、ループ alloc-free                 | PENDING | allocbound witness <B>|L16{…} を lrun ≤ B(段 2d) ── キーストーン(あ) |
| C-REPRO     | v1 stdout == v0 stdout、byte 一致                   | PARTIAL | dual-oracle diff(段 5)、$print_str discipline trap で blocked(段 4) |
| C-FAITHFUL  | render した Rust が MIR を refine、Ferrocene compile| PENDING | render_rust + rust_pattern 忠実 + Ferrocene(段 6-7) ── キーストーン(い) |

Falsification(否定空間、receipt-logic §5):
  C-SAFE  : render 成果物の UAF PoC 1 つ ⟹ false
  C-REPRO : v1≠v0 stdout の host 1 つ ⟹ false
  C-WCET  : B を超えて確保する実行 1 つ ⟹ false
  C-PROVEN: sorry / build 失敗 / 反例プログラム
```

Full translation of the preserved receipt example: title: Almide v1 flight reference app, pid_control.almd. Reproduce with make verify-trust && make receipt; trusted base and limits: proofs/TRUSTED_BASE.md. App: single-axis Q16.16 PID control law, 16-step counted simulation. Subset: scalar fixed-point + counted range + 1 declared {Stdout} effect.

- C-PROVEN: checker soundness depends only on the Coq kernel; PASS; 45 theorems, Print Assumptions closed, coqchk (stage 2).
- C-SAFE/own: no double-free/UAF in pid_control; PASS; all fn ownership witnesses ACCEPT (stages 2a/3).
- C-SAFE/name: no dangling MIR references in call graph; PASS; checker names ACCEPT (stage 2b).
- C-SAFE/cap: main reaches only declared {Stdout}, PID math pure; PASS; reachable⊆declared ACCEPT (stage 2c).
- C-WCET: total allocation statically bounded, loop alloc-free; PENDING; allocbound witness &lt;B&gt;|L16{…} with lrun ≤ B (stage 2d), keystone (あ).
- C-REPRO: v1 stdout == v0 stdout, byte identity; PARTIAL; dual-oracle diff (stage 5), blocked by $print_str discipline trap (stage 4).
- C-FAITHFUL: rendered Rust refines MIR, Ferrocene compile; PENDING; render_rust + faithful rust_pattern + Ferrocene (stages 6-7), keystone (い).

Falsification (negative space, receipt-logic §5): C-SAFE is false with one UAF PoC in the rendered artifact; C-REPRO is false with one host where v1≠v0 stdout; C-WCET is false with one execution allocating above B. C-PROVEN falsifiers: sorry / build failure / counterexample program. These translate the historical receipt, not fresh verdicts.

**What to show physically**: 60 lines of `.almd` (readable in 1 minute and recognizable as a PID control law) → 1 command `make verify-trust && make receipt` rederives the table **on the customer's machine** (CI outside the trusted base) → 4 green C-SAFE/C-PROVEN rows + honest C-WCET/C-FAITHFUL PENDING. **Honesty in not hiding PENDING is credibility.**

---

<a id="4-当初の-de-risking-順最小先行"></a>

## 4. Original de-risking order (minimum first)

- **Slice 0 — pure scalar counted-loop arithmetic, no print (green now)**. `pid_step` + accumulating driver (`fn sim() -> Int = { var acc=0; for t in 0..16 { acc = acc + pid_step(...) }; acc }`). Fully lowers now, ownership+names+caps (∅) pass, corpus wall passes, **scalar wasmtime execution matches v0**. Green stages: 1, 2a-c, 3, 4, 5. Natural home of the CRC-16 runner-up. **Minimum trustworthy flight-shaped artifact going end to end except print and the 2 keystones.**
- **Slice 1 — add `println(int.to_string(cmd))` loop (G-F0 frontier)**. Pulls in self-hosted `int.to_string` (`Alloc` `i`, ownership verification) + `$print_str`. Unblocks C-REPRO (stage 5), but first cross the `handwritten_wasm_runtime_does_not_grow` discipline trap (active G-F0). **Critical-path unlock**: as soon as print self-hosts, the full PID application runs and byte-matches v0; C-SAFE+C-REPRO become fully green.
- **Slice 2 — keystone (あ) unlocks C-WCET**. `CountedLoopStart{bound}`/`End` (`lib.rs:248`); `try_lower_scalar_for_range` emits with a literal bound + alloc-free-body wall; `alloc_bound_witness_string`, `CountedLoop.v`/`NoAllocInLoop.v`/`AllocBound.v` (Brick 0→1). Receipt C-WCET PENDING→PASS — “WCET-by-construction” becomes a *proved* claim over the real PID loop. **The row turning “memory-safe” into “*flight*-shaped.”**
- **Slice 3 — keystone (い) unlocks C-FAITHFUL + Ferrocene**. Production `render_rust_program`, port the faithful `rust_pattern` layer (proof is cheap; `eager_copy_refines_safety` already proved), render PID into review-grade Rust, compile with Ferrocene. C-FAITHFUL PENDING→PASS; receipt complete end to end.

---

<a id="5-当初のギャップ一覧printrendererreceipt-の後続状況は冒頭参照"></a>

## 5. Original gap inventory (see opening for later print/renderer/receipt status)

1. **Ferrocene compile + readable Rust (stages 6-7)** require a production MIR→Rust renderer (keystone (い)). Currently a 368-line `Vec<i64>` demo. C-FAITHFUL honestly PENDING. Proof cost low; engineering (≈5× render_wasm) is the real work.
2. **Alloc-bound/WCET claim (C-WCET)** requires keystone (あ). `CountedLoopStart` absent; checker currently flat-folds loops and proves nothing about trip count. Application currently *memory-safe inside loops*, but WCET/allocation bound is an *unproved claim*.
3. **Print path (C-REPRO end-to-end execution)** = active G-F0 frontier. Actual `println` runs and matches v0, but hits `handwritten_wasm_runtime_does_not_grow` (baseline 11) trap. Stages 4/5 blocked until clean `$print_str` self-host (no-print Slice 0 green).
4. **Functional correctness is the application's responsibility, not the certificate's**. Certificates prove the PID kernel memory-safe/name-total/cap-bounded/(after keystone (あ))WCET-bounded; **they prove nothing about correctness of the control law (stable gains/response)**. Pillar ③ (requirements traceability + MC/DC) out of scope. Receipt must not imply C-SAFE = “the controller is correct.”
5. **Capability vocabulary is Stdout only**. `eprintln`/abort/fs/net are unnamed host effects. PID stays within Stdout, so not a blocker for this app, but receipt must honestly scope C-SAFE/cap to “no undeclared **Stdout** effect.”
6. **Subset shape constrains the app**. Per-iteration heap-tuple reassignment DEFERRED; `i64::MIN` an unhandled `int.to_string` edge. The selected app keeps integral in scalar `var` to remain in-subset — design constraints to respect, not paper over.
