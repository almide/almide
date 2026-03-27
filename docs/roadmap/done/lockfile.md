<!-- description: Dependency lockfile with git-based resolution and reproducible builds -->
<!-- done: 2026-03-17 -->
# almide.lock [DONE — 1.0 Phase III]

## 実装済み

- [x] `almide.toml` の `[dependencies]` セクション
- [x] `almide.lock` 生成 (`almide build` 時に自動生成)
- [x] 依存解決: git clone + tag/branch/commit 指定
- [x] `almide add <pkg>`: 依存追加コマンド (short specifier: `user/repo@tag`)
- [x] locked commit での再現可能ビルド
- [x] 再帰的依存解決 (transitive deps)
- [x] major version による依存の統一・共存

## 1.0 に入れないもの

- パッケージレジストリ (on-hold/package-registry.md)
- バージョン範囲解決 (semver constraint solving)
- private registry
- ワークスペース (monorepo)
