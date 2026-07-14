<!-- description: Post-Wasm-3.0 platform tracking — WASI 0.3 / Component Model, stack switching, shared-everything-threads -->
# WASM Platform Frontier — beyond core Wasm 3.0

> **Active scope: Phase 0-1** — deterministic profile 明文化 + `almide_host` ABI 設計。
> Phase 2-4 (component target, 0.3 async, threads) は標準側の成熟待ちで段階導入。
> **Exit criteria (Phase 1)**: ブラウザ(fetch shim)と wasmtime(host fn)の両ホストで
> 同一 .wasm が `http.get/post` を実行できる。

コア Wasm 3.0 (2025-09 標準化) への追従は**ほぼ完了している**。残る投資先は
コア仕様の外側 — WASI / Component Model と post-3.0 proposals — にある。
この roadmap は「何が済んでいて、何を取らないと決め、何を待つか」を記録する。

## Core Wasm 3.0 — 現状監査 (2026-06-05)

### 採用済み

| 機能 | 実装 |
|------|------|
| tail calls | `pass_tail_call_mark.rs` が末尾位置を検出。ユーザー関数 `return_call` / クロージャ `return_call_indirect` (`emit_wasm/calls.rs` `emit_tail_call`) |
| SIMD (v128) | list map 系に 4× v128 アンロール fast path (`calls_list_closure2.rs`)。※v128 自体は 2.0 機能。適用拡大は [wasm-optimization-roadmap](wasm-optimization-roadmap.md) |
| 厳格検証 | wasmtime 45+ / V8 strict validator 前提 (StackBalancePass) |

### 取る (小さくてミッション直結)

- **Deterministic profile 準拠の明文化** — Wasm 3.0 が決定的実行プロファイル
  (NaN 正規化、非決定機能の排除) を正式定義した。byte-identical cross-target
  保証の仕様語彙そのもの。TODO: NaN ビットパターンが観測可能な経路
  (float ビット再解釈系) の監査 + 「relaxed SIMD 不使用」宣言 + xtarget gate への一文。
  現状 emit_wasm に NaN 正規化は無い。
  ※対象は **emit されたプログラムの実行決定性**。コンパイラ自身の出力決定性
  (emitter = pure fn of (IR, target)) は [determinism-belt](determinism-belt.md) が担当。
- extended const expressions — global 初期化の柔軟化。微小。

### 検討 (実測で問題になってから)

- **例外処理 (`try_table`/`throw`)** — effect fn のエラー伝播をタグ検査なしにできる。
  ブロッカー: **Perceus RC と unwinding の相互作用** (巻き戻し中の RC decrement
  スキップ = リーク。landing pad 相当の cleanup 設計が必要)。Result は第一級の値
  なので値表現は残り、伝播 fast path だけの二重エンコードになる。ROI 低〜中。

### 取らない (理由付き)

| 機能 | 理由 |
|------|------|
| GC | linear↔wasm-gc の二重バックエンドは builtin parity 税を払う。RC/COW の自前管理こそ byte-identical gate の土台 |
| relaxed SIMD | 仕様として実装依存の結果を許す = 等価性保証と正面衝突。SIMD は 2.0 fixed v128 の適用拡大で取る |
| typed function references (`call_ref`) | ref 型は線形メモリに格納不可 → クロージャを線形メモリ構造体 + テーブル番号で持つ限り構造的に使えない (GC 採用時のみ意味を持つ) |
| memory64 | 4GB で足りる用途に bounds check コストだけ増える |
| multiple memories | 単一メモリは iOS Safari 互換のための意図的設計 (`emit_wasm/mod.rs` Memory section コメント) |

## フロンティア — 3.0 の外側

### 1. WASI 0.2/0.3 + Component Model — http/async の標準 ABI

現状の wasm ターゲットは WASI preview 1 のみ (fs/clock/random/stdio/proc_exit)。
ソケット・HTTP クライアント・プロセス起動は無い (`calls_http.rs` は
`http.response`/`http.json` の純粋ビルダーのみ)。

標準側の現在地 (2026-06 検証済み):
- WASI 0.2 stable。`wasi-http` は wasmtime にホスト実装あり、ブラウザ/Node は jco
- **WASI 0.3 released 2026-02** (wasmtime 37+ プレビュー)。canonical ABI レベルの
  native async (`stream<T>`/`future<T>`)。WASI 1.0 は 2026 年後半目標
- ブラウザは WASI を直接話さない — 0.2 でも 0.3 でも jco かシムが必要

解禁されるもの: `wasi-http` + native async stream = LLM API への SSE ストリーミング。
エージェントループを Almide で書き 1 つの component として wasmtime / Spin /
wasmCloud / edge ホストへデプロイできる。ホスト側 outgoing request 検閲
(capability-scoped network) は [effect-system-capability](effect-system-capability.md)
の Layer 2/3 とそのまま噛み合う。

コスト: canonical ABI lifting/lowering (string UTF-8 境界変換、list/record/variant、
resource handle) は同期版だけでも大工事。0.3 async ABI は Rust 本体ですら
2026 年ゴールの最前線。wasmtime の p1→p2 アダプタは既存 p1 API しかマップしない
ため **http はアダプタ経由では手に入らない**。

段階導入:
1. **Phase 1 (今): custom `almide_host` import ABI** — `http.get/post` 等を
   ホストインポートで提供。ブラウザ = fetch shim、サーバー = wasmtime host fn。
   コンパイラ側は import 追加のみで canonical ABI 不要
2. **Phase 2: p1→p2 アダプタ** — 既存モジュールの component 化 (fs-only、ほぼ無料)
3. **Phase 3: 0.2 同期 canonical ABI emit** — wasi-http 直結
4. **Phase 4: 0.3 async** — WASI 1.0 が見えてから。custom ABI を薄い互換層に畳んで廃止

### 2. stack switching (post-3.0 proposal) — wasm 上の async 実行モデル

コルーチン/async の基盤。proposal phase 進行を追跡。Phase 4 の async canonical ABI と合流する。

### 3. shared-everything-threads (post-3.0 proposal) — `fan.*` の wasm 側本格化

wasm 単スレッド制約が `fan.*` の cross-target 意味論差の根本原因だった
(fan.timeout はこの制約ゆえ 0.29.0 で言語から削除)。本物の共有メモリ
スレッドが入れば fan の wasm 実装を native と同型にできる。proposal phase
進行を追跡。

## 追跡すべきバージョン番号

「Wasm 4.0」は存在せず計画も無い。追うのは:
- **WASI 0.3.x → 1.0** (リリーストレイン、1.0 = 2026 年後半目標)
- **stack switching / shared-everything-threads の proposal phase**

## References

- [Wasm 3.0 announcement](https://webassembly.org/news/2025-09-17-wasm-3.0/)
- [WASI roadmap](https://wasi.dev/roadmap)
- [WebAssembly proposals tracking](https://github.com/WebAssembly/proposals)
- [wasmtime-wasi-http](https://docs.wasmtime.dev/api/wasmtime_wasi_http/index.html)
