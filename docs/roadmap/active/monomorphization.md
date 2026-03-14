# Monomorphization [ACTIVE]

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

- [ ] Container protocols integration (`F: Mappable`)
- [ ] Transitive monomorphization: A → B → C のチェーン呼び出し
- [ ] Multiple structural bounds per function (`[A: { .. }, B: { .. }]`)
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
