# AI-DLC 運用モデル — Loop Engineering

> AWS の AI-Driven Development Lifecycle（AI-DLC）メソッド定義（Raja SP, AWS）を Almide の開発運用に写像し、
> 人間の関与を Mob ポイント（critical juncture）だけに圧縮するための運用規範。
> 実行計画そのものは [docs/roadmap/ROAD_TO_1_0.md](./roadmap/ROAD_TO_1_0.md)、
> ループの実体は [docs/AI_DLC_BOLT_LOOP.md](./AI_DLC_BOLT_LOOP.md)。

## 写像 — AI-DLC の語彙 ↔ Almide の実体

| AI-DLC | Almide での実体 | 備考 |
|---|---|---|
| Intent | decade アーク（0.4x 計測器と編集ループ、…） | ROAD_TO_1_0 の各節。出口ゲート付き |
| Unit | minor バージョン行（ladder の 1 行） | DoD = 行の大機能 + 台帳ルール（issue リンク・テスト・contract 同 PR） |
| Bolt | goal-prompt 駆動の 1 作業サイクル（数時間） | exit gate = CI 緑。1 Bolt ≧ 1 push |
| Deployment Unit | リリース済み minor（tag + 5 プラットフォームバイナリ） | release.yml が生成 |
| Mob Elaboration | ladder の構造変更（行の新設・decade 跨ぎの移動） | 人間必須 |
| Mob Programming / Testing | 通常は不要 — CI ゲート群が代替。Mob ポイント該当時のみ発生 | 下表 |
| Context Memory | git 履歴 + issues + ROAD_TO_1_0 + docs/roadmap/active/ + goal prompt | 新しい状態ファイルは作らない |
| 人間の監督 = loss function | 機械ゲートが拾えない誤差だけを人間が拾う | 次節の分担表 |

AI-DLC の Inception フェーズは 0.41–0.99 について**実施済み** — ROAD_TO_1_0 がその成果物
（Unit 分解・DoD・依存順序・リスク = ゲート）である。以後の Inception は ladder の構造変更時のみ再発生する。

## loss function の分担 — 機械が拾う誤差 / 人間が拾う誤差

**機械（CI ゲート群）が拾う:**

- 型・所有権・メモリ安全 — trust spine、wall（「黙って 0」は存在しない）
- 挙動等価 — 3-way oracle、differential fuzz、byte gate
- 後退 — ratchet 群（perf 双方向、fallback、`flagged-for-revision` は下がるだけ）
- 契約 — `scripts/check-contracts.sh`（fixture ↔ C-NNN 双方向リンク）
- API 表面の完備性 — matrix gate

**人間（Mob）だけが拾える:**

- 意図のドリフト — DoD を満たすが目的とずれる
- 外向き主張の誠実さ — 2026-07 監査の教訓（honesty gradient の逆転）
- 言語の趣味・設計の筋 — syntax、stdlib 境界
- 事業判断 — program track

## Mob ポイント（人間必須の critical junctures）

| # | 事象 | ループの挙動 |
|---|---|---|
| M1 | decade ゲートリリース（0.50 / 0.60 / 0.70 / 0.80 / 0.90） | 監査ブリーフを添えて承認待ち。**通常 minor のリリースは自動** |
| M2 | 言語表面・仕様の決定 — syntax / stdlib 境界 / observable behavior の変更（= contract ledger の追加・変更）/ concurrency の立場 / ALS / 凍結 | 実装前に escalate |
| M3 | 外向き主張の新設・文言変更（README / BENCHMARKS のクレーム）。ゲート済みスクリプトによる数字更新は自動でよい | escalate |
| M4 | ratchet / wall の緩和 — AI は絶対に行わない。fix forward が唯一の道 | 即 escalate |
| M5 | program track（法人・DER・資金・デプロイ） | 人間レーン。ループは触らない |
| M6 | 同一原因の失敗が修正試行後も 2 回連続 / DoD の解釈が割れる / 想定外の破壊的変更が必要 | escalate し、独立な次 Bolt へ。無ければ停止 |

**escalate** = `mob` ラベルの issue を立て（事象 / 根拠 / 選択肢 / 推奨を本文に）、PushNotification を送る。
人間の応答を待つ間、独立な作業があればそちらを続け、無ければループは自ら停止する。

## ループ台帳

| Loop | 中身 | 走らせ方 | 状態 |
|---|---|---|---|
| L1 Bolt ループ（Construction） | [AI_DLC_BOLT_LOOP.md](./AI_DLC_BOLT_LOOP.md) — Unit 選択 → Bolt 計画（issue 上）→ 1 Bolt 実行 → push → CI → リリース判定 | 手元: `/loop` に本手順を渡す（動的ペーシング）。クラウド: `/schedule` で routine 化 | 稼働可 |
| L2 Ops ループ（Operations） | fuzz-nightly triage・CI 赤監視・ratchet ドリフト検知 → 自動修正 or escalate | 0.41 で計測器が蘇ってから `/schedule` の nightly routine に | 0.41 後に開設 |
| L3 Release ループ | L1 に内蔵 — Unit DoD 緑で bump → develop→main PR → merge on green → tag → 検証 | L1 の一部 | 稼働可 |

## 可視性の設計（手放し ≠ 盲目）

新しい状態ファイルは作らず、すべて既存の traceable な場所に書く:

- Bolt の進捗 = commit（英語 1 行）+ Unit issue のチェックリスト更新（証拠リンク付き）
- Unit の完了 = リリースノート（既存 release workflow の成果物）
- 人間が要る事象 = `mob` issue + PushNotification
- 全体の残量 = ROAD_TO_1_0 の issue リンクが closed になっていく様子そのもの

## 手放しの起動手順

1. **手元で回す**（在席時・進捗がターミナルに見える）: `/loop docs/AI_DLC_BOLT_LOOP.md に従い 1 iteration 実行`
2. **離席**: そのまま回し続けてよい — Mob ポイントに当たると通知が来て、独立作業が無ければループは自ら停止する
3. **クラウド常駐**（将来形）: `/schedule` で L1 を日中、L2 を nightly に routine 化する

## この運用モデル自身の DoD

- ループが 1 Unit（まず 0.41）を Mob ポイント以外の人間介入ゼロでリリースまで通せる。
  Mob 以外で人間が呼ばれたら、それはループの欠陥としてこの文書と almide-bolt を直す
- escalate の precision — `mob` issue の 8 割以上が「実際に人間の判断が必要だった」であること。
  ノイズ escalate もループの欠陥
- ratchet 緩和ゼロ・claim ドリフトゼロが継続していること
