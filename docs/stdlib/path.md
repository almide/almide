# path

Path string manipulation. `import path`.

Every function below is pure: `path` never touches the filesystem. It works on
path *strings*, so it behaves identically on both targets and needs no
capability. Use `fs` for anything that reads or writes.

## Path components

### `path.join_within(base: String, child: String) -> Result[String, String]`

`path.join(base, child)` for a relative `child` that stays inside `base`, and
`err("path traversal rejected: <child>")` for any other — an absolute child,
a `..` on either separator, a `~`, a drive letter or a NUL — so the result
always names a place under `base`.

```almd run
import path

fn main() -> Unit = {
  println(path.join_within("/srv/uploads", "a/b.txt") ?? "refused")
  println(path.join_within("/srv/uploads", "/etc/passwd") ?? "refused")
  println(path.join_within("/srv/uploads", "../../etc/passwd") ?? "refused")
}
```
```output
/srv/uploads/a/b.txt
refused
refused
```

### `path.join(base: String, child: String) -> String`

Join two segments with a single separator, whether or not `base` already ends
in one. An absolute `child` replaces `base`, and a `..` in `child` climbs out
of it: for a child that crossed a trust boundary use `path.join_within`.

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

### `path.dirname(path: String) -> String`

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

### `path.basename(path: String) -> String`

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

### `path.stem(path: String) -> String`

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

### `path.extension(path: String) -> Option[String]`

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
  println(show(path.extension(".gitignore")))
  println(show(path.extension(".bashrc.bak")))
}
```
```output
some("txt")
none
none
some("bak")
```

### `path.is_absolute(path: String) -> Bool`

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

### `path.normalize(path: String) -> String`

Collapse `.` segments, resolve `..` against the preceding segment, and squeeze
repeated separators. Purely lexical — no symlink resolution, no filesystem
access, so it can differ from the OS's answer when symlinks are involved. Its cost is
linear in the length of the path.

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

## Signature index (11 functions)

```
// base/child; an absolute child replaces base.
// @since 0.2.0 or earlier
path.join(base: String, child: String) -> String

// base/child; err if child could leave base.
// @since unreleased
path.join_within(base: String, child: String) -> Result[String, String]

// path minus last segment; "" if no slash.
// @since 0.2.0 or earlier
path.dirname(path: String) -> String

// Last non-empty segment; "" for / or empty path.
// @since 0.2.0 or earlier
path.basename(path: String) -> String

// After basename's last dot; none if no dot or dotfile.
// @since 0.2.0 or earlier
path.extension(path: String) -> Option[String]

// True when path starts with /.
// @since 0.2.0 or earlier
path.is_absolute(path: String) -> Bool

// Basename minus last extension; dotfiles as-is.
// @since 0.3.0 or earlier
path.stem(path: String) -> String

// Resolves . and .. lexically; "" becomes ".".
// @since 0.3.0 or earlier
path.normalize(path: String) -> String

// Segments of normalize(path), led by "/" when absolute; ["."] for "" or ".".
// @since unreleased
path.components(path: String) -> List[String]

// path relative to base, lexically; "." when equal; none when not under base.
// @since unreleased
path.relative_to(path: String, base: String) -> Option[String]

// path with its last extension replaced by ext; "" removes it; unchanged without a file name.
// @since unreleased
path.with_extension(path: String, ext: String) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
