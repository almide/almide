<!-- description: Flight gate G-F4 — the reference application (a fixed-point Q16.16 PID control-law kernel over a counted sim loop) that passes `make verify` end-to-end, the 7-stage verify pipeline (exist vs gated on keystones), the receipt it emits (C-SAFE/C-PROVEN green, C-WCET/C-FAITHFUL pending), and the de-risking order (Slice 0 scalar-no-print green now → Slice 1 print=G-F0 frontier → Slice 2 keystone-あ unlocks C-WCET → Slice 3 keystone-い unlocks Ferrocene). -->
# Flight Reference App — PID 制御則カーネル + make verify + receipt(G-F4)

> **Goal**: [flight-profile](flight-profile.md) ラダー **G-F4** の具体設計 ── 飛行プロファイルで
> 書かれ `make verify` を端まで通る**リファレンスアプリ**(= スライドを成果物に変える「弾」)。
> 走る → oracle byte 一致 → 毎ビルド証明書 → 可読 Rust → Ferrocene。飛行サブセット
> ([flight-subset-spec](flight-subset-spec.md))内に収まること。
> **関連**: [flight-profile](flight-profile.md) §7.2 / [receipt-logic](receipt-logic.md) / [flight-wcet-loops](flight-wcet-loops.md) / [flight-rust-ferrocene](flight-rust-ferrocene.md)。

---

## 1. リファレンスアプリ ── 採用:Fixed-Point PID 制御則カーネル

`pid_control.almd` ── 単軸 PID コントローラを整数 fixed-point(Q16.16)で、**有界 counted
シミュレーションループ**(N ステップ)で回し、各ステップの飽和アクチュエータ指令を print。
ラダーが G-F4 の例示に挙げる「制御則カーネル」そのもの([flight-profile](flight-profile.md) §7.2)。

**採用理由(基準ごと)**:
- **(a) 安全臨界に見える**:PID + 飽和 + anti-windup は航空/自動車制御で最も普遍的な形
  (FADEC・舵面アクチュエータ・ECU トルク)。cert エンジニアが「制御則」を即座に DO-178C/
  ISO 26262 のメンタルモデルに写す。CRC(=utility に見える)や ring buffer(=plumbing)より強い。
- **(b) 飛行サブセットに収まる**:全スカラ Int(fixed-point)、hot path に動的確保なし、ループ
  本体に再帰なし、driver は counted range `for t in 0..N` ── まさに `try_lower_scalar_for_range`
  (`crates/almide-mir/src/lower/control.rs:766`)が下す形。PID `step` は pure 算術 `fn`(heap/caps
  なし)、driver の `println(int.to_string(cmd))` のみ Stdout 到達。
- **(c) 小さいが実物**:~60-80 行。Q16.16 乗算(`/ 65536` で rescale)、飽和(`math.max`/`math.min`、
  self-host 済・pure)、積分 anti-windup(accumulator clamp)。制御エンジニアが全項を認識。
- **(d) 証明書を端まで行使**:全証明性質に触れる ── **ownership**(各 `int.to_string` が 1 `Alloc`
  `i`、`println` に move-out、drop)、**name totality**(driver→`pid_step`→`clamp` の call graph)、
  **capability bound**(driver が Stdout を**宣言**して到達 ⇒ ACCEPT、積分/微分 math は pure ∅)、
  **counted loop** = WCET キーストーンの最初の実 witness 対象。

**構造スケッチ**(`docs/CHEATSHEET.md` + self-host 面に接地):

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

*(counted scalar ループ内の scalar `var` 再代入は `scalar_loop_depth` 許容形(`control.rs:816`)。
`(cmd, integral)` の per-iteration heap-tuple co-return は DEFERRED サブセットなので integral を
scalar `var` に保つ ── §5 参照。)*

**Runner-up: CRC-16/CCITT**(`crc16.almd`)── 固定バイト表上の counted ループで CRC レジスタを
shift/XOR fold。**より trivially in-subset**(`list.sum` の反復形そのもの)で**最小スライスの種**
(§4)だが、cert エンジニアには「utility」に見える。**最小先行**用に残し、デモは PID で先導。

---

## 2. 端から端の make verify パイプライン(exist vs gated)

CI Trust Spine は `make verify-trust`→`make receipt`(`Makefile:77-84`)。本アプリの段:

| # | 段 | コマンド/ツール | 産出 | "pass" | 状態 |
|---|---|---|---|---|---|
| 1 | source→IR→MIR | `render_program` example(`examples/render_program.rs:28-50`) | per-fn MIR | 全 fn `Ok`(in-subset)、walled ゼロ | **EXISTS** |
| 2a | ownership 証明書 + ACCEPT | `gate.sh`: `emit_cert_from_source`→`checker ownership` | `ownership.cert` | `check_all_sound` 0 | **EXISTS** |
| 2b | name-totality + ACCEPT | `checker names` | `names.cert` | `check_names_cert_sound` 0 | **EXISTS** |
| 2c | capability + ACCEPT | `checker caps`/`tcaps`。`main`={Stdout}、`pid_step`=∅ | `caps.cert` | reachable⊆declared:main の Stdout は宣言済 ⇒ **ACCEPT** | **EXISTS**(アプリの novel positive case:*宣言された* effect が ACCEPT) |
| 2d | alloc-bound/counted-loop witness | `checker allocbound`([flight-wcet-loops](flight-wcet-loops.md)) | `<B>\|L16{…}` | `lrun` が `total≤B` 再導出、本体 alloc-free ⇒ trip 非依存 | **GATED キーストーン(あ)**(`CountedLoopStart` 未在、`lib.rs:254` は generic `LoopStart` のみ) |
| 3 | corpus wall 所属 | `corpus-wall.sh` | wall report | `lower_function` 全域、in-profile witness 全 ACCEPT | **EXISTS** |
| 4 | wasmtime 実行 | `render_program`→`.wat`→wasmtime | 指令列 stdout | 走る、exit 0 | **PARTIAL GATED**:scalar+heap 実行在、実 `println`→`$print_str` が discipline test `handwritten_wasm_runtime_does_not_grow`(baseline 11)に当たる = 「v0 trap」、G-F0 frontier |
| 5 | oracle byte 一致(dual oracle vs v0) | v0(`almide run --target wasm`)と `diff`(C-REPRO「until v1 parity」) | byte 一致 | v1 stdout == v0 stdout | **gate 機構 EXISTS**、段 4 の print path 着地後に本アプリ被覆 |
| 6 | 可読 Rust render | `render_rust_program`([flight-rust-ferrocene](flight-rust-ferrocene.md)) | review-grade `.rs` | `cargo fmt` 済が人間 review 通過 | **GATED キーストーン(い)**(今 368 行 `Vec<i64>` demo のみ) |
| 7 | Ferrocene compile | `ferrocene` rustc | 資格化 toolchain で compile | clean compile = object-code 信頼(Gap-1 迂回) | **GATED キーストーン(い)** + Ferrocene access |

**exist/gated 要約**:段 **1・2a-c・3** は値意味論+scalar-counted-loop サブセットで**今 green**。
段 **2d**(WCET 主張)= キーストーン(あ)gated。段 **6-7**(Ferrocene leg)= キーストーン(い)gated。
段 **4-5** = 能動的 **G-F0 frontier**(「scalar/heap が走る」と「ループで print し v0 byte 一致」の
間の*唯一*の物 = `$print_str` self-host)。

---

## 3. receipt ── 見せる成果物

`make receipt`(`receipt.sh`)が checked facts を C-SAFE/C-REPRO/C-FAITHFUL/C-PROVEN バンドルに畳む。
`pid_control.almd` 用(全体 receipt を app-scoped ブロックで拡張):

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

**物理的に見せるもの**:60 行の `.almd`(1 分で読め PID 制御則と認識)→ 1 コマンド
`make verify-trust && make receipt` が**顧客マシンで**(CI は信頼基底外)上表を再導出 → 4 つの green
C-SAFE/C-PROVEN 行 + C-WCET/C-FAITHFUL の正直な PENDING。**PENDING を隠さない正直さが信頼性。**

---

## 4. De-risking 順(最小先行)

- **Slice 0 ── pure scalar counted-loop 算術、print なし(今 green)**。`pid_step` + 累積 driver
  (`fn sim() -> Int = { var acc=0; for t in 0..16 { acc = acc + pid_step(...) }; acc }`)。今フル lower、
  ownership+names+caps(∅)pass、corpus wall pass、**scalar wasmtime 実行が v0 一致**。green 段:1・2a-c・3・4・5。
  CRC-16 runner-up の自然な home。**print と 2 キーストーンを除き端まで行く最小の信頼できる飛行形成物。**
- **Slice 1 ── `println(int.to_string(cmd))` ループ追加(G-F0 frontier)**。self-host `int.to_string`
  (`Alloc` `i`、ownership 検証)+ `$print_str` を引く。C-REPRO(段 5)を unblock ── だが先に
  `handwritten_wasm_runtime_does_not_grow` discipline trap を越える(能動的 G-F0)。**クリティカルパス
  unlock**:print が self-host した瞬間、フル PID アプリが走り v0 byte 一致、C-SAFE+C-REPRO がフル green。
- **Slice 2 ── キーストーン(あ)が C-WCET を unlock**。`CountedLoopStart{bound}`/`End`(`lib.rs:248`)、
  `try_lower_scalar_for_range` がリテラル bound + alloc-free-body 壁で emit、`alloc_bound_witness_string`、
  `CountedLoop.v`/`NoAllocInLoop.v`/`AllocBound.v`(Brick 0→1)。receipt の C-WCET が PENDING→PASS ──
  「WCET-by-construction」が実 PID ループ上の*証明済み*主張に。**「memory-safe」を「*flight*-shaped」に変える行。**
- **Slice 3 ── キーストーン(い)が C-FAITHFUL + Ferrocene を unlock**。本番 `render_rust_program`、
  `rust_pattern` 忠実層 port(証明は安い、`eager_copy_refines_safety` 既証明)、PID を review-grade Rust に
  render、Ferrocene compile。C-FAITHFUL が PENDING→PASS、receipt が端まで完成。

---

## 5. 正直なギャップ(まだ実証できないもの)

1. **Ferrocene compile + 可読 Rust(段 6-7)** = 本番 MIR→Rust renderer(キーストーン(い))要。今は
   368 行 `Vec<i64>` demo。C-FAITHFUL は正直に PENDING。証明コストは低い、エンジニアリング(≈5× render_wasm)が真の作業。
2. **alloc-bound/WCET 主張(C-WCET)** = キーストーン(あ)要。`CountedLoopStart` 未在、checker は今ループを
   flat fold し trip count を何も証明しない。今アプリは*ループ内 memory-safe* だが WCET/確保上限は*未証明主張*。
3. **print path(C-REPRO 端まで実行)** = 能動的 G-F0 frontier。実 `println` が走り v0 一致するが
   `handwritten_wasm_runtime_does_not_grow`(baseline 11)trap。`$print_str` clean self-host まで段 4/5 blocked
   (print なし Slice 0 は green)。
4. **機能正しさはアプリの責務、証明書のでない**。cert は PID カーネルが memory-safe/name-total/cap-bounded/
   (キーストーン(あ)後)WCET-bounded を証明、**制御則が正しい(gain/応答が安定)かは何も証明しない**。柱③
   (要求トレーサ + MC/DC)= 範囲外。receipt は C-SAFE ≠「コントローラが正しい」を含意してはならない。
5. **capability 語彙は Stdout のみ**。`eprintln`/abort/fs/net は未命名の host effect。PID は Stdout 内なので
   本アプリの blocker でないが、receipt は C-SAFE/cap を「宣言外の **Stdout** effect なし」と正直に scope すべき。
6. **サブセット形がアプリを制約**。per-iteration heap-tuple 再代入は DEFERRED、`i64::MIN` は `int.to_string` の
   未処理 edge。採用アプリは integral を scalar `var` に保ち in-subset を維持 ── paper over せず尊重すべき設計制約。
