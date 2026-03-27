<!-- description: Function monomorphization for generic structural bounds in Rust codegen -->
<!-- done: 2026-03-15 -->
# Monomorphization

Generic structural bounds (`T: { name: String, .. }`) の Rust codegen に必要な、関数のモノモーフィゼーション基盤。

## Why

現在の Rust codegen は 1 関数 = 1 Rust 関数。open record の Phase 1 は field projection（呼び出し側で必要なフィールドだけ AlmdRec に詰める）で回避したが、以下は projection では不可能:

| Feature | 問題 |
|---------|------|
| Generic structural bounds `T: { name, .. }` | `T` の具体型ごとに `.name` アクセスのコードが異なる。戻り値が `T` の場合、具体型を保持して返す必要がある |
| Container protocols `F: Mappable` | `List` と `Option` で異なる `.map()` 呼び出しを生成する必要がある |

共通解: **呼び出し時の具体型ごとに関数を複製（monomorphize）** する。

## Current status

### Done ✅

- [x] Generic structural bounds syntax: `fn set_name[T: { name: String, .. }](x: T) -> T`
- [x] Parser: `parse_generic_param` で `{ field: Type, .. }` を構造的制約としてパース
- [x] AST: `GenericParam.structural_bound: Option<TypeExpr>`
- [x] Checker: 構造的制約を `FnSig.structural_bounds` と `TypeEnv.structural_bounds` に登録
- [x] Checker: call-site で TypeVar を具象型に unify（`T` → `Dog`）
- [x] Checker: `check_member_access` で構造的制約からフィールドを解決（`x.name` on `T`）
- [x] Monomorphization pass (`src/mono.rs`): IR→IR 変換
  - [x] Instantiation discovery: call graph 走査で具象型を収集
  - [x] Function cloning: `set_name` → `set_name__Dog`, `set_name__Person`
  - [x] Type substitution: 関数本体の `TypeVar`/`Named("T")` を具象型に置換
  - [x] Call-site rewriting: `set_name(dog)` → `set_name__Dog(dog)`
- [x] Rust codegen: 構造的制約付き関数をスキップ（monomorphized 版のみ emit）
- [x] Formatter: 構造的制約の出力 (`[T: { name: String, .. }]`)
- [x] Tests: 16 テスト全 pass（構造的制約 3 + monomorphization 2 + 既存 11）

### Remaining

- [x] Transitive monomorphization: fixed-point loop で A → B → C チェーンを解決
- [x] Multiple structural bounds per function: 既に動作（bindings が全 TypeVar を追跡）
- [ ] Container protocols integration (`F: Mappable`) — 設計は下記
- [ ] TS target: 構造的制約を型注釈として出力（mono 不要、structural typing）

## Codegen model

```almide
fn set_name[T: { name: String, .. }](x: T, n: String) -> T =
  { ...x, name: n }

// Dog で呼ばれた → Dog 版を生成:
// fn set_name__Dog(x: Dog, n: String) -> Dog { Dog { name: n, ..x } }

// Person で呼ばれた → Person 版を生成:
// fn set_name__Person(x: Person, n: String) -> Person { Person { name: n, ..x } }
```

- 1 function × N concrete types = N Rust functions
- 関数本体が具体型を知っている → field access, spread, return が型安全
- 元の generic 関数は emit しない（specialized 版のみ）

## Name mangling

```
set_name[T=Dog]                 → set_name__Dog
set_name[T=Person]              → set_name__Person
set_name[T={name, age}]         → set_name__age_name
transform[T=List[Int]]          → set_name__List_Int
```

## Affected files

| File | Change |
|------|--------|
| `src/ast.rs` | `GenericParam` に `structural_bound` 追加 |
| `src/types.rs` | `FnSig.structural_bounds`, `TypeEnv.structural_bounds` 追加 |
| `src/parser/types.rs` | `T: { .. }` パース |
| `src/check/mod.rs` | 構造的制約の登録・解除 |
| `src/check/calls.rs` | call-site unification, `check_member_access` 拡張 |
| `src/mono.rs` (new) | モノモーフィゼーションパス |
| `src/emit_rust/program.rs` | 構造的制約付き関数スキップ |
| `src/main.rs` | パイプラインに mono パス挿入 |
| `src/fmt.rs` | 構造的制約のフォーマット |
| `src/lib.rs` | `pub mod mono` |

## Risk

- **Code size explosion**: N types × M functions = N×M Rust functions。実際は small N (< 10) が多いので許容範囲
- **Compile time**: Instantiation discovery は O(calls × types)。プログラムが大きくなった時の性能は要監視

## Container Protocols Design

### Problem
`list.map`, `option.map`, `result.map` は同じ「中の値に関数を適用」という意味だが、別々の stdlib 関数。
ユーザーが generic に書くには:

```almide
// これは書けない — list.map は List 専用
fn double_all[F: Mappable](container: F[Int]) -> F[Int] =
  container.map((x) => x * 2)
```

### Almide のアプローチ: Trait なし、Convention ベース

Almide は trait/typeclass を持たない。代わりに **固定 convention** で protocol を表現:

```almide
// Protocol = 構造的制約 + 固定メソッド名
// Mappable は「.map(f) を持つコンテナ」

fn transform[C: Mappable[Int, Int]](c: C, f: fn(Int) -> Int) -> C =
  c.map(f)
```

### 実装方針: Structural bounds の型パラメータ版

```
Mappable[A, B] = { map: fn(fn(A) -> B) -> Self[B] }
```

これは HKT (Higher-Kinded Types) に近いが、Almide では以下で代替:

1. **Protocol = 固定名の関数セット** — `Mappable` は `map` メソッドを持つことを意味
2. **Mono pass で解決** — `C = List[Int]` のとき、`c.map(f)` → `list.map(c, f)` に書き換え
3. **Protocol 対応型は固定** — List, Option, Result の3つ。ユーザー定義不可（Canonicity）

### Codegen model

```almide
fn transform[C: Mappable](xs: C[Int], f: fn(Int) -> Int) -> C[Int] =
  xs.map(f)

// C = List[Int] の場合:
// fn transform__List_Int(xs: Vec<i64>, f: impl Fn(i64) -> i64) -> Vec<i64> {
//     list_map(xs, f)
// }

// C = Option[Int] の場合:
// fn transform__Option_Int(xs: Option<i64>, f: impl Fn(i64) -> i64) -> Option<i64> {
//     xs.map(f)
// }
```

### Priority

Container protocols は表現力は高いが:
- 使用頻度は低い（大半のコードは具体型で書く）
- HKT に近い複雑さがあり、LLM の modification survival rate に影響する可能性
- 実装コストが高い（型パラメータの kind 判定、protocol 解決、mono 拡張）

**判定: on-hold**。structural bounds + derive conventions で当面の需要は満たせる。
Container protocols は「必要になってから」実装する。
