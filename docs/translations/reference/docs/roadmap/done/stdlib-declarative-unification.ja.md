原文の description メタデータ：stdlib を単一の正本へ近づける：`.almd` + 複数 target ABI attribute
原文の done メタデータ：2026-04-19
<a id="stdlib-declarative-unification--toward-a-single-source-of-truth"></a>

# Stdlib の宣言的統一 — 単一の正本へ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](stdlib-declarative-unification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](stdlib-declarative-unification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](stdlib-declarative-unification.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md)

> 原文注記：完了報告は、未チェック一覧や以前の inline template 案を含む歴史的計画に優先する。Stage 2 は五 module と記し六つ列挙する。サイズ付き型の件数も推測で修正せず保持する。旧来の全除去目標と、残る runtime/intrinsic wrapper・脱出口構文を共に記録する。古い MLIR/egg 計画、工期、test、性能上の含意は原文主張であり、独立に検証した現行実装や測定ではない。


<a id="completion-status-2026-04-19"></a>

## 完了状況（2026-04-19）

Stage 2 → 4 にまたがる取り組みを届け、最後の二つの commit で完了した：

- **PR 3b00b1ba**（2026-04-19）：最後の L3 dispatch pattern 13 件（bytes の `Endian` dispatch wrapper 12 件 + `http.serve`）を `@inline_rust` から同梱 Almide body + `@intrinsic` runtime wrapper へ移行した。支援する compiler 作業も出荷：`pass_borrow_inference` の `check_needs_refmut`（同梱 body が `&mut` を正しく転送）、`RefMut` parameter の walker 自動再借用、`ResolveCallsPass` が書き換えた Named call も検索できる mangled-key signature mirror。
- **PR 49d341f6**（2026-04-19）：残りの `@inline_rust` 宣言をすべて廃止。`error`/`string` を `@intrinsic` に移行。サイズ付き型 8 module（`int8…uint64`、`float32`）を、標準の `int`/`float` を介する pure Almide body に書き換えた。compiler 側：`@intrinsic` 関数が `pass_intrinsic_lowering` による AST-default 対応を獲得。UFCS Method→Named 書換えは小文字始まりの同梱 module に `almide_rt_` を前置し、WASM の `dispatch_runtime_fallback` は `to_*` に加え `from_*` を受け入れる。

取り組み完了後の grep（2026-04-19）：

```bash
$ grep -c '^@inline_rust' stdlib/*.almd
0
```

219/219 spec test + cargo test 全件が green。残したもの：`@inline_rust` 構文そのものと `pass_stdlib_lowering` template dispatcher。将来の edge case 用の脱出口として残す（現在の利用者なし）が、stdlib には不要になった。

<a id="original-plan-retained-for-history"></a>

## 当初計画（歴史記録として保持）

<a id="current-state-after-v0147"></a>

## v0.14.7 後の現状

各 stdlib 関数は**三か所**で定義される：

| 層 | file | 役割 |
|---|---|---|
| Signature + Rust template | `stdlib/defs/<m>.toml` | 型チェッカーが参照し、pass_stdlib_lowering が Rust template に `{xs}` / `{f.body}` を代入 |
| Rust runtime body | `runtime/rs/src/<m>.rs` | `almide_rt_<m>_<f>(…)` の手書き Rust 実装 |
| WASM inline emit | `crates/almide-codegen/src/emit_wasm/calls_<m>.rs` | 手書きの `wasm!(self.func, { … })` sequence |

25 module × 約20 関数 = 約500 関数、それぞれを 3 層で触る。v0.14.7 Phase 3 は **dispatch 層**を単一 source にした（`CallTarget` はどの backend でも必ず一か所へ行く）が、**定義**層は三重のまま。

三重定義モデルの症状：

- **ずれ**：Rust runtime の調整（例：`list.binary_search` の境界検査）が、人間の気づくまで WASM に伝わらない。
- **参加時の負担**：`list.intercalate` の追加には TOML signature + Rust template + Rust 実装 + WASM emit の記述が必要。一つの概念変更に 4 編集。
- **同梱 Almide が競争できない**：`stdlib/list.almd` は pure Almide 関数で stdlib を拡張できる（v0.14.6 以降）が、TOML signature + dispatch path が不可欠なので TOML ベース関数を*置換*できない。

<a id="goal"></a>

## 目標

stdlib 関数ごとに一つの正本：`stdlib/<m>.almd`。複数 target の ABI は codegen が読む Almide attribute で符号化する。

```almide
// stdlib/list.almd
@pure
@inline_rust("{xs}.iter().cloned().filter(|x| {f.body}).collect()")
@wasm_intrinsic(
  // declarative WASM recipe; emitter compiles directly
  iter_chain(elem=Auto, filter=f)
)
fn filter[T](xs: List[T], f: fn(T) -> Bool) -> List[T] =
  // pure-Almide fallback body (used when no intrinsic for the target)
  list.fold(xs, [], (acc, x) => if f(x) then acc + [x] else acc)
```

コードのコメントの訳：WASM の宣言的 recipe を emitter が直接コンパイルする。pure Almide fallback body は、その target に intrinsic がないときに使う。

すると：

- codegen は `stdlib/list.almd` の `@inline_rust` を読み、template を inline 出力する（TOML template を置換）。
- codegen は `@wasm_intrinsic` を読み、recipe から WASM bytecode を出力する（`emit_wasm/calls_list.rs` を置換）。
- 当該 target の attribute がない → Almide body に fallback（v0.14.6 の同梱 dispatch 経路）。
- `stdlib/defs/*.toml`、`runtime/rs/*.rs`、`emit_wasm/calls_*.rs` がすべてなくなる。

<a id="scope-per-module"></a>

## module ごとの範囲

現状三重定義の各 module を一段階とする：

- [ ] math（done：v0.14.5、`StdlibOp::FloatUnaryCall` dispatch）
- [ ] string の一部（done：v0.14.5、`StdlibOp` による 10 関数）
- [ ] float の一部（done：v0.14.5、`to_string`）
- [ ] list — **価値最大、複雑度高**（closure + iter chain）
- [ ] option / result — generic、浅い
- [ ] bytes — 低水準、memory 固有
- [ ] value — 動的多相、tag dispatch が必要
- [ ] map / set — hash ベース、closure が多い
- [ ] int — 単純な scalar operation
- [ ] datetime / regex / random / io / process / fs / env / http / json / testing / base64 / hex / matrix — その他

各 module に必要なもの：

- pure Almide body のある `stdlib/<m>.almd`（fallback / WASM default）
- Rust fast-path 用 `@inline_rust(...)` attribute（手書き template に価値がある場所）
- `@wasm_intrinsic(...)` attribute、または pure Almide body + 同梱 dispatch（v0.14.6 の経路）
- 新しい attribute recipe ごとの codegen 対応
- 回帰確認：移行後 Rust + WASM が spec/ に合格、dojo MSR が不変

<a id="移行構造-stage-1-4"></a>

## 移行構造（Stage 1-4）

- **Stage 1**（完了）：attribute 構文と codegen reader を定義。`int.to_string` を `stdlib/int.almd` に `@inline_rust` 経由で移行。TOML / runtime / emit は並列維持。
- **Stage 2**（完了 2026-04-19）：closure を持たない 5 module を `@intrinsic` に移行：
  - `int` — 22 関数（+ `stdlib/defs/int.toml` 削除）
  - `float` — runtime-call 関数 17 件（残り 10 件は `as T` 型変換で runtime 関数なし、`@inline_rust` 維持が正解）
  - `base64` — 4 関数全移行
  - `hex` — 3 関数全移行
  - `bytes` — pass-through 関数 84 件移行、残り 12 件は Endian runtime dispatch + sized-numeric cast の混在（L3）で維持
  - `env` — 9 関数全移行（`pass_result_propagation::is_template_dispatch` の `intrinsic` 漏れを同時修正。effect fn + `@intrinsic` の組合せが bare fn-name 衝突で cross-module retyping を起こしていた）

  Runtime Rust 関数（`runtime/rs/src/<m>.rs`）+ WASM `calls_<m>.rs` は引き続き維持（`dispatch_runtime_fallback` が symbol から逆引き）。
- **Stage 3**（大部分完了 2026-04-19）：closure を含む module。
  - `list` — closure / container 関数 30 件を `@intrinsic` + 新しい `@consume(xs[, ys])` attribute に移行。`Vec<T>` を取る List runtime 関数は、既定の `RefSlice` でなく `Own` borrow に固定。pipeline の順序を変更し、`LambdaTypeResolve` + 二回目の `ConcretizeTypes` を Rust と WASM 両 target で早期に実行（以前は WASM のみ）。これにより `@intrinsic` 後の `RuntimeCall` node でも lambda parameter 型を解決し、fold callback で `MatchSubject` が `c: String` を認識する。また `MatchSubjectPass` が `RuntimeCall` / `TailCall` / `InlineRust` 引数へ再帰するよう修正。`LambdaTypeResolve::resolve_call_lambdas` + `resolve_list_elem_ty` が `Named { almide_rt_list_* }` と `RuntimeCall` を解読し、`pass_concretize_types::resolve_call_ret_ty` が戻り値型置換のため同じ形を解読するよう修正した。副次効果：`fs.read_bytes_raw` の WASM emit stub を追加。nn WASM で skip していた `ggml_whisper.almd` の既存 ICE が今は green 12/12。
  - `map` — `map_values` 移行済み。残る 2 entry（`List[Tuple]` 用 `from_list`、その他）は一括作業で対応。
  - `option` / `set` — 既にすべて `@intrinsic`。
  - `result` — **残**：`.map` / `.ok()` / `.err()` / `.and_then` は Rust method chain（`almide_rt_result_*` runtime 関数なし）。`@intrinsic` 移行前に `runtime/rs/src/result.rs` の wrapper 層が必要。別の取り組み。
- **Stage 4**：effect module（fs、http、process、io、env〔done〕、datetime、random、regex、json、testing、matrix）。WASM runtime との相互作用に異なる attribute 形が必要かもしれない。

目標 release 周期：`0.14.N` / 初期 `0.15.x` ごとに一段階。完全統一は `0.15.0` に入る。

<a id="technical-debt-during-migration"></a>

## 移行中の技術的負債

移行では、Stage 3 前に統合すべき patch 的な継ぎ目を意図的に残す：

1. **同梱 source parse 経路の重複。** `almide-frontend::bundled_sigs`（`FnSig` 検索用）と `almide-codegen::pass_stdlib_lowering::parse_bundled_inline_rust`（`@inline_rust` template 検索用）が、各同梱 `stdlib/<m>.almd` source を独立に parse する。source 文字列自体は共有（`almide_lang::stdlib_info::bundled_source`）だが、双方が独自 cache と抽出 pass を持つ。理想の最終形：同梱 module は codegen 前処理で常に `IrModule` へ lowering される（`resolve.rs` を経由しない unit test も含む）。これにより両 consumer が同じ型付き IR を見る。
2. **二つの `module_functions` API。** `almide-frontend::stdlib::module_functions` は TOML のみの一覧を返し（main crate の除去処理で使用）、`module_functions_all` は同梱分を統合する。この分離は（1）と対応する。全関数が IR を通れば、区別は一つの「登録済み全関数」query に収束する。
3. **`@inline_rust` parameter の borrow inference 短絡。** `pass_borrow_inference::infer_function_borrows` は `@inline_rust` / `@wasm_intrinsic` 関数をすべて Own にする。body が穴であり、穴から推論すると誤った `RefStr` / `RefSlice` borrow が生じるため。borrow 意味論の唯一の正本は template。後で template と並べて `@pure` な pure Almide fallback body を育てるなら、「template 選択時は template、その他は body から推論」に変える必要がある。

<a id="mlir-backend--egg-arc-との関係"></a>

## MLIR Backend + Egg の取り組みとの関係

この取り組みは [mlir-backend-adoption.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/mlir-backend-adoption.md) の**準備運動**でもある。各出力は次の取り組み（egg + MLIR）でそのまま活用される：

- `stdlib/<m>.almd` の pure Almide body → Almide dialect（MLIR）への入力
- 型付き intrinsic（`@intrinsic(rust=..., wasm=...)`）→ MLIR FunctionImport
- `@rewrite` 宣言的規則 → egg e-graph rewrite rule に自動コンパイル
- `@schedule` block → affine dialect schedule attribute

これを飛ばし直接 MLIR に進むことは技術的に可能だが、stdlib が 3 層定義のままでは MLIR 移植工数が 2 倍になる。順序は動かさない。

<a id="non-goals"></a>

## 対象外

- 移行中に手書き module の TOML *構文*を変えない（一括除去では停止してしまう）。
- 同梱 dispatch 経路が宣言で表せない primitive（`alloc`、`panic_hook` など）用の Rust runtime 関数をまだ必要とする間は `runtime/rs` を除去しない。対象は stdlib 関数ごとの実装 file だけ。

<a id="success-signal"></a>

## 成功の兆候

- 新 stdlib 関数（例：`list.intercalate`）の追加が一 file 変更になる。`stdlib/list.almd` を編集し、関数 + 任意 attribute を加える。
- `stdlib/defs/` を grep → 空（または残る基盤定義のみ）。
- `grep -r "emit_list_call" crates/almide-codegen/src/emit_wasm/` → attribute recipe を読む単一 dispatch 箇所。関数ごとの match arm なし。

<a id="scope-estimate"></a>

## 範囲見積り

全体：**集中作業で 2-4 週間**、四つの release に分割。一回の作業時間で終える task ではない。

<a id="decision-points-before-starting"></a>

## 開始前の決定事項

- attribute 構文（Almide らしい `@...` か `#[...]` か）。小さいが ecosystem から見える議論。提案：Python / Swift decorator 風の `@name(args)` で LLM の馴染みを保つ。
- WASM intrinsic recipe 形式：宣言 DSL か埋込み WASM か。宣言 DSL（例：`iter_chain(elem=T, filter=f)`）は保守しやすいが解釈層が必要。
- 移行順序：単純な scalar（`int`）が先か、最大価値（`list`）が先か。おそらく基盤の証明のため scalar が先、その後実益の list。

dojo run #11 が v0.14.7 の基準値を作った後に開始する。phase4 差分測定にはクリーンな baseline が必要。
