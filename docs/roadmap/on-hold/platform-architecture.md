# Almide Platform Architecture Vision

**優先度:** post-1.0 (2.x)
**リサーチ:** Flutter/RN New Architectureの教訓

## ビジョン

Almideを「app runtime **にもなる** 汎用言語」として設計する。
CLI/server/scriptingの実用性を維持しつつ、app runtime層を追加レイヤーとして乗せる。
言語が先、プラットフォームは後。Kotlinの軌跡（JVM → Android → Multiplatform → Server）と同じ方向。

```
[Almide Language + DSL]
        ↓
[Typed IR / Nanopass Compiler]       ← 完成済み
        ↓
[Reactive Runtime + Effects]         ← effect fn + fan (基礎あり)
        ↓
[Domain Core / Sync / Persistence]   ← 未着手
        ↓
[Host Bindings via IDL + Codegen]    ← stdlib TOML (原型あり)
        ↓
[Pluggable Renderer]                 ← 未着手
        ↓
[iOS / Android / Web / Desktop]
```

## 5層アーキテクチャ

### Layer 1: Language Kernel ✅
- メモリ管理 (borrow/clone analysis)
- 並行実行 (fan)
- エフェクト分離 (effect fn)
- capability permission (effect isolation Layer 1)

### Layer 2: Typed Host Boundary (1.x)
- 現在: stdlib TOML + build.rs codegen
- 目標: IDL/schema-first, codegen-first host bindings
- ゼロコピー境界、同期/非同期明示

### Layer 3: Core Domain Runtime (2.x)
- state machine
- offline-first data graph
- sync engine (CRDT/merge)
- cache/persistence abstraction

### Layer 4: Pluggable Renderer (2.x)
- Native widget renderer (OS親和)
- Custom scene renderer (Flutter的一貫性)
- Hybrid renderer (画面単位切替)

### Layer 5: Evolution Layer (1.x-)
- edition system ✅
- ABI versioning
- module-level migration
- schema versioning

## Almideが既に持っている優位性

| 要素 | 状態 |
|---|---|
| Typed IR + Nanopass compiler | ✅ 完成 |
| Multi-target codegen (Rust/TS) | ✅ 完成 |
| Effect isolation | ✅ Layer 1 |
| Fan concurrency | ✅ thread + Promise |
| Package management | ✅ almide.lock |
| Edition system | ✅ 2026 |
| Template-driven target extension | ✅ TOML + pass |

## 次のステップ (1.x で着手可能)

1. **Capability declarations** — effect fn の permission model 拡張
2. **IDL for host bindings** — stdlib TOML → 汎用IDL への進化
3. **Hot module replacement** — signed + ABI checked module swap
4. **Devtools** — reactive graph inspector, host call profiler

## 設計原則

1. **UIを中核にしない** — 実行モデル + 型付き境界 + データ同期が中核
2. **ブリッジではなくFFI的宣言境界** — schema-first / codegen-first
3. **描画とホスト統合を分離** — Renderer と Host API は別レイヤー
4. **フレームワークではなく "OS for apps"** — runtime + package + permission + diagnostics + devtools
