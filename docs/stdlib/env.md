# env

Environment and system. import env, effect.

### `env.unix_timestamp() -> Int`

Get the current Unix timestamp in seconds.

```almd
let ts = env.unix_timestamp()
```

### `env.args() -> List[String]`

Get the command-line arguments as a list of strings.

```almd
let args = env.args()
```

### `env.get(name: String) -> Option[String]`

Get the value of an environment variable, or none if not set.

```almd
env.get("HOME") // => some("/Users/alice")
```

### `env.set(name: String, value: String) -> Unit`

Set an environment variable.

```almd
env.set("MY_VAR", "hello")
```

### `env.cwd() -> Result[String, String]`

Get the current working directory.

```almd
let dir = env.cwd()
```

### `env.millis() -> Int`

Get the current time in milliseconds since epoch.

```almd
let ms = env.millis()
```

### `env.sleep_ms(ms: Int) -> Unit`

Sleep for the given number of milliseconds.

```almd
env.sleep_ms(1000) // sleep 1 second
```

### `env.temp_dir() -> String`

Get the system temporary directory path.

```almd
let tmp = env.temp_dir()
```

### `env.os() -> String`

Get the operating system name (linux, macos, windows).

```almd
env.os() // => "macos"
```
