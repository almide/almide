# env

Environment and system. import env, effect.

### `env.args() -> List[String]`

Get the command-line arguments as a list of strings: `argv[1..]`, verbatim, on
every target. A `--` among the program's arguments is the program's (`./p a -- b`
sees `["a", "--", "b"]`); `almide run app.almd -- a -- b` consumes its own
separator and forwards the rest unchanged.

```almd check
import env

effect fn main() -> Unit = {
  let args = env.args()
  println("${args}")
}
```

### `env.get(name: String) -> Option[String]`

Get the value of an environment variable, or none if not set.

```almd check
import env

effect fn main() -> Unit = {
  let home = env.get("HOME") // => some("/Users/alice")
  println(home ?? "unset")
}
```

### `env.set(name: String, value: String) -> Unit`

Set an environment variable. Errs, and sets nothing, when the name is empty
or contains `=` or NUL, or the value contains NUL — pairs no process
environment can hold (C-421): `env.set: empty name`,
`env.set: name contains '='`, `env.set: name contains NUL`,
`env.set: value contains NUL`.

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")!
  println(env.get("MY_VAR") ?? "unset")
}
```

### `env.cwd() -> Result[String, String]`

Get the current working directory.

```almd check
import env

effect fn main() -> Unit = {
  let dir = env.cwd()!
  println(dir)
}
```

### `env.sleep_ms(ms: Int) -> Unit`

Sleep for the given number of milliseconds. A zero or negative duration is an
elapsed one and returns at once, on every target. A deadline is different: a `process.ProcessCommand`'s
`timeout_ms` of `0` means no deadline at all.

```almd check
import env

effect fn main() -> Unit = {
  env.sleep_ms(1000) // sleep 1 second
  println("awake")
}
```

### `env.os() -> String`

Get the operating system name (linux, macos, windows).

```almd check
import env

effect fn main() -> Unit = {
  println(env.os()) // => "macos"
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (6 functions)

```
// Program arguments, argv[0] excluded.
// @since 0.5.0 or earlier
effect env.args() -> List[String]

// Value of name, or none if unset.
// @since 0.5.0 or earlier
effect env.get(name: String) -> Option[String]

// Sets name for this process and later spawns; err if name is empty or holds `=` or NUL, or value holds NUL.
// @since 0.5.0 or earlier
effect env.set(name: String, value: String) -> Unit

// Absolute current directory; err if unreadable.
// @since 0.5.0 or earlier
effect env.cwd() -> String

// Blocks ms milliseconds; a negative ms hangs.
// @since 0.5.0 or earlier
effect env.sleep_ms(ms: Int) -> Unit

// Host OS: macos, linux, windows or unknown; "wasi" on wasm (C-189).
// @since 0.5.0 or earlier
env.os() -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
