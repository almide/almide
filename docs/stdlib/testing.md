# testing

Test assertions. import testing.

### `testing.assert_throws(f: fn() -> Unit, expected: String) -> Unit`

Assert that a function throws an error containing the expected message.

```almd
testing.assert_throws(fn() => panic("oh no"), "oh no")
```

### `testing.assert_contains(haystack: String, needle: String) -> Unit`

Assert that a string contains a substring.

```almd
testing.assert_contains("hello world", "world")
```

### `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit`

Assert two floats are approximately equal within tolerance.

```almd
testing.assert_approx(3.14, 3.14159, 0.01)
```

### `testing.assert_gt(a: Int, b: Int) -> Unit`

Assert that a is greater than b.

```almd
testing.assert_gt(10, 5)
```

### `testing.assert_lt(a: Int, b: Int) -> Unit`

Assert that a is less than b.

```almd
testing.assert_lt(3, 7)
```

### `testing.assert_some(opt: Option[String]) -> Unit`

Assert that an Option is some (not none).

```almd
testing.assert_some(some("value"))
```

### `testing.assert_ok(result: Result[String, String]) -> Unit`

Assert that a Result is ok (not err).

```almd
testing.assert_ok(ok("success"))
```
