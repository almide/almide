## Almide Rules
- No `return` — last expression is the return value
- `if` uses `then`: `if x > 0 then "yes" else "no"` (not `if (x > 0) { }`)
- Strings: `"Hello, ${name}"`, concatenation is `++` (not `+`)
- Lists are immutable: `xs ++ [y]` returns new list
- Mutable: `var x = 0` (not `let mut`)
- No null: use `some(v)` / `none` with `Option[T]`
- No try/catch: use `ok(v)` / `err(e)` with `Result[T, E]`
- Equality: `==` is deep equality, works on records and lists
- Tests: `test "name" { assert_eq(a, b) }`
