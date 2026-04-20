<!-- description: minimum compiler version pinning in almide.toml (Cargo rust-version style) -->
<!-- done: 2026-04-19 -->
# Compiler Version Pin

## Current State

`almide.toml` にコンパイラバージョンを宣言する手段がない。プロジェクトが特定のバージョン以降の機能（新しい stdlib 関数、最近のバグフィックス等）に依存していても、それを表明・強制する仕組みがない。

### Problem

例: obsid v0.3 は `bytes.set_f32_le` を使う。これは Almide v0.13.2 で追加された機能。

- v0.13.1 のユーザーが obsid をビルド → エラーメッセージが分かりにくい（unknown function 等）
- 「Almide v0.13.2 以上が必要」を伝える正規の場所がない
- README に書くしかなく、CI でも手動チェックが必要

### What's missing

- **Minimum compiler version field** in `[package]`
- **Compile-time check**: ビルド開始前に確認、不足ならエラー
- **Helpful error message**: "this package requires almide >= 0.13.2 (you have 0.13.1)"
- **Optional**: `almide self-update` での自動更新提案

## Design

### Field

Cargo の `rust-version` をそのまま踏襲。シンプルで主流。

```toml
[package]
name = "obsid"
version = "0.3.0"
almide = "0.13.2"
```

- 単一の最低バージョン指定（exact ではなく minimum）
- semver range は v1 では入れない（必要になったら追加）
- フィールド名は `almide`（言語名そのまま、他言語のパターンに従う）

### Behavior

ビルド開始前に最初にチェック:

```
$ almide build
error: package 'obsid' requires almide >= 0.13.2
       installed version: 0.13.1
       run 'almide self-update' to update, or set ALMIDE_SKIP_VERSION_CHECK=1 to bypass
```

- **Hard error** (Cargo style)。曖昧にしない
- **Escape hatch**: 環境変数で無効化可能（CI等で必要なら）
- `almide check` / `almide test` / `almide run` でも同じチェック

### Comparison to other languages

| Language | Field | Hard/Soft |
|---|---|---|
| Rust | `rust-version` | Hard (Cargo) |
| Python | `requires-python` | Hard (pip) |
| Go | `go 1.21` | Hard + auto-download |
| Node.js | `engines.node` | Soft (default) |
| Swift | `swift-tools-version` | Hard |

→ Almide は Rust スタイル（Hard error、シンプルな単一フィールド）が思想に合う。

### Optional: Future extension

Go の `toolchain` ディレクティブに似た自動切替:

```toml
[package]
almide = "0.13.2"
```

→ `almide self-update --to 0.13.2` で正確なバージョンに合わせる、あるいは将来 `.almide-version` ファイル + シェル統合で自動切替（rbenv 風）。

これは v1 では入れず、最低限のチェック機能だけ実装する。

## Implementation

### 1. Parse `almide` field in `almide.toml`

`src/project.rs` の `Manifest` struct に追加:

```rust
#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    almide: Option<String>,  // 新規
}
```

### 2. Compile-time check

`src/cli/build.rs` (および run, check, test) のエントリで:

```rust
fn check_compiler_version(manifest: &Manifest) -> Result<(), String> {
    let Some(required) = &manifest.package.almide else { return Ok(()); };
    let installed = env!("CARGO_PKG_VERSION");
    if !version_satisfies(installed, required) {
        return Err(format!(
            "package '{}' requires almide >= {}\n  installed version: {}\n  run 'almide self-update' to update",
            manifest.package.name, required, installed
        ));
    }
    Ok(())
}
```

### 3. Semver compare

最小限の実装（既存の `semver` crate があるなら使う、なければ簡易比較）:

```rust
fn version_satisfies(installed: &str, required: &str) -> bool {
    // Parse "X.Y.Z" → (X, Y, Z) tuples and compare
}
```

### 4. Tests

- バージョン一致 → OK
- バージョン超過 → OK  
- バージョン不足 → Error
- フィールド省略 → チェックなし（後方互換）
- 不正な semver → エラー

## Acceptance Criteria

- [ ] `almide.toml [package]` の `almide` フィールドをパース
- [ ] ビルド/run/check/test 開始前にバージョンチェック
- [ ] バージョン不足時に明確なエラー（installed/required を表示）
- [ ] フィールド省略時は後方互換でチェックスキップ
- [ ] テストカバレッジ
- [ ] obsid に `almide = "0.13.2"` を追加して動作確認

---

## Resolution (2026-04-19)

- `[package].almide` field parsed into `Package::almide_min: Option<String>`.
- `project::check_compiler_version` uses `semver` crate (`>=` range).
- Check fires in `try_compile_with_ir` (single choke point for run/build/check/test).
- `ALMIDE_SKIP_VERSION_CHECK=1` bypasses; omitted field is a no-op.
- 7 cases covered in `tests/version_pin_test.rs`.
