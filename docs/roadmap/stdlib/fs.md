# stdlib: fs (拡充) [Tier 1]

現在 19 関数。基本的な read/write/exists はあるが、ディレクトリ操作・メタデータ・temp が足りない。

## 現状 (v0.5.13)

read_text, write_text, read_bytes, write_bytes, append_text, exists?, is_dir?, is_file?, remove, rename, copy, create_dir, list_dir, read_lines, file_size, current_dir, home_dir, canonical, metadata

## 他言語比較

### ディレクトリ操作

| 操作 | Go | Python | Rust | Deno | Almide |
|------|-----|--------|------|------|--------|
| list dir | `os.ReadDir` | `Path.iterdir()` | `fs::read_dir` | `Deno.readDir` | ✅ `fs.list_dir` |
| create dir | `os.Mkdir` | `Path.mkdir()` | `fs::create_dir` | `Deno.mkdir` | ✅ `fs.create_dir` |
| create dir (再帰) | `os.MkdirAll` | `Path.mkdir(parents=True)` | `fs::create_dir_all` | `Deno.mkdir({recursive})` | ❌ |
| remove dir (再帰) | `os.RemoveAll` | `shutil.rmtree()` | `fs::remove_dir_all` | `Deno.remove({recursive})` | ❌ |
| walk/traverse | `filepath.WalkDir` | `Path.walk()` | walkdir crate | `walk()` (@std/fs) | ❌ |
| glob | `filepath.Glob` | `Path.glob()` | glob crate | `expandGlob()` | ❌ |

### ファイルメタデータ

| 操作 | Go | Python | Rust | Deno | Almide |
|------|-----|--------|------|------|--------|
| size | `info.Size()` | `stat().st_size` | `metadata.len()` | `info.size` | ✅ `fs.file_size` |
| permissions | `info.Mode()` | `stat().st_mode` | `metadata.permissions()` | `info.mode` | ❌ |
| modified time | `info.ModTime()` | `stat().st_mtime` | `metadata.modified()` | `info.mtime` | ❌ |
| is symlink | `Mode&ModeSymlink` | `is_symlink()` | `is_symlink()` | `info.isSymlink` | ❌ |

### Temp ファイル

| 操作 | Go | Python | Rust | Deno | Almide |
|------|-----|--------|------|------|--------|
| temp dir path | `os.TempDir()` | `tempfile.gettempdir()` | `env::temp_dir()` | `Deno.makeTempDir()` | ❌ |
| create temp file | `os.CreateTemp` | `NamedTemporaryFile()` | tempfile crate | `Deno.makeTempFile()` | ❌ |
| create temp dir | `os.MkdirTemp` | `TemporaryDirectory()` | tempfile crate | `Deno.makeTempDir()` | ❌ |

### Symlink

| 操作 | Go | Python | Rust | Deno | Almide |
|------|-----|--------|------|------|--------|
| create | `os.Symlink` | `symlink_to()` | `unix::fs::symlink` | `Deno.symlink` | ❌ |
| read | `os.Readlink` | `readlink()` | `fs::read_link` | `Deno.readLink` | ❌ |

## 追加候補 (~15 関数)

### P0 (基本操作)
- `create_dir_all(path)` — 再帰ディレクトリ作成
- `remove_all(path)` — 再帰削除
- `walk(path) -> List[String]` — 再帰ファイル一覧
- `glob(pattern) -> List[String]` — glob パターンマッチ
- `temp_dir() -> String` — temp ディレクトリパス
- `create_temp_file(prefix) -> String` — 一時ファイル作成
- `create_temp_dir(prefix) -> String` — 一時ディレクトリ作成

### P1 (メタデータ)
- `modified_at(path) -> Int` — 更新時刻 (Unix timestamp)
- `created_at(path) -> Int` — 作成時刻
- `permissions(path) -> Int` — ファイルパーミッション
- `set_permissions(path, mode)` — パーミッション設定
- `is_symlink?(path) -> Bool`

### P2 (Symlink)
- `create_symlink(target, link)`
- `read_symlink(path) -> String`

### P3 (高度)
- `watch(path) -> ???` — ファイル変更監視（async 前提、Phase D 以降）

## 実装戦略

TOML + runtime。Rust: `std::fs` + `tempfile` crate。TS: `Deno.` API / Node `fs` module。
