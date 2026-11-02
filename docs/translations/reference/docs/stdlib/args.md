# args

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](args.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](args.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](args.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/args.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


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

Arguments from `env.args()` (which already excludes argv[0]) that are not flags, followed by every argument
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


