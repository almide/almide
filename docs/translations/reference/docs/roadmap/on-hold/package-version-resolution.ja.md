# 依存関係のバージョン制約

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](package-version-resolution.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](package-version-resolution.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](package-version-resolution.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-version-resolution.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットであり、自動同期されない。状態・実装・計測の記述は原文時点の記録で、翻訳時の独立検証や現行保証ではない。計画を実装済みとは扱わない。

説明: 提供済み MVS リゾルバーを超える、依存範囲の意味論と利用可能なバージョン選択の残る作業。

## 提供済みの基盤

MVS、semver の順序づけ、ロックファイル、`almide update` は実装済みである。現在の動作は 2 つ目の実装計画ではなく、[パッケージシステム仕様](../../../../../specs/package-system.md) に属する。

- 同じパッケージ識別子に対しては、要求された最大バージョンが選ばれる。異なるメジャーバージョンは共存する。`src/project_fetch.rs::semver_of` と依存関係の走査がこれを実装する。`tests/mvs_resolution_test.rs` が選択と宣言順序への非依存性を固定して検証する。
- `almide.lock` は直接依存ごとに、選択したチェックアウトの取得元、ref、コミットを記録する。`tests/dep_lock_pairing_test.rs` と `tests/lock_roundtrip_test.rs` は選択バージョンの対応づけと形式を検証する。
- `almide update [dep]` は追従型 ref を進める。タグによる固定は進めない。[CLI リファレンス](../../../../../specs/cli.md#almide-update) と `src/project_fetch.rs::update_locked_deps` を参照。

## 残る提案 — 保留中

提供済みの「要求された最大バージョン」規則は、一般的な依存制約ソルバーでは**ない**。`resolve_dep_version` は先頭の非数字を除去し、バージョン構成要素を補完するが、キャレット、チルダ、複合範囲は評価しない。取得は引き続き `dep_ref_name` を通じて `tag`、`branch`、`main` のいずれかを選択する。`version` フィールドを、一致するタグが検索・選択されるという約束として説明してはならない。

- サポートする範囲構文と、MVS、1.0 未満の同一性、プレリリース、互換性のない上限との関係を定義する。
- git タグ（または将来のレジストリ）から利用可能なバージョンを発見し、宣言された制約を満たすバージョンを選択する。無効または充足不能な要求は、依存経路を示す診断で拒否する。
- 範囲を認識する `add` と `update` が既存のロックや厳密なタグ固定とどう相互作用するかを規定し、再現性テストでその規則を検証する。
- 新たに選択するバージョンに必要な完全性メタデータを決める。レジストリの来歴は [中央レジストリの作業](../../../../../roadmap/on-hold/package-registry.md) が担当する。

この提案を実装するためだけに 2 つ目の semver パーサーを追加したり、既存のロック形式を置き換えたりしない。既存の基盤を再利用し、その実行可能テストを拡張する。

## 設計の参考資料

- [Go MVS](https://research.swtch.com/vgo-mvs) — 要求された最小値の最大
- [Cargo resolver](https://doc.rust-lang.org/cargo/reference/resolver.html) — 範囲解決の比較
