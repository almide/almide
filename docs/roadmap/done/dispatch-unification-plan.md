<!-- description: Unify Rust + WASM stdlib dispatch via IR-level RuntimeCall; attributes become sugar -->
<!-- done: 2026-04-19 -->
# Dispatch Unification Plan (S3 Phase 1e)

## Completion status (2026-04-19)

全 sub-commit landed。Option 3 (理想形) 到達:

- **1e-1** (parser + docs): landed — `Attribute` の既存 generic 構造で `@intrinsic("sym")` は最初から受理されていた。
- **1e-2** (`IrExprKind::RuntimeCall` + `IntrinsicLoweringPass`): landed — `pass_intrinsic_lowering.rs` で `@intrinsic` Module call を `RuntimeCall { symbol, args }` に rewrite。borrow decoration は `pass_borrow_inference::intrinsic_borrow_mode` が param 型から自動導出。
- **1e-3** (env / int simple 移行): landed — `env` と `int` の single-runtime-call 系は `@intrinsic`。
- **1e-4+** (L1/L2 モジュール順次移行): landed — `Stdlib Declarative Unification` arc (commit 49d341f6) で list / map / option / result / random / regex / testing / matrix / http / json / datetime / fs / process / io / bytes / float / base64 / hex / string / error + sized type module 全 8 ファイルまで貫通。
- **1e-last** (`@inline_rust` deprecate): landed — PR 49d341f6 で stdlib から `^@inline_rust` 宣言ゼロ到達。`@inline_rust` 機能自体は escape hatch として残すが、stdlib は使わない。

**Net outcome:**
- `calls_<m>.rs` の L0-L1 dispatch arm は大幅削減
- Rust / WASM は `RuntimeCall { symbol, args }` を共通 IR node として消費
- MLIR arc / egg arc が `RuntimeCall` を MLIR Op / e-graph node に 1-to-1 で写像可能

**Relationship to:** `stdlib-declarative-unification.md` (already done, 2026-04-19) — 本 arc の適用先。

## Original plan (retained for history)

> **立ち位置**: `codegen-ideal-form.md §Phase 3 Arc Step S3` の Phase 1e 設計。
> Phase 1b (bundled → Named rewrite)、Phase 1c (stub panic 化)、Phase 1d
> (helpers delete + inline panic) は landed。Phase 1e は Rust / WASM dispatch
> 層の非対称性を解消する arc 本丸。

## 1. 理想形 (final target) — MLIR + egg + MSR から逆算

LLM は `.almd` 1 ファイルで stdlib fn を宣言し、両 target に自動で流れる。
compiler の最終状態は以下:

```almide
// stdlib/int.almd (ideal):
@intrinsic("almide_rt_int_parse")
fn parse(s: String) -> Result[Int, String]

// 合成は純 Almide で書く (template 文字列で小細工しない)
fn parse_or_zero(s: String) -> Int = parse(s) ?? 0
```

- attribute は **primitive 宣言の symbol 1 個**だけ
- IR 上では `IrExprKind::RuntimeCall { symbol, args }` として存在
- Rust emit: `almide_rt_int_parse(&*s)` (`&*` は IR 型から導出)
- WASM emit: `call(rt.int.parse)` (pre/post 装飾は IR 型から導出)
- 任意 Rust source template は排除 — 合成は Almide で書く

これが **MLIR arc** (Op を IR に embed / dialect lowering で target 固有 code
生成) および **egg arc** (IR パターンマッチで rewrite/fusion) および **MSR**
(LLM が書くのは .almd だけ) の共通終着点。

## 2. 実装経路 — Option 2 (共通 `@intrinsic`) を経由

理想形 (Option 3) を一気に狙うと、現行 stdlib の複雑 `@inline_rust` template
(例: `almide_rt_int_parse(&*{s}).map(|r| r.unwrap_or(0))`) を **Almide source
に昇格**する並行作業が発生し blocker になる。

現実解は **Option 2 を踏み台に Option 3 を育てる**:

- `@intrinsic(symbol)` attribute を新設し、`@inline_rust` と **共存**
- 単純な single-runtime-call 系は `@intrinsic` に移行
- 複雑 template は当面 `@inline_rust` で残す (将来 Almide source に昇格)
- すべてが移行しきったら `@inline_rust` を deprecate → Option 3 到達

Option 1 (target-specific `@wasm_intrinsic("prefix:...")`) は並列 attribute の
冗長が残り、MLIR 導入時にもう一度書き直しになるため **detour**。採用しない。

## 3. 現状 (2026-04-18 時点)

### Rust target — declarative

```
stdlib/int.almd: @inline_rust("almide_rt_int_parse(&*{s})") fn parse(...) = _
  ↓ pass_stdlib_lowering
IrExprKind::InlineRust { template, args }
  ↓ walker
"almide_rt_int_parse(&*s)"   // string substitution
```

### WASM target — imperative (19k 行)

```
pass_stdlib_lowering は WASM target では skip
  ↓
emit_wasm/calls.rs: _ if module == "int" => emit_int_call(func, args)
  ↓
emit_wasm/calls_numeric.rs: "parse" => handwritten wasm! instructions
```

`@inline_rust` attribute は WASM 側から**一切参照されない**。
`@wasm_intrinsic` attribute は AST にあるが **argument 無しフラグ**として
bundled body skip にしか使われていない。

### WASM emit pattern 分類

実際の `calls_<m>.rs` を見ると 4 層:

| Level | パターン | 代表例 | 移行難度 |
|---|---|---|---|
| **L0** | literal const (`i32_const`, intern) | `env.temp_dir`, `env.os` | 容易 |
| **L1** | 単一 runtime fn call | `int.parse`, `datetime.year`, `env.unix_timestamp` | 容易 |
| **L2** | scratch alloc + runtime + finalize | `string.concat`, `list.push` | 中 |
| **L3** | inline 算術 / 制御フロー | `datetime.from_parts` (JDN), `list.sort`, `fan.race` | **不可能 — 手書き維持** |

行数分布 (`wc -l calls_*.rs`):

| File | 行数 | 主な Level |
|---|---:|---|
| calls_bytes.rs | 2449 | L2-L3 |
| calls_matrix.rs | 2217 | L3 |
| calls_value.rs | 1894 | L2-L3 |
| calls_fs.rs | 1730 | L2 |
| calls_list.rs | 1090 | L2-L3 |
| calls_list_closure.rs | 1081 | L3 |
| calls_map.rs | 1002 | L2-L3 |
| calls_numeric.rs | 667 | **L1 中心 — 移行候補筆頭** |
| calls_string.rs | 707 | L1-L2 |
| calls_env.rs | 71 | **L0-L1 — 最小移行対象** |
| calls_datetime.rs | ~450 | L1-L3 混在 |
| calls_http.rs | ~350 | L1 中心 |
| calls_process.rs | ~275 | L1 中心 |
| calls_io.rs | ~420 | L1-L2 |
| calls_random.rs | ~218 | L1 中心 |

## 4. 段取り

### Sub-commit 1e-1: `@intrinsic` parse 受理 + docs 固め *(this PR)*

**scope**: parser は既に汎用 `@name(args)` を受理するため変更不要。docs で
理想形 (Option 3) → 実装経路 (Option 2) を確立、既存 `@inline_rust` / 新
`@intrinsic` の責務分離を明文化。smoke test で `@intrinsic("sym")` が
`Attribute { name: "intrinsic", args: [String("sym")] }` に落ちることを
確認する。

**見積**: 30min-1h、docs + 1 test。

### Sub-commit 1e-2: `IrExprKind::RuntimeCall` + resolve pass 拡張

**scope**: IR に `RuntimeCall { symbol: Sym, args: Vec<IrExpr> }` variant
追加。新 pass (`pass_intrinsic_lowering.rs` or `pass_resolve_calls` 拡張) が
`@intrinsic` を持つ fn への Module call を `RuntimeCall` に rewrite。Rust emit
+ WASM emit の両方で `RuntimeCall` を受けて target 固有 code を生成。borrow
decoration (`&*`, `.clone()`, `.as_str()`) は **emit 側が IR 型から導出**。

**見積**: 4-6h、下流 pass 経路 (`pass_borrow_inference`, `pass_clone_insertion`
等) への影響も要確認。

### Sub-commit 1e-3: env / int simple 移行

**scope**: `env.temp_dir` (L0)、`env.os` (L0)、`int.parse` / `int.to_string`
(L1) など single-runtime-call 型 fn の `@inline_rust` を `@intrinsic` に置き
換え。対応する `emit_env_call` / `emit_int_call` match arm を削除。

**見積**: 2-3h、env を完全 decl 化 → `calls_env.rs` 削除。

### Sub-commit 1e-4+: L1 中心モジュール順次移行

float, random, process, http。次いで L2 (string, io) 部分移行。

**見積**: 2-3h × 5-8 モジュール = 合計 10-20h、複数 sub-commit。

### Sub-commit 1e-last: `@inline_rust` deprecate

**scope**: 残る `@inline_rust` を Almide source に昇格 (`fn parse_or_zero(s) =
parse(s) ?? 0` 形式)、または L3 intrinsic (`@wasm_builtin("...")` 的 DSL) を
別途設計。`@inline_rust` attribute の削除で stdlib は **Option 3 到達**。

**見積**: 別アーク扱い、MLIR arc 直前の clean up。

## 5. 非 goal / 後送り

- **L3 の declarative 化** — JDN 算術や sort は手書き維持
- **runtime fn (`runtime/rs/src/<m>.rs`) と WASM runtime (`emit_wasm/rt_*.rs`)
  の共通化** — 別 arc
- **単一 `CallTarget` variant への集約** — IR 全層に波及、scope 外
- **MLIR 導入** — 本 arc 完了後、独立アーク (`mlir-backend-adoption.md`)

## 6. Why (この arc を通す根拠)

- **MSR**: 新 stdlib fn 追加のコストが「Rust template + `.almd` signature +
  `calls_<m>.rs` match arm」の 3 層作業 → `.almd` 2 行で済む → LLM にも一意に
  教えられる
- **NumPy 超え / Matrix[T]**: egg rewrite / fusion pass は `RuntimeCall` IR
  node をパターンマッチ。`Matrix.add(Matrix.mul(A, B), C)` を fused SIMD call
  に書き換える rule が書ける
- **MLIR arc**: `RuntimeCall` は MLIR Op に 1-to-1 で写像可能。attribute 層を
  消さずに MLIR dialect lowering へ繋がる
- **dispatch 行数削減**: L0-L1 declarative 化で `calls_<m>.rs` を 50-70% 削減
  (19k の大半が L2-L3、だが簡単な部分は確実に縮む)

## 7. How to apply (次セッション向け)

- Sub-commit 1e-2 は IR 変更が核。`RuntimeCall` variant 追加時は serializer /
  visitor / pretty-printer / verify_program など一連の trait 実装が必要
- borrow decoration の IR 型からの導出は `pass_borrow_inference` と同じ
  logic を再利用する想定
- 各 sub-commit で `spec/` + `nn` 両 target で hard contract (debug panic) を
  通ること = 移行完了のシグナル
- L0-L1 粒度を誤って L2 パターンを DSL に押し込まない — L2 以降は
  `@inline_rust` のまま残すか Almide source に昇格

## 参考

- `docs/roadmap/active/stdlib-declarative-unification.md` — 上位 arc
- `docs/roadmap/active/mlir-backend-adoption.md` — この arc の次
- `stdlib/int.almd` / `stdlib/env.almd` — `@inline_rust` の実例
- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — Rust 側 lowering の模範
- `crates/almide-codegen/src/emit_wasm/calls_env.rs` — L0-L1 の最小移行対象
- `crates/almide-codegen/src/emit_wasm/calls_datetime.rs:12-72` — L3 の例
  (移行しない)
- `crates/almide-syntax/src/ast.rs:281` — `Attribute` / `AttrArg` / `AttrValue`
  は既に `@intrinsic("sym")` を受理できる generic 構造
