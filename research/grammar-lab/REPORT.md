# Grammar Lab: Lambda Syntax 実験レポート

## 実験概要

**仮説:** 短縮 lambda `(x) => expr` は現行構文 `fn(x) => expr` と同等の modification survival rate を持つ

**手法:** 同じ修正タスクを 2 つの構文バリアントで LLM に実行させ、compile + test の pass 率を比較

**特徴:** paren-lambda は Almide に未実装。LLM の出力を現行構文にトランスパイルしてから compile + test する（構文実装なしで実験可能）

---

## 結果

### 最終結果 (Run 4)

| Model | fn-lambda | paren-lambda |
|-------|-----------|-------------|
| claude-sonnet-4-6 | **100%** (25/25) | **100%** (25/25) |
| claude-haiku-4-5 | **88%** (22/25) | **96%** (24/25) |

### 改善の推移

| Run | 変更 | Sonnet fn/paren | Haiku fn/paren |
|-----|------|-----------------|----------------|
| 1 | 初回 | 100% / 93% | — |
| 2 | テスト型二重定義を修正 | 100% / 100% | — |
| 3 | Layer1 に `if then` ルール追加 | — | 80% / 72% |
| 4 | test block strip + prompt 改善 | 100% / 100% | 88% / 96% |

### タスク別 breakdown (Haiku, Run 4)

| Task | fn-lambda | paren-lambda |
|------|-----------|-------------|
| t01 (filter 条件変更) | 100% | 100% |
| t02 (map step 追加) | 100% | 100% |
| t03 (sort key 変更) | 100% | 100% |
| t04 (fold 関数追加) | 100% | 100% |
| t05 (複合 pipe 修正) | 40% | 80% |

t05 以外は両 variant とも 100%。残りの fail は t05 の `list.take` をpipe chainに挿入する方法で Haiku が迷うパターン。

---

## 知見

### 1. paren-lambda は fn-lambda と同等かそれ以上

統計的有意差は N=25 では出ないが、方向としては paren-lambda が fn-lambda に劣る証拠はない。Haiku では逆に paren-lambda の方が高い (96% vs 88%)。

**短縮 lambda `(x) => expr` を Almide に導入しても survival rate は下がらない可能性が高い。**

### 2. 実験基盤の品質が結果を支配する

4 回の改善のうち、3 回は LLM の問題ではなく **実験基盤のバグ** だった:

| Run | 原因 | LLM のせいか |
|-----|------|------------|
| 1→2 | テストファイルの型二重定義 | ❌ runner のバグ |
| 2→3 | `if then` ルール不足 | △ Layer 1 の不備 |
| 3→4 | LLM が test block を出力 → 二重定義 | ❌ runner が strip してなかった |

**教訓: 実験を信頼できるデータにするには、まず基盤の品質を上げる必要がある。**

### 3. Layer 1 のルール 1 行が直接効く

`if` の構文ルールを 1 行追加しただけで Sonnet の fail が消えた。LLM が Almide を知らないという前提で、「間違えやすいポイント」を的確に伝えることが重要。

### 4. トランスパイルで未実装構文を事前テストできる

paren-lambda は Almide の parser に未実装だが、`(x) =>` → `fn(x) =>` のテキスト変換で compile + test を通せる。**構文を実装する前に survival rate を測定し、実装の判断材料にできる。**

### 5. 弱いモデルで差が出る

Sonnet は全タスク 100% で天井に張り付く。Haiku で初めて差が見える。**構文設計の評価には弱いモデルをベンチマークに使うべき。**

---

## 実験設計の反省

### うまくいったこと

- **3 層 prompt 設計** (Layer 1: rules, Layer 2: examples, Layer 3: task hints) — 研究に基づく設計が機能した
- **Claude Code プロバイダ** — API key 管理不要で実験を回せた
- **トランスパイル** — 未実装構文のテストを可能にした
- **Almide で runner を書いた** — dogfooding として機能し、言語の問題点も発見

### 改善が必要なこと

- **N が足りない** — 25 では統計的有意差が出にくい。30+ が必要
- **タスクの難易度が偏ってる** — t01-t04 が簡単すぎて 100% になり、t05 だけが難しい
- **t05 の指示が曖昧** — タスク設計の質がそのまま結果に出る
- **トランスパイルが文字列ベース** — regex back reference が使えないため、パターン列挙式。新しい構文実験のたびに transpile ルールを書く必要がある

---

## 次のステップ

1. **t05 の指示改善 + タスク追加** — 難易度のバランスを取る
2. **N=30 で再実行** — 統計的有意差の検定
3. **他の構文実験** — template keyword, builder emit/embed, UFCS なども同じ基盤でテスト可能
4. **regex back reference 対応** — transpile の汎用性を上げる
5. **結果の自動分析** — Fisher's exact test, エラー分類の自動化

---

## 技術的な副産物

Grammar Lab の開発過程で、Almide コンパイラの 3 つの問題を発見:

1. **effect fn main の Err が silent exit** → 修正済み (`eprintln!` 追加)
2. **nested loop での ownership move** → codegen バグ。未修正
3. **process.exec_with_stdin に `?` が欠落** → stdlib TOML のバグ。未修正

これらは Grammar Lab がなければ見つからなかった問題。dogfooding の価値。
