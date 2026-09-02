# testing

Test assertions. import testing.

### `testing.assert_throws(f: fn() -> Unit, expected: String) -> Unit`

Assert that a function throws an error containing the expected message.

```almd check
import testing

test "panics with the expected message" {
  testing.assert_throws(() => panic("oh no"), "oh no")
}
```

### `testing.assert_contains(haystack: String, needle: String) -> Unit`

Assert that a string contains a substring.

```almd check
import testing

test "substring present" {
  testing.assert_contains("hello world", "world")
}
```

### `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit`

Assert two floats are approximately equal within tolerance.

```almd check
import testing

test "close enough" {
  testing.assert_approx(3.14, 3.14159, 0.01)
}
```

### `testing.assert_gt(a: Int, b: Int) -> Unit`

Assert that a is greater than b.

```almd check
import testing

test "greater" {
  testing.assert_gt(10, 5)
}
```

### `testing.assert_lt(a: Int, b: Int) -> Unit`

Assert that a is less than b.

```almd check
import testing

test "less" {
  testing.assert_lt(3, 7)
}
```

### `testing.assert_some(opt: Option[String]) -> Unit`

Assert that an Option is some (not none).

```almd check
import testing

test "option is some" {
  testing.assert_some(some("value"))
}
```

### `testing.assert_ok(result: Result[String, String]) -> Unit`

Assert that a Result is ok (not err).

```almd check
import testing

test "result is ok" {
  let result: Result[String, String] = ok("success")
  testing.assert_ok(result)
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (9 functions)

```
testing.assert_throws(f: () -> Unit, expected: String) -> Unit
testing.assert_contains(haystack: String, needle: String) -> Unit
testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit
testing.assert_gt(a: Int, b: Int) -> Unit
testing.assert_lt(a: Int, b: Int) -> Unit
testing.assert_some(opt: Option[A]) -> Unit
testing.assert_ok(result: Result[A, B]) -> Unit
testing.assert_none(opt: Option[A]) -> Unit
testing.assert_err(result: Result[A, B]) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
