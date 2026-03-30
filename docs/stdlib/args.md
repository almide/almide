# args

CLI argument parsing. `import args`.

### `args.flag(name: String) -> Bool`

Check if a boolean flag is present in command-line arguments.

```almd
args.flag("--verbose") // => true if --verbose was passed
```

### `args.option(name: String) -> Option[String]`

Get the value of a named option, or none if not present.

```almd
args.option("--output") // => some("out.txt") if --output out.txt
```

### `args.option_or(name: String, fallback: String) -> String`

Get the value of a named option, or a default value.

```almd
args.option_or("--format", "json")
```

### `args.positional() -> List[String]`

Get all positional (non-flag, non-option) arguments.

```almd
args.positional() // => ["file1.almd", "file2.almd"]
```
