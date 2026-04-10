<!-- description: UFCS method calls fail type resolution in dependency module context -->
<!-- done: 2026-04-02 -->
# UFCS Resolution in Dependency Modules

## Status

調査の結果、bare UFCS (`xs.map(fn)`) は dependency module 内でも正しく解決されることを確認。almide-lander ビルドの実際のブロッカーは **pipe `|>` と `+` (リスト結合) の演算子優先順位問題** だった。→ [pipe-operator-precedence.md](pipe-operator-precedence.md)

以下は当初の想定と調査結果の記録。

## Original Problem (Revised)

almide-lander ビルド時に dependency module (almide-bindgen) のコード生成が失敗する。

```almide
// javascript.almd — ビルド失敗の原因箇所
let exports = types |> list.flat_map((t) => ...) + (functions |> list.map((f) => ...))
```

当初は bare UFCS が原因と考えていたが、実際にはパイプ `|>` の優先順位が `+` より低いため:
- `types |> (list.flat_map(f) + ...)` とパースされる
- `list.flat_map(f)` が1引数で呼ばれ、`types` がパイプの最外殻に回る
- 生成された Rust コードが壊れる

## Verification

```bash
# UFCS自体は dependency module でも動く
# /tmp/ufcs-test2/src/mod.almd:
#   fn test(xs: List[String]) -> String = xs.map((s) => string.to_upper(s)).join(", ")
# → almide_rt_list_join(&(...).into_iter().map(...).collect(), ...) ← 正しい
```

bare UFCS の全パターン（`xs.map(fn)`, `xs.join(sep)`, `xs.filter(fn).join(sep)`, chain UFCS）が `import self` module 内で正しく `list.*` / `string.*` に解決されることを確認。

## Remaining Concern

型推論が `Unknown` になるケース（複雑な推論チェーン、パッケージ間の型伝搬）で UFCS のフォールバックが誤ったモジュールを選ぶ可能性は残る。

```
resolve_ufcs_candidates("join") → ["string", "list"]
resolve_ufcs_module("join")     → "string" (first candidate)
```

`List[T].join(sep)` に対して型が Unknown の場合、`string.join` にフォールバックする。ただし現時点ではこの問題に遭遇する具体的なケースは未確認。

## Files

- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — `resolve_unresolved_ufcs`, `resolve_module_from_ty`
- `crates/almide-types/src/stdlib_info.rs` — `resolve_ufcs_candidates`, `resolve_ufcs_module`
- `crates/almide-frontend/src/lower/calls.rs` — `lower_call_target` の builtin module 検出
