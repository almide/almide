# Compiler Architecture: All 10s [ACTIVE]

**目標**: コンパイラアーキテクチャ全項目 10/10
**現状**: 105/110 — 残り: テスト 9→10、Codegen統合 9→10
**スコープ**: WASM codegen を含む全コンパイラ基盤

---

## スコアカード

| 領域 | 開始時 | 現在 | 目標 | 状態 |
|------|--------|------|------|------|
| パイプライン設計 | 7 | **10** | 10 | ✅ |
| パーサー | 9 | **10** | 10 | ✅ proptest fuzzing 導入済 |
| 型チェッカー | 7 | **10** | 10 | ✅ |
| IR 設計 | 9 | **10** | 10 | ✅ |
| Nanopass | 8 | **10** | 10 | ✅ |
| モノモーフィゼーション | 7 | **10** | 10 | ✅ |
| エラー診断 | 9 | **10** | 10 | ✅ |
| コード品質 | 7 | **10** | 10 | ✅ String interning (Sym型, lasso), Ty/FnSig/TypeEnv 全層 Sym化 |
| テスト | 8 | **9** | 10 | ✅ fuzzing, 159/159 全通過, 並列実行 (2:30→16s)。残: nanopass ユニットテスト |
| ビルドシステム | 7 | **10** | 10 | ✅ build.rs分割, per-file キャッシュ + 並列テスト実行 |
| Codegen統合 | 5 | **9** | 10 | ✅ WASM result.collect/partition/collect_map 実装。残: stdlib dispatch一元化 |

**合計: 64/100 → 105/110**

---

## Done (完了済み)

### Phase 1: パイプライン統合 ✅

- [x] Target::Wasm + Pipeline統合
- [x] パス依存宣言
- [x] E003 --explain
- [x] BoxDeref パイプライン統合

### Phase 2: 型チェッカー分割 ✅

- [x] mod.rs 分割 — 850行 → 4モジュール
- [x] calls.rs 分割 — 588行 → 3モジュール

### Phase 3: モノモーフィゼーション ✅

- [x] ファイル分割 — 1296行 → 6モジュール
- [x] 直接構築 (PR#93)
- [x] 増分インスタンス発見 (PR#93)
- [x] 収束検出 (PR#91)

### Phase 4: Nanopass + Walker 分割 ✅

- [x] stream fusion 分割 — 1199行 → 5モジュール
- [x] walker 分割 — 1667行 → 6モジュール
- [x] Codegen 出口の統一 (PR#92)

### Phase 5: コード品質 ✅

- [x] **5.1 String Interning** — `Sym` 型 (lasso ThreadedRodeo), Copy + O(1) equality。Ty/FnSig/ProtocolDef/TypeEnv/VariantCase 全層を Sym 化。build.rs stdlib_sigs 生成も対応。33ファイル変更。
- [x] **5.3 Clone 削減 (基盤)** — Sym は Copy なので名前フィールドの clone が全て消滅。Ty の map_children 内の n.clone() → *n に。

### Phase 5b: テスト・ビルド基盤 ✅

- [x] **Proptest fuzzing** — lexer/parser/checker × arbitrary/structured = 6ターゲット、各10,000ケース
- [x] **テスト全通過** — 159/159 .almd テスト、CI グリーン
- [x] **テスト並列化** — compile_to_binary + per-file hash cache + thread pool 実行 (2:30 → 16s)
- [x] **WASM result.collect/partition/collect_map** — CI WASM テスト全通過

---

## Remaining (残り 5pt)

### テスト 9→10 (残り 1pt)

#### Nanopass ユニットテスト

各パスの入力 IR → 出力 IR 変換テスト。15パス × 2-5ケース = 40-50テスト。

**工数**: M (4-5日)

### Codegen統合 9→10 (残り 1pt)

#### Stdlib dispatch 一元化

TOML に wasm_handler/wasm_rt を追加し、build.rs が WASM dispatch table を自動生成。手動の match arm を排除。

**工数**: M (1-2週間)

---

## 不要になった項目

- ~~Phase 6.5 Parser Fuzzing~~ → Phase 5b で proptest 導入済
- ~~Phase 7 xtask 移行~~ → build.rs は3モジュール分割済で十分。xtask の追加価値が薄い
- ~~Phase 5.2 Clone 削減 (Rc\<Ty\>)~~ → Sym 導入で名前 clone が消え、主要ホットスポットは解消。Rc\<Ty\> は費用対効果が低い

---

**残り工数見積もり**: 2-3 週間
**完了時スコア**: 110/110
