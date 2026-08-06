> **Deprecated (E039, ADR-0007)**: `result.collect` / `collect_map` are in their removal window — all-errors collection is spelled with `partition`.

# result

Result type operations. auto-imported.

### `result.map(r: Result[A, E], f: Fn[A] -> B) -> Result[B, E]`

Transform the ok value using a function. If err, passes through unchanged.

```almd
result.map(ok(2), fn(x) => x * 10)
```

### `result.map_err(r: Result[A, E], f: Fn[E] -> F) -> Result[A, F]`

Transform the err value using a function. If ok, passes through unchanged.

```almd
result.map_err(err("fail"), fn(e) => "wrapped: " ++ e)
```

### `result.flat_map(r: Result[A, E], f: Fn[A] -> Result[B, E]) -> Result[B, E]`

Chain a Result-returning function on the ok value. Flattens nested Results.

```almd
result.and_then(ok(5), fn(x) => if x > 0 then ok(x) else err("negative"))
```

### `result.unwrap_or(r: Result[A, E], default: A) -> A`

Get the ok value, or return a default if err.

```almd
result.unwrap_or(err("fail"), 0)
```

### `result.unwrap_or_else(r: Result[A, E], f: Fn[E] -> A) -> A`

Get the ok value, or compute a default from the error using a function.

```almd
result.unwrap_or_else(err("fail"), fn(e) => string.len(e))
```

### `result.is_ok(r: Result[A, E]) -> Bool`

Check if the Result is ok.

```almd
result.is_ok(ok(42))
```

### `result.is_err(r: Result[A, E]) -> Bool`

Check if the Result is err.

```almd
result.is_err(err("fail"))
```

### `result.to_option(r: Result[A, E]) -> Option[A]`

Convert ok to some, err to none. Discards the error value.

```almd
result.to_option(ok(42))
```

### `result.to_err_option(r: Result[A, E]) -> Option[E]`

Convert err to some, ok to none. Discards the ok value.

```almd
result.to_err_option(err("fail"))
```

### `result.collect(rs: List[Result[T, E]]) -> Result[List[T], List[E]]`

Collect a list of Results. All ok → ok(values), any err → err(all_errors).

```almd
result.collect([ok(1), ok(2), ok(3)]) // => ok([1, 2, 3])
```

### `result.partition(rs: List[Result[T, E]]) -> (List[T], List[E])`

Partition a list of Results into ok values and err values.

```almd
result.partition([ok(1), err("x"), ok(2)]) // => ([1, 2], ["x"])
```

### `result.collect_map(xs: List[T], f: Fn[T] -> Result[U, E]) -> Result[List[U], List[E]]`

Map a function over a list and collect Results. All ok → ok(values), any err → err(all_errors).

```almd
result.collect_map([1, 2, 3], fn(x) => if x > 0 then ok(x) else err("neg"))
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
