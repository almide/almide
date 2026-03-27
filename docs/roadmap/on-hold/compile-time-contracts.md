<!-- description: Compile-time preconditions and type invariants via where clauses -->
# Compile-Time Contracts [ON HOLD]

**優先度:** 2.x — 型システム安定後
**原則:** LLM が「動くけど間違っている」コードを生成する確率を下げる。modification survival rate を型チェック + α で引き上げる。
**構文コスト:** `where` 句 1 つのみ。新キーワード 1 語。

> 「型は "何であるか" を保証する。contract は "どの範囲であるか" を保証する。」

---

## Why

Almide の型チェッカーは「Int を渡すべきところに String を渡した」を防ぐ。しかし「0 を渡してはいけないところに 0 を渡した」は防げない。

```almd
fn divide(a: Int, b: Int) -> Int = a / b

// LLM がこう書いたら型は通る。実行時にゼロ除算
let x = divide(10, 0)
```

contract は関数の事前条件・事後条件・型の不変条件をコンパイル時に検証可能にする仕組み。SMT ソルバーは使わない。コンパイラが静的に評価できる述語に限定し、評価不能な場合はランタイムチェックに降格する。

### Modification survival rate への効果

LLM がコードを修正したとき:
1. 型エラー → 即座に検出 (**現状**)
2. contract 違反 → コンパイルエラーまたはランタイム即座に検出 (**この提案**)
3. ロジックバグ → 検出不能 (どの言語でも残る)

**contract は 1 と 3 の間を埋める。** 特に境界条件（0除算、範囲外アクセス、負数渡し）は LLM が頻繁に間違えるパターンであり、contract で捕捉できる領域と重なる。

---

## Design

### 関数 contract

```almd
fn divide(a: Int, b: Int) -> Int
  where b != 0
= a / b

fn clamp(value: Int, lo: Int, hi: Int) -> Int
  where lo <= hi
= if value < lo then lo
  else if value > hi then hi
  else value
```

- `where` は関数シグネチャと本体の間に書く
- 複数の条件はカンマ区切り: `where b != 0, lo <= hi`
- 条件は引数のみ参照可能（本体のローカル変数は不可）
- `where` を持つ関数の呼び出し側で、コンパイラが条件の成立を静的に検証する

### 型 contract (invariant)

```almd
type Percentage = newtype Int
  where self >= 0, self <= 100

type NonEmpty[T] = newtype List[T]
  where self.len() > 0

type Port = newtype Int
  where self >= 0, self <= 65535
```

- `newtype` と組み合わせて使う
- `self` で値自身を参照
- 構築時に contract が検証される

### 静的検証と動的降格

コンパイラは contract を 3 段階で処理する:

| 判定 | 処理 | 例 |
|------|------|-----|
| **静的に真** | チェックを除去 | `divide(10, 3)` — リテラル 3 != 0 は自明 |
| **静的に偽** | コンパイルエラー | `divide(10, 0)` — リテラル 0 != 0 は偽 |
| **不明** | ランタイムチェック挿入 | `divide(a, b)` — b の値は実行時まで不明 |

静的検証の範囲:
- リテラル値の評価
- 定数畳み込み (`const` 伝播)
- 単純な区間解析 (if 分岐内で条件が成立)
- guard 後の条件伝播

```almd
// 静的に検証可能: guard 後は b != 0 が保証される
effect fn safe_divide(a: Int, b: Int) -> Result[Int, String] = {
  guard b != 0 else { return err("zero division") }
  ok(divide(a, b))  // ← コンパイラは b != 0 を知っている
}

// 静的に検証不能: ランタイムチェックが挿入される
let result = divide(x, y)
// ↓ コンパイラが生成するコード (概念)
// if !(y != 0) { panic("contract violation: b != 0 at divide()") }
// let result = x / y
```

**SMT ソルバーを使わない理由:**
- コンパイル時間の予測不能性 (Z3 は worst case で指数時間)
- 「Unknown」判定が実用コードで頻発し、ユーザー体験が悪化する
- LLM が SMT-friendly な述語を書けるとは限らない
- 静的に判定できない場合はランタイムチェックで十分 — 「動くけど間違っている」より「即座にクラッシュ」のほうが遥かにましで、デバッグ可能

### LLM にとっての負荷

`where` 句は:
- 書かなくても動く (opt-in)
- 書くべき場所が明確 (ゼロ除算、範囲外、空リスト)
- 構文が関数シグネチャの自然な延長

LLM の学習コスト: 最小。CHEATSHEET.md に数行追加するだけで生成できるようになる。
LLM が `where` を間違える確率: 低い。条件式は `if` と同じ構文で、新しい概念はない。

---

## Multi-Target Codegen

### Rust

```rust
fn divide(a: i64, b: i64) -> i64 {
    debug_assert!(b != 0, "contract: b != 0");
    a / b
}

// newtype
struct Percentage(i64);
impl Percentage {
    fn new(value: i64) -> Percentage {
        debug_assert!(value >= 0 && value <= 100, "contract: 0..=100");
        Percentage(value)
    }
}
```

- `debug_assert!` でデバッグビルド時のみチェック (リリースビルドでは除去)
- または `--contracts=always` フラグでリリースでも有効化

### TypeScript

```typescript
function divide(a: number, b: number): number {
    if (!(b !== 0)) throw new Error("contract violation: b != 0");
    return Math.trunc(a / b);
}
```

- 常にランタイムチェック (TS にはコンパイル時定数畳み込みがないため)

### WASM

```wasm
(func $divide (param $a i64) (param $b i64) (result i64)
  local.get $b
  i64.eqz
  if
    unreachable  ;; contract violation
  end
  local.get $a
  local.get $b
  i64.div_s
)
```

---

## Scope 制限 — やらないこと

| やらないこと | 理由 |
|---|---|
| 事後条件 (`ensures`) | 戻り値の検証は多くの場合テストのほうが適切。構文コストに見合わない |
| 量化子 (`forall`, `exists`) | SMT 前提。コンパイル時間が予測不能になる |
| 依存型 | 型レベルプログラミングは LLM の精度を下げる |
| 副作用を含む条件 | `where` 内で関数呼び出し可能なのは pure fn のみ (len, is_empty 等) |
| ループ不変条件 | 検証器が必要。contract の scope 外 |

**`where` は事前条件と型不変条件に限定する。** この制限が、SMT 不要かつ LLM が正確に書ける範囲を担保する。

---

## 既存機能との関係

| 既存機能 | 関係 |
|---|---|
| `guard` | ランタイムの早期リターン。contract は guard の「コンパイル時昇格版」 |
| `effect fn` | contract は pure fn にも effect fn にも付与可能 |
| `newtype` | `newtype` + `where` で制約付き型を作る自然な組み合わせ |
| 型チェッカー | contract 検証は型チェックの後、lowering の前に実行 |
| nanopass | `ContractCheckPass` として nanopass パイプラインに挿入 |

### guard との相補関係

```almd
// guard: 実行時に条件を検査し、失敗時は early return
effect fn parse_port(s: String) -> Result[Port, String] = {
  let n = int.parse(s)?
  guard n >= 0, n <= 65535 else { return err("invalid port") }
  ok(Port(n))
}

// contract: Port 型自体が 0..65535 を保証
type Port = newtype Int
  where self >= 0, self <= 65535

// guard で検証済みの値から Port を構築 → 静的に contract 成立
```

guard は「ユーザー入力をバリデーションする」、contract は「バリデーション済みの値の性質を型に焼き付ける」。2つは直交し、組み合わせて使う。

---

## Implementation Sketch

### Phase 1: Parser + Checker

- `where` キーワードの追加 (43番目のキーワード)
- Parser: 関数宣言と newtype 宣言で `where` 句をパース
- AST: `WhereClause { conditions: Vec<Expr> }` を FnDecl / TypeDecl に追加
- Checker: where 条件内の式が Bool を返すことを検証
- Checker: where 条件内で参照可能な変数を制限 (引数 or self のみ)

### Phase 2: Static Verification

- Lowering 後に `ContractCheckPass` nanopass を挿入
- リテラル引数の定数評価
- guard / if 分岐後の条件伝播 (simple dataflow)
- 静的に偽 → diagnostic error
- 静的に不明 → IR にランタイムチェックノードを挿入

### Phase 3: Codegen

- Rust: `debug_assert!` 生成
- TS: `if (!cond) throw new Error(...)` 生成
- WASM: `if ... unreachable` 生成
- `--contracts=always` / `--contracts=debug` / `--contracts=off` フラグ

### Phase 4: Diagnostic

- contract 違反のエラーメッセージ: `contract violated: b != 0 at divide()`
- 呼び出し元のコード位置を表示
- hint: `guard b != 0 else { ... }` を挿入するか、引数を変更するか

---

## Success Criteria

- `divide(10, 0)` がコンパイルエラーになる
- `divide(10, n)` が guard なしではランタイムチェック付きでコンパイルされる
- `type Percentage = newtype Int where self >= 0, self <= 100` が動く
- 既存テスト全通過 (`where` なしのコードは一切影響を受けない)
- CHEATSHEET.md への追記が 10 行以内
- LLM が `where` を正しく使ったコードを生成できることを検証

## Why ON HOLD

- 型システムと nanopass パイプラインの安定化が前提
- Phase 3 (Effect System) との競合を避ける
- 1.0 の scope 外 — 言語仕様の凍結後に検討
- ただし newtype との組み合わせが自然なため、newtype の設計を壊さない限り後付けで入れられる
