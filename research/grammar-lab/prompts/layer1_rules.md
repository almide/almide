## Almide Rules
- No `return` — the last expression is the value
- `let` is a statement, never an expression (no `let ... in`). Put local bindings in a block `{ }`;
  the block's last line is its value:
    fn area(w: Int, h: Int) -> Int = {
      let s = w * h
      s + 1
    }
- `if` uses `then`: `if x > 0 then "yes" else "no"`
- `+` concatenates strings and lists: `"a" + "b"`, `xs + [y]`
- Lambdas: `(x) => x + 1`; destructure a pair in the parameter: `((a, b)) => a + b`
- Pipes: `xs |> list.map((x) => x * 2) |> list.filter((x) => x > 0)`
- Lists are immutable; `xs[i]` reads an element; `list.take`, `list.drop`, `list.len`, `list.sort_by`
- `match` with list patterns: `match xs { [] => 0, [h, ..rest] => h }`
- No null: `some(v)` / `none` with `Option[T]`; `??` gives a fallback
- Equality: `==` is deep equality on records and lists
- Tests: `test "name" { assert_eq(a, b) }`
