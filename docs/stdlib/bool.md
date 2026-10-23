# bool

The `Bool` module: read, order and print booleans. Auto-imported (no `import`).

Pure — no capability, identical on both targets. `bool.to_string` and
`bool.parse` are each other's inverse; `bool.compare` is the function form of
`<` on `Bool`, where `false < true`.

### `bool.parse(s: String) -> Result[Bool, String]`

`"true"` and `"false"` exactly; any other text, including `"True"` or `" true"`,
is an `err`.

```almd run
fn main() -> Unit = {
  println("${bool.parse("true") ?? false} ${bool.parse("yes") ?? false}")
}
```
```output
true false
```

### `bool.compare(a: Bool, b: Bool) -> Int`

```almd run
fn main() -> Unit = {
  println("${bool.compare(false, true)} ${bool.compare(true, true)} ${bool.compare(true, false)}")
}
```
```output
-1 0 1
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (3 functions)

```
// "true" or "false"; bool.parse reads it back.
// @since unreleased
bool.to_string(b: Bool) -> String

// ok(true) for "true", ok(false) for "false", err for anything else.
// @since unreleased
bool.parse(s: String) -> Result[Bool, String]

// Negative if a < b, 0 if equal, positive if a > b; false < true.
// @since unreleased
bool.compare(a: Bool, b: Bool) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
