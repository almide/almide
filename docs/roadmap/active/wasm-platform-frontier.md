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
| tail calls | almide-mir の `tail_call_indexes`（render_wasm_b.rs）が関数末尾 CallFn を分類し `return_call` を出力（#864 で再移植、C-178）。self 再帰は上流 TCO ループ。`return_call_indirect`（クロージャ末尾）も**移植済 (2026-08-03)** — 同じ分類が CallIndirect を admit（fixture: closure_tail_recursion + 命令 gate: tail_call_indirect_test.rs）。純 indirect 無限サイクルは creator 側 drop が正当に decline するため深度主張は fixture ヘッダの通り正直化 |
| SIMD (v128) | 旧 almide-codegen 時代の 4× v128 アンロールは almide-mir 未移植（デフォルト出力に v128 なし、#864 調査で確認）。適用拡大は [wasm-optimization-roadmap](../done/wasm-optimization-roadmap.md) |
| 厳格検証 | wasmtime 45+ / V8 strict validator 前提 (StackBalancePass) |

### 取る (小さくてミッション直結)

- **Deterministic profile 準拠の明文化** — **済 (2026-08-03, C-210)**。監査の結論:
  NaN の生ビットが観測面に届く経路は `float.to_bits` と bytes float write 族の
  2 面だけ (Float は hash 不可、to_string は "NaN" 固定)。両面とも観測境界で
  canonical NaN (f64 0x7FF8000000000000 / f32 0x7FC00000) に正規化 —
  x86 の sign-set NaN、エンジンの payload propagation 差、from_bits 経由の
  payload 密輸をまとめて遮断し、プロファイルより強く**両ターゲット + 全ホスト
  アーキで同一**。fixture 2 本 (nan_canonical_*) + 命令サブセット gate
  (deterministic_profile_test.rs — relaxed SIMD / atomics / shared 不使用の
  機械検査)。副産物: 監査が self-host リンクの同名異署名衝突 =
  invalid-wasm-as-Ok 脱出 (#1068) を発見し、リンカを完全一致 merge + 衝突 wall 化。
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

標準側の現在地 (2026-08 再検証):
- WASI 0.2 stable。`wasi-http` は wasmtime にホスト実装あり、ブラウザ/Node は jco
- **WASI 0.3.0 正式リリース 2026-06-11・同月 ratify で stable** (RC は 2025-11 の
  Spin v3.5 が初出、正式サポートは wasmtime 43+)。canonical ABI レベルの native
  async (`async func` / `stream<T>` / `future<T>`)、`wasi:io` は Canonical ABI に
  吸収され廃止。以後 0.3.x はリリーストレイン。WASI 1.0 は 2026 年後半目標のまま
- ブラウザは WASI を直接話さない — 0.2 でも 0.3 でも jco かシムが必要
- 本 repo の CI は wasmtime 47.0.3 に pin（2026-08 更新: 42.0.1 から bump。0.3
  ホスト 43+ と厳格検証 45+ の両前提を満たす）

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
