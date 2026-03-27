<!-- description: Rewrite the compiler in Almide for a self-contained 350KB WASM toolchain -->
# Self-Hosting: Autonomous Bootstrap Compiler

**Status**: On Hold (Phase 3+ prerequisite)
**Priority**: Strategic — ミッション直結
**Prerequisite**: 言語仕様安定, WASM direct emit 完成, Protocol/Generics 成熟

## なぜやるのか

Almide のミッションは **modification survival rate** — LLM が最も正確に書ける言語であること。

セルフホスティングはこのミッションの論理的帰結になる:

1. **コンパイラのソースが Almide で書かれている** → LLM が最も正確に読み書きできるコンパイラになる
2. **WASM direct emit で 350KB 級の全部入りバイナリが出せる** → compiler + formatter + test runner + type checker が単一バイナリに収まる
3. **1 + 2 の組み合わせ** → LLM がコンパイラ自体を改変 → 再コンパイル → 改変されたコンパイラでさらに改変、というループが自己完結する

つまり: **dev container に 350KB の WASM バイナリを 1 つ置くだけで、LLM が自分の道具を自分で研ぐループが回り始める。**

普通の言語でこれは成立しない:
- ソースが複雑すぎて LLM が壊す → 修正生存率が低い
- ツールチェーンの依存関係で環境構築に失敗する → セットアップコストが高い
- バイナリが大きすぎて配布が重い → 起動コストが高い

Almide は 3 つとも解決できる位置にいる。

## ゴール像

```
350KB WASM バイナリ 1 つ:
  almide compile  — セルフコンパイル可能
  almide fmt      — フォーマッタ内蔵
  almide test     — テストランナー内蔵
  almide check    — 型チェッカー内蔵

実行環境:
  wasmtime / wasmer / ブラウザ / エッジ — どこでも同じバイナリが動く

LLM ループ:
  LLM がソースを読む → 修正 → almide test → almide compile → 新しいコンパイラ
  ↑ このループが外部依存ゼロで回る
```

## 段階的移行

| Phase | 対象 | 理由 |
|-------|------|------|
| 0 | formatter | 文字列処理中心、壊れても被害が限定的 |
| 1 | test runner | コンパイラ本体への依存が少ない |
| 2 | lexer | 文字列処理中心、依存が少ない |
| 3 | parser | 再帰下降、データ構造操作 |
| 4 | type checker | 最も複雑、言語機能をフルに使う |
| 5 | lowering + codegen | IR 変換、WASM emit |
| 6 | bootstrap | 古い Almide コンパイラで新しい Almide コンパイラをコンパイル |

Phase 0-1 は言語機能の成熟度テストを兼ねる。ここで足りない機能が見つかれば言語側にフィードバックする。

## 前提条件

- [ ] WASM direct emit が stdlib 含めて完成している
- [ ] Protocol / Generics が安定している（コンパイラの内部データ構造に必要）
- [ ] ファイル I/O が WASI 経由で動く
- [ ] 言語仕様がほぼ凍結されている（bootstrap breakage を最小化）
- [ ] テストカバレッジがコンパイラの正しさを十分に保証している

### 不足している言語機能

**データ構造・型システム**

| 機能 | 状態 | 備考 |
|------|------|------|
| 効率的な HashMap/BTreeMap | ❌ | 現在の `Map` は限定的。シンボルテーブル、型環境に必須 |
| Trait / typeclass | ❌ | 共通インターフェース（Display, Eq, Hash）の抽象化 |
| 代数的データ型の再帰 | ⚠️ | AST/IR 表現に必須。再帰 variant の動作確認要 |
| ジェネリクスの成熟 | ⚠️ | コンテナ型、visitor パターンに必要 |

**文字列・バイナリ操作**

| 機能 | 状態 | 備考 |
|------|------|------|
| char 単位の操作 | ❌ | レキサーに必須（peek, advance, char category 判定） |
| バイト列操作 | ❌ | WASM バイナリ生成に必須（LEB128 エンコード等） |
| StringBuilder 相当 | ❌ | コード生成の効率的な文字列組み立て |

**ランタイム・制御**

| 機能 | 状態 | 備考 |
|------|------|------|
| ファイルシステム（ディレクトリ走査） | ❌ | 複数ファイルのコンパイルに必要 |
| プロセス引数・終了コード | ⚠️ | CLI として動作するために必要 |
| パニック / 回復不能エラー | ❌ | ICE (Internal Compiler Error) のハンドリング |

## 技術的課題

- **Bootstrap 信頼チェーン**: 最初の 1 回は Rust 版からビルドする必要がある
- **WASM 上でのファイル I/O**: WASI で解決可能だが、コンパイラのファイルアクセスパターンとの整合が要る
- **コンパイラの複雑さ**: 型推論・パターンマッチ・IR 変換はAlmide の言語機能を限界まで使う。足りなければ言語を拡張する必要がある
- **バイナリサイズ**: 350KB 目標を維持するために、コンパイラ自体のコードサイズを意識した設計が必要

## 成功したとき何が起きるか

**Almide は「LLM が最も正確に書ける言語」から「LLM が自律的に進化させられるツールチェーン」になる。**

350KB の WASM バイナリが dev container に 1 つあれば、外部依存なしに:
- コンパイラ自体のバグを LLM が修正できる
- 新しい最適化パスを LLM が追加できる
- 新しいターゲットを LLM が実装できる
- これらすべてをテスト付きで検証できる

コンパイラが小さく、ソースが LLM に読みやすく、ツールが自己完結している — この 3 つが揃ったとき、コンパイラは **ソフトウェアではなくエージェントの一部** になる。
