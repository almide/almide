# env

Environment and system. import env, effect.

### `env.unix_timestamp() -> Int`

Get the current Unix timestamp in seconds.

```almd check
import env

effect fn main() -> Unit = {
  let ts = env.unix_timestamp()
  println(int.to_string(ts))
}
```

### `env.args() -> List[String]`

Get the command-line arguments as a list of strings.

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

Set an environment variable.

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")
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

### `env.millis() -> Int`

Get the current time in milliseconds since epoch.

```almd check
import env

effect fn main() -> Unit = {
  let ms = env.millis()
  println(int.to_string(ms))
}
```

### `env.sleep_ms(ms: Int) -> Unit`

Sleep for the given number of milliseconds.

```almd check
import env

effect fn main() -> Unit = {
  env.sleep_ms(1000) // sleep 1 second
  println("awake")
}
```

### `env.temp_dir() -> String`

Get the system temporary directory path.

```almd check
import env

effect fn main() -> Unit = {
  let tmp = env.temp_dir()
  println(tmp)
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

## Signature index (9 functions)

```
// Wall-clock seconds since the Unix epoch.
effect env.unix_timestamp() -> Int

// Program arguments, argv[0] excluded.
effect env.args() -> List[String]

// Value of name, or none if unset.
effect env.get(name: String) -> Option[String]

// Sets name for this process and later spawns.
effect env.set(name: String, value: String) -> Unit

// Absolute current directory; err if unreadable.
effect env.cwd() -> String

// Wall-clock ms since the Unix epoch.
effect env.millis() -> Int

// Blocks ms milliseconds; a negative ms hangs.
effect env.sleep_ms(ms: Int) -> Unit

// Host temp dir; may end with /.
effect env.temp_dir() -> String

// Host OS: macos, linux, windows or unknown.
env.os() -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
