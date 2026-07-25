# error

Error construction and inspection. import error.

### `error.context(r: Result[T, E], msg: String) -> Result[T, String]`

Add context message to an error result.

```almd
error.context(result, "failed to load config")
```

### `error.message(r: Result[T, String]) -> String`

Extract the error message from a Result, or empty string if ok.

```almd
error.message(err("oops")) // => "oops"
```

### `error.chain(outer: String, cause: String) -> String`

Chain two error messages with a cause separator.

```almd
error.chain("load failed", "file not found") // => "load failed: file not found"
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (3 functions)

```
error.chain(outer: String, cause: String) -> String
error.context(r: Result[T, String], msg: String) -> Result[T, String]
error.message(r: Result[T, String]) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
