<!-- description: MLIR backend + egg e-graph rewriter for pure-Almide optimal lowering -->
# MLIR Backend + Egg Rewrite Engine

## Decision

Almide は以下の 2 本柱で「世界最高の dispatch 基盤」を構築する:

1. **egg (e-graph) ベースの equality saturation rewriter** を Almide dialect レベルで適用する front-end optimizer
2. **MLIR progressive lowering** を back-end に採用し、Rust/WASM/native/GPU 多ターゲットを formal に扱う

この arc の前段として Stdlib Declarative Unification arc (`active/stdlib-declarative-unification.md`) を完遂する。宣言ルールと typed intrinsic はそのまま e-graph rewrite ルール / MLIR FunctionImport に変換されるため、投資は無駄にならない。

## 全体アーキテクチャ

```
Source (.almd)
     ↓
AST + Type Check (既存)
     ↓
Almide IR (既存、強化)
     ↓
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Front-end optimizer: egg e-graph equality saturation
  - 宣言的 rewrite rule (@rewrite attribute から自動変換)
  - algebraic identities, stream fusion, monad laws
  - cost function (target-aware: Rust/WASM/GPU で重みが違う)
  - 等価クラス全探索 → 最適形を extract
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     ↓
Almide dialect (MLIR)
     ↓
  ┌────────── Rust emit (direct, LLVM 経由しない) ──→ Rust source
  │
  └─ Progressive lowering (MLIR pass manager)
       ↓ affine / arith / memref / scf dialects
       ↓ LLVM dialect  ──→ native binary
       ↓ wasm dialect  ──→ WebAssembly binary
       ↓ SPIR-V / NVVM ──→ GPU kernel
```

**重要な境界**: egg は Almide dialect 以下 (高レベル) で働く。MLIR pass は Almide dialect からの lowering を担当。役割が明確に分離される。

## なぜ egg + MLIR の組合せか

### egg 単体ではダメな理由

- egg は rewrite engine だけ。target specific な backend 最適化 (vectorize, inline, register alloc) は LLVM に任せたい
- GPU codegen を egg で書くのは現実的でない (SPIR-V 生成は既存ツールチェインに乗る)

### MLIR 単体ではダメな理由

- MLIR の PDL (Pattern Description Language) は priority-ordered rewriting。equality saturation (**全等価形を同時保持し、最後に最適形を選ぶ**) ができない
- confluence 問題 / phase ordering 問題が残る ("先に fusion か、先に inline か") — これを egg は構造的に解決する

### 両者を組み合わせる理由

- egg = 最適**形**の発見 (代数的探索)
- MLIR = 最適**コード**の生成 (target lowering + LLVM 連携)
- この分業で「pure Almide で書かれたコードが、全 target で最適形に落ちる」が初めて可能に

### 先行事例

- **Cranelift ISLE**: 命令選択に egg 式の e-graph を使用 (Rust native)
- **Herbie**: 数値式の等価性探索に egg 採用
- **egglog / eggcc**: e-graph + datalog で compiler 最適化を試みる研究
- **torch-mlir**: MLIR で PyTorch を multi-target lowering

Almide が目指すのは「**egg + MLIR の統合を言語コンパイラとして最初に本気で形にする**」ポジション。研究と実用の交差点。

## Rust source output: Option B 採択

### 選択肢の比較

**Option A**: Rust source 出力を捨て、MLIR → LLVM → native/WASM に一本化
- 失うもの: 「Almide が読める Rust を吐く」信頼感、Cargo ecosystem との source-level 統合
- 得るもの: MLIR の power 全開、複雑性が減る

**Option B (採択)**: Rust emit は Almide dialect から直接 (LLVM 経由しない)
- Rust target 用の emit path は Almide dialect → Rust source walker として維持
- WASM / native / GPU は MLIR progressive lowering
- 複雑性: emit path が 2 本になる
- 利点: 既存 trust + debuggability を失わない

Rust source output は Almide のミッション「LLM が最も正確に書ける言語」と integral。LLM/ユーザーが emit 結果を読める信頼感、Rust crate との source-level interop、cargo という shared infrastructure — これらを失う代償は egg + MLIR の統一美学より重い。

## サブステージ (Stage 1-4)

### Stage 1 — egg rewriter + Almide dialect (3 ヶ月 / `0.16.0`)

**egg rewriter の導入** (pure Rust、FFI 不要):

- `egg` crate を dep に追加
- Almide IR を egg の `Language` trait に expose
- Stdlib Unification arc で蓄積された `@rewrite` 宣言を egg rule にコンパイル
- cost function v1: node count + target hint
- equality saturation を既存 Nanopass の先頭に挿入 (opt-in: `--opt egg`)
- 既存 StreamFusionPass を egg rule 化 → 動作等価確認

**MLIR dialect 定義**:

- `melior` crate で MLIR C API を Rust から叩く (C++ 依存は melior にカプセル化)
- Almide dialect を builder API で定義 (TableGen は後回し):
  - ops: `almide.call`, `almide.let`, `almide.match`, `almide.variant`, `almide.record`, `almide.lambda`
  - types: `AlmideList<T>`, `AlmideOption<T>`, `AlmideResult<T, E>`, `AlmideRecord<...>`, `AlmideClosure<...>`
- Almide IR → Almide dialect の変換
- MLIR verifier を動作確認 (Postcondition::Custom を置換)

**既存パスとの共存**: imperative Nanopass は並走保持、新経路は opt-in flag

### Stage 2 — Progressive lowering + Rust emit 再配線 (2 ヶ月 / `0.16.x`)

- Almide dialect → arith / scf / memref への lowering 実装
- Rust emit 経路を Almide dialect から直接生成するよう書き換え (Option B)
- typed intrinsic を MLIR FunctionImport に mapping
- schedule DSL (Stdlib Unification で導入) を affine dialect attribute に変換
- spec/ 全通過、regression 0 を Rust target で確認

### Stage 3 — WASM / native / GPU (3 ヶ月 / `0.17.0`)

- LLVM dialect → WASM backend (emscripten free、pure wasi)
  - 既存 direct WASM emit との性能比較ベンチマーク
  - direct emit は runtime が軽量 (~10KB) なので hybrid 保持の判断は残す
- LLVM dialect → native binary (Linux/macOS/Windows の AOT)
- **初の GPU PoC**: SPIR-V または NVVM dialect で `matrix.multiply` を offload
  - schedule DSL が `@schedule(device=gpu)` として機能
  - `almide build --target cuda matrix_kernel.almd` を experimental flag で公開

### Stage 4 — 安定化と最適化 (2 ヶ月 / `0.17.x`)

- egg の cost function を profile-guided に拡張 (PGO)
- LLVM の標準最適化 (inline, LICM, vectorize) を default pipeline に組み込み
- 既存 imperative Nanopass を全廃 (全て egg rule + MLIR pattern に移行済)
- dojo MSR で arc 前後の差分測定、regression 0 確認
- `--opt egg` flag を default 化

合計 **10 ヶ月**の本気コミット。0.15.0 で Stdlib Unification 完了、0.16.0 で egg + MLIR 基盤、0.17.0 で GPU PoC。

## Stdlib Unification arc との関係

この arc は [stdlib-declarative-unification.md](./stdlib-declarative-unification.md) の **投資回収段階**でもある:

| Stdlib Unification アウトプット | 本 arc での扱い |
|---|---|
| `stdlib/<m>.almd` の pure Almide body | Almide dialect への入力 |
| typed intrinsic (`@intrinsic(rust=..., wasm=...)`) | MLIR FunctionImport として表現 |
| `@rewrite` declarative rule | egg rewrite rule に**自動コンパイル** |
| `@schedule` block | affine dialect schedule attribute に変換 |

つまり Stdlib Unification arc は本 arc の **入力形式の整備**。skip は可能だが、stdlib が 3 層定義のままだと MLIR 移植工数が 2 倍になる。順序は動かさない。

## 非目標

- **Rust borrow checker を MLIR で再現しない**。Rust emit path は rustc に借用検査を任せる
- **MLIR ecosystem 全取り込みはしない**。TensorFlow dialect / Torch-MLIR / CIRCT 等は当面スコープ外
- **LLVM IR を Almide のユーザー向け surface にしない**。LLVM dialect は内部のみ
- **egg を frontend compile に露出させない**。ユーザーは rewrite rule を `@rewrite` で書くだけ、egg は内部エンジン

## 未決事項

1. **egg の Language trait 設計**: Almide IR を egg node にどう埋めるか。`Expr` を単一型にするか、node kind ごとに split するか
2. **cost function v1 の形**: ノード数 + target 係数から開始、profile-guided は Stage 4 送り。中間で user-defined cost を許すか
3. **TableGen vs builder API**: Almide dialect 定義。初期は builder API (Rust で完結) が開発速い、安定後に TableGen 移行の可否を再評価
4. **WASM direct emit の将来**: Stage 3 で LLVM-WASM と性能比較後、hybrid 保持か完全置換かを決定
5. **GPU runtime 第一 target**: CUDA / ROCm / Vulkan / WebGPU。WebGPU は WASM 延長で自然だが ecosystem 成熟度が低い
6. **equality saturation の termination**: egg は iteration limit ベース。Almide では実測で timeout がどれだけ許容されるか要観測
7. **Incremental compilation との統合**: salsa-style と MLIR pass manager の境界設計

## 成功判定

- `0.17.0` で `almide build --target cuda matrix_multiply.almd` が動作
- Rust target / WASM target の regression は 0 (spec/ 全通過、dojo MSR 変動なし)
- stdlib の三層定義が 0 (Stdlib Unification 完了と合わせて)
- `cargo build --release` の compilation speed が現状比 ±20% 以内 (egg + MLIR 経由でも劣化しない)
- egg equality saturation が既存 StreamFusion より厳密に多くの最適形を発見する (ベンチで実証)

## スケジュール

```
0.14.x (現在)  — 前段完了 (v0.14.7 Dispatch Ideal Form)、Stdlib Unification 準備
0.15.0         — Stdlib Unification 完了
0.16.0         — Stage 1 (egg + Almide dialect 基盤、opt-in)
0.16.x         — Stage 2 (progressive lowering + Rust emit 再配線)
0.17.0         — Stage 3 (LLVM WASM / native / GPU PoC)
0.17.x         — Stage 4 (PGO、imperative pass 全廃、egg default 化)
1.0 前提       — 本 arc 完走
```

本 arc 完走時点で Almide は「**pure Almide で書いて、egg で最適形に整理され、Rust/WASM/native/GPU どこにでも最適コードで落ちる**」コンパイラになる。これが「世界最高の理想形」の dispatch + 最適化側の到達点。
