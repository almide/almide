# fs

File system. import fs, effect.

### `fs.read_text(path: String) -> Result[String, String]`

Read file contents as a UTF-8 string

```almd
let text = fs.read_text("config.toml")
```

### `fs.read_bytes(path: String) -> Result[List[Int], String]`

Read file contents as a list of bytes

```almd
let bytes = fs.read_bytes("image.png")
```

### `fs.write(path: String, content: String) -> Result[Unit, String]`

Write a string to a file, creating or overwriting it

```almd
fs.write("output.txt", "hello")
```

### `fs.write_bytes(path: String, bytes: List[Int]) -> Result[Unit, String]`

Write a list of bytes to a file

```almd
fs.write_bytes("out.bin", [0, 1, 2])
```

### `fs.append(path: String, content: String) -> Result[Unit, String]`

Append a string to a file, creating it if it doesn't exist

```almd
fs.append("log.txt", "new line\n")
```

### `fs.mkdir_p(path: String) -> Result[Unit, String]`

Create a directory and all parent directories

```almd
fs.mkdir_p("data/cache/images")
```

### `fs.exists(path: String) -> Bool`

Check if a file or directory exists

```almd
if fs.exists("config.toml") then ...
```

### `fs.read_lines(path: String) -> Result[List[String], String]`

Read a file as a list of lines

```almd
let lines = fs.read_lines("data.csv")
```

### `fs.remove(path: String) -> Result[Unit, String]`

Delete a file

```almd
fs.remove("temp.txt")
```

### `fs.list_dir(path: String) -> Result[List[String], String]`

List entries in a directory

```almd
let entries = fs.list_dir("src/")
```

### `fs.is_dir(path: String) -> Bool`

Check if a path is a directory

```almd
if fs.is_dir("src") then ...
```

### `fs.is_file(path: String) -> Bool`

Check if a path is a regular file

```almd
if fs.is_file("readme.md") then ...
```

### `fs.copy(src: String, dst: String) -> Result[Unit, String]`

Copy a file from src to dst

```almd
fs.copy("a.txt", "b.txt")
```

### `fs.rename(src: String, dst: String) -> Result[Unit, String]`

Rename or move a file

```almd
fs.rename("old.txt", "new.txt")
```

### `fs.walk(dir: String) -> Result[List[String], String]`

Recursively list all files in a directory tree

```almd
let all_files = fs.walk("src/")
```

### `fs.remove_all(path: String) -> Result[Unit, String]`

Recursively delete a directory and all its contents

```almd
fs.remove_all("build/")
```

### `fs.file_size(path: String) -> Result[Int, String]`

Get file size in bytes

```almd
let size = fs.file_size("data.bin")
```

### `fs.temp_dir() -> String`

Get the system temporary directory path

```almd
let tmp = fs.temp_dir()
```

### `fs.stat(path: String) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]`

Get file metadata: size, type, and modification time

```almd
let info = fs.stat("file.txt") // {size, is_dir, is_file, modified}
```

### `fs.glob(pattern: String) -> Result[List[String], String]`

Find files matching a glob pattern

```almd
let files = fs.glob("src/**/*.almd")
```

### `fs.create_temp_file(prefix: String) -> Result[String, String]`

Create a temporary file with a given prefix, return its path

```almd
let path = fs.create_temp_file("almide-")
```

### `fs.create_temp_dir(prefix: String) -> Result[String, String]`

Create a temporary directory with a given prefix, return its path

```almd
let dir = fs.create_temp_dir("build-")
```

### `fs.is_symlink(path: String) -> Bool`

Check if a path is a symbolic link

```almd
if fs.is_symlink("link") then ...
```

### `fs.modified_at(path: String) -> Result[Int, String]`

Get file modification time as Unix timestamp (seconds)

```almd
let ts = fs.modified_at("file.txt")
```
