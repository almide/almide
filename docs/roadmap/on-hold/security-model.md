<!-- description: Package boundary, runtime sandbox, and supply chain integrity layers -->
# Security Model — Layer 3–5

Almide のセキュリティは 5 層で構成される。

- **Layer 1** (Effect Isolation) — 実装済み ([done/effect-isolation.md](../done/effect-isolation.md))
- **Layer 2** (Capability Restriction) — 実装済み ([done/effect-system-phase1-2.md](../done/effect-system-phase1-2.md))
  - 自動 capability 推論 (IO/Net/Env/Time/Rand/Fan/Log)
  - `almide.toml [permissions]` で制限
  - `almide check --effects` で可視化
  - 通常の `almide check` に統合

## Layer 3: Package Boundary — dependency の capability 制限

```toml
[dependencies.api-client]
git = "https://github.com/example/api-client"
allow = ["Net"]  # IO は禁止

[dependencies.markdown-lib]
allow = []  # pure only
```

- 依存パッケージが `allow` で許可されていない capability を使うとコンパイルエラー
- → [active/effect-system.md](../active/effect-system.md) Phase 3

## Layer 4: Runtime Sandbox — 実行時の隔離

- WASM ターゲットでの capability-based security
- ファイルシステムアクセスの仮想化
- ネットワークアクセスの allowlist

## Layer 5: Supply Chain Integrity — パッケージの信頼性検証

- パッケージの capability 宣言とコードの整合性を検証
- pure fn only のパッケージは自動的に「安全」マーク
- effect fn を含むパッケージは capability audit が必要

## 実装優先度

| Layer | 内容 | 状態 |
|-------|------|------|
| 1 | Effect Isolation | ✅ 完了 |
| 2 | Capability Restriction | ✅ 完了 |
| 3 | Package Boundary | 未実装 (2.x) |
| 4 | Runtime sandbox | 未実装 (2.x+) |
| 5 | Supply chain | 未実装 (2.x+) |
