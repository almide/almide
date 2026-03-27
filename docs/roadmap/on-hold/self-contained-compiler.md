<!-- description: Emit LLVM IR directly to eliminate rustc dependency for end users -->
# Self-Contained Compiler: Remove rustc Dependency [ACTIVE]

**目標**: `almide build` が rustc を必要としない。Go のように自己完結したコンパイラ。

```
現在:  almide build → Rust ソース生成 → rustc → LLVM → binary (rustc 必須)
目標:  almide build → LLVM IR 生成 → LLVM → binary (rustc 不要)
```

---

## Stage 1: ユーザーが rustc 不要

Almide コンパイラ自体は Rust で書かれたまま。ビルド時に Rust ランタイムを LLVM bitcode に焼き込み、ユーザーのコンパイル時には rustc を呼ばない。

### Architecture

```
Almide ビルド時 (cargo build):
  runtime/rs/src/*.rs → rustc --emit=llvm-bc → runtime.bc → embed in almide binary

ユーザーのコンパイル時 (almide build):
  .almd → Almide IR → (nanopass pipeline) → LLVM IR
  LLVM IR + embedded runtime.bc → llvm-link → opt → llc → binary
```

### 実装ステップ

| Step | 内容 | 依存 |
|------|------|------|
| 1. LLVM IR emitter | Almide IR → LLVM IR テキスト（基本型: Int, Float, Bool, if/for/call） | inkwell crate |
| 2. Runtime bitcode | `build.rs` で `rustc --emit=llvm-bc` してランタイムを bitcode 化 | 既存 runtime/ |
| 3. Bitcode 埋め込み | `include_bytes!` でランタイム bitcode を Almide バイナリに内蔵 | Step 2 |
| 4. LLVM リンク + 最適化 | inkwell で bitcode 結合 → opt → 実行可能バイナリ生成 | Steps 1-3 |
| 5. Stdlib dispatch | TOML テンプレート → LLVM IR call 生成（`almide_rt_*` 関数への call） | Step 1 |
| 6. Rust ターゲットとの共存 | `--backend llvm` / `--backend rust` で選択可能に | Step 4 |

### 技術的課題

- **型の ABI**: String (`{ ptr, len, cap }`), Vec, HashMap の LLVM レベル表現。ランタイムが Rust で書かれているので ABI は Rust ABI → bitcode に含まれる
- **ジェネリクス**: `Vec<i64>` と `Vec<String>` で monomorphized された別の関数が必要。ランタイム側で主要な型のインスタンスを事前生成するか、ユーザーコード側で生成
- **Drop / デストラクタ**: LLVM IR レベルで適切なタイミングで drop を呼ぶ必要がある
- **LLVM バージョン**: inkwell が依存する LLVM バージョンと、ユーザー環境の互換性

### 見積もり

- プロトタイプ（Int/Float + 四則演算 + if/for）: 2-3 週間
- 基本動作（String/List/Map + stdlib 主要関数）: 1-2 ヶ月
- 本番品質（全 stdlib + エラーハンドリング + テスト）: 3-4 ヶ月

### 得られるもの

| メリット | 効果 |
|---------|------|
| **ユーザー体験** | `cargo install almide` だけで完結。rustc インストール不要 |
| **コンパイル速度** | rustc フロントエンド (50-70% of compile time) をスキップ |
| **LLVM アノテーション** | pure → `readonly`/`willreturn`、immutable → `noalias` を直接付与 |
| **配布サイズ** | Almide 単体で配布可能（rustc + cargo 不要） |

---

## Stage 2: Almide 自体を Almide で書く (Self-Hosting)

コンパイラ自体を Almide で書き直す。Rust 依存を完全に除去。

### 前提条件

- Stage 1 完了（LLVM 直接出力が動く）
- Almide の言語機能が十分成熟（ジェネリクス、trait/protocol、ファイルI/O、文字列処理）
- 十分なテストカバレッジ（コンパイラの正しさを保証）

### 段階的移行

1. **lexer.rs → lexer.almd**: 文字列処理中心、依存が少ない
2. **parser/ → parser/**: 再帰下降、データ構造操作
3. **check/ → check/**: 型推論、最も複雑
4. **lower/ → lower/**: IR 変換
5. **codegen/ → codegen/**: LLVM IR 生成
6. **ブートストラップ**: 古い Almide コンパイラで新しい Almide コンパイラをコンパイル

### 見積もり

- 1-2 年（Stage 1 完了後）
- 言語の安定化が先決

---

## 優先度

Stage 1 >> Stage 2

Stage 1 はユーザー体験とコンパイル速度に直結。Stage 2 は技術的達成だが実用上の優先度は低い。
