<!-- description: Fix cases where Rust codegen fails to insert necessary clones -->
<!-- done: 2026-03-17 -->
# Borrow/Clone Gaps

Thoroughly eliminate cases where Rust codegen fails to insert necessary variable clones.

## Root Cause

`use_count` is syntactic (counts Var nodes in the IR) and does not consider semantics (control flow branching, loop iterations). The clone insertion logic `use_count > 1 && !is_copy` is insufficient for the following cases.

## Known Cases

### Case 1: Variable used in both function arguments and string interpolation (FIXED: fc2b17f)

```almide
let dir = "output"
process.exec("mkdir", [dir])     // dir moved
println("Saved: ${dir}")         // ERROR: use after move
```

`use_count = 2` inserts a clone, but depending on generated code order, access may occur after move.

### Case 2: Variable moved in one if/else branch + reused later (FIXED: fc2b17f)

```almide
let x = some_list()
let result = if cond then [] else x   // x moved in else
let other = x                          // ERROR: x might be moved
```

### Case 3: Nested for-in iterable (FIXED: ae9b64e)

```almide
for x in xs {
  for y in ys { ... }   // ys moved on first outer iteration
}
```

Fix: Always clone when for-in iterable is a variable.

### Case 4: Result and non-Result types mixed across match branches (FIXED: d94da78)

```almide
effect fn dispatch(cmd: String) -> Unit = {
  match cmd {
    "a" => fn_a()    // effect fn → Result
    _ => err("bad")  // Result
  }
}
```

Fix: Exclude `Try` from `is_result_expr`.

## Clone Decision Points (all locations)

| Location | File | Line | Current Logic |
|------|---------|-----|-------------|
| Var reference | lower_rust_expr.rs:19-36 | `use_count > 1 && !is_copy && !is_borrowed` |
| for-in iterable | lower_rust_expr.rs:123-139 | Always clone (except Range, ListLiteral) |
| Record spread base | lower_rust_expr.rs:257-266 | `!is_single_use_var` |
| Member access | lower_rust_expr.rs:268-273 | `!is_copy && !is_single_use_var` |
| String interp | lower_rust_expr.rs:289-305 | Delegates to Var reference rules |

### Case 5: Default arg expressions lack type annotation (FIXED)

```almide
fn greet(name: String, prefix: String = "Hello") -> String =
  "${prefix}, ${name}!"
```

`[ICE] lower: missing type for expr id=NNN` is emitted. Tests themselves pass, but the checker is not generating ExprId → Ty mappings for default value expressions.

### Case 6: Recursive variant Box deref (FIXED: next commit)

```almide
type IntList = | Cons(Int, IntList) | Nil
fn sum(xs: IntList) -> Int = match xs {
  Cons(head, tail) => head + sum(tail)   // tail is Box<IntList>, needs *tail
  Nil => 0
}
```

Auto-Box converts `IntList` to `Box<IntList>`, but when passing pattern-matched binding `tail` to a function, `*tail` is not generated.

### Case 7: Nested impl Fn return (FIXED)

```almide
fn curry_add(a: Int) -> (Int) -> (Int) -> Int = (b) => (c) => a + b + c
```

Rust does not allow `impl Fn() -> impl Fn()` as a function return type. Requires `Box<dyn Fn>` or type erasure.

### Case 8: Codegen for passing function variables to HOF (FIXED)

```almide
fn transform(xs: List[Int], f: (Int) -> Int, pred: (Int) -> Bool) -> List[Int] =
  xs |> list.map(f) |> list.filter(pred)
```

`list.map(xs, f)` does not expand `f` as a closure; it passes as a value. The stdlib TOML template `|{f.args}| {{ {f.body} }}` assumes inline lambda and does not handle variable references.

### Case 9: var mutation inside closure (FIXED)

```almide
fn running_sum(xs: List[Int]) -> List[Int] = {
  var acc = 0
  list.map(xs, (x) => { acc = acc + x; acc })
}
```

Cannot assign to `acc` in a `Fn` closure. `FnMut` is needed, but the runtime `almide_rt_list_map` takes `Fn`.

## Fix Strategy

Fundamental fix: make clone decisions for Var references more conservative.

**Current**: `use_count > 1 && !is_copy && !is_borrowed_param` → clone
**Proposed**: Non-Copy type variables are **always cloned**. Single-use optimization only skips clone when `use_count == 1`.

This may increase unnecessary clones, but Rust compiler optimization passes will eliminate them, keeping runtime performance impact minimal. Correctness takes priority.
