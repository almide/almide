# Almide Runtime — 地球上最高の性能を目指すコンパイラ

> Do not win by being safe. Win by reaching the ideal.

## なぜ可能か

既存の言語は汎用性の代償を払っている。Almide は制約が強い。それが武器になる:

```
制約 = 情報 = 最適化の余地
```

| Almide が知ってること | 汎用コンパイラは知らない | 最適化に使える |
|---------------------|----------------------|--------------|
| `fn` = pure (副作用なし) | LLVM は関数の純粋性を推測する | 自動並列化、メモ化、呼び出し順入れ替え |
| `let` = immutable | C/C++ は全変数が可変前提 | コピー不要、参照で十分 |
| `var` は明示的 | mutation point 不明 | SSA 変換が trivial |
| null なし | null check 必要 | null check ゼロ |
| 例外なし (Result) | unwind テーブル必要 | 呼び出し規約がシンプル |
| use-count がある | lifetime 推論が必要 | GC/RC 不要な場合を静的判定 |
| 全プログラムが見える | 分割コンパイル前提 | whole-program optimization が常時可能 |
| コードを書くのが LLM | 人間の癖を想定 | LLM 生成パターン特化の codegen |

---

## Architecture: Multi-Tier Compilation

```
Source (.almd)
  │
  ▼
Almide IR (Typed, Pure/Effect annotated)
  │
  ├─── Tier 0: Direct Interpret     (0ms compile, 10x slow)     ← 開発 REPL
  ├─── Tier 1: Copy-and-Patch JIT   (1ms compile, 2x slow)      ← 開発 run
  ├─── Tier 2: Almide Optimizer     (100ms compile, 1.0x)        ← テスト・ステージング
  └─── Tier 3: LLVM / rustc         (10s compile, 0.95x — C 級)  ← 本番ビルド
```

同じ IR から全 Tier が出る。開発中は Tier 0-1、リリース時だけ Tier 2-3。

---

## 神最適化 1: Static Region Memory

GC なし。RC なし。Borrow checker なし。全部コンパイル時に決まる。

```almide
fn process(data: List[Int]) -> List[Int] =
  data
    |> list.filter((x) => x > 0)      // region A
    |> list.map((x) => x * 2)          // region B (A は死ぬ)
    |> list.take(10)                    // region C (B は死ぬ)
```

コンパイラが見えること:
- `filter` の結果は `map` だけが使う → region A は `map` 完了時に一括解放
- `map` の結果は `take` だけが使う → region B は `take` 完了時に一括解放
- 中間データはヒープに散らばらない。連続メモリに確保して一発で捨てる

Rust の borrow checker より高レベルな判断。use-count + pure fn 保証 + パイプライン解析の組み合わせ。

---

## 神最適化 2: Automatic Parallelism

`fan` は明示的並列。pure fn なら暗黙的にも並列化できる:

```almide
fn expensive_a(x: Int) -> Int = ...  // pure, 重い
fn expensive_b(x: Int) -> Int = ...  // pure, 重い

fn process(x: Int) -> (Int, Int) = {
  let a = expensive_a(x)   // 依存関係なし
  let b = expensive_b(x)   // 依存関係なし
  (a, b)
}
```

effect system が「この 2 つは独立」と証明している。コンパイラが自動で並列化:

```
process(x) → spawn(expensive_a(x)), spawn(expensive_b(x)), join
```

Go の goroutine より賢い。Go は「全部並列にできるかも」と推測する。Almide は「これは確実に並列にできる」と知っている。

---

## 神最適化 3: Speculative Deforestation (Stream Fusion)

関数型プログラミングの最大の敵: 中間データ構造の生成。

```almide
xs |> list.map(f) |> list.filter(g) |> list.fold(0, h)
```

ナイーブ実装: 3 つのリストを作って捨てる。

Almide コンパイラは pure fn の合成として認識して、中間リストをゼロにする:

```rust
// 生成コード: リストを1回だけ走査
let mut acc = 0;
for x in xs {
    let y = f(x);
    if g(y) {
        acc = h(acc, y);
    }
}
```

Haskell の GHC がやっている stream fusion / deforestation。Almide は effect system があるから安全にできる。GHC は「たぶん純粋」と推測する。Almide は「確実に純粋」と知っている。

---

## 神最適化 4: Shape-Specialized Codegen

```almide
type Point = { x: Float, y: Float }
let points: List[Point] = ...
```

汎用コンパイラ: `List<Box<Point>>` — ポインタの配列。キャッシュミスだらけ。

Almide コンパイラ: 型が完全に見えるから Structure of Arrays に変換:

```rust
struct PointList {
    xs: Vec<f64>,  // x 座標だけ連続
    ys: Vec<f64>,  // y 座標だけ連続
}
```

SIMD で一気に処理できる。ゲームエンジンが手動でやっている最適化を、コンパイラが自動でやる。

---

## 神最適化 5: LLM-Aware Compilation

Almide の固有性。コードを書くのが LLM だとわかっているなら:

- LLM が生成するコードパターンを統計的にプロファイル
- 頻出パターンに特化した codegen テンプレートを用意
- LLM が書くコードの特徴（深いパイプライン、多めの中間変数、match の多用）に最適化

```
LLM が書く → コンパイラが最適化 → 実行結果を LLM にフィードバック → より良いコードを書く
```

コンパイラと LLM の共進化ループ。他の言語にはこの視点がない。

---

## Self-Hosting による数学的保証

Almide でコンパイラを書き直すと:

1. **全パスが pure fn** — コンパイラ自身が証明する
2. **不動点検証** — `compile(compiler_source) = compiler` が成立すれば正しさの強い証拠
3. **Trusting Trust 防御** — pure fn は I/O 不可能。コンパイラにバックドアを仕込めない
4. **コンパイラが自分を最適化する再帰** — 上記の神最適化がコンパイラ自身にも適用される

---

## 実現ロードマップ

### Phase 0: 基盤 (今ある武器)
- ✅ Typed IR
- ✅ Pure/Effect split
- ✅ Use-count analysis
- ✅ Multi-target codegen (Rust, TS, JS, WASM)
- ✅ Cross-target CI (91/91)

### Phase 1: 即実行体験
- IR interpreter (Tier 0) — rustc バイパス。即実行
- TS path 改善 — `almide run` のデフォルトを TS に

### Phase 2: Pipe Fusion
- `map |> filter |> fold` → 1 パス走査
- 中間リスト除去
- Pure fn 保証で安全に fusion

### Phase 3: Region Memory
- Region inference — パイプラインの中間データを region で管理
- One-shot deallocation — region 単位で一括解放
- GC/RC 完全不要の静的メモリ管理

### Phase 4: JIT
- Copy-and-Patch baseline JIT (Tier 1)
- Almide IR → machine code テンプレート
- コンパイル時間 1ms 以下

### Phase 5: Auto-Parallelism
- Pure fn のデータ依存解析
- 独立した pure 呼び出しの自動並列化
- fan との統合 (明示的 + 暗黙的並列の共存)

### Phase 6: Optimizing Backend (Tier 2)
- Almide 特化の最適化パイプライン
- Shape specialization (SoA 変換)
- SIMD 自動ベクトル化
- Profile-guided optimization

### Phase 7: Self-Hosting
- User-defined generic functions (前提条件)
- Almide でコンパイラを書き直す
- Bootstrap test (不動点検証)
- コンパイラが自分自身を最適化する再帰

### Phase 8: LLM Co-Evolution
- LLM 生成コードのパターン統計
- 頻出パターン特化の codegen
- コンパイラ ↔ LLM フィードバックループ

---

## 競合比較

| 言語 | コンパイル速度 | 実行速度 | メモリ管理 | 並行性 |
|------|-------------|---------|-----------|--------|
| C | 速い | 最速 | 手動 (危険) | 手動 (危険) |
| Rust | 遅い | 最速級 | Borrow checker | 手動 + async |
| Go | 速い | 良い | GC (pause) | goroutine |
| Zig | 速い | 最速級 | 手動 (安全) | 手動 |
| **Almide (目標)** | **最速** | **最速級** | **Static region (安全)** | **Auto-parallel (安全)** |

Almide の目標: **Go のコンパイル速度 × Rust の実行速度 × 完全自動のメモリ管理 × 自動並列化**。

制約の強さが武器。人間の自由を奪った分だけ、コンパイラが賢くなる。

---

## 参考技術

- **Copy-and-Patch JIT**: CPython 3.13 で採用。テンプレート化された machine code を貼り合わせる
- **Stream Fusion**: GHC (Haskell) の中間リスト除去。`foldr/build` 規則
- **Region Inference**: MLKit (ML) の静的メモリ管理。GC 不要
- **Structure of Arrays**: Data-Oriented Design。ゲームエンジン (Unity DOTS, Bevy ECS)
- **Deforestation**: Wadler (1988)。中間データ構造の除去
- **YJIT / ZJIT**: Ruby の JIT。Copy-and-Patch ベース
