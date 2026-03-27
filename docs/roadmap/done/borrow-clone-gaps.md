<!-- description: Fix cases where Rust codegen fails to insert necessary clones -->
# Borrow/Clone Gaps

Rust codegen が変数の clone を挿入し損ねるケースを徹底的に潰す。

## Root Cause

`use_count` はシンタクティック（IR の Var ノードを数える）で、セマンティック（制御フロー分岐・ループ反復）を考慮しない。clone 挿入ロジックは `use_count > 1 && !is_copy` だが、以下のケースで不足する。

## Known Cases

### Case 1: 変数が関数引数 + 文字列補間で使われる（FIXED: fc2b17f）

```almide
let dir = "output"
process.exec("mkdir", [dir])     // dir moved
println("Saved: ${dir}")         // ERROR: use after move
```

`use_count = 2` で clone は挿入されるが、生成コードの順序によっては move 後のアクセスになる。

### Case 2: 変数が if/else の片方のブランチで move + 後続で再利用（FIXED: fc2b17f）

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

### Case 5: Default args の式に型が付かない（FIXED）

```almide
fn greet(name: String, prefix: String = "Hello") -> String =
  "${prefix}, ${name}!"
```

`[ICE] lower: missing type for expr id=NNN` が出る。テスト自体は pass するが、checker が default 値の式に ExprId → Ty マッピングを生成していない。

### Case 6: 再帰 variant の Box deref（FIXED: next commit）

```almide
type IntList = | Cons(Int, IntList) | Nil
fn sum(xs: IntList) -> Int = match xs {
  Cons(head, tail) => head + sum(tail)   // tail is Box<IntList>, needs *tail
  Nil => 0
}
```

Auto-Box で `IntList` を `Box<IntList>` に変換しているが、パターンマッチで binding された `tail` を関数に渡す際に `*tail` が生成されない。

### Case 7: ネスト impl Fn 返却（FIXED）

```almide
fn curry_add(a: Int) -> (Int) -> (Int) -> Int = (b) => (c) => a + b + c
```

Rust は `impl Fn() -> impl Fn()` を関数戻り値に許可しない。`Box<dyn Fn>` にするか、型消去が必要。

### Case 8: 関数変数を HOF に渡す codegen（FIXED）

```almide
fn transform(xs: List[Int], f: (Int) -> Int, pred: (Int) -> Bool) -> List[Int] =
  xs |> list.map(f) |> list.filter(pred)
```

`list.map(xs, f)` で `f` がクロージャとして展開されず、値として渡される。stdlib の TOML テンプレート `|{f.args}| {{ {f.body} }}` が inline lambda 前提で、変数参照に対応していない。

### Case 9: closure 内の var mutation（FIXED）

```almide
fn running_sum(xs: List[Int]) -> List[Int] = {
  var acc = 0
  list.map(xs, (x) => { acc = acc + x; acc })
}
```

`Fn` クロージャで `acc` に代入できない。`FnMut` が必要だが、ランタイムの `almide_rt_list_map` は `Fn` を受け取る。

## Fix Strategy

根本修正: Var 参照の clone 判定を保守的にする。

**現状**: `use_count > 1 && !is_copy && !is_borrowed_param` → clone
**修正案**: 非 Copy 型の変数は**常に clone**。single-use 最適化は `use_count == 1` の場合のみ clone をスキップ。

これにより不要な clone が増える可能性があるが、Rust コンパイラの最適化パスが不要 clone を除去するので実行時性能への影響は最小限。正しさを優先。
