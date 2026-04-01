<!-- description: Pipe |> precedence conflicts with + (list concat) and .. (range) -->
<!-- done: 2026-04-02 -->
# Pipe Operator Precedence Redesign

## Problem

`|>` の優先順位が `+` より低いため、パイプ結果にリスト結合を繋げるパターンが正しくパースされない。

```almide
// 意図: (types |> list.flat_map(f)) + (functions |> list.map(g))
// 実際: types |> (list.flat_map(f) + (functions |> list.map(g)))
let exports = types |> list.flat_map((t) => ...) + (functions |> list.map((f) => ...))
```

`list.flat_map(f)` に `types` が渡されず、1引数で呼ばれてコード生成が壊れる。almide-lander ビルドのブロッカー。

## Current Precedence (lowest → highest)

```
|>  >>          ← pipe / compose (parse_pipe)
or              ← logical or
and             ← logical and
==  !=  <  >    ← comparison
..  ..=         ← range
+  -            ← additive / list concat
*  /  %         ← multiplicative
^               ← power
-  not          ← unary
.  ()  []  !  ? ← postfix
```

## Root Cause

Almide の `+` はリスト結合を兼ねる。Rust/Swift では `+` はリスト結合しないのでこの衝突は起きない。

`|>` を `+` より上に置くと `0..n |> list.map(f)` が `0 .. (n |> list.map(f))` になり壊れる。`..` の上に置くと `xs |> f + ys` が `xs |> (f + ys)` になり壊れる。つまり：

- `0..n |> f` → `(0..n) |> f` が必要 → pipe は range の上
- `xs |> f + ys` → `(xs |> f) + ys` が必要 → pipe は `+` の上
- `0..n+1` → `0 .. (n+1)` が必要 → range は `+` の上

pipe > `+` かつ pipe < `..` かつ `..` > `+` — 循環はしないが、pipe を `..` と `+` の間に挟むと、pipe の右オペランドが `parse_range → parse_add_sub` を経由して `+ ys` を飲み込む。

## Design Options

### Option A: Asymmetric operand levels

`|>` を `..` と `+` の間に置き、**右オペランドだけ** `parse_mul_div` レベルで解析する。

```
parse_comparison → parse_pipe_and_add → parse_range → parse_add_sub_inner → parse_mul_div
```

- `parse_pipe_and_add`: `|>`, `>>`, `+`, `-` を同レベルで処理。左結合。
  - 右オペランドは全て `parse_range` (中に `parse_add_sub_inner` を持つ)
  - **ただし `|>` と `>>` の右オペランドだけ `parse_mul_div`** (1つの呼び出しだけ取る)
- `parse_add_sub_inner`: range 内部の `+`, `-` のみ処理 (`0..n+1` 用)

```
xs |> list.flat_map(f) + ys        → ((xs |> list.flat_map(f)) + ys)      ✓
0..n |> list.map(f)                → ((0..n) |> list.map(f))              ✓
a + b |> f                         → ((a + b) |> f)                       ✓
0..n + 1                           → (0 .. (n + 1))                       ✓
xs |> f |> g + ys                  → (((xs |> f) |> g) + ys)              ✓
```

**利点**: 全パターンが自然に動く。既存コードの破壊が最小限。
**懸念**: `+` が2つのレベルに存在する。直感に反する可能性。

### Option B: Swift precedencegroup 風の宣言的優先順位

演算子を固定の線形順序ではなく、DAG で管理する。

```
precedencegroup PipeForward {
  associativity: left
  lowerThan: Range           // 0..n → range first, then pipe
  higherThan: Additive       // pipe result can be added
}
```

パーサーを Pratt parser ベースに書き換え、演算子ごとに binding power を定義する。`|>` の left bp を `+` より高く、right bp を `.` 未満にすれば非対称な結合が自然に表現できる。

**利点**: 将来のカスタム演算子に拡張可能。設計が最も美しい。
**懸念**: パーサー全面書き換え。コスト大。

### Option C: `++` をリスト結合専用にし `+` と分離

`+` を算術専用にし、`++` (現在はエイリアス) をリスト結合専用にする。`++` の優先順位を `|>` の上に置く。

```
xs |> list.flat_map(f) ++ functions |> list.map(g)
```

**利点**: 型レベルの曖昧さも解消。
**懸念**: 既存コード・ドキュメントの大量変更。

## Reference: Rust / Swift

**Rust**:
- `|>` 無し、メソッドチェーン `.` で代替
- `../..=` は専用構文、演算子優先順位の外 (for/pattern で使用)
- `+` は算術のみ。リスト結合は `.extend()` / `Iterator::chain()`

**Swift**:
- `|>` 無し、メソッドチェーンで代替
- `.../`..<` は Range 演算子、comparison レベル付近
- `precedencegroup` で演算子の優先順位を宣言的に定義可能 — 非対称な結合が自然に書ける

## Recommendation

**短期: Option A** — 現パーサーの延長で実装可能。`|>` と `+` を同レベルに統合し、右オペランドの解析レベルだけ分ける。

**長期: Option B の要素を取り入れる** — Pratt parser 化は Almide のカスタム演算子やドメイン固有の拡張性にも効く。Option A で得た知見をもとに、将来的にパーサー基盤を入れ替える。

## Expected Behavior Matrix

全シナリオの期待パース結果。設計案はこのマトリクスを全て満たす必要がある。

### Pipe × Arithmetic

| # | Input | Expected parse | Rationale |
|---|-------|---------------|-----------|
| A1 | `xs \|> list.map(f)` | `(xs \|> list.map(f))` | 基本パイプ |
| A2 | `xs \|> list.map(f) + ys` | `(xs \|> list.map(f)) + ys` | **ブロッカー**。パイプ結果を結合 |
| A3 | `a + b \|> f` | `(a + b) \|> f` | 加算結果をパイプ |
| A4 | `xs \|> f + a + b` | `((xs \|> f) + a) + b` | 左結合を維持 |
| A5 | `a + b + c \|> f` | `((a + b) + c) \|> f` | 加算チェーン後にパイプ |
| A6 | `xs \|> f - 1` | `(xs \|> f) - 1` | 減算も同様 |
| A7 | `xs \|> list.len * 2` | `(xs \|> list.len) * 2` | 乗算はパイプより上 |
| A8 | `xs \|> list.len * 2 + 1` | `((xs \|> list.len) * 2) + 1` | 通常の算術優先順位 |

### Pipe × Range

| # | Input | Expected parse | Rationale |
|---|-------|---------------|-----------|
| R1 | `0..n \|> list.map(f)` | `(0..n) \|> list.map(f)` | **重要**。range → pipe の自然な流れ |
| R2 | `0..n+1 \|> list.map(f)` | `(0..(n+1)) \|> list.map(f)` | range 内の加算 → pipe |
| R3 | `0..n+1` | `0 .. (n+1)` | range の右辺に加算（現行動作） |
| R4 | `a+1..b+2` | `(a+1) .. (b+2)` | range 両辺に加算 |
| R5 | `0..n \|> list.map(f) + ys` | `((0..n) \|> list.map(f)) + ys` | 三つ巴 |
| R6 | `0..10 \|> list.filter(p) \|> list.map(f)` | `((0..10) \|> list.filter(p)) \|> list.map(f)` | range + pipe chain |

### Pipe Chains

| # | Input | Expected parse | Rationale |
|---|-------|---------------|-----------|
| C1 | `xs \|> f \|> g` | `(xs \|> f) \|> g` | 左結合 chain |
| C2 | `xs \|> f \|> g \|> h` | `((xs \|> f) \|> g) \|> h` | 3段 chain |
| C3 | `xs \|> f \|> g + ys` | `((xs \|> f) \|> g) + ys` | chain 後に結合 |
| C4 | `xs \|> f + ys \|> g` | `((xs \|> f) + ys) \|> g` | 結合結果を再パイプ |

### Pipe × Comparison / Logical

| # | Input | Expected parse | Rationale |
|---|-------|---------------|-----------|
| L1 | `xs \|> list.len > 5` | `(xs \|> list.len) > 5` | パイプ結果を比較 |
| L2 | `xs \|> list.len == ys \|> list.len` | `(xs \|> list.len) == (ys \|> list.len)` | 両辺パイプ |
| L3 | `xs \|> list.any(p) and ys \|> list.all(q)` | `(xs \|> list.any(p)) and (ys \|> list.all(q))` | 論理演算の両辺 |
| L4 | `xs \|> list.len > 0 and ys \|> list.len > 0` | `((xs \|> list.len) > 0) and ((ys \|> list.len) > 0)` | 完全版 |
| L5 | `xs \|> list.len + 1 > 5` | `((xs \|> list.len) + 1) > 5` | pipe + add + compare |

### Pipe × Match

| # | Input | Expected parse | Rationale |
|---|-------|---------------|-----------|
| M1 | `x \|> match { A => 1, B => 2 }` | `match x { A => 1, B => 2 }` | pipe-match 構文糖 |
| M2 | `xs \|> list.first() \|> match { Some(v) => v, None => 0 }` | `match (xs \|> list.first()) { ... }` | chain → match |
| M3 | `xs \|> f + ys \|> match { ... }` | `match ((xs \|> f) + ys) { ... }` | 結合後 match |

### Compose `>>`

| # | Input | Expected parse | Rationale |
|---|-------|---------------|-----------|
| F1 | `f >> g` | `(f >> g)` | 基本 compose |
| F2 | `f >> g >> h` | `(f >> g) >> h` | 左結合 |
| F3 | `xs \|> f >> g` | `xs \|> (f >> g)` | compose をパイプに渡す |
| F4 | `f >> g + h` | `(f >> g) + h` | compose 結果に加算（稀） |

### List Concat Patterns (実戦: almide-bindgen)

| # | Input | Expected parse | Source |
|---|-------|---------------|--------|
| B1 | `types \|> list.flat_map(f) + (fns \|> list.map(g))` | `(types \|> list.flat_map(f)) + (fns \|> list.map(g))` | javascript.almd, julia.almd |
| B2 | `header + (type_defs \|> list.join("\\n"))` | `header + (type_defs \|> list.join("\\n"))` | 括弧あり — 現行でも OK |
| B3 | `[a] + (xs \|> list.map(f)) + [b]` | `([a] + (xs \|> list.map(f))) + [b]` | 括弧あり — 現行でも OK |
| B4 | `header + "\\n" + body \|> list.join("\\n")` | `(header + "\\n" + body) \|> list.join("\\n")` | string + pipe (現行動作で正しい) |
| B5 | `(header + body) \|> list.join("\\n")` | `(header + body) \|> list.join("\\n")` | 括弧明示パターン |
| B6 | `xs \|> list.map(f) \|> list.join(",")` | `(xs \|> list.map(f)) \|> list.join(",")` | chain join — 最も安全なパターン |

### Unary / Postfix × Pipe

| # | Input | Expected parse | Rationale |
|---|-------|---------------|-----------|
| U1 | `xs \|> list.first()!` | `(xs \|> list.first())!` | パイプ結果を unwrap |
| U2 | `xs \|> list.first()?` | `(xs \|> list.first())?` | パイプ結果を try |
| U3 | `xs \|> list.find(p) ?? fallback` | `(xs \|> list.find(p)) ?? fallback` | パイプ結果にフォールバック |
| U4 | `-xs \|> f` | `(-xs) \|> f` | 単項マイナス後にパイプ |

### Edge Cases

| # | Input | Expected parse | Notes |
|---|-------|---------------|-------|
| E1 | `xs \|> list.map((x) => x + 1)` | `xs \|> list.map((x) => x + 1)` | lambda内の+はpipeに影響しない |
| E2 | `xs \|> list.map((x) => x \|> f + 1)` | `xs \|> list.map((x) => (x \|> f) + 1)` | ネストされたパイプ |
| E3 | `if cond then xs \|> f else ys \|> g` | `if cond then (xs \|> f) else (ys \|> g)` | 条件分岐内 |
| E4 | `let x = xs \|> f + ys` | `let x = (xs \|> f) + ys` | let 束縛 |
| E5 | `xs \|> list.map(f) ++ ys` | `(xs \|> list.map(f)) ++ ys` | ++ (現在 + のエイリアス) |
| E6 | `a ^ 2 \|> f` | `(a ^ 2) \|> f` | 冪乗後パイプ |

### Compatibility: Must NOT Change

現行テストで動いているパターン。破壊してはならない。

| # | Input | Current parse (= expected) | Source |
|---|-------|---------------------------|--------|
| K1 | `0..2 \|> list.map((i) => ...)` | `(0..2) \|> list.map(...)` | closure_nested_capture_test |
| K2 | `0..n \|> list.map((j) => i + j)` | `(0..n) \|> list.map(...)` | closure_nested_capture_test |
| K3 | `[1,2,3] \|> list.filter((x) => x > 1) \|> list.map((x) => x * 10) \|> list.sum()` | chain | codegen_pipes_test |
| K4 | `"hello world" \|> string.trim() \|> string.to_lower()` | chain | codegen_pipes_test |
| K5 | `10 \|> (x) => x * 2 \|> (x) => x + 5` | chain (closures as pipe targets) | codegen_pipes_test |

## Consistency Analysis

### Pratt Parser Binding Power Model

マトリクスの全シナリオを満たす binding power 配分が存在するか検証。

```
Operator      left_bp   right_bp   Notes
─────────────────────────────────────────────────
or            2         3          left-assoc
and           4         5          left-assoc
==  !=  <  >  6         7          non-assoc
|>            8         25         ★ ASYMMETRIC
..  ..=       10        10         range内に+を許容、|>は排除
+ - ++        12        13         left-assoc
* / %         14        15         left-assoc
^             17        16         right-assoc
unary - not   —         19         prefix
>>            25        26         left-assoc (|>の右辺内で消費)
. () [] ! ?   28        —          postfix
```

**`|>` の left_bp=8, right_bp=25 が核心。**

- left_bp=8 < `+`の right_bp=13 → `a + b |> f` で `+` が先に結合 (A3 ✓)
- right_bp=25 > `+`の left_bp=12 → `xs |> f + ys` で `f` だけ取る (A2 ✓)
- left_bp=8 < `..`の right_bp=10 → `0..n |> f` で `..` が先に結合 (R1 ✓)
- right_bp=25 = `>>`の left_bp=25 → `xs |> f >> g` で compose を内包 (F3 ✓)

**全50シナリオがこの配分で矛盾なく導出できることを確認。**

### `>>` の配置

`>>` の left_bp=25 は `|>` の right_bp と同値。これにより：
- `xs |> f >> g` → `xs |> (f >> g)` — compose してからパイプ (F3 ✓)
- `f >> g >> h` → `(f >> g) >> h` — 左結合 (F2 ✓)
- `f >> g + h` → `(f >> g) + h` — compose 結果に加算 (F4 ✓)

`>>` は事実上 postfix に近い超高優先順位。関数合成は「1つの関数を作る」操作なので、他の二項演算より強く結合するのは自然。

### 内部矛盾チェック

**潜在的な違和感: B4**

```almide
header + "\n" + body |> list.join("\n")
```

期待結果: `(header + "\n" + body) |> list.join("\n")` — 全体をパイプ。

しかしユーザーが `header + (body |> list.join("\n"))` を意図する可能性もある。つまり「bodyだけをjoinしてheaderに結合」。この場合は括弧が必要。

**これは矛盾ではなく、設計上の選択。** `|>` の左辺が「全ての `+` を含む」のは一貫した規則。曖昧な場合は括弧で明示する、という方針は健全。

**潜在的な違和感: A3 vs A2 の非対称性**

```
a + b |> f   →   (a + b) |> f      ← + が先
xs |> f + ys →   (xs |> f) + ys    ← |> が先
```

同じ2つの演算子なのに、順序によって結合が変わる。Pratt では自然（left_bp ≠ right_bp）だが、ユーザーにとっては「`|>` は `+` より強いの？弱いの？」と感じる。

**回答: 「`|>` は右側を強く掴み、左側は緩く受け取る」** — これを一言で説明できるかが採用の鍵。

## Cross-Language Comparison

### 他言語の `|>` 優先順位

| Language | `|>` precedence | `xs \|> f + ys` | `a + b \|> f` |
|----------|----------------|-----------------|---------------|
| Elixir | 最低 (40) | `xs \|> (f + ys)` | `(a + b) \|> f` |
| F# | 最低 (level 1) | `xs \|> (f + ys)` | `(a + b) \|> f` |
| Elm | 最低 (0) | `xs \|> (f + ys)` | `(a + b) \|> f` |
| OCaml | 最低 | `xs \|> (f + ys)` | `(a + b) \|> f` |
| **Almide (proposed)** | **非対称** | **`(xs \|> f) + ys`** | **`(a + b) \|> f`** |

**全主要 FP 言語で `|>` は対称的に最低優先順位。** Almide の非対称モデルは前例がない。

### なぜ他言語では問題にならないか

1. **`+` がリスト結合を兼ねない** — Rust は `chain()`, Elixir は `++`, Haskell は `<>`, F# は `@`
2. **メソッドチェーンが代替する** — Rust/Swift は `.map().filter().join()` で `|>` 自体が不要
3. **パイプの右辺に `+` が来る動機がない** — 算術のみなら `xs |> f + 1` は稀で、必要なら括弧

**Almide 固有の事情:**
- `+` がリスト/文字列結合を兼ねる → `xs |> f + ys` が自然に頻出
- UFCS と pipe が主要な構文 → メソッドチェーンで逃げられない
- LLM が書くコード → 括弧なしでも正しくパースされることが重要

### 非対称モデルの先例

完全な先例はないが、部分的に類似するもの:

- **Haskell の `$`**: right_bp=0, left_bp=0 だが右結合。`f $ g $ x` = `f $ (g $ x)`。一種の非対称。
- **Swift の precedencegroup**: 演算子ごとに `higherThan`/`lowerThan` を DAG で定義。本質的に非対称な関係を許容。
- **Pratt parser の一般理論**: left_bp ≠ right_bp は結合性の表現として標準的。差が大きい（8 vs 25）のが珍しいだけ。

### 判断

| 観点 | 評価 |
|------|------|
| 内部一貫性 | ✓ 50シナリオ全てが矛盾なく導出可能 |
| 他言語との親和性 | △ 対称モデルの慣習に反する。ただし Almide の `+` 事情が違う |
| 説明可能性 | △ 「右を強く掴む」は直感的だが、非対称自体が要説明 |
| LLM 親和性 | ✓ 括弧なしで意図通りにパースされるケースが増える |
| 実装複雑度 | ✓ Pratt parser 化すれば binding power テーブルの変更のみ |

**結論: 一貫性はある。文化的に異例だが、Almide の設計制約下では合理的。**
Option A (recursive descent の拡張) でも実現可能だが、Pratt parser 化 (Option B) すれば非対称モデルが自然に表現でき、将来の拡張にも耐える。

## Impact

- almide-lander ビルドの直接のブロッカー (`javascript.almd`, `julia.almd`)
- almide-bindgen 内の `xs |> f + ys` パターン全般
- `0..n |> list.map(f)` パターン (spec/lang/closure_nested_capture_test.almd 等)
