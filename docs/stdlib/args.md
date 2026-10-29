# args

Command-line argument parsing over `env.args()`. `import args`.

`args` is a thin, allocation-light reader — there is no parser object and no
schema. Each call re-reads the process arguments, in any order.

Every reader is an `effect fn` (dialect epoch 13): the arguments are input,
and a plain `fn` never reads input. Call them from an `effect fn` and
propagate with `!` like any effect fn's call, or read them once in
`effect fn main` and pass the values to plain fns.

Program arguments come after `--`:

```bash
almide run app.almd -- --verbose --output=out.txt input.csv
```

### `args.option(name: String) -> Option[String]`

The value of `--name`, accepting both spellings — `--name=value` and
`--name value`, before `--`. `none` when the flag is absent or has no value
after it.

```almd check
import args
import fs

effect fn main() -> Unit = {
  let body = "report body"
  match args.option("output")! {
    some(path) => fs.write_text(path, body)!,
    none => println(body),
  }
}
```

### `args.positional(options: List[String]) -> List[String]`

Arguments that are not flags, with argv[0] dropped, followed by every argument
after `--`. `options` names every option the program reads with `args.option`:
the argument after `--name` is that option's value, not a positional, so
`--out report.txt input.txt` with `["out"]` gives `["input.txt"]`. Pass `[]`
when the program reads no options.

```almd check
import args

fn process(file: String) -> Unit = println("processing ${file}")

effect fn main() -> Unit = {
  let out = args.option("out")! ?? "report.txt"
  for file in args.positional(["out"])! { process(file) }
  println("writing ${out}")
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (3 functions)

```
// True on --name before `--`; a one-letter name also matches -name. No short form is derived.
// @since unreleased
effect args.has_flag(name: String) -> Bool

// Value of --name=v or --name v before `--`; none if absent.
// @since 0.2.0 or earlier
effect args.option(name: String) -> Option[String]

// Args before `--` that are not flags or values of the named options, then all after `--`.
// @since 0.2.0 or earlier
effect args.positional(options: List[String]) -> List[String]
```

<!-- END GENERATED SIGNATURE INDEX -->
