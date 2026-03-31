<!-- description: Abstract filesystem and I/O behind traits for testability -->
# Trait-Based I/O Abstraction

ファイルシステム・HTTP 等の I/O をトレイトで抽象化し、テストでインメモリ実装に差し替え可能にする。

## 参考

- **Gleam**: `FileSystemReader`, `FileSystemWriter`, `HttpClient` トレイト
  - テストで `InMemoryFileSystem` を使用（OS I/O なし）
  - `LanguageServerTestIO` が全 I/O をラップ
  - LSP テストもインメモリで高速実行

## 現状

Almide コンパイラは `std::fs::read_to_string` 等を直接呼び出し。テストでファイルシステムのモックができない。

## ゴール

```rust
trait FileSystem {
    fn read(&self, path: &Path) -> Result<String, Error>;
    fn write(&self, path: &Path, content: &str) -> Result<(), Error>;
    fn exists(&self, path: &Path) -> bool;
}

struct RealFileSystem;      // 本番用
struct InMemoryFileSystem;  // テスト用
```

- resolve.rs, project.rs, project_fetch.rs のファイルアクセスをトレイト経由に
- コンパイラのユニットテストがファイルシステムに依存しない
- WASM playground のコンパイラもインメモリ FS で動作（仮想ファイルシステム）
