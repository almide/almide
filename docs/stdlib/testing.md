# testing

Test assertions. import testing.

### `testing.assert_aborts[T](expected: String, f: () -> T) -> Unit`

Assert that `f` aborts with text containing `expected`. Only `f` runs, in a
child process (native) or a fresh instance (wasm) that starts the program
afresh, so structured Almide aborts count — `panic`, an index out of bounds, a division by
zero, a failed assert — and the test continues after the call. `f` must be a
lambda written in the call that captures nothing of the test (E098), and the
call must sit directly in a `test` block (E099). The child's output is shown
only when the assertion fails. A raw host panic, unstructured Wasm trap,
or non-abort exit is not accepted as an Almide abort.

```almd check
import testing

fn nth(xs: List[Int], i: Int) -> Int = xs[i]

test "reading past the end aborts" {
  testing.assert_aborts("index out of bounds", () => nth([1, 2, 3], 5))
  testing.assert_aborts("PANIC: oh no", () => panic("oh no"))
  assert_eq(nth([1, 2, 3], 0), 1)
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

### `testing.assert_gt(a: A, b: A) -> Unit`

Assert that a is greater than b.

```almd check
import testing

test "greater" {
  testing.assert_gt(10, 5)
}
```

### `testing.assert_lt(a: A, b: A) -> Unit`

Assert that a is less than b.

```almd check
import testing

test "less" {
  testing.assert_lt(3, 7)
}
```

### `testing.assert_some(opt: Option[A]) -> Unit`

Assert that an Option is some (not none).

```almd check
import testing

test "option is some" {
  testing.assert_some(some("value"))
}
```

### `testing.assert_ok(result: Result[A, E]) -> Unit`

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
// Fails unless f, run alone in a child, aborts with text containing expected.
// @since 0.68.0
testing.assert_aborts(expected: String, f: () -> A) -> Unit

// Fails unless needle is in haystack; "" passes.
// @since 0.6.0 or earlier
testing.assert_contains(haystack: String, needle: String) -> Unit

// Fails unless abs(a - b) < tolerance (strict).
// @since 0.6.0 or earlier
testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit

// Fails unless a > b; equal fails.
// @since 0.6.0 or earlier
testing.assert_gt(a: A, b: A) -> Unit

// Fails unless a < b; equal fails.
// @since 0.6.0 or earlier
testing.assert_lt(a: A, b: A) -> Unit

// Fails when o is none.
// @since 0.6.0 or earlier
testing.assert_some(o: Option[A]) -> Unit

// Fails when r is an err.
// @since 0.6.0 or earlier
testing.assert_ok(r: Result[A, B]) -> Unit

// Fails when o is some.
// @since 0.54.0
testing.assert_none(o: Option[A]) -> Unit

// Fails when r is ok.
// @since 0.54.0
testing.assert_err(r: Result[A, B]) -> Unit

// Fails unless actual == expected; --update-snapshots rewrites.
// @since 0.62.0
testing.assert_snapshot(actual: String, expected: String) -> Unit
```

<!-- END GENERATED SIGNATURE INDEX -->
