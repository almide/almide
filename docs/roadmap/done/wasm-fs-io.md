# WASM Filesystem I/O [DONE]

**完了日:** 2026-03-25

## 実装内容

WASI preview1 経由でファイル I/O を WASM ターゲットに実装。

### 追加した WASI imports
- `path_open` — ファイルを開く
- `fd_read` — ファイル読み取り
- `fd_close` — fd を閉じる
- `fd_seek` — ファイルシーク
- `fd_filestat_get` — ファイルメタデータ取得
- `path_filestat_get` — パスからメタデータ取得

### 実装した stdlib 関数
- `fs.read_text(path)` → `Result[String, String]`
- `fs.write(path, content)` → `Result[Unit, String]`
- `fs.exists(path)` → `Bool`

### その他
- wasmtime に `--dir=.` を追加してファイルシステムアクセス許可
- bump allocator を 8 バイトアライン化（i64 の load/store で trap しない）
- `env.unix_timestamp()`, `env.millis()` を WASI `clock_time_get` で実装
- `env.args()` を WASI `args_sizes_get`/`args_get` で実装

## 残り → [active/wasm-remaining-fs.md](../active/wasm-remaining-fs.md)
