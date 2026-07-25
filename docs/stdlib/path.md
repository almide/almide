# path

Path string manipulation, plus the `SafePath` capability type. `import path`.

Every function below is pure: `path` never touches the filesystem. It works on
path *strings*, so it behaves identically on both targets and needs no
capability. Use `fs` for anything that reads or writes.

## SafePath — validated paths

`SafePath` is an opaque wrapper around a path string that has been checked for
traversal segments. It exists so a function that accepts a caller-supplied path
can demand a *validated* one in its signature instead of trusting a bare
`String`.

### `path.from_string(s: String) -> Result[SafePath, String]`

Validate a path and wrap it. Rejects anything containing a `..` segment.

```almd
match path.from_string(user_input) {
  ok(p) => fs.read_text(path.to_string(p)),
  err(e) => err("rejected: " + e),
}
```

### `path.trusted(s: String) -> SafePath`

Wrap a path WITHOUT validating it. For paths the program itself constructed —
a literal, or a name joined onto a directory it owns. Never call this on input
that crossed a trust boundary.

### `path.to_string(p: SafePath) -> String`

Unwrap back to the underlying string.

## Path components

### `path.join(base: String, child: String) -> String`

Join two segments with a single separator, whether or not `base` already ends
in one.

```almd
path.join("/usr", "bin")   // "/usr/bin"
path.join("/usr/", "bin")  // "/usr/bin"
```

### `path.dirname(p: String) -> String`

Everything before the last separator.

```almd
path.dirname("/usr/bin/node")  // "/usr/bin"
```

### `path.basename(p: String) -> String`

Everything after the last separator.

```almd
path.basename("/usr/bin/node")  // "node"
```

### `path.stem(p: String) -> String`

The basename with its extension removed.

```almd
path.stem("/tmp/report.tar.gz")  // "report.tar"
```

### `path.extension(p: String) -> Option[String]`

The extension WITHOUT the dot, or `none` when there is none. A leading-dot
filename (`.gitignore`) has no extension.

```almd
path.extension("file.txt")  // some("txt")
path.extension("Makefile")  // none
```

### `path.is_absolute(p: String) -> Bool`

True when the path starts at the filesystem root.

```almd
path.is_absolute("/usr")           // true
path.is_absolute("relative/path")  // false
```

### `path.normalize(p: String) -> String`

Collapse `.` segments, resolve `..` against the preceding segment, and squeeze
repeated separators. Purely lexical — no symlink resolution, no filesystem
access, so it can differ from the OS's answer when symlinks are involved.

```almd
path.normalize("/a/./b/../c")  // "/a/c"
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (10 functions)

```
path.from_string(s: String) -> Result[SafePath, String]
path.trusted(s: String) -> SafePath
path.to_string(p: SafePath) -> String
path.join(base: String, child: String) -> String
path.dirname(p: String) -> String
path.basename(p: String) -> String
path.extension(p: String) -> Option[String]
path.is_absolute(p: String) -> Bool
path.stem(p: String) -> String
path.normalize(p: String) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
