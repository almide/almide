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
