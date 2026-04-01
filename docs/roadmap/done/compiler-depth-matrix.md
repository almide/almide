<!-- description: Map Almide's compiler depth against industrial compilers and plan next tiers -->
<!-- done: 2026-04-01 -->
# Compiler Depth Matrix

Almide のコンパイラが産業級コンパイラに対してどこに位置し、次にどこを深めるかの見取り図。

---

## 現在地

### 型推論

| Level | 技術 | 代表 | Almide |
|-------|------|------|--------|
| 1 | 型注釈必須 | C, Go | ✅ 超えている |
| 2 | Local inference | Kotlin, Swift | ✅ 超えている |
| 3 | HM + constraint | OCaml, Haskell 98 | **← ここ** |
| 4 | Trait solving / typeclasses | Rust, Haskell | 部分的（protocol bounds） |
| 5 | GADTs / dependent types | Scala 3, Idris | — |

**Almide の現在**: HM + constraint-based unification (Union-Find)。Protocol bounds による制約伝搬あり。Let-polymorphism は instantiation ベースで実装済み。Row polymorphism (open record) は構造的マッチングで近似。

**内部 → 外部の段階的露出**:
- HKT / GADTs / associated types はコンパイラ内部の型表現として先に実装する
- ユーザー構文には露出させず、内部 dispatch で恩恵だけを提供する
- 十分に安定したら、必要に応じて構文を開放する

### IR 最適化パス

| Level | パス数 | 代表 | Almide |
|-------|--------|------|--------|
| 1 | 1-5 | 学習用コンパイラ | ✅ 超えている |
| 2 | 5-15 | 実用言語 v1 | **← ここ** (Rust: 16, WASM: 9) |
| 3 | 15-40 | Go, Swift, Zig | 次のターゲット |
| 4 | 40-100 | GHC, javac + JIT | — |
| 5 | 100+ | LLVM, GCC | — (バックエンドに委任する領域) |

**Almide の現在 (Rust target)**:
1. BoxDeref — Box パターン変数の Deref 挿入
2. TailCallOpt — 自己再帰末尾呼び出し → ループ
3. LICM — ループ不変式の巻き上げ
4. TypeConcretization — 型具象化
5. StreamFusion — イテレータチェーン融合
6. BorrowInsertion — 借用挿入
7. CaptureClone — クロージャキャプチャの clone
8. CloneInsertion — 所有権ベースの clone 挿入
9. MatchSubject — match 対象の型変換
10. EffectInference — effect fn 推論
11. StdlibLowering — stdlib 呼び出しの低レベル化
12. AutoParallel — 純粋リスト操作の並列化
13. ResultPropagation — `?` 演算子挿入
14. BuiltinLowering — 組み込み関数 → マクロ展開
15. Peephole — swap/reverse/rotate 特殊化
16. FanLowering — fan 並行パターンの低レベル化

**さらに optimize 層** (Lower → IR 間): ConstantFold, ConstantPropagation, DCE (×2)

---

## Next Tier: Level 3 への道

### 型システム → 内部最高級・外部愚直

**原則**: コンパイラ内部は最高級の仕組みを持つ。外側にはいつでも出せる状態にしておいて、ユーザーには愚直な形だけを見せる。

#### Phase 1: 内部 HKT — 組み込み protocol の統一 dispatch

ユーザー構文は変えない。コンパイラ内部で `Self[_]` を扱える組み込み protocol を持ち、UFCS dispatch を統一する。

```almide
// ユーザーが書くコード — 今と同じ
items |> list.map(f)
maybe_val |> maybe.map(f)

// 将来: 型から自動解決（構文の露出ではなく dispatch の統一）
items |> map(f)
maybe_val |> map(f)
```

| Step | 内容 | ユーザーへの見え方 | Effort |
|------|------|-------------------|--------|
| 1-a | 組み込み protocol に `Self[_]` を内部表現として許可 | 変化なし | M |
| 1-b | `map`, `flat_map`, `filter` を統一 dispatch | `list.map` も `map` も書ける | M |
| 1-c | パイプラインで container 型を推論解決 | `xs |> map(f) |> filter(g)` が型を問わず動く | M |

#### Phase 2: Protocol 拡張 — 表現力の段階的開放

| Item | 内部用途 | 外部露出の条件 | Effort |
|------|----------|---------------|--------|
| **Protocol default methods** | stdlib protocol の実装簡素化 | 安定したら開放 | M |
| **Protocol inheritance** | `Ord: Eq` を内部で使い、ユーザーには `Ord` だけ見せる | 開放可 | M |
| **Associated types** | `Container { type Item }` で内部の型解決を強化 | HKT 構文なしで恩恵を出せるなら保留 | L |
| **Protocol-based operator dispatch** | `+` を内部的に protocol method として統一 | 演算子オーバーロード構文として開放検討 | M |

#### Phase 3: 内部 GADTs — 型安全な IR

コンパイラ自身の IR 型を GADT 的に設計し、パス間の型不整合をコンパイル時に検出する。言語機能としての GADTs 露出は、ユーザーが DSL を書く需要が出てから検討する。

**設計判断**: Rust の trait solving の複雑さ（orphan rules, coherence, specialization）は持ち込まない。内部表現の豊かさと外部構文の単純さを独立に進化させる。LLM が書く構文は常に愚直でいい。

### IR パス → 15-25 パス

| Pass | カテゴリ | 効果 | Effort |
|------|----------|------|--------|
| **Inlining** | 最適化 | 小関数のインライン展開 | M |
| **CSE** (Common Subexpression Elimination) | 最適化 | 重複計算の除去 | M |
| **Escape analysis** | 所有権 | ヒープ回避、borrow 精度向上 | L |
| **Copy propagation** | 最適化 | 不要コピーの除去 | S |
| **Strength reduction** | 最適化 | `x * 2` → `x << 1` 等 | S |
| **Algebraic simplification** | 最適化 | `x + 0` → `x`, `x * 1` → `x` | S |
| **ShadowResolve** (既存・未接続) | 正確性 | 変数シャドウイングの解決 | S |
| **MatchLowering** (既存・未接続) | 低レベル化 | match の decision tree 化 | M |
| **ResultErasure** (既存・未接続) | ターゲット固有 | Result 型の除去 | S |

**Note**: ShadowResolve, MatchLowering, ResultErasure は `impl NanoPass` が存在するが Rust pipeline に未接続。接続検討の価値あり。

---

## 設計原則

**内部最高級・外部愚直**: コンパイラの内部メカニズムは産業級の最高水準を目指す。ユーザーと LLM に見える構文は常に単純に保つ。高度な機能は内部で完成させ、外部には必要になった時にだけ露出する。

- **Nanopass は semantic transform**: 各パスは「言語の意味を保ったまま、ターゲットに近い表現に変換する」
- **最適化は IR 層で**: const fold, DCE, const prop は IR-to-IR で全ターゲット共通
- **深い最適化はバックエンドに委任**: Rust target は rustc/LLVM が、WASM target は wasm-opt が引き受ける
- **パス数の目標は 20-25**: 自前でやる価値のある semantic transform に集中し、低レベル最適化はバックエンドに任せる
- **型システムは氷山**: 水面下に HKT・GADTs・associated types を持ち、水面上は愚直な protocol と型注釈だけ
