# args

Command-line argument parsing over `env.args()`. `import args`.

`args` is a thin, allocation-light reader — there is no parser object and no
schema. Each call re-reads the process arguments, so the functions are safe to
call anywhere and in any order.

Program arguments come after `--`:

```bash
almide run app.almd -- --verbose --output=out.txt input.csv
```

### `args.raw() -> List[String]`

The argument list as given, argv[0] included.

```almd check
import args

fn main() -> Unit = {
  let all = args.raw()
  println("${all}")
}
```

### `args.flag(name: String) -> Bool`

True when either the long form `--name` or the short form `-n` (the first
character of `name`) is present.

```almd check
import args

fn main() -> Unit = {
  if args.flag("verbose") then println("loud") else ()
}
```

### `args.option(name: String) -> Option[String]`

The value of `--name`, accepting both spellings — `--name=value` and
`--name value`. `none` when the flag is absent or has no value after it.

```almd check
import args
import fs

effect fn main() -> Unit = {
  let body = "report body"
  match args.option("output") {
    some(path) => fs.write(path, body),
    none => println(body),
  }
}
```

### `args.option_or(name: String, fallback: String) -> String`

`args.option` with a default.

```almd check
import args

fn main() -> Unit = {
  let out = args.option_or("output", "out.txt")
  println(out)
}
```

### `args.positional() -> List[String]`

Arguments that are not flags, with argv[0] dropped. Note that this filters on a
leading `-`, so a value supplied as `--name value` stays in the list.

```almd check
import args

fn process(file: String) -> Unit = println("processing ${file}")

fn main() -> Unit = {
  for file in args.positional() { process(file) }
}
```

### `args.positional_at(i: Int) -> Option[String]`

The i-th positional argument, or `none` when there are fewer.

```almd check
import args

fn main() -> Unit = {
  let input = args.positional_at(0) ?? "-"
  println(input)
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (6 functions)

```
args.raw() -> List[String]
args.flag(name: String) -> Bool
args.option(name: String) -> Option[String]
args.option_or(name: String, fallback: String) -> String
args.positional() -> List[String]
args.positional_at(i: Int) -> Option[String]
```

<!-- END GENERATED SIGNATURE INDEX -->
