# Borrow/Clone Gaps [ACTIVE]

Rust codegen が変数の clone を挿入し損ねるケースを徹底的に潰す。

## Root Cause

`use_count` はシンタクティック（IR の Var ノードを数える）で、セマンティック（制御フロー分岐・ループ反復）を考慮しない。clone 挿入ロジックは `use_count > 1 && !is_copy` だが、以下のケースで不足する。

## Known Cases

### Case 1: 変数が関数引数 + 文字列補間で使われる（OPEN）

```almide
let dir = "output"
process.exec("mkdir", [dir])     // dir moved
println("Saved: ${dir}")         // ERROR: use after move
```

`use_count = 2` で clone は挿入されるが、生成コードの順序によっては move 後のアクセスになる。

### Case 2: 変数が if/else の片方のブランチで move + 後続で再利用（OPEN）

```almide
let x = some_list()
let result = if cond then [] else x   // x moved in else
let other = x                          // ERROR: x might be moved
```

### Case 3: ネスト for-in のイテラブル（FIXED: ae9b64e）

```almide
for x in xs {
  for y in ys { ... }   // ys moved on first outer iteration
}
```

Fix: for-in iterable が変数なら常に clone。

### Case 4: match ブランチ間で Result 型と非 Result 型が混在（FIXED: d94da78）

```almide
effect fn dispatch(cmd: String) -> Unit = {
  match cmd {
    "a" => fn_a()    // effect fn → Result
    _ => err("bad")  // Result
  }
}
```

Fix: `is_result_expr` から `Try` を除外。

## Clone Decision Points（全箇所）

| 箇所 | ファイル | 行 | 現状ロジック |
|------|---------|-----|-------------|
| Var 参照 | lower_rust_expr.rs:19-36 | `use_count > 1 && !is_copy && !is_borrowed` |
| for-in iterable | lower_rust_expr.rs:123-139 | 常に clone（Range, ListLiteral 除く） |
| Record spread base | lower_rust_expr.rs:257-266 | `!is_single_use_var` |
| Member access | lower_rust_expr.rs:268-273 | `!is_copy && !is_single_use_var` |
| String interp | lower_rust_expr.rs:289-305 | Var 参照ルールに委譲 |

## Fix Strategy

根本修正: Var 参照の clone 判定を保守的にする。

**現状**: `use_count > 1 && !is_copy && !is_borrowed_param` → clone
**修正案**: 非 Copy 型の変数は**常に clone**。single-use 最適化は `use_count == 1` の場合のみ clone をスキップ。

これにより不要な clone が増える可能性があるが、Rust コンパイラの最適化パスが不要 clone を除去するので実行時性能への影響は最小限。正しさを優先。
