# HKT 基盤 — フェーズ 1–4 + ストリーム融合（全 6 法則）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](hkt-foundation-phase1.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](hkt-foundation-phase1.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](hkt-foundation-phase1.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/hkt-foundation-phase1.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットであり、自動同期されない。状態・実装・計測の記述は原文時点の記録で、翻訳時の独立検証や現行保証ではない。計画を実装済みとは扱わない。

説明: 型コンストラクターと代数法則を伴う HKT 基盤のフェーズ 1–4。
記録上の完了日: 2026-03-19。

**完了日:** 2026-03-19
**PR:** #49

## 実装の詳細

### フェーズ 1: Ty ヘルパー + TypeConstructor 基盤
- `TypeConstructorId`、`Kind`、`AlgebraicLaw` の型定義
- `TypeConstructorRegistry` — 組み込み型とユーザー定義型の自動登録
- `Ty::children()`、`map_children()`、`map_children_mut()` — 統一された型走査
- `Ty::constructor_id()`、`type_args()`、`constructor_name()`、`is_container()`、`any_child_recursive()`、`all_children_recursive()`
- `Ty::list()`、`option()`、`result()`、`map_of()` スマートコンストラクター
- `Ty::inner()`、`inner2()`、`is_list()`、`is_option()`、`is_result()`、`is_map()` アクセサー
- IrProgram に `type_registry` フィールドを追加し、lowering 時に自動登録
- 14 関数の match アームを簡素化（-120 行）

### フェーズ 2: 代数法則テーブル
- 6 法則: FunctorComposition、FunctorIdentity、FilterComposition、MapFoldFusion、MapFilterFusion、MonadAssociativity
- List: Functor + Filterable + Foldable
- Option: Functor + Monad
- Result: Functor

### フェーズ 3: ストリーム融合 — 全 6 代数法則に対する IR 書き換え
- パイプチェーンの検出（ネストした呼び出しと let 束縛チェーン）
- **FunctorComposition**: `map(map(x,f),g)` → `map(x, f>>g)` — 中間 List を除去
- **FunctorIdentity**: `map(x, id)` → `x` — map 自体を除去
- **FilterComposition**: `filter(filter(x,p),q)` → `filter(x, p&&q)`
- **MapFoldFusion**: `fold(map(x,f),init,g)` → `fold(x, init, g∘f)` — map を除去し、単一走査にする
- **MapFilterFusion**: `filter(map(x,f),p)` → `filter_map(x, ...)` — 単一走査
- **MonadAssociativity**: `flat_map(flat_map(x,f),g)` → `flat_map(x, f>>=g)`
- ラムダ合成 + 述語合成 + 変数置換
- ALMIDE_DEBUG_FUSION=1 による解析出力

### フェーズ 4: Ty::Applied への統一
- `Ty::List`、`Ty::Option`、`Ty::Result`、`Ty::Map` を削除
- `Ty::Applied(TypeConstructorId, Vec<Ty>)` に統一
- 342 の構築箇所 → スマートコンストラクター経由に変更
- 約 200 の match アーム → Applied パターンに移行
- 23 ファイルを変更
- build.rs を更新（generated/ も自動処理）

## 完了時点（2026-03-19）で残っていた事項

元の `active/hkt-foundation.md` 計画はもう存在しない。以下は完了時に記録された残作業のスナップショットであり、現在のタスクリストではない。

- フェーズ 5: コンパイラ内部のエフェクト情報注釈（構文変更なし）→ 2.x
- フェーズ 6: トレイト統合 → 2.x

## テスト
- 617+ Rust ユニットテスト（56→617）
- 110/110 almide テスト
- 警告 0 件
