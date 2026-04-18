<!-- description: Unify Rust + WASM stdlib dispatch via @wasm_intrinsic template expansion -->
# Dispatch Unification Plan (S3 Phase 1e)

> **立ち位置**: `codegen-ideal-form.md §Phase 3 Arc Step S3` の Phase 1e 設計ドキュメント。
> Phase 1b (bundled → Named rewrite)、Phase 1c (stub panic 化)、
> Phase 1d (helpers delete + inline panic) は landed。Phase 1e は Rust / WASM
> dispatch 層の非対称性を解消する arc 本丸。

## 1. 現状の非対称

### Rust target — declarative

```
stdlib/int.almd:
  @inline_rust("almide_rt_int_parse(&*{s})")
  fn parse(s: String) -> Result[Int, String] = _
                ↓
pass_stdlib_lowering:
  CallTarget::Module { int, parse } + args[s]
                ↓
  IrExprKind::InlineRust { template: "almide_rt_int_parse(&*{s})", args: [s] }
                ↓
walker:
  "almide_rt_int_parse(&*s)"
```

テンプレート文字列 → Rust source 文字列への substitution。

### WASM target — imperative

```
stdlib/int.almd:
  @inline_rust("almide_rt_int_parse(&*{s})")
  fn parse(s: String) -> Result[Int, String] = _
                ↓ (pass_stdlib_lowering skip on WASM)
emit_wasm/calls.rs:
  _ if module == "int" => emit_int_call(func, args)
                ↓
emit_wasm/calls_numeric.rs:
  "parse" => emit_int_parse(args)
                ↓
  self.emit_expr(&args[0]);
  wasm!(self.func, { call(self.emitter.rt.int.parse); });
```

19k 行の手書き `calls_<module>.rs` で、`@inline_rust` attribute は一切
参照されない。`@wasm_intrinsic` attribute は AST に存在するが **argument 無し
フラグ**として bundled body skip にしか使われていない。

## 2. WASM emit pattern 分類

実際に `calls_<m>.rs` を見ると 4 層に分かれる:

| Level | パターン | 代表例 | declarative 化可能性 |
|---|---|---|---|
| **L0** | literal const | `env.temp_dir` (`i32_const intern("/tmp")`), `env.os` (`"wasi"`) | 容易 (1 行 attribute) |
| **L1** | 単一 runtime fn call | `int.parse` (`call rt.int.parse`), `datetime.year` 等の clock-based | 容易 (call template) |
| **L2** | scratch alloc + runtime + finalize | `string.concat`, `list.push` の一部 | 表現力次第 |
| **L3** | inline 算術 / 制御フロー | `datetime.from_parts` (JDN 算術), `list.sort`, `fan.race` | **不可能** — Rust で書き続ける |

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
| calls_env.rs | 71 | **L0-L1** |
| calls_datetime.rs | ~450 | L1-L3 混在 |
| calls_http.rs | ~350 | L1 中心 |
| calls_process.rs | ~275 | L1 中心 |
| calls_io.rs | ~420 | L1-L2 |
| calls_random.rs | ~218 | L1 中心 |

## 3. 統合案 — `@wasm_intrinsic` attribute の拡張

現在 argument なしで fn マーキングだけに使われている `@wasm_intrinsic` に
WASM 側の dispatch 情報を embed する。

### DSL 候補

**形式 A: target-specific template 文字列 (既存 `@inline_rust` と対称)**

```almide
@inline_rust("almide_rt_int_parse(&*{s})")
@wasm_intrinsic("call:int.parse:{s}")         // L1: runtime fn call
fn parse(s: String) -> Result[Int, String] = _

@inline_rust("almide_rt_env_temp_dir()")
@wasm_intrinsic("const_str:/tmp")              // L0: literal
fn temp_dir() -> String = _

@inline_rust("almide_rt_env_os().to_string()")
@wasm_intrinsic("const_str:wasi")              // L0
fn os() -> String = _
```

prefix で category を切り分ける:

| Prefix | Shape | 用途 |
|---|---|---|
| `const_str:` | literal string intern | L0 |
| `const_i32:` / `const_i64:` | 即値 | L0 |
| `call:` | runtime fn 名 + arg 順 | L1 |
| `custom` | "WASM 側は従来の `emit_<m>_call` match arm に dispatch" | L2-L3 escape hatch |

**形式 B: 構造化 sexp (表現力高、実装重)**

```almide
@wasm_intrinsic([(const_str "/tmp")])
@wasm_intrinsic([(local.get s) (call rt.int.parse)])
```

### 推奨: **形式 A**

- parser 拡張が lexer レベルで完結 (文字列リテラル 1 個を受け取るだけ)
- `@inline_rust` とシンボル対称
- L2-L3 の複雑系は `custom` で escape して従来路線維持
- 将来 B に昇格するパスが残る (当初は `call:...` / `const_str:...` の prefix スキーム、後で sexp parser 導入)

## 4. 実装の段取り

### Sub-commit 1e-1: attribute 受け取り infrastructure

**scope**: `@wasm_intrinsic("...")` に引数を取れるようにする + パーサー拡張 +
AST 構造拡張。まだ使わない (downstream は今のまま)。

- `almide-syntax` parser — `@wasm_intrinsic(<string-literal>)` を受理
- `almide-syntax` AST — `Attr` が引数を保持できるか確認 (既に `@inline_rust`
  が文字列受けているので同じ shape で済む可能性大)
- stdlib `.almd` 検証テスト — 文法だけ通ればよし
- 既存 `@wasm_intrinsic` (無引数) は後方互換として受け続ける

**見積**: 1-2h、1 sub-commit。

### Sub-commit 1e-2: WASM lowering pass 新設 + L0 モジュール移行

**scope**: `pass_stdlib_lowering` の WASM 版を作る。env モジュール
(9 fns、うち L0 2 個 / L1 7 個) を完全移行、`calls_env.rs` 削除。

- 新 pass `pass_wasm_intrinsic_lowering.rs` — target = WASM のみ
- `@wasm_intrinsic("const_str:...")` / `("call:...")` を parse し
  `IrExprKind::WasmIntrinsic { form, args }` に rewrite
- `emit_wasm/expressions.rs` に `IrExprKind::WasmIntrinsic` emit ハンドラ追加
- `emit_wasm/calls.rs` の `_ if module == "env"` 分岐を削除
- `emit_wasm/calls_env.rs` 削除 (71 行)
- `stdlib/env.almd` に `@wasm_intrinsic("...")` を全 fn に追加

**見積**: 3-4h、1 sub-commit。env がクリーンに通れば方式確立。

### Sub-commit 1e-3: L1 中心モジュール移行 (int, float, random, process, http)

**scope**: sub-1e-2 の確立したパスに沿って複数モジュールを移行。各
`calls_<m>.rs` の L1 パターン (single runtime fn call) を `@wasm_intrinsic`
に寄せ、`emit_<m>_call` match arm を縮退。L2-L3 パターンは `custom` で残す。

**見積**: 2-3h / モジュール × 5 = 10-15h、複数 sub-commit に分割推奨。

### Sub-commit 1e-4+: L2 パターンの段階移行

string, io, bytes, fs などを順次。L3 は永続的に手書き維持。

## 5. 最終状態のイメージ

- `calls_<m>.rs` の **行数 50-70% 削減** (L0-L1 部分が declarative に移行)
- `stdlib/<m>.almd` が単一の dispatch 宣言場所 (Rust + WASM 両 target)
- 新 stdlib fn 追加 = `.almd` に 2 attribute 書くだけで Rust + WASM 両方対応
- 手書き emit は L3 だけ (本当に compiler 側で算術を組み立てる必要のあるもの)

## 6. 非 goal / 後送り

- **L3 の declarative 化** — `datetime.from_parts` の JDN 算術や
  `list.sort` の comparison-based sort を DSL で書くのは overkill。
  永続的に手書き維持。
- **Rust target のテンプレート表現の変更** — `@inline_rust` はそのまま
  維持、WASM 側を追いかけるだけ。
- **TOML backed stdlib の復活** — stdlib は既に bundled `.almd` に unify 済。
- **runtime fn の共通化** — `runtime/rs/src/<m>.rs` と `emit_wasm/rt_*.rs`
  の統合は別 arc。
- **単一 `CallTarget` variant への集約** — 現状 `Module` / `Named` /
  `Method` / `Computed` の 4 variant。統合は IR 全層に波及、scope 外。

## 7. Why (この arc を通す根拠)

- S4 (Sized Numeric Types)、Matrix[T] dtype、MLIR arc は全て **stdlib 拡張**
  が前提。現状の imperative 手書き dispatch では、新 fn 1 個追加のコストが
  「Rust template 書く + `.almd` signature 書く + `calls_<m>.rs` に match
  arm 書く」の 3 層作業。Phase 1e 後は 2 層で済む。
- dispatch 統合 = **emit コード 10k 行削減** が視界に入る規模 (19k のうち
  L0-L1 割合がざっくり半分)。
- `@inline_rust` / `@wasm_intrinsic` が対称になれば、LLM からも「stdlib fn の
  追加方法」が一意に記述できる (MSR への寄与)。

## 8. How to apply (次セッション向け)

- Sub-commit 1e-1 は syntax + AST 変更のみ。テストは parser レベル、behavior
  regression は起きない設計。まずここから
- 1e-2 で env 完了 → 「この方式で全 module 行ける」の確証を得る
- 各 sub-commit で `spec/` + `nn` 両 target で flip 後の hard contract (debug
  panic) を通ること = 移行完了のシグナル
- L0-L1 粒度を誤って L2 パターンを DSL に押し込まない — L2 以降は `custom`
  で escape

## 参考

- `docs/roadmap/active/stdlib-declarative-unification.md` — 上位 arc
  (stdlib 宣言化)。このドキュメントはその下位の compiler infrastructure 設計
- `stdlib/int.almd` / `stdlib/env.almd` — `@inline_rust` の実例
- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — Rust 側 lowering の模範
- `crates/almide-codegen/src/emit_wasm/calls_env.rs` — L0-L1 の最小移行対象
- `crates/almide-codegen/src/emit_wasm/calls_datetime.rs:12-72` — L3 の例
  (移行しない)
