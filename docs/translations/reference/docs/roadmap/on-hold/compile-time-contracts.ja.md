<!-- description: where 節によるコンパイル時事前条件と型不変条件 -->
<a id="compile-time-contracts"></a>

# コンパイル時契約

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](compile-time-contracts.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](compile-time-contracts.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](compile-time-contracts.zh-CN.md)

> 非規範的な履歴参照資料。固定原本を正本として変更せず保持し、自動同期しない。引用コマンド・測定・コンパイラ検証は実行していない。契約構文・フラグ・lowering は提案で、出荷済み機能ではない。冒頭の事後条件への言及と後の対象外指定、lowering 前後の記載差は原文どおり併存する。LLM 正確性の主張は設計上の期待で、新たな測定結果ではない。linking v2 は提案のまま。旧件数・bug・host の記述は現行の全量調査や普遍的保証ではなく、embedded Wasm host 対応は原文冒頭の限定を保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md)

**優先度：** 2.x — 型システム安定後
**原則：** LLM が「動くが間違っている」コードを生成する確率を下げる。型検査＋αで modification survival rate を高める。
**状態：** 一般関数の事前条件と型の値不変条件は提案のまま。下記の `newtype`／関数 `where` は当初の設計スケッチであり、現行構文ではない。`where` はテスト文脈に既存だが、出荷済みのインライン generic protocol bound は別機能（[type-where-constraints.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md)）。

> 「型は、それが何であるかを保証する。契約は、それがどの範囲に収まるかを保証する。」

---

<a id="why"></a>

## なぜ必要か

Almide の型検査器は「Int が必要な場所に String を渡す」を防ぐ。しかし「0 を渡してはいけない場所に 0 を渡す」は防げない。

```almd
fn divide(a: Int, b: Int) -> Int = a / b

// If the LLM writes this, it type-checks. Zero division at runtime
let x = divide(10, 0)
```

契約は関数の事前条件、事後条件、型不変条件をコンパイル時に検証可能にする仕組みである。SMT solver は使わない。コンパイラが静的評価できる述語に限定し、評価できないときは実行時検査へ移行する。

<a id="effect-on-modification-survival-rate"></a>

### Modification Survival Rate への効果

LLM がコードを変更した場合：
1. 型エラー → 即座に検出（**現状**）
2. 契約違反 → コンパイルエラーまたは即座の実行時検出（**本提案**）
3. ロジックのバグ → 検出できない（どの言語でも残る）

**契約は 1 と 3 の間を埋める。** 境界条件（ゼロ除算、範囲外アクセス、負数の受け渡し）は LLM がよく間違えるパターンであり、契約で捕捉できる領域と重なる。

---

<a id="design"></a>

## 設計

<a id="function-contracts"></a>

### 関数契約

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
- 複数条件はカンマ区切り：`where b != 0, lo <= hi`
- 条件が参照できるのは引数のみ（本体の局所変数は不可）
- `where` を持つ関数の呼び出し位置で、条件成立をコンパイラが静的検証する

<a id="proposed-type-contracts-invariants-original-newtype-syntax-sketch"></a>

### 提案された型契約（不変条件。当初の `newtype` 構文スケッチ）

```almd
type Percentage = newtype Int
  where self >= 0, self <= 100

type NonEmpty[T] = newtype List[T]
  where self.len() > 0

type Port = newtype Int
  where self >= 0, self <= 65535
```

- `newtype` と組み合わせて使う
- `self` は値そのものを参照する
- 構築時に契約を検証する

<a id="static-verification-and-dynamic-degradation"></a>

### 静的検証と動的検査への移行

コンパイラは契約を3段階で処理する：

| 判定 | 動作 | 例 |
|----------|--------|---------|
| **静的に真** | 検査を削除 | `divide(10, 3)` — リテラルの 3 != 0 は自明 |
| **静的に偽** | コンパイルエラー | `divide(10, 0)` — リテラルの 0 != 0 は偽 |
| **不明** | 実行時検査を挿入 | `divide(a, b)` — b の値は実行時まで不明 |

静的検証の範囲：
- リテラル値の評価
- 定数畳み込み（`const` 伝播）
- 簡単な範囲解析（if 分岐内で成立する条件）
- guard 後の条件伝播

```almd
// Statically verifiable: after guard, b != 0 is guaranteed
effect fn safe_divide(a: Int, b: Int) -> Result[Int, String] = {
  guard b != 0 else { return err("zero division") }
  ok(divide(a, b))  // ← compiler knows b != 0
}

// Cannot be statically verified: runtime check is inserted
let result = divide(x, y)
// ↓ Compiler-generated code (conceptual)
// if !(y != 0) { panic("contract violation: b != 0 at divide()") }
// let result = x / y
```

**SMT solver を使わない理由：**
- コンパイル時間を予測できない（Z3 は最悪の場合、指数的）
- 実際のコードでは「不明」判定が多く、利用体験が悪化する
- LLM が SMT に適した述語を書くとは限らない
- 静的検証できない場合は実行時検査で十分。「即座にクラッシュ」は「動くが間違う」よりはるかに望ましく、デバッグしやすい

<a id="cognitive-load-for-llms"></a>

### LLM の認知負荷

`where` 節は：
- なくても動く（opt-in）
- 書く場所が明確（ゼロ除算、範囲外、空リスト）
- 関数シグネチャの自然な構文拡張

LLM の学習コスト：最小。CHEATSHEET.md に数行追加すれば生成を可能にできる。
LLM が `where` を間違える確率：低い。条件式は `if` と同じ構文で、新しい概念はない。

---

<a id="proposed-lowering-not-existing-contract-codegen"></a>

## 提案された lowering（既存の契約 codegen ではない）

<a id="rust"></a>

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

- `debug_assert!` は debug build のみで検査する（release build では削除）
- または `--contracts=always` フラグで release build でも有効にする

<a id="original-typescript-alternative-historical-only-target-retired"></a>

### 当初の TypeScript 案（履歴のみ。ターゲットは廃止済み）

```typescript
function divide(a: number, b: number): number {
    if (!(b !== 0)) throw new Error("contract violation: b != 0");
    return Math.trunc(a / b);
}
```

- これは実行時検査方式の例であり、現行コンパイラターゲットでも、あらゆる JavaScript engine の最適化に関する主張でもない。

<a id="wasm"></a>

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

<a id="scope-limitations--what-we-wont-do"></a>

## 範囲の制限 — 行わないこと

| 行わないこと | 理由 |
|---|---|
| 事後条件（`ensures`） | 多くの場合、戻り値検証はテストが適している。構文コストに見合わない |
| 量化子（`forall`, `exists`） | SMT が必要で、コンパイル時間を予測できなくなる |
| 依存型 | 型レベルプログラミングは LLM の正確さを下げる |
| 副作用を持つ条件 | `where` では pure fn 呼び出しのみ許可（len、is_empty など） |
| ループ不変条件 | 検証器が必要。契約の対象外 |

**`where` は事前条件と型不変条件に限定する。** この制約により SMT が不要で、LLM が正確に記述できるようにする。

---

<a id="relationship-with-existing-features"></a>

## 既存機能との関係

| 既存機能 | 関係 |
|---|---|
| `guard` | 実行時の早期 return。契約は guard を「コンパイル時へ昇格」したもの |
| `effect fn` | pure fn と effect fn の両方に契約を付けられる |
| Wrapper type | 提案された値不変条件には構築／検証の契約が必要。古い `newtype` スケッチは現行構文でない |
| 型検査器 | 契約検証は型検査後、lowering 前に実行 |
| nanopass | `ContractCheckPass` の提案。実装済みとは確認されていない |

<a id="complementary-relationship-with-guard"></a>

### guard との補完関係

```almd
// guard: checks condition at runtime, early return on failure
effect fn parse_port(s: String) -> Result[Port, String] = {
  let n = int.parse(s)!
  guard n >= 0, n <= 65535 else { return err("invalid port") }
  ok(Port(n))
}

// contract: the Port type itself guarantees 0..65535
type Port = newtype Int
  where self >= 0, self <= 65535

// Construct Port from guard-verified value → contract statically satisfied
```

guard は「ユーザー入力の検証」、契約は「検証済み値の性質を型に焼き込む」ためのもの。両者は直交し、組み合わせて使う。

---

<a id="implementation-sketch"></a>

## 実装のスケッチ

<a id="phase-1-parser--checker"></a>

### Phase 1：Parser + Checker

- 既存のテスト文脈 `where` と衝突しない関数契約文法を定める
- Parser：採用された関数／型契約文法を実装。当初の `newtype` 表記は現行の前提条件ではない
- AST：FnDecl / TypeDecl に `WhereClause { conditions: Vec<Expr> }` を追加
- Checker：where 条件式が Bool を返すことを検証
- Checker：where 条件で参照できる変数を制限（引数か self のみ）

<a id="phase-2-static-verification"></a>

### Phase 2：静的検証

- lowering 後に `ContractCheckPass` nanopass を挿入
- リテラル引数の定数評価
- guard / if 分岐後の条件伝播（簡単なデータフロー）
- 静的に偽 → 診断エラー
- 静的に不明 → IR に実行時検査ノードを挿入

<a id="phase-3-codegen"></a>

### Phase 3：Codegen

- Rust：`debug_assert!` を生成
- TS の実装ターゲットはない。当初の TS backend スケッチは履歴
- WASM：`if ... unreachable` を生成
- `--contracts=always` / `--contracts=debug` / `--contracts=off` フラグ

<a id="phase-4-diagnostic"></a>

### Phase 4：診断

- 契約違反メッセージ：`contract violated: b != 0 at divide()`
- 呼び出し側のコード位置を表示
- ヒント：`guard b != 0 else { ... }` を挿入するか、引数を変更

---

<a id="success-criteria"></a>

## 成功基準

- `divide(10, 0)` がコンパイルエラーになる
- guard がない `divide(10, n)` は実行時検査付きでコンパイルされる
- 採用された制約型構文が 0..100 の値の構築を検証する（当初の `newtype ... where` 表記はスケッチのまま）
- 全既存テストが通る（`where` のないコードは完全に不変）
- CHEATSHEET.md への追加が10行以下
- LLM が `where` で正しくコードを生成できることを検証

<a id="why-on-hold"></a>

## 保留中の理由

- 先に型システムと nanopass pipeline の安定が必要
- Phase 3（Effect System）との衝突を避ける
- 1.0 の範囲外。言語仕様凍結後に検討
- 現行 wrapper type と generic bound に値不変条件をどう組み合わせるか再検討。当初の `newtype` 設計が出荷済みだとは仮定しない
