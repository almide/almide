# エフェクトシステム — フェーズ1〜2

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-system-phase1-2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](effect-system-phase1-2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](effect-system-phase1-2.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/effect-system-phase1-2.md)の非規範的な読者向け翻訳です。自動同期されない歴史的な固定スナップショットです。状態・計画・件数は原文時点の記述であり、現在の実装や完了を立証しません。この翻訳では主張を独立に実行・検証していません。

> 原文の説明：7カテゴリのエフェクト推論エンジンとチェッカー統合。
> 原文の完了メタデータ：2026-03-19。

**完了：** 2026-03-19
**PR：** #49

## 実装

### フェーズ1：エフェクト推論エンジン
- `EffectInferencePass` Nanopass — 7つのエフェクトカテゴリ（IO、Net、Env、Time、Rand、Fan、Log）
- stdlib モジュール → エフェクトの対応（fs→IO、http→Net、env→Env など）
- 直接エフェクトの収集（Module call + Named call + fan 式）
- 呼び出しグラフの構築と、不動点反復による推移的なエフェクト推論
- IrProgram に `effect_map` フィールドを追加
- `almide check --effects <file>` CLI コマンド
- 解析出力用の ALMIDE_DEBUG_EFFECTS=1

### フェーズ2：自己パッケージの制限（セキュリティレイヤー2）
- `almide.toml [permissions] allow = ["IO", "Net"]` を解析
- `almide check --effects` による違反検出とヒント表示
- 通常の `almide check` に統合 — [permissions] があるとき違反を自動検出
- `project.rs` の `Project` 構造体に `permissions` フィールドを追加

## 完了時点の残作業（2026-03-19）

元の `active/effect-system.md` 計画はもう存在しません。現在のケイパビリティのロードマップは [エフェクトシステムとケイパビリティ](../../../../../roadmap/active/effect-system-capability.md) を参照してください。以下のフェーズ番号は、この完了記録に属します。

- フェーズ3：依存関係の制限（`[dependencies.X].allow`）→ 2.x
- フェーズ4：型レベルの統合（HKT Foundation フェーズ4と統合）→ 2.x

## テスト
- 内部テスト5件（module_to_effect、runtime_name_to_effect、format_effects）
- almide テスト110/110
