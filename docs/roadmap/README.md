# Almide Roadmap

> [GRAND_PLAN.md](GRAND_PLAN.md) — 5フェーズの全体戦略

## Active

なし。1.0 リリース準備完了。

## 1.0 Remaining

- [PRODUCTION_READY.md](PRODUCTION_READY.md) — チェックリスト (残: examples/cookbook, LLM計測)

## On Hold — Phase 2: Production Language (1.x)

| 項目 | 説明 | Grand Plan |
|---|---|---|
| [LSP Server](on-hold/lsp.md) | diagnostics → hover → go-to-def | Phase 2 |
| [Incremental Compilation](on-hold/incremental-compilation.md) | rustc skip when unchanged | Phase 2 |
| [Package Registry](on-hold/package-registry.md) | 公開パッケージ配布 | Phase 2 |
| [Go Target](on-hold/go-target.md) | TOML + 2-3 pass | Phase 2 |
| [Streaming](on-hold/streaming.md) | WebSocket, SSE | Phase 2 |
| [Fan Map Limit](on-hold/fan-map-limit.md) | `fan.map(xs, limit: n, f)` | Phase 2 |
| [Server Async](on-hold/server-async.md) | http.serve effect 化 | Phase 2 |

## On Hold — Phase 3: Runtime Foundation (2.x)

| 項目 | 説明 | Grand Plan |
|---|---|---|
| [Platform Architecture](on-hold/platform-architecture.md) | 5層 app runtime ビジョン | Phase 3-5 |
| [Security Model](on-hold/security-model.md) | Layer 2-5, capability | Phase 3 |
| [Secure by Design](on-hold/secure-by-design.md) | | Phase 3 |
| [Async Backend](on-hold/async-backend.md) | tokio opt-in runtime | Phase 3 |
| [Supervision & Actors](on-hold/supervision-and-actors.md) | | Phase 3 |
| [Rainbow Bridge](on-hold/rainbow-bridge.md) | 外部コード → Almide | Phase 3 |
| [Rainbow FFI Gate](on-hold/rainbow-gate.md) | Almide → 外部ライブラリ | Phase 3 |

## On Hold — Phase 4-5: App Runtime & Platform (2.x+)

| 項目 | 説明 |
|---|---|
| [Almide UI](on-hold/almide-ui.md) | Reactive UI framework |
| [Web Framework](on-hold/web-framework.md) | Hono 相当 |
| [Almide Shell](on-hold/almide-shell.md) | AI-native REPL |
| [LLM Integration](on-hold/llm-integration.md) | `almide forge`, `almide fix` |
| [LLM → IR Generation](on-hold/llm-ir-generation.md) | Parser bypass |
| [Self-Hosting](on-hold/self-hosting.md) | |

## On Hold — Compiler Internals

| 項目 | 説明 | 状態 |
|---|---|---|
| [build.rs syn Scanner](on-hold/buildrs-syn-scanner.md) | runtime scanner堅牢化 | 壊れたらやる |
| [Direct WASM Emission](on-hold/emit-wasm-direct.md) | rustc bypass | 実験的 |
| [IR Interpreter](on-hold/ir-interpreter.md) | rustc不要で即実行 | 実験的 |

## On Hold — Research / Misc

| 項目 | 説明 |
|---|---|
| [Research: MSR Paper](on-hold/research-modification-survival-rate-paper.md) | LLM accuracy 論文 |
| [Benchmark Report](on-hold/benchmark-report.md) | |
| [REPL](on-hold/repl.md) | |

## Archived (完了 or 統合済み)

以下はdone/に移動済み、または他の項目に統合:

- ~~Design Debt~~ → 完了 (gen_generated_call排除、emit_rust/emit_ts削除)
- ~~Grammar Codegen~~ → 完了 (build.rs TOML codegen)
- ~~Multi-Target Strategy~~ → Go Target に統合
- ~~New Codegen Targets~~ → Go Target に統合
- ~~Lower 2パス分離~~ → 確認済み (既に分離されてた)
- ~~Checker InferTy/Ty統一~~ → 完了 (InferTy廃止)
- ~~Polish (Immediate)~~ → 完了
- ~~Production Ready (old)~~ → PRODUCTION_READY.md に統合
- ~~Stdlib Strategy~~ → Verb Reform完了、387関数で凍結
- ~~Template~~ → codegen v3 TOML templates に統合
- ~~UFCS External~~ → stdlib UFCS で対応済み
- ~~Concat Operator Reform~~ → 完了 (++ → +)
- ~~Cross-Target AOT~~ → codegen v3 で対応
- ~~Cross-Target Semantics~~ → cross-target CI 106/106 で検証済み
- ~~Platform / Target Separation~~ → Platform Architecture に統合
- ~~Stdlib 3-Layer Design~~ → stdlib verb reform で対応
- ~~TS Edge-Native~~ → v3 TS codegen で対応
- ~~Tooling (remaining)~~ → CLI-First 完了
- ~~Editor & GitHub Integration~~ → LSP に統合
- ~~LLM Immutable Sugar~~ → LLM Immutable Patterns 完了
- ~~Built-in Protocols~~ → Derive Conventions 完了
- ~~Almide Runtime~~ → Platform Architecture に統合

## Done

100+ items completed. See `docs/roadmap/done/` directory.
