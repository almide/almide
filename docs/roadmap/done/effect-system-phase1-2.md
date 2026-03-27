<!-- description: Effect inference engine with 7 categories and checker integration -->
# Effect System — Phase 1-2

**完了日:** 2026-03-19
**PR:** #49

## 実装内容

### Phase 1: Effect 推論エンジン
- `EffectInferencePass` Nanopass — 7 effect カテゴリ (IO, Net, Env, Time, Rand, Fan, Log)
- stdlib モジュール → effect マッピング (fs→IO, http→Net, env→Env, etc.)
- 直接 effect 収集 (Module call + Named call + fan 式)
- コールグラフ構築 + fixpoint iteration による推移的 effect 推論
- IrProgram に `effect_map` フィールド追加
- `almide check --effects <file>` CLI コマンド
- ALMIDE_DEBUG_EFFECTS=1 で分析出力

### Phase 2: Self-package 制限 (Security Layer 2)
- `almide.toml [permissions] allow = ["IO", "Net"]` のパース
- `almide check --effects` で違反検出 + hint 表示
- 通常の `almide check` にも統合 — [permissions] があれば自動で違反検出
- `project.rs` の `Project` struct に `permissions` フィールド追加

## 残り (Phase 3-4 → active/effect-system.md に記載)

- Phase 3: Dependency 制限 (`[dependencies.X].allow`) → 2.x
- Phase 4: 型レベル統合 (HKT Foundation Phase 4 と合流) → 2.x

## テスト
- 5 internal tests (module_to_effect, runtime_name_to_effect, format_effects)
- 110/110 almide tests
