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

```almd check
import path
import fs

effect fn read_validated(user_input: String) -> String =
  match path.from_string(user_input) {
    ok(p) => fs.read_text(path.to_string(p)),
    err(e) => err("rejected: " + e),
  }

effect fn main() -> Unit = {
  match read_validated("../etc/passwd") {
    ok(text) => println(text),
    err(e) => println(e),
  }
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

```almd run
import path

fn main() -> Unit = {
  println(path.join("/usr", "bin"))
  println(path.join("/usr/", "bin"))
}
```
```output
/usr/bin
/usr/bin
```

### `path.dirname(p: String) -> String`

Everything before the last separator.

```almd run
import path

fn main() -> Unit = {
  println(path.dirname("/usr/bin/node"))
}
```
```output
/usr/bin
```

### `path.basename(p: String) -> String`

Everything after the last separator.

```almd run
import path

fn main() -> Unit = {
  println(path.basename("/usr/bin/node"))
}
```
```output
node
```

### `path.stem(p: String) -> String`

The basename with its extension removed.

```almd run
import path

fn main() -> Unit = {
  println(path.stem("/tmp/report.tar.gz"))
}
```
```output
report.tar
```

### `path.extension(p: String) -> Option[String]`

The extension WITHOUT the dot, or `none` when there is none. A leading-dot
filename (`.gitignore`) has no extension.

```almd run
import path

fn show(o: Option[String]) -> String = match o {
  some(s) => "some(\"${s}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(path.extension("file.txt")))
  println(show(path.extension("Makefile")))
}
```
```output
some("txt")
none
```

### `path.is_absolute(p: String) -> Bool`

True when the path starts at the filesystem root.

```almd run
import path

fn main() -> Unit = {
  println("${path.is_absolute("/usr")}")
  println("${path.is_absolute("relative/path")}")
}
```
```output
true
false
```

### `path.normalize(p: String) -> String`

Collapse `.` segments, resolve `..` against the preceding segment, and squeeze
repeated separators. Purely lexical — no symlink resolution, no filesystem
access, so it can differ from the OS's answer when symlinks are involved.

```almd run
import path

fn main() -> Unit = {
  println(path.normalize("/a/./b/../c"))
}
```
```output
/a/c
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
