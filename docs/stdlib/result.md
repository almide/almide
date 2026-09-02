> **Removed (ADR-0007)**: `result.collect` / `collect_map` are gone — a call is now an ordinary [E002](../diagnostics/E002.md). All-errors collection is spelled with `result.partition`.

# result

Result type operations. auto-imported.

### `result.map(r: Result[A, E], f: Fn[A] -> B) -> Result[B, E]`

Transform the ok value using a function. If err, passes through unchanged.

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.map(ok(2), (x) => x * 10)))
  println(show(result.map(err("fail"), (x) => x * 10)))
}
```
```output
ok(20)
err("fail")
```

### `result.map_err(r: Result[A, E], f: Fn[E] -> F) -> Result[A, F]`

Transform the err value using a function. If ok, passes through unchanged.

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.map_err(err("fail"), (e) => "wrapped: " + e)))
  println(show(result.map_err(ok(1): Result[Int, String], (e) => "wrapped: " + e)))
}
```
```output
err("wrapped: fail")
ok(1)
```

### `result.flat_map(r: Result[A, E], f: Fn[A] -> Result[B, E]) -> Result[B, E]`

Chain a Result-returning function on the ok value. Flattens nested Results.

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.flat_map(ok(5), (x) => if x > 0 then ok(x) else err("negative"))))
  println(show(result.flat_map(ok(-5), (x) => if x > 0 then ok(x) else err("negative"))))
}
```
```output
ok(5)
err("negative")
```

### `result.unwrap_or(r: Result[A, E], default: A) -> A`

Get the ok value, or return a default if err.

```almd run
fn main() -> Unit = {
  println("${result.unwrap_or(err("fail"), 0)}")
  println("${result.unwrap_or(ok(7): Result[Int, String], 0)}")
}
```
```output
0
7
```

### `result.unwrap_or_else(r: Result[A, E], f: Fn[E] -> A) -> A`

Get the ok value, or compute a default from the error using a function.

```almd run
fn main() -> Unit = {
  println("${result.unwrap_or_else(err("fail"), (e) => string.len(e))}")
}
```
```output
4
```

### `result.is_ok(r: Result[A, E]) -> Bool`

Check if the Result is ok.

```almd run
fn main() -> Unit = {
  println("${result.is_ok(ok(42): Result[Int, String])}")
}
```
```output
true
```

### `result.is_err(r: Result[A, E]) -> Bool`

Check if the Result is err.

```almd run
fn main() -> Unit = {
  println("${result.is_err(err("fail"): Result[Int, String])}")
}
```
```output
true
```

### `result.to_option(r: Result[A, E]) -> Option[A]`

Convert ok to some, err to none. Discards the error value.

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(result.to_option(ok(42): Result[Int, String])))
  println(show(result.to_option(err("fail"): Result[Int, String])))
}
```
```output
some(42)
none
```

### `result.to_err_option(r: Result[A, E]) -> Option[E]`

Convert err to some, ok to none. Discards the ok value.

```almd run
fn show(o: Option[String]) -> String = match o {
  some(e) => "some(\"${e}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(result.to_err_option(err("fail"): Result[Int, String])))
  println(show(result.to_err_option(ok(42): Result[Int, String])))
}
```
```output
some("fail")
none
```

### `result.partition(rs: List[Result[T, E]]) -> (List[T], List[E])`

Partition a list of Results into ok values and err values. This is the
all-errors collection primitive — the removed `result.collect` /
`result.collect_map` are spelled with it:

```almd run
fn main() -> Unit = {
  println("${result.partition([ok(1), err("x"), ok(2)])}")
}

// result.collect(rs)          → let (oks, errs) = result.partition(rs)
// result.collect_map(xs, f)   → let (oks, errs) = result.partition(list.map(xs, f))
```
```output
([1, 2], ["x"])
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (15 functions)

```
result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]
result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]
result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]
result.unwrap_or(r: Result[A, E], default: A) -> A
result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A
result.is_ok(r: Result[A, E]) -> Bool
result.is_err(r: Result[A, E]) -> Bool
result.to_option(r: Result[A, E]) -> Option[A]
result.to_err_option(r: Result[A, E]) -> Option[E]
result.partition(rs: List[Result[T, E]]) -> ()
result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]
result.to_list(r: Result[A, E]) -> List[A]
result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(), E]
result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]
result.filter(r: Result[A, E], pred: (A) -> Bool, err_val: E) -> Result[A, E]
```

<!-- END GENERATED SIGNATURE INDEX -->
