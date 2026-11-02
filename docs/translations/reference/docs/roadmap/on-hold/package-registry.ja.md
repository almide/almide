<a id="central-package-registry"></a>

# 中央パッケージレジストリ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](package-registry.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](package-registry.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](package-registry.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットであり、自動同期されない。状態・実装・計測の記述は原文時点の記録で、翻訳時の独立検証や現行保証ではない。計画を実装済みとは扱わない。

説明: 既存の git ベースのパッケージシステムを超える、中央でのパッケージ公開、所有権、来歴。

<a id="existing-foundation"></a>

## 既存の基盤

Almide には git ベースのパッケージ取得、`almide add <name>` の省略記法、MVS、コミットを固定するロックファイルがすでにある。省略記法は同梱のパッケージ名マップまたは既定の GitHub 所有者を通して解決される。中央レジストリでは**ない**。現在の動作と取得元の信頼規則は [パッケージシステム仕様](../../../../../specs/package-system.md) と [CLI リファレンス](../../../../../specs/cli.md#almide-add) に記載されている。

実装の根拠: `src/project_fetch.rs::resolve_add_target`、`resolve_package_spec`、`update_lock_file`、`update_locked_deps`、`src/almide_packages.txt`、`tests/mvs_resolution_test.rs`、`tests/dep_lock_pairing_test.rs`、`tests/lock_roundtrip_test.rs`。

ロックファイル生成と基本的なバージョン選択は、もはや未着手項目ではない。残る依存関係の範囲指定の意味論は [依存関係のバージョン制約](../../../../../roadmap/on-hold/package-version-resolution.md) で別途追跡している。

<a id="remaining-proposal--on-hold"></a>

## 残る提案 — 保留中

- パッケージ／バージョンのメタデータと、パッケージ名から公開内容への明確な対応づけを備えたレジストリサービス。
- 不変のバージョンと公開時検証を含む公開ワークフロー（`almide publish` は提案されたコマンドであり、提供済みのインターフェースではない）。
- 名前の予約、所有権、移転、復旧の規則。
- レジストリ内容の完全性と来歴の検証、および現在の git 所有者とネイティブコードへの許可とは区別された明確な信頼境界。
- ロック済み依存関係の同一性や再現性を変えず、明示的な git 取得元から移行する仕組み。

ホスティングと配布形式はまだ設計上の選択事項である。レジストリのエンドポイントを宣伝したり、現在の名前省略記法をレジストリへの公開と扱ったりしてはならない。
