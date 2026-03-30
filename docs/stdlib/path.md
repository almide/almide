# path

File path manipulation. `import path`.

### `path.join(base: String, child: String) -> String`

Join two path segments. If child is absolute, it replaces base.

```almd
path.join("/usr", "local") // => "/usr/local"
```

### `path.dirname(p: String) -> String`

Get the directory portion of a path.

```almd
path.dirname("/usr/local/bin") // => "/usr/local"
```

### `path.basename(p: String) -> String`

Get the file name portion of a path.

```almd
path.basename("/usr/local/bin") // => "bin"
```

### `path.extension(p: String) -> Option[String]`

Get the file extension, or none if there is no extension.

```almd
path.extension("file.txt") // => some("txt")
```

### `path.is_absolute(p: String) -> Bool`

Check if a path is absolute.

```almd
path.is_absolute("/usr") // => true
```
