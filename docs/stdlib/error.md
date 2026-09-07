# error

Error construction and inspection. import error.

### `error.context(r: Result[T, E], msg: String) -> Result[T, String]`

Add context message to an error result.

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  let result: Result[Int, String] = err("file not found")
  println(show(error.context(result, "failed to load config")))
  println(show(error.context(ok(1), "failed to load config")))
}
```
```output
err("failed to load config: file not found")
ok(1)
```

### `error.message(r: Result[T, String]) -> String`

Extract the error message from a Result, or empty string if ok.

```almd run
fn main() -> Unit = {
  println(error.message(err("oops"): Result[Int, String]))
}
```
```output
oops
```

### `error.chain(outer: String, cause: String) -> String`

Chain two error messages: the outer message on one line, the cause on the next, prefixed `caused by:`.

```almd run
fn main() -> Unit = println(error.chain("load failed", "file not found"))
```
```output
load failed
caused by: file not found
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (3 functions)

```
error.chain(outer: String, cause: String) -> String
error.context(r: Result[T, String], msg: String) -> Result[T, String]
error.message(r: Result[T, String]) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
