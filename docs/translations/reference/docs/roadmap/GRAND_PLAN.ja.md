<a id="almide-grand-plan"></a>

# Almide 全体計画

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](GRAND_PLAN.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](GRAND_PLAN.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](GRAND_PLAN.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/GRAND_PLAN.md)
> 完了・未完了のチェックは固定計画の履歴で、新たな実装監査ではない。特に古い未完了の build-cache 項目は現行 CLI にキャッシュが無いという主張ではなく、1.0 チェックリストも現在の readiness・安全性判定ではない。後段の runtime／platform 項目は計画として読む。


> 「1.0で言語として勝ち切る。その後に "app runtimeにもなる" ための土台を、UIより先に積む」
>
> バージョン単位の実行計画: [ROAD_TO_1_0.md](../../../../roadmap/ROAD_TO_1_0.md) — 0.65 → 0.99 の全 minor に大機能を割付けた台帳(2026-09-27 に引き直し、0.41–0.64 は出荷記録)
>
> 評価軸から見た勝ち筋: [ALL_AXES.md](../../../../roadmap/ALL_AXES.md) — 速度 / 性能 / 正しさ / LLM writability / DX /
> エコシステムの 6 軸それぞれの現在値・勝利条件・降りる軸

---

<a id="phase-1-original-10-readiness-assessment-historical"></a>

## Phase 1：当初の 1.0 readiness 評価（履歴）

以下のチェックリストは以前の評価の記録であり、現行リリースの readiness 判定や API 凍結ではない。件数と除外モジュールは履歴である。現在の言語／API の動作は [SPEC](../../../../SPEC.md)、[stdlib リファレンス](../../../../stdlib/README.md)、実際のリリース／版メタデータを参照する。この文書は 1.0 のリリース決定を意味しない。

言語として信用を取る。機能を増やさず、完成の定義を固定する。

- [x] 言語仕様の凍結 (構文・演算子・キーワード確定, protocol 追加)
- [x] コンパイラ挙動の安定化 (ICE=0, warning=0, cross-target 106/106)
- [x] stdlib 境界の確定 (22モジュール 390関数, set追加, uuid/crypto/toml/compress/term 除外)
- [x] エラーメッセージ品質 (E001-E010, --json, hint system)
- [x] 破壊的変更ポリシー (edition, FROZEN_API, REJECTED_PATTERNS)
- [x] showcases (CLI/API/Data/DevTool/Script 5本)
- [x] テスト 2,500+ (2,681 達成)
- [x] Protocol System (ユーザー定義 convention, generic bounds)

1.0 後の継続改善:
- 使用例／クックブック／移行の道筋
- LLM計測 (MSR, 初回正答率) — ブロックしない

**当初の評価目標**: 安定性の根拠を明示して利用判断できる状態。上の旧チェック結果だけで
現行の全用途に対する安全性・互換性・1.0 readiness を保証しない。

<a id="phase-2-production-language"></a>

## Phase 2：本番向けの言語

配布単位を強くする。モジュールを中心とする経済圏（module economy）を作る。

- [x] パッケージマニフェスト（almide.toml）
- [x] ロックファイル（almide.lock）
- [ ] semver 規約の明文化
- [ ] 公開APIの安定性ルール
- [ ] ビルドキャッシュ（増分コンパイル）
- [ ] 再現可能ビルド
- [ ] monorepo / polyrepo 対応
- [ ] ターゲット互換性契約（multi-target を「約束」に）

<a id="phase-3-runtime-foundation"></a>

## Phase 3：ランタイム基盤

Almideの独自性を確立する。**effect を capability に育てる。**

- [x] **[HKT Foundation Phase 1-4](../../../../roadmap/done/hkt-foundation-phase1.md)** — TypeConstructor/Kind/代数法則, Stream Fusion, Ty::Applied 統一（完了記録の範囲）
- [ ] **HKT の旧残計画 Phase 5-6** — Effect型統合, Trait統合（Phase 4 の Ty 統一は完了記録に含む）
- [x] **[Effect System Phase 1-2](../../../../roadmap/done/effect-system-phase1-2.md)** — 自動推論 capability (7カテゴリ, 推移的), almide check --effects, Security Layer 2
- [ ] **[Effect System Phase 3-4](../../../../specs/effect-system.md)** — Dependency制限, 型レベル統合
- [ ] **型付きホスト束縛** — IDL／スキーマを先に、コード生成を先に置き、手書き FFI を不要にする
- [ ] **リソース／タスクモデル** — タスク／スケジューラ、リソース寿命
- [ ] **永続化／同期プリミティブ** — ローカル優先、CRDT／マージ、同期キュー
- [ ] **観測可能性** — effect トレース、IR ダンプ、パス検査器、ランタイムイベントの時系列
- [ ] **[コンパイル時契約（Compile-Time Contracts）](../../../../roadmap/on-hold/compile-time-contracts.md)** — 一般の関数事前条件・型値不変条件は提案。既存の generic bounds / test-context where とは別で、旧 newtype/where スケッチを現行構文とは扱わない

**命令: "effect をランタイムの権限モデル（runtime permission model）の核にせよ"**

<a id="phase-4-app-runtime-layer"></a>

## Phase 4：アプリランタイム層

UIより先にアプリが継続的に動く実行モデルを作る。

- [ ] ホストのライフサイクルモデル
- [ ] ストレージ／同期ランタイム
- [ ] アプリマニフェスト／権限マニフェスト
- [ ] モジュールのホットアップグレード（署名付き、ABI 検査済み、ロールバック可能）
- [ ] 任意の UI 抽象（ボタンより先に状態／effect／同期／ホスト統合）

<a id="phase-5-platform-expansion"></a>

## Phase 5：プラットフォーム拡張

- [ ] モバイルランタイム（iOS/Android）
- [ ] デスクトップランタイム
- [ ] Web 埋め込み
- [ ] プラグインマーケットプレイス
- [ ] 管理された配布／更新モデル

---

<a id="やらないことリスト-今は"></a>

## やらないことリスト (今は)

| 項目 | 理由 |
|---|---|
| 専用UIフレームワーク | ランタイムの基本機構とホスト境界が先 |
| 独自レンダラー | host UIに寄せるか薄い抽象で十分 |
| 巨大stdlib | 凍結コストが重い。小さく強く |
| "全部できる" ポジショニング | 何が得意で何をまだやらないかを明確に |

---

<a id="3つの最重要命令"></a>

## 3つの最重要命令

1. **1.0を出せ** — 未来を語る前に言語として信用を取る
2. **effectをcapabilityに育てろ** — Almideの独自性はここ
3. **UI より先にランタイムの基本機構を作れ** — app runtime化の本丸は描画ではなく実行モデル

---

<a id="一文で"></a>

## 一文で

> 汎用言語として1.0を完成させよ。
> そのうえでeffect systemをcapability runtimeの核に変換し、
> 型付きホスト境界とランタイムの基本機構を整備せよ。
> UI フレームワークはその後でよい。
