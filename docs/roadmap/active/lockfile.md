# almide.lock [ACTIVE — 1.0 Phase III]

> Cargo は Rust 1.0 の 6 ヶ月前に登場し、最大の競争優位になった。
> Go は GOPATH で 6 年間苦しんだ。Python は pyproject.toml に 23 年かかった。

## 概要

`almide.lock` で依存解決の再現性を保証する。レジストリは不要 — git ベース依存で 1.0 は十分。

## スコープ

### 1.0 に入れるもの

- [ ] `almide.toml` の `[dependencies]` セクション
  ```toml
  [dependencies]
  utils = { git = "https://github.com/user/almide-utils", tag = "v0.1.0" }
  ```
- [ ] `almide.lock` 生成: `almide build` 時に自動生成、VCS にコミット
- [ ] 依存解決: git clone + tag/branch/commit 指定
- [ ] `almide add <git-url>`: 依存追加コマンド
- [ ] `almide update`: lock ファイル更新

### 1.0 に入れないもの

- パッケージレジストリ (on-hold/package-registry.md)
- バージョン範囲解決 (semver constraint solving)
- private registry
- ワークスペース (monorepo)

## 設計方針

- **`almide.toml` が唯一の設定ファイル** — 他の config 形式は永久に追加しない (Python の教訓)
- **実行可能な build script は作らない** — TOML 宣言のみ (Python setup.py の教訓)
- **lock ファイルは人間が読める形式** — TOML or JSON
