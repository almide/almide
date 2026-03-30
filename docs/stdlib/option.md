# option

Option type operations. auto-imported.

### `option.map(o: Option[A], f: Fn[A] -> B) -> Option[B]`

Transform the inner value using a function. If none, returns none.

```almd
option.map(some(2), (x) => x * 10) // => some(20)
```

### `option.flat_map(o: Option[A], f: Fn[A] -> Option[B]) -> Option[B]`

Chain an Option-returning function on the inner value. Flattens nested Options.

```almd
option.flat_map(some(5), (x) => if x > 0 then some(x) else none)
```

### `option.flatten(o: Option[Option[A]]) -> Option[A]`

Flatten a nested Option. some(some(x)) becomes some(x), some(none) becomes none.

```almd
option.flatten(some(some(42))) // => some(42)
```

### `option.unwrap_or(o: Option[A], default: A) -> A`

Get the inner value, or return a default if none.

```almd
option.unwrap_or(none, 0) // => 0
```

### `option.unwrap_or_else(o: Option[A], f: Fn[Unit] -> A) -> A`

Get the inner value, or compute a default using a function.

```almd
option.unwrap_or_else(none, () => 42) // => 42
```

### `option.is_some(o: Option[A]) -> Bool`

Check if the Option contains a value.

```almd
option.is_some(some(42)) // => true
```

### `option.is_none(o: Option[A]) -> Bool`

Check if the Option is none.

```almd
option.is_none(none) // => true
```

### `option.to_result(o: Option[A], err: String) -> Result[A, String]`

Convert some to ok, none to err with the given error message.

```almd
option.to_result(some(42), "missing") // => ok(42)
```

### `option.filter(o: Option[A], f: Fn[A] -> Bool) -> Option[A]`

Keep the value if it satisfies the predicate, otherwise return none.

```almd
option.filter(some(5), (x) => x > 3) // => some(5)
```

### `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]`

Combine two Options into an Option of a tuple. None if either is none.

```almd
option.zip(some(1), some(2)) // => some((1, 2))
```

### `option.or_else(o: Option[A], f: Fn[Unit] -> Option[A]) -> Option[A]`

Return the Option if some, otherwise call the function to produce an alternative.

```almd
option.or_else(none, () => some(42)) // => some(42)
```

### `option.to_list(o: Option[A]) -> List[A]`

Convert some(x) to [x], none to [].

```almd
option.to_list(some(42)) // => [42]
```
