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

### `testing.assert_snapshot(actual: String, expected: String) -> Unit`

Assert that `actual` equals the snapshot written at the call site. The
expectation is the literal itself — there is no sidecar file — and the accept
step rewrites it in place: start with `""`, run
`almide test --update-snapshots <file>`, and the found value is written back as
the second argument (a heredoc when it spans lines). A later drift fails the
plain `almide test` run with a diff and the accept hint; `--update-snapshots`
rewrites it again. In CI mode (`--ci` or `CI=true`) nothing is ever written —
a new or drifted snapshot fails there, so snapshots are committed and reviewed
like code.

```almd check
import testing

fn render(xs: List[Int]) -> String = xs |> list.map((x) => "item ${x}") |> list.join("\n")

test "single line" {
  testing.assert_snapshot("hello", "hello")
}

test "multi line, written back as a heredoc" {
  testing.assert_snapshot(render([1, 2]), """
    item 1
    item 2
    """)
}
```

The second argument must be a plain string literal (no interpolation) for the
accept step to rewrite it; a mismatch aborts the run with the structured block
`Error: snapshot mismatch` / `at:` / `expected:` / `found:` and exit code 1 on
every target (contract C-336).

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (10 functions)

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
testing.assert_snapshot(actual: String, expected: String) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
