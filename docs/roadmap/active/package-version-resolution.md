<!-- description: MVS version resolution with semver constraints for almide.toml -->
# Package Version Resolution

## Current State

Almide のパッケージシステムは依存の取得と配置ができるが、バージョン制約の解決がない。

### What works

- `almide add almide/pkg@v0.1.0` — 依存の追加
- `almide.toml` に依存を記述
- GitHub からのソース取得とキャッシュ
- 同名パッケージの異なるメジャーバージョン共存（`v0` / `v1` は別パッケージ扱い）
- ダイヤモンド依存時の警告（`src/project_fetch.rs:183`）

### What's missing

- **バージョン制約構文**: `>= 0.2.0, < 0.3.0` のような範囲指定がない
- **MVS (Minimal Version Selection)**: 複数の依存が同じパッケージの異なるバージョンを要求したとき、最小のバージョンを選ぶアルゴリズム
- **Semver 解析**: バージョン文字列のパースと比較
- **Lock file**: 解決結果の固定（再現性）
- **衝突エラー**: 互換性のないバージョン要求の検出と報告

## Design

### 1. Semver in almide.toml

```toml
[dependencies]
http-client = { source = "almide/http-client", version = "^0.3.0" }
json-parser = { source = "almide/json-parser", version = ">=1.0.0, <2.0.0" }
utils = { source = "almide/utils", version = "0.5.0" }  # exact version
```

Constraint syntax (compatible with Cargo/npm):

| Syntax | Meaning | Example |
|---|---|---|
| `^0.3.0` | Compatible with 0.3.x | `>= 0.3.0, < 0.4.0` |
| `^1.2.3` | Compatible with 1.x.x | `>= 1.2.3, < 2.0.0` |
| `~0.3.0` | Patch-level changes only | `>= 0.3.0, < 0.3.1` |
| `>=1.0.0, <2.0.0` | Explicit range | As specified |
| `0.5.0` | Exact version | `= 0.5.0` |

### 2. Minimal Version Selection (MVS)

Go の MVS アルゴリズムを採用。SAT solver 不要、決定的、高速。

**Core rule**: 全ての制約を満たす**最小の**バージョンを選ぶ。

```
A depends on C ^1.0.0
B depends on C ^1.2.0
→ select C v1.2.0 (not the latest v1.x.x)
```

MVS の利点:
- **決定的**: 同じ依存グラフから常に同じ結果
- **Lock file がなくても再現可能**: 最小バージョンなので、新バージョンの公開で結果が変わらない
- **シンプル**: トポロジカルソートと max 演算のみ
- **LLM フレンドリー**: 結果が予測可能

### 3. Lock file (almide.lock)

MVS は理論上 lock file 不要だが、実用上は有用:
- 依存の依存（transitive）のバージョンを記録
- CI/CD での再現性保証
- `almide update` で明示的にバージョンを更新

```toml
# almide.lock (auto-generated)
[[package]]
name = "http-client"
version = "0.3.2"
source = "github.com/almide/http-client"
checksum = "sha256:..."

[[package]]
name = "json-parser"
version = "1.0.0"
source = "github.com/almide/json-parser"
checksum = "sha256:..."
```

### 4. Error reporting

```
error: version conflict for package 'json-parser'
  → myapp requires ^2.0.0 (via almide.toml)
  → http-client v0.3.2 requires ^1.0.0
  These constraints are incompatible.
  hint: Consider using json-parser v2 and json-parser v1 as separate major versions
```

## Implementation Plan

### Phase 1: Semver parsing and comparison

- [ ] Create `crates/almide-base/src/semver.rs`
- [ ] Parse version strings: `v1.2.3`, `1.2.3`, `1.2.3-beta`
- [ ] Parse constraints: `^`, `~`, `>=`, `<`, `=`, compound ranges
- [ ] Version ordering: `v1.2.3 < v1.2.4 < v1.3.0 < v2.0.0`
- [ ] Constraint satisfaction: `v1.2.3` satisfies `^1.0.0`?
- [ ] Unit tests

### Phase 2: MVS resolver

- [ ] Create `src/resolve_versions.rs`
- [ ] Build dependency graph from `almide.toml` (recursive)
- [ ] Topological sort
- [ ] For each package: compute the minimum version satisfying all constraints
- [ ] Detect conflicts (no version satisfies all constraints)
- [ ] Return resolved version map

### Phase 3: Lock file

- [ ] Generate `almide.lock` from resolved versions
- [ ] Read `almide.lock` and skip resolution if present
- [ ] `almide update` — re-resolve ignoring lock file
- [ ] `almide update json-parser` — re-resolve a single package

### Phase 4: Integration

- [ ] `almide add pkg@^1.0.0` — add with version constraint
- [ ] `almide deps` — show resolved versions
- [ ] `almide.toml` の version field を fetch/build パイプラインに接続
- [ ] Available versions の取得（GitHub tags/releases）
- [ ] Checksum verification

## Files to Modify

- `crates/almide-base/src/semver.rs` — NEW: semver parsing and comparison
- `src/resolve_versions.rs` — NEW: MVS resolver
- `src/project_fetch.rs` — integrate version resolution into fetch pipeline
- `src/cli/mod.rs` — `almide add` with version constraints, `almide update`
- `src/project_config.rs` — parse `version` field from almide.toml

## References

- [Go MVS specification](https://research.swtch.com/vgo-mvs) — Russ Cox's original design
- [Cargo resolver](https://doc.rust-lang.org/cargo/reference/resolver.html) — SAT-based (more complex, not MVS)
- [docs/specs/package-system.md](../docs/specs/package-system.md) — Almide package system spec
