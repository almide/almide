# Codegen Unification: WASM と Rust/TS/JS の共通基盤 [ACTIVE]

## 背景

WASM direct emission の完成（129/129, DCE済み）により、コンパイラに2つの独立したcodegenパスが存在する:

- **Rust/TS/JS**: Nanopass pipeline → Walker + TOML Templates → ソースコード文字列
- **WASM**: (TCO + ResultProp 手動呼び出し) → FuncCompiler + wasm! マクロ → バイナリ

Mono以前は完全共通。Mono以降が別世界になっている。

## 現状の問題

1. **Nanopassの不整合**: WASMは `Target::Rust` をハードコードして TCO + ResultProp だけ手動実行。StreamFusion, EffectInference, FanLowering など恩恵のあるpassが実行されない
2. **Stdlib dispatchの二重管理**: 381関数の追加・変更時に TOML テンプレート（Rust/TS/JS）と Rust match文（WASM calls_*.rs）の両方を手動同期する必要がある
3. **Pass追加時のコスト**: 新しいターゲット非依存passを追加するとき、WASMだけbuild.rsに手動追加が必要

## Phase A: Target::Wasm 追加（小）

`pass.rs` の Target enum に Wasm を追加し、WASM用のnanopass pipelineを定義。build.rsの手動pass呼び出しを廃止。

```rust
// pass.rs
pub enum Target {
    Rust, TypeScript, JavaScript, Go, Python, Wasm,
}

// target.rs
Target::Wasm => Pipeline::new()
    .add(TailCallOptPass)
    .add(EffectInferencePass)
    .add(StreamFusionPass)
    .add(ResultPropagationPass)
    .add(FanLoweringPass)
```

**変更箇所:**
- `src/codegen/pass.rs` — Target enum に Wasm 追加
- `src/codegen/target.rs` — Wasm pipeline 定義
- `src/cli/build.rs` — 手動pass呼び出しを `config.pipeline.run()` に置換
- `src/cli/commands.rs` — テストランナーも同様
- `src/codegen/mod.rs` — `emit_wasm_binary` に pipeline 実行を統合

**効果:** 新しいpassを追加したとき、WASMも自動的に恩恵を受ける。

## Phase B: Stdlib dispatch宣言の一元化（中）

`stdlib/defs/*.toml` にWASMディスパッチ情報を追加。calls.rsの巨大match文をbuild.rsで自動生成。

現在のTOML（Rust/TS/JS用）:
```toml
[string.contains]
rust = "{0}.contains({1})"
ts = "{0}.includes({1})"
```

拡張案:
```toml
[string.contains]
rust = "{0}.contains({1})"
ts = "{0}.includes({1})"
wasm_handler = "emit_string_call"  # calls_string.rs にルーティング
wasm_rt = "__str_contains"         # 対応するruntime関数名（省略可）
```

build.rsが `wasm_handler` からdispatch tableを生成 → calls.rsのmatch文が宣言的に。

**効果:** 新しいstdlib関数を追加するとき、TOMLに1行足すだけで全ターゲットにルーティングされる。

## やらないこと

**共通IR lowering layer（Walker/FuncCompiler間の抽象化）** — テキスト生成とバイナリ生成は本質的に異なる。WASMのスタックマシン最適化（scratch local再利用、block nesting depth管理）は共通IRでは表現できない。無理に共通化すると両方劣化する。

## 優先度

| Phase | 規模 | 効果 | 依存 |
|-------|------|------|------|
| A     | 1日  | Pass追加の自動伝搬、StreamFusion/FanLowering恩恵 | なし |
| B     | 1-2週 | Stdlib管理の一元化、WASM calls.rs自動生成 | Phase A推奨 |
