原文の説明メタデータ：意味情報を利用した最適化で手書き Rust を超える研究計画
<a id="performance-research-path-to-world-1"></a>

# 性能研究：世界 #1 への道

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](performance-research.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](performance-research.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](performance-research.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/performance-research.md)

> 原文注記：測定設定の未記録な歴史的観測、目標案、発表仮説を含む研究計画であり、測定済み世界順位や現在の速度保証ではない。初期の意味注釈表や純粋関数コメントは条件を省略している。不変性だけでは非エイリアス性、単一使用だけでは非 capture、純粋性だけでは停止・unwind 不在・全メモリ特性を立証しない。後段のエイリアシング・適用条件の限定と初期概略をともに保持する。コンパイラ構成、現在・完了のラベル、LLVM 時間、法則基盤の主張、競合数値は歴史的原文主張であり、最適化・ベンチマーク・外部文書を独立検証していない。

**研究の主張**：コンパイルパイプラインを通して保存される高水準の意味情報により、従来の低水準コンパイラでは不可能な最適化が可能になる。LLM の正確さを目指して設計された言語は、同時にコンパイラ解析にも最適化される。どちらも明示的意味論、明確なデータフロー、最小限のエイリアシングから恩恵を受ける。

**当初の観測（toolchain / host / run はここに未記録）**：n-body は1.74s（Almide）対1.69s（手書き Rust）、オーバーヘッドは約2.9%。これは現在のベンチマーク結果ではない。新しい比較には、揃えたワークロードとビルド設定が必要。

**目標**：標準ベンチマーク全般で一貫して手書き Rust を上回る。

---

<a id="phase-0-close-the-29-gap-engineering"></a>

## Phase 0：2.9% の差を埋める（エンジニアリング）

**目標**：n-body および追加の3+ベンチマークで手書き Rust 比1.00x。

**当該観測についての当初の根本原因仮説**：Copy 型の冗長な `.clone()`、不要な `as f64` キャスト、最適でない変数束縛パターン。

| 作業 | 効果 | 難度 |
|------|--------|-----------|
| Copy 型の clone 除去 | i64/f64/bool の `.clone()` → 除去 | 低 |
| 冗長キャスト除去 | x が既に f64 の `x as f64` → 除去 | 低 |
| let 束縛削減 | tmp を1回使う `let tmp = expr; use(tmp)` → インライン化 | 中 |
| Stream fusion Phase 2 | 検出済み → 実際の IR 書換えを実装 | 中 |
| Clone reduction Phase 4 | for-in、メンバーアクセス、match 対象、record spread | 中 |

**指標**：n-body ≤ 1.69s、spectral-norm ≤ Rust 参照実装、binary-trees ≤ Rust 参照実装。

`research/benchmark/perf/` に**ベンチマーク実装は現在存在する**（`nbody`、`spectralnorm`、`binarytrees`、`fannkuchredux`、`mandelbrot`）。存在は性能目標の達成を立証しない。比率を報告する前に再実行する：
- [x] n-body（重力シミュレーション）
- [x] spectral-norm（固有値）
- [x] binary-trees（GC ストレス / 割当）
- [x] fannkuch-redux（組合せ）
- [x] mandelbrot（浮動小数点並列処理）

---

<a id="phase-1-surpass-human-written-rust-applied-research"></a>

## Phase 1：人が書いた Rust を超える（応用研究）

**目標**：≥3 ベンチマークで手書き Rust を ≥5% 上回る。

**研究課題**：プログラム全体の意味解析は、人が書かないコードを生成できるか？

<a id="1a-whole-program-inlining"></a>

### 1a. プログラム全体のインライン化

Almide はプログラム全体を見る。rustc のインライン化は crate 境界とヒューリスティクスに制約される。

```
Almide approach:
  1. Build call graph with use-counts (already have this)
  2. Inline functions with use_count == 1 unconditionally
  3. Inline small functions (body ≤ 3 IR nodes) unconditionally
  4. For hot paths: inline up to depth 3
  5. Emit single monolithic function for hot loops
```

コード外訳：Almide の方針：1. 使用回数付き呼出しグラフを構築（既に存在）。2. use_count == 1 の関数を無条件にインライン化。3. 小さい関数（本体 ≤ 3 IR ノード）を無条件にインライン化。4. ホットパスは深さ3までインライン化。5. ホットループに単一の一体化した関数を出力。

rustc + LTO も一部行うが、Almide はより良い情報を持つ。純粋な関数（effect system）、escape する変数（借用解析）、正確な使用回数を把握する。

<a id="1b-allocation-elimination"></a>

### 1b. 割当除去

Almide の型システムはデフォルトの不変性を保証する。これは次を意味する：

```
// Almide knows this list is never modified after creation
let xs = [1, 2, 3, 4, 5]
let sum = xs.fold(0, |acc, x| acc + x)

// Instead of Vec::new() + push + push + ..., emit:
// static array on stack + direct fold (no heap allocation)
```

コード外のコメント訳：Almide はこのリストが作成後変更されないと知っている。Vec::new() + push + push + … の代わりに、スタック上の静的配列 + 直接 fold（ヒープ割当なし）を出力する。

| パターン | 当初のベースライン概略 | 望ましい最適化 codegen |
|---------|----------------|-------------------|
| 小さいリストリテラル | `vec![1, 2, 3]`（ヒープ） | `[1, 2, 3]`（スタック配列） |
| fold でのみ使うリスト | `Vec<T>` + iterator | 直接累積（リストなし） |
| 文字列連結の連鎖 | 複数の `String::new()` + `push_str` | サイズを事前指定する `String::with_capacity` |
| 作成後すぐ分解する record | struct 割当 + フィールドアクセス | 値を直接転送 |

<a id="1c-semantic-aware-dead-code-elimination"></a>

### 1c. 意味情報を利用したデッドコード除去

effect system は積極的な DCE を可能にする：

```
// Almide knows: pure fn never has side effects
fn square(x: Int) -> Int = x * x

let a = square(10)  // pure, use_count == 0 → eliminate entire call
```

コード外のコメント訳：Almide は純粋関数に副作用がないと知っている。純粋、use_count == 0 → 呼出し全体を除去。

現在の DCE は保守的（全関数呼出しを残す）。effect 注釈があれば、結果を使わない純粋関数呼出しを除去できる。

<a id="1d-type-specialized-container-operations"></a>

### 1d. 型に特化したコンテナ操作

Almide は全呼出し箇所で具体的要素型を把握する：

```
// list.sort_by knows T = Int at this call site
users.sort_by(|u| u.age)

// Instead of generic comparison, emit specialized:
// .sort_by(|a, b| a.age.cmp(&b.age))  ← already specific
// But also: for Int lists, use counting sort / radix sort when len > threshold
```

コード外のコメント訳：list.sort_by はこの呼出し箇所で T = Int と知っている。汎用比較の代わりに特殊化したものを出力（示した .sort_by は既に具体的）。加えて Int リストでは len > threshold のとき counting sort / radix sort を使う。

**指標**：≥3 の Benchmarks Game プログラムで Almide が Rust 参照実装を ≥5% 上回る。

---

<a id="phase-2-llvm-ir-direct-emission-engineering--research"></a>

## Phase 2：LLVM IR の直接出力（エンジニアリング + 研究）

**目標**：rustc を完全に迂回。Almide IR から LLVM IR を出力。

**研究課題**：Rust 抽象化層の除去は、新しい最適化機会を可能にするか？

<a id="architecture"></a>

### アーキテクチャ

```
Current:   Almide IR → Rust source → rustc → LLVM IR → LLVM → binary
Proposed:  Almide IR → LLVM IR → (custom passes) → LLVM → binary
```

図の訳：現在は Almide IR → Rust source → rustc → LLVM IR → LLVM → binary。提案は Almide IR → LLVM IR →（カスタムパス）→ LLVM → binary。

<a id="why-this-matters"></a>

### なぜ重要か

1. **コンパイル時間**：rustc frontend はコンパイル時間の60-70%。LLVM IR 直接出力により `almide run` を約2sから約0.6sへ短縮する。
2. **カスタム LLVM パス**：Almide lowering と LLVM 最適化の間に、Almide の意味情報を利用する最適化パスを挿入する。
3. **細粒度の制御**：Almide の effect / purity 解析に基づき、LLVM IR 注釈（`noalias`、`readonly`、`nounwind`、`willreturn`）を精密に出力できる。

<a id="semantic-annotations-almide-can-emit"></a>

### Almide が出力できる意味注釈

| Almide の知識 | LLVM 注釈 | 効果 |
|-----------------|-----------------|--------|
| 純粋関数（effect なし） | `readonly`, `willreturn`, `nounwind` | 積極的な CSE、LICM を可能にする |
| 不変変数 | ポインタ引数の `noalias` | load 除去を可能にする |
| 使用回数 == 1 | `nocapture` | スタックへの昇格を可能にする |
| エイリアシングなし（ownership） | `noalias`, `dereferenceable` | ベクトル化を可能にする |
| 既知のループ境界（Range） | `!llvm.loop` メタデータ | ループ展開を可能にする |

rustc も一部付けるが、Almide はより高水準の意味情報を持つため**より精密に**できる。

<a id="implementation-path"></a>

### 実装経路

| 手順 | 作業 | LOC 見積り |
|------|------|-------------|
| LLVM IR テキスト出力器 | nanopass パイプラインの新 codegen target | ~2,000 |
| LLVM IR ランタイムライブラリ | core_runtime.txt を LLVM bitcode に移植 | ~1,000 |
| 意味注釈パス | effect 解析から noalias/readonly/nounwind を出力 | ~500 |
| カスタム LLVM パス：deforestation | 中間データ構造を除去 | ~1,500（C++） |
| カスタム LLVM パス：arena coalescing | 生存期間が既知の割当を統合 | ~1,000（C++） |

**依存**：`inkwell` crate（Rust LLVM bindings）または生の LLVM-C API。

---

<a id="phase-3-semantic-aware-optimization-pure-research"></a>

## Phase 3：意味情報を利用した最適化（純粋研究）

**目標**：Almide の豊富な意味情報を活用する新しい最適化パス。

**研究課題**：コンパイラが純粋性、effect、ownership、代数法則を把握すると、どんな最適化が可能になるか？

<a id="3a-deforestation-intermediate-structure-elimination"></a>

### 3a. Deforestation（中間構造除去）

Stream fusion（Phase 0）は `map.filter.fold` を単一ループへ融合する。Deforestation はこれを**全中間データ構造**へ一般化する：

```almide
fn process(users: List[User]) -> List[String] =
  users
    |> filter(|u| u.age > 18)
    |> map(|u| u.name)
    |> sort_by(|n| n)
    |> take(10)

// Current: filter → new Vec → map → new Vec → sort → new Vec → take → new Vec
// Deforested: single pass with in-place sort on pre-filtered view
```

コード外のコメント訳：現在は filter → 新 Vec → map → 新 Vec → sort → 新 Vec → take → 新 Vec。Deforestation 後は、事前 filter 済み view 上で in-place sort を行う単一パス。

**研究上の貢献**：TypeConstructorRegistry の代数法則に導かれる deforestation。Almide は既に法則の基盤を持つ（HKT foundation Phase 1-4）。

<a id="3b-effect-guided-parallelization"></a>

### 3b. Effect に導かれる並列化

effect 注釈は自動並列化を可能にする：

```almide
// Almide knows: square is pure, map preserves order
let results = large_list.map(|x| square(x))

// Safe to parallelize because:
// 1. square has no effects
// 2. map is a Functor operation (preserves structure)
// 3. No mutable state captured
```

コード外のコメント訳：Almide は square が純粋で map が順序を保つと知っている。安全に並列化できる理由：1. square に effect がない。2. map は Functor 操作（構造を保存）。3. 可変状態を capture しない。

次の場合に `rayon::par_iter()` または SIMD intrinsics を出力：
- 関数が純粋（effect 解析）
- 操作が容易に並列化可能（代数法則：map は分配的）
- コレクションサイズが閾値を超える

<a id="3c-memoization-of-pure-functions"></a>

### 3c. 純粋関数のメモ化

```almide
fn fib(n: Int) -> Int =
  if n <= 1 { n }
  else { fib(n - 1) + fib(n - 2) }

// Almide knows: fib is pure, recursive, Int → Int
// Auto-memoize: HashMap<i64, i64> cache
```

コード外のコメント訳：Almide は fib が純粋、再帰的、Int → Int であると知っている。自動メモ化：HashMap&lt;i64, i64&gt; キャッシュ。

条件：純粋関数 + 再帰的 + 小さい入力領域。

<a id="3d-data-layout-optimization"></a>

### 3d. データ配置最適化

Almide は使用回数解析からフィールドアクセスパターンを把握する：

```almide
type User { name: String, age: Int, email: String, score: Float }

// If hot loop only accesses .age and .score:
for u in users { process(u.age, u.score) }

// Emit SoA (Struct of Arrays) for hot fields:
// ages: Vec<i64>, scores: Vec<f64> — cache-friendly
```

コード外のコメント訳：ホットループが .age と .score にしかアクセスしないなら、ホットなフィールドを SoA（配列の構造体）として出力：ages: Vec&lt;i64&gt;, scores: Vec&lt;f64&gt; — キャッシュに適する。

**研究上の貢献**：フィールド使用回数とループ解析に導かれる自動 AoS → SoA 変換。

---

<a id="phase-4-validation--publication"></a>

## Phase 4：検証と発表

<a id="benchmark-targets"></a>

### ベンチマーク目標

| ベンチマーク | 当初の観測 | Phase 0 目標 | Phase 1 目標 | Phase 2-3 目標 |
|-----------|---------|---------|---------|-----------|
| n-body | 1.03x | 1.00x | 0.97x | 0.90x |
| spectral-norm | — | 1.00x | 0.95x | 0.90x |
| binary-trees | — | 1.00x | 0.90x | 0.85x |
| fannkuch-redux | — | 1.00x | 0.98x | 0.92x |
| mandelbrot | — | 1.00x | 0.95x | 0.88x |

（x = 最良の Rust 実装に対する比率。< 1.0 = Almide のほうが速い。）

<a id="publication-targets"></a>

### 発表目標

| Phase | 題名 | 発表先 |
|-------|-------|-------|
| 0-1 | 「Almide：意味情報を利用した Rust 生成による、ほぼゼロオーバーヘッドの高水準コンパイル」 | arXiv プレプリント |
| 2 | 「LLVM の意味注釈：低水準最適化への高水準言語特性の活用」 | CGO / CC |
| 3 | 「Effect に導かれる最適化：純粋性はいかに自動並列化と deforestation を可能にするか」 | PLDI / OOPSLA |
| 全段階 | 「修正生存率 × 性能：LLM とハードウェア双方のために設計した言語」 | POPL |

<a id="original-competitive-hypotheses-not-measured-rankings"></a>

### 当初の競争上の仮説（測定した順位ではない）

以下のバージョンを特定しない比較は、研究動機としてのみ保持する。現在の相対性能、MSR、速度向上倍率を立証しない。

| 言語 | 性能 | LLM の正確さ | Almide の優位点 |
|----------|------------|--------------|-----------------|
| C | ベースライン | 低（UB、手動メモリ） | 意味最適化でより安全かつ高速 |
| C++ | ~C | 低（複雑さ） | より単純で同等 |
| Rust | ~C | 中（borrow checker エラー） | 同じ backend + 意味最適化 |
| Zig | ~C | 中 | 意味情報の豊富さ |
| Mojo | ~C と主張 | 不明 | 実証された MSR 指標 |
| Go | ~2x C | 高 | 10-50x 高速 |
| TypeScript | ~20x C | 高 | 100x+ 高速 |

---

<a id="key-insight-llm-friendliness--optimization-friendliness"></a>

## 重要な洞察：LLM への適性 = 最適化への適性

これは評価すべき研究仮説であり、独自性が立証されたという主張ではない。言語を LLM が正確に書きやすくする特性は、最適化にも豊富な情報を与える：

| LLM に適した特性 | 最適化の利点 |
|-----------------------|---------------------|
| デフォルトで不変 | 推論を単純化し得る。不変値もエイリアスし得るため、最適化には明示的 ownership / purity 条件が必要 |
| 明示的 effect（`effect fn`） | 純粋性解析 → DCE、並列化、メモ化 |
| 隠れた制御フローなし（純粋関数に例外なし） | 予測可能な最適化 |
| IR で型により分派する演算子 | 実行時型検査なし |
| UFCS | 適格な呼出しを解決。関数値と間接分派には引き続き呼出しグラフ解析が必要 |
| 継承なし | 関数値 / closure の分派を排除しない。インライン化は引き続き適用条件に依存 |

**この相関は偶然ではなく、構造的なものだ。** LLM の曖昧さを最小化する言語は最適化器の曖昧さも最小化する。この主張は、ベンチマークで検証できれば発表可能な成果になる。

---

<a id="priority"></a>

## 優先順位

Phase 0（差を埋める）→ Phase 1b-1c（割当除去 + 意味 DCE）→ Phase 1a（インライン化）→ Phase 2（LLVM 直接出力）→ Phase 3a（deforestation）→ Phase 3b（並列化）。

Phase 0 はエンジニアリング。Phase 1 は応用研究。Phase 2-3 は新規研究。Phase 4 は検証。
