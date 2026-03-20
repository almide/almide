# Almide Grand Plan

> 「1.0で言語として勝ち切る。その後に "app runtimeにもなる" ための土台を、UIより先に積む」

---

## Phase 1: 1.0 Completion ← **NOW**

言語として信用を取る。機能を増やさず、完成の定義を固定する。

- [x] 言語仕様の凍結 (構文・演算子・キーワード確定)
- [x] コンパイラ挙動の安定化 (ICE=0, warning=0, cross-target 106/106)
- [x] stdlib 境界の確定 (22モジュール 381関数, verb reform完了, uuid/crypto/toml/compress/term 除外)
- [x] エラーメッセージ品質 (E001-E010, --json, hint system)
- [x] 破壊的変更ポリシー (edition, FROZEN_API, REJECTED_PATTERNS)
- [x] showcases (CLI/API/Data/DevTool/Script 5本)
- [ ] examples / cookbook / migration story
- [ ] LLM計測 (MSR, 初回正答率)

**ゴール: "この言語はもう信用して使っていい" という状態**

## Phase 2: Production Language

配布単位を強くする。module economy を作る。

- [x] package manifest (almide.toml)
- [x] lockfile (almide.lock)
- [ ] semver 規約の明文化
- [ ] 公開APIの安定性ルール
- [ ] build cache (incremental compilation)
- [ ] 再現可能ビルド
- [ ] monorepo / polyrepo 対応
- [ ] target compatibility contract (multi-target を "約束" に)

## Phase 3: Runtime Foundation

Almideの独自性を確立する。**effect を capability に育てる。**

- [x] **[HKT Foundation Phase 1-3](done/hkt-foundation-phase1.md)** — TypeConstructor/Kind/代数法則, Stream Fusion (map+map, filter+filter, map+fold 融合)
- [ ] **[HKT Foundation Phase 4-6](active/hkt-foundation.md)** — Ty統一リファクタ, Effect型統合, Trait統合
- [x] **[Effect System Phase 1-2](done/effect-system-phase1-2.md)** — 自動推論 capability (7カテゴリ, 推移的), almide check --effects, Security Layer 2
- [ ] **[Effect System Phase 3-4](active/effect-system.md)** — Dependency制限, 型レベル統合
- [ ] **Typed host bindings** — IDL/schema-first, codegen-first, 手書きFFI不要
- [ ] **Resource/task model** — task/scheduler, resource lifetime
- [ ] **Persistence/sync primitives** — local-first, CRDT/merge, sync queue
- [ ] **Observability** — effect trace, IR dump, pass inspector, runtime event timeline

**命令: "effect を runtime permission model の核にせよ"**

## Phase 4: App Runtime Layer

UIより先にアプリが継続的に動く実行モデルを作る。

- [ ] Host lifecycle model
- [ ] Storage/sync runtime
- [ ] App manifest / permission manifest
- [ ] Module hot-upgrade (signed, ABI checked, rollback possible)
- [ ] Optional UI abstraction (button より先に state/effect/sync/host integration)

## Phase 5: Platform Expansion

- [ ] Mobile runtime (iOS/Android)
- [ ] Desktop runtime
- [ ] Web embedding
- [ ] Plugin marketplace
- [ ] Managed distribution/update model

---

## やらないことリスト (今は)

| 項目 | 理由 |
|---|---|
| 専用UIフレームワーク | runtime primitives と host boundary が先 |
| 独自レンダラー | host UIに寄せるか薄い抽象で十分 |
| 巨大stdlib | 凍結コストが重い。小さく強く |
| "全部できる" ポジショニング | 何が得意で何をまだやらないかを明確に |

---

## 3つの最重要命令

1. **1.0を出せ** — 未来を語る前に言語として信用を取る
2. **effectをcapabilityに育てろ** — Almideの独自性はここ
3. **UIより先にruntime primitivesを作れ** — app runtime化の本丸は描画ではなく実行モデル

---

## 一文で

> 汎用言語として1.0を完成させよ。
> そのうえでeffect systemをcapability runtimeの核に変換し、
> typed host boundaryとruntime primitivesを整備せよ。
> UI frameworkはその後でよい。
