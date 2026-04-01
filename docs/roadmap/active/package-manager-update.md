<!-- description: Add almide update command to refresh dependencies and rewrite lock file -->
# `almide update` — Dependency Update Command

## Goal

```bash
almide update              # 全依存を最新に更新、almide.lock 書き換え
almide update bindgen      # 特定の依存だけ更新
```

## Current State

依存の更新は手動:
1. `almide clean` でキャッシュ削除
2. 次回ビルドで再 fetch → 新しい lock が書かれる

`almide add` (依存追加) と `almide deps` (一覧) は既にある。`update` だけ欠けている。

## Implementation

`src/project_fetch.rs` に `update_dep()` を追加:

1. `almide.toml` から依存リストを読む
2. 指定された依存（または全部）のキャッシュディレクトリを削除
3. 再 fetch（`git clone --depth 1`、最新の ref）
4. `almide.lock` を新しいコミットハッシュで上書き

```rust
pub fn cmd_update(name: Option<&str>) {
    let project = parse_toml(&Path::new("almide.toml"))?;
    let deps = if let Some(name) = name {
        project.dependencies.iter().filter(|d| d.name == name).collect()
    } else {
        project.dependencies.iter().collect()
    };
    for dep in &deps {
        // Delete cached version
        let cache = cache_dir().join(&dep.name);
        let _ = std::fs::remove_dir_all(&cache);
        // Re-fetch latest
        let fetched = fetch_dep(dep)?;
        eprintln!("Updated {} → {}", dep.name, git_head_hash(&fetched)?);
    }
    // Rewrite lock file
    update_lock_file(&deps, &fetched_list)?;
}
```

## CLI

`src/main.rs` の Commands enum に追加:

```rust
/// Update dependencies to latest versions
Update {
    /// Specific dependency to update (default: all)
    name: Option<String>,
},
```

## Files

- `src/main.rs` — Commands::Update 追加
- `src/project_fetch.rs` — `cmd_update()` 実装、キャッシュ削除 + 再 fetch + lock 更新
- `src/cli/commands.rs` — CLI ハンドラ
