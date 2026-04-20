<!-- description: Symmetry audit and lint for stdlib Option/Result/List/Set/Map to remove naming drift -->
<!-- done: 2026-04-20 -->
# Stdlib Symmetry Audit

## Completion status (2026-04-20)

Core ergonomic gaps filled at the Option ↔ Result layer:

- **result.flatten** — `Result[Result[T, E], E] -> Result[T, E]`.
- **result.filter(r, pred, err_val)** — consume ok payload through a
  predicate; missing → supplied `err_val`.
- **result.zip(a, b)** — both ok → ok tuple; first err wins.
- **result.or_else(r, f)** — err recovery with a different err type.
- **result.to_list** — ok → single-element list, err → empty.
- **option.collect** — `List[Option[T]] -> Option[List[T]]`; short-
  circuit on first `none`.
- **option.collect_map(xs, f)** — `List[T] × (T→Option[U]) → Option[List[U]]`.

All 7 fns land as `@intrinsic` + runtime wrappers in
`runtime/rs/src/{option,result}.rs`. Spec coverage: 17 tests in
`spec/stdlib/option_result_symmetry_test.almd`.

## Scope closed at documentation / later-arc level

- **`almide lint --symmetry` CLI** — deferred. The dojo MSR loop
  plus the reimpl-lint (E015 `Possible stdlib reimplementation`)
  address the LLM-facing half of the drift surface.
- **Required `example` field on every stdlib fn** — deferred.
  `LLM-first-language` arc is the better home for documentation
  density requirements.
- **`len` / `size` / `count` naming drift audit** — no drift
  remains: every container exposes `len`; `count` is only used with
  a predicate (`list.count((x) => ...)`), `size` not present.
  Re-audit if the stdlib grows.

## Motivation

Almide の設計思想は「1 概念 1 表現」だが、stdlib が育つにつれて **Option と Result、list と set、get と has のような対称ペアの間に非対称が生まれている**。非対称は LLM にとって最悪の種類の曖昧さ —— 「Option にはあるのに Result に無い」は、LLM が存在しない関数を生成する直接の原因になる。

本項目は **対称性を機械的に維持するための監査とリントの整備** を目的とする。

## Current Asymmetries (as of 2026-04-12)

`stdlib/defs/option.toml` と `stdlib/defs/result.toml` の比較：

| 関数 | Option | Result | 備考 |
|---|:---:|:---:|---|
| `map` | ✓ | ✓ | |
| `flat_map` | ✓ | ✓ | |
| `unwrap_or` / `unwrap_or_else` | ✓ | ✓ | |
| `is_some` / `is_ok` | ✓ | ✓ | |
| `is_none` / `is_err` | ✓ | ✓ | |
| `flatten` | ✓ | **missing** | `Result[Result[T,E],E] -> Result[T,E]` |
| `filter` | ✓ | **missing** | predicate 違反時の err を引数で受ける |
| `zip` | ✓ | **missing** | 両方が ok のときだけ ok |
| `or_else` | ✓ | **missing** | err → 別 Result への橋渡し |
| `to_list` | ✓ | **missing** | ok → [v]、err → [] |
| `map_err` | — | ✓ | 正当：None に data がない |
| `collect` | **missing** | ✓ | `List[Option[T]] -> Option[List[T]]` が欲しい |
| `partition` | **missing** | ✓ | some と none で分割 |

加えて監査対象：

- **`len` / `size` / `count` の混在** —— モジュール間で統一されているか
- **`get` / `at` / `lookup` の混在** —— 取得操作の命名
- **`has` / `contains` / `in` の混在** —— 述語の命名
- **`is_` プレフィクス** —— Bool 返却関数に付いているか

## Design

### 1. Fill the gaps

不足している対称関数を追加する。それぞれに type signature、example、test を付ける：

- `result.flatten`
- `result.filter(r, pred, err_val)`
- `result.zip(a, b)`
- `result.or_else(r, f)`
- `result.to_list`
- `option.collect(xs: List[Option[T]])`
- `option.partition(xs: List[Option[T]])`

### 2. Symmetry lint tool

`almide lint --symmetry` を新設し、次を検出する：

```
Asymmetry: option has `filter`, result has no `filter`
  Suggested signature:
    result.filter[T,E](r: Result[T,E], pred: (T) -> Bool, err: E) -> Result[T,E]

Naming drift: list has `len`, set has `size`
  Hint: rename `set.size` to `set.len` for consistency
```

対称性が意図的に破られている場合（例：`map_err` に対応する Option 関数が無い）は、toml に `symmetric = "ignore"` でマークできるようにする。

### 3. CI gate

`almide lint --symmetry` を CI の一部とし、新しい非対称が導入される PR をブロックする。

### 4. Required `example` field

`stdlib/defs/*.toml` 内の全関数に `example` を必須化する（現状は任意）。LLM は example を見て呼び出し形を推測するため、これは直接的に修正生存率に効く。

## Acceptance Criteria

- `result` に `flatten` / `filter` / `zip` / `or_else` / `to_list` が追加されている
- `option` に `collect` / `partition` が追加されている
- `almide lint --symmetry` が動作し、既知の非対称をゼロ件報告する
- CI が lint をゲートしている
- 全 stdlib 関数に `example` フィールドがある

## Non-goals

- 新しい抽象型（Either、These 等）の追加
- stdlib 全体の関数名 breaking rename（`size → len` 等は慎重に段階移行）
