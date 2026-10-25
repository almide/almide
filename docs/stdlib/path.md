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

Validate a path and wrap it. Admits only a path that stays inside the
directory it is later joined to: it must be relative (no leading `/` or `\`,
no drive letter such as `C:`, no leading `~`), hold no NUL, and have no `..`
segment when split on either separator — `..\..\x` is refused on every
host. Percent-encoded forms such as `%2e%2e` are NOT decoded here: to the
filesystem they are ordinary names; decode URL input before validating it.

To use a validated name, join it onto the directory it belongs to with
`path.join_within`, or open that directory with `fs.open_root` and read
through the rooted `fs.root_*` calls, which also refuse a symlink that leads
out of it.

```almd check
import path
import fs

effect fn read_upload(name: String) -> String =
  match path.join_within("uploads", name) {
    ok(p) => fs.read_text(p)!,
    err(e) => err("rejected: " + e),
  }

effect fn main() -> Unit = {
  match read_upload("../etc/passwd") {
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

### `path.join_within(base: String, child: String) -> Result[String, String]`

`path.join(base, child)` for a `child` that `path.from_string` admits, and
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

## Signature index (14 functions)

```
// SafePath of s; err unless relative and inside.
// @since 0.18.1 or earlier
path.from_string(s: String) -> Result[SafePath, String]

// SafePath of s with no traversal check.
// @since 0.18.1 or earlier
path.trusted(s: String) -> SafePath

// Path text inside p, unchanged.
// @since 0.18.1 or earlier
path.to_string(p: SafePath) -> String

// base/child; an absolute child replaces base.
// @since 0.2.0 or earlier
path.join(base: String, child: String) -> String

// base/child; err if child could leave base.
// @since unreleased
path.join_within(base: String, child: String) -> Result[String, String]

// p minus last segment; "" if no slash.
// @since 0.2.0 or earlier
path.dirname(p: String) -> String

// Last non-empty segment; "" for / or empty p.
// @since 0.2.0 or earlier
path.basename(p: String) -> String

// After basename's last dot; none if no dot or dotfile.
// @since 0.2.0 or earlier
path.extension(p: String) -> Option[String]

// True when p starts with /.
// @since 0.2.0 or earlier
path.is_absolute(p: String) -> Bool

// Basename minus last extension; dotfiles as-is.
// @since 0.3.0 or earlier
path.stem(p: String) -> String

// Resolves . and .. lexically; "" becomes ".".
// @since 0.3.0 or earlier
path.normalize(p: String) -> String

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
