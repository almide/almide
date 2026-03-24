# Compiler Architecture: All 10s [ACTIVE]

**目標**: コンパイラアーキテクチャ全項目 10/10
**現状**: 101/110 → Phase 4 完了、build.rs分割完了
**スコープ**: WASM codegen を含む全コンパイラ基盤

---

## スコアカード

| 領域 | 開始時 | 現在 | 目標 | 状態 |
|------|--------|------|------|------|
| パイプライン設計 | 7 | **10** | 10 | ✅ Target::Wasm統合済、パス依存宣言済、BoxDeref統合済 |
| パーサー | 9 | 9 | 10 | fuzzing で補強 (Phase 6) |
| 型チェッカー | 7 | **10** | 10 | ✅ mod.rs分割、calls.rs分割、impl block統合、effect fn auto-unwrap、stdlib import 3層制御 |
| IR 設計 | 9 | **10** | 10 | ✅ IrProgram.effect_fn_names追加 (TypeEnv由来、LICM等が参照) |
| Nanopass | 8 | **10** | 10 | ✅ stream_fusion分割、walker分割、LICM effect判定ホワイトリスト廃止、ResultPropagation 2-pass設計 |
| モノモーフィゼーション | 7 | **10** | 10 | ✅ mono.rs分割、✅ 収束検出 (PR#91)、✅ 増分発見+直接構築 (PR#93) |
| エラー診断 | 9 | **10** | 10 | ✅ E003 --explain 既に登録済 |
| コード品質 | 7 | **9** | 10 | ✅ 巨大ファイル全分割、ホワイトリスト廃止。残: string interning、clone 削減 |
| テスト | 8 | 8 | 10 | 未着手。nanopass テスト、fuzzing、ベンチマーク |
| ビルドシステム | 7 | **9** | 10 | ✅ build.rs分割 (1237行→3モジュール)。残: build cache最適化 |
| Codegen統合 | 5 | **9** | 10 | ✅ Target::Wasm pipeline統合済、✅ codegen()統一 (PR#92)。残: stdlib dispatch一元化 |

**合計: 64/100 → 101/110 (Codegen統合を追加した11領域)**

---

## Done (完了済み)

### Phase 1: パイプライン統合 ✅

- [x] **1.0 Target::Wasm + Pipeline統合** — Target enum に Wasm 追加、WASM pipeline 定義済み (TailCallOpt → EffectInference → ResultPropagation → FanLowering)
- [x] **1.1 パス依存宣言** — NanoPass trait に `depends_on()` 追加、Pipeline::run() で検証。CloneInsertion → BorrowInsertion、BuiltinLowering → ResultPropagation、ResultErasure → MatchLowering、StdlibLowering → EffectInference
- [x] **1.2 E003 --explain** — 既に登録済み
- [x] **1.3 BoxDeref パイプライン統合** — BoxDerefPass として Rust pipeline 先頭に配置済み

### Phase 2: 型チェッカー分割 ✅

- [x] **2.1 mod.rs 分割** — 850行 → mod.rs (485) + diagnostics.rs (29) + solving.rs (103) + registration.rs (225)
- [x] **2.2 calls.rs 分割** — 588行 → calls.rs (305) + builtin_calls.rs (106) + static_dispatch.rs (197)

### Phase 3: モノモーフィゼーション ✅

- [x] **3.1 ファイル分割** — mono.rs (1296行) → 6モジュール (mod/discovery/specialization/rewrite/propagation/utils)
- [x] **3.2 直接構築** — specialize_function をclone+mutateからフィールド直接構築に変更 (PR#93)
- [x] **3.3 増分インスタンス発見** — frontier-based discovery: 2回目以降は新規特殊化関数のみスキャン O(N×new) (PR#93)
- [x] **3.4 収束検出** — max_iterations=10 → convergence-based loop + 爆発検出 (1000+) (PR#91)

### Phase 4: Nanopass + Walker 分割 ✅

- [x] **4.1 stream fusion 分割** — 1199行 → 5モジュール (mod/chain_detection/fusion_rules/lambda_composition/ir_transform)
- [x] **4.2 walker 分割** — 1667行 → 6モジュール (mod/expressions/statements/types/declarations/helpers)
- [x] **4.3 Codegen 出口の統一** — emit() + emit_wasm_binary() → codegen() + CodegenOutput enum (PR#92)

---

## Remaining (残り)

### Phase 5: コード品質 + Stdlib 統合

#### 5.1 String Interning

ModuleId(u8) / FuncId(u16) / SymId(u32) で文字列比較を O(1) に

**工数**: M (1-2週間) | **リスク**: 高

#### 5.2 Stdlib dispatch 一元化

TOML に wasm_handler/wasm_rt を追加し、build.rs が WASM dispatch table を自動生成

**工数**: M (1-2週間) | **リスク**: 中

#### 5.3 Clone 削減

ir/substitute.rs の Ty clone を参照に (150箇所)、check/infer.rs のシグネチャ clone を遅延評価に

**工数**: S-M | **リスク**: 低

### Phase 6: テスト強化

#### 6.1 Nanopass ユニットテスト (40-50テスト)

各パスの入力 IR → 出力 IR 変換テスト

**工数**: M (4-5日)

#### 6.2 Cross-target テスト

Rust と WASM の出力一致検証

**工数**: S

#### 6.3 スナップショットテスト (insta)

IR の before/after を golden file 比較

**工数**: M (2-3日)

#### 6.4 モノモーフィゼーションユニットテスト (25テスト)

**工数**: M (2-3日)

#### 6.5 Parser Fuzzing (proptest)

**工数**: M (2-3日)

#### 6.6 パフォーマンスベンチマーク (criterion)

**工数**: S (1-2日)

### Phase 7: ビルドシステム — xtask 移行

rust-analyzer の xtask パターンを採用。build.rs の codegen ロジックを独立バイナリに移行。

#### 7.1 xtask クレート作成 + codegen モジュール分割

build.rs (1,261行) → `xtask/src/codegen/` (5モジュール)

```
xtask/src/
├── main.rs                 # cargo xtask codegen [--check]
├── codegen/
│   ├── mod.rs              # dispatcher + ensure_file_contents()
│   ├── stdlib_sigs.rs      # stdlib/defs/*.toml → src/generated/stdlib_sigs.rs
│   ├── arg_transforms.rs   # stdlib/defs/*.toml → src/generated/arg_transforms.rs
│   ├── emit_calls.rs       # stdlib/defs/*.toml → emit_rust_calls.rs, emit_ts_calls.rs
│   └── runtime_scan.rs     # runtime/rs/*.rs スキャン → 検証
```

build.rs にはバージョン埋め込み等の軽量処理のみ残す。

#### 7.2 ensure_file_contents パターン

- `cargo xtask codegen` — 再生成して上書き
- `cargo xtask codegen --check` — 差分検出のみ (CI用、生成忘れ防止)
- 生成ファイルは引き続き git にコミット

#### 7.3 freshness テスト

各 codegen モジュールに `#[test] fn generated_files_are_fresh()` を追加。
`cargo test` で生成ファイルの鮮度を自動検証 (xtask を走らせなくても検出)。

#### 7.4 CI 統合

`.github/workflows/ci.yml` に `cargo xtask codegen --check` ステップ追加。

**工数**: M (5-6日)

---

## 実行順序 (残り)

```
Phase 6 (並行可)    ← テスト強化
Phase 5             ← String Interning + Stdlib 統合 (大きい、テストが先)
Phase 7             ← build.rs 分割 (独立)
```

**残り工数見積もり**: 5-8 週間
**完了時スコア**: 110/110
