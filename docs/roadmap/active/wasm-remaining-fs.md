<!-- description: Implement remaining filesystem operations for the WASM target -->
# WASM Remaining FS Operations

**優先度:** 中 — read_text/write/exists は実装済み。残りは実用上の必要に応じて追加
**前提:** WASI path_open, fd_read, fd_write, fd_close, fd_filestat_get, path_filestat_get 登録済み

---

## 実装済み

- [x] `fs.read_text(path)` — path_open → fd_filestat_get → fd_read → String 構築
- [x] `fs.write(path, content)` — path_open(O_CREAT|O_TRUNC) → fd_write
- [x] `fs.exists(path)` — path_filestat_get → errno チェック
- [x] wasmtime `--dir=/` (root preopened) + WASI absolute path strip
- [x] top-level let 動的初期化 (`compile_init_globals`)
- [x] mutable collection operations: `list.push`, `list.pop`, `list.clear`, `map.insert`, `map.delete`, `map.clear`

## 未実装（優先度順）

### 高
- [ ] `fs.list_dir(path)` — fd_readdir のパース、dirent 構造体の解析が必要
- [ ] `fs.mkdir_p(path)` — path_create_directory + パス分割による再帰作成
- [ ] `fs.remove(path)` — path_unlink_file

### 中
- [ ] `fs.read_lines(path)` — read_text + split("\n") で合成可能（コンパイラ側不要、Almide で書ける）
- [ ] `fs.append(path, content)` — path_open の oflags を O_APPEND に変更するだけ
- [ ] `fs.rename(src, dst)` — path_rename WASI 呼び出し
- [ ] `fs.copy(src, dst)` — read_text + write で合成

### 低
- [ ] `fs.read_bytes(path)` — read_text と類似（String ではなく List[Int] を構築）
- [ ] `fs.write_bytes(path, bytes)` — write と類似
- [ ] `fs.is_dir?(path)` / `fs.is_file?(path)` — path_filestat_get のフラグ解析
- [ ] `fs.stat(path)` — fd_filestat_get の結果を Record に変換

## 技術的注意

- bump allocator は解放なし — 大量ファイル操作でメモリ使用量が増加する
- fd_readdir の dirent 構造体パースが最も複雑（可変長レコード）
- read_lines / copy は Almide コードで合成可能なため、コンパイラ側の実装は不要かもしれない
