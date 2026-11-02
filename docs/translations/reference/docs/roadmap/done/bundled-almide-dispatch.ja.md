原文の description メタデータ：stdlib/&lt;module&gt;.almd による TOML module の拡張を可能にする（codegen dispatch 修正）
原文の done メタデータ：2026-04-16
<a id="bundled-almide-dispatch-for-stdlib-modules"></a>

# 標準ライブラリ module の同梱 Almide dispatch

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bundled-almide-dispatch.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](bundled-almide-dispatch.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bundled-almide-dispatch.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-dispatch.md)

> 原文注記：done 日付と末尾の解決記録は、先の選択肢 3 推薦・承認待ちに優先する。選択肢 3 は採用されず、option/result の先の説明も訂正されている。工数、効果、target に関する記述は歴史的な原文主張であり、新規測定や保証ではない。将来の AST API や書換え規則の実装済みを示すものではない。


動機：dojo run #10 は、`almide fix` の再試行ループ統合が効果を増幅することを示した。自動書換え規則、helper primitive、そのほかの stdlib 拡張を Almide で書ければ（Rust runtime + TOML template ではなく）、貢献者はより速く進められる。しかし当時は、同梱 `.almd` file による `list.*` / `string.*` などの拡張が機能しない。codegen が出自にかかわらずすべての stdlib 呼出しに `almide_rt_<module>_<fn>` を出力するため、同梱の `fn binary_search` が存在しない Rust runtime 関数を呼ぼうとする。

<a id="current-behavior-the-bug"></a>

## 現在の挙動（バグ）

```
stdlib/list.almd: fn bundled_probe() = 42     # Almide source
↓ parser loads file (BUNDLED_MODULES opt-in)
↓ canonicalize: bundled fn is NOT merged into the `list` namespace
↓ lowering: no IrFunction named `bundled_probe` exists in the program
↓ codegen: user code calls `list.bundled_probe()` → rt_list_bundled_probe
↓ rustc: error[E0425]: cannot find function `almide_rt_list_bundled_probe`
```

図の説明：Almide の probe を parser が読み込んでも list 名前空間へ統合されず、IR に関数がないまま runtime 関数呼出しとして出力され、Rust の未定義関数エラーになる。

**2026-04-16 の調査**（「理想形」検討中）：`list` を `BUNDLED_MODULES` + `AUTO_IMPORT_BUNDLED` に追加し、probe 関数を含む `stdlib/list.almd` を同梱すると、問題は codegen *だけ*ではないと分かった。module 名が TOML ベースの module と重複すると、同梱関数は IR の `functions` 一覧にまったく入らない。当時の説明では、`option` / `result` の同梱 source が動くのは、Tier-1 module の frontend 解決が Tier-2 / TOML stdlib と異なる経路を通り、`list` は TOML 側にあるためとされた。

したがって修正は二層になる。

1. **Frontend 側**：同梱 `.almd` file が TOML 登録済み stdlib module と同じ名前を持つ場合、その関数を module の公開 surface（env.fn 登録）へ統合し、呼出し箇所が実際の `IrFunction` に解決されるようにする。
2. **Codegen 側**：`list.<func>` 呼出しが user IrFunction に解決される場合（つまり IR に関数定義がある場合）、`almide_rt_<module>_<func>` の出力を迂回し、通常の user 関数呼出し経路を使う。

片方だけでは不十分である。当初 roadmap は codegen だけを対象にしていたが、今は両層が対象になる。

<a id="proposed-fix"></a>

## 提案する修正

pass が「この関数には TOML template がある」と「この関数は同梱 Almide である」を区別できるようにする。同梱のみの関数なら `rt_` 出力を飛ばし、非 stdlib の `Module` 呼出し経路へ進む。walker は通常の関数参照として出力し、同梱 IR 関数に解決される。

実装の概略：

```rust
// pass_stdlib_lowering.rs
let is_stdlib = is_stdlib_module(&module);
let is_toml_fn = is_stdlib && toml_module_has(&module, &func);
if !is_toml_fn {
    // Either a non-stdlib module, or a stdlib module whose `func`
    // comes from bundled Almide source. Leave as Module call.
    return leave_as_module_call(...);
}
// rt_ emission (unchanged)
```

コードのコメントの説明：stdlib 以外の module、または関数が同梱 Almide source 由来の stdlib module なら Module 呼出しのままにする。rt_ 出力は変更しない。

`toml_module_has(module, func)` の検索は codegen crate から利用できなければならない。現状の `module_functions()` は `almide-frontend::stdlib` にあり、frontend の build script 中に TOML から構築されるため、codegen にはアクセスできない。

<a id="plumbing-options-pick-one"></a>

### 接続方法の選択肢（一つ選ぶ）

1. **TOML 関数一覧を `almide-types`（または `almide-lang`）へ移す**。frontend と codegen の双方が読める場所に置く。build-script 生成工程の移動が必要。長期的には最もすっきりする。
2. **一覧を codegen 入力として渡す**。`CodegenCtx`（または `Target` レベルの設定）に field を追加し、frontend の `stdlib::module_functions` から初期化する。差分は小さいが、codegen を開始するすべての entry point で引き回す必要がある。
3. **IR を使って判定する**：同梱 Almide file を IR へ lowering した後、各同梱 `IrFunction` に `source: FunctionSource::{Bundled, User, Extern}` tag を持たせられる。pass は「`<module>.<func>` という名前で `Bundled` tag の `IrFunction` があるか」を調べ、rt_ 出力を飛ばす。意味的には最もすっきりする。正本は IR 自身である。IR struct の追加が必要。

推奨：**選択肢 3**。IR が自己記述的になり（codegen は外部データを必要としない）、lowering 時の判定は単純になる（`program.functions` の HashMap 検索）。

<a id="what-this-unlocks"></a>

## これによって可能になること

同梱 Almide が stdlib module を拡張できるようになれば：

- **Almide で自動書換え規則を記述**：`almide fix` の修正関数は現状 `src/cli/fix.rs` の Rust code にある。同梱 dispatch が動けば、AST を読み取り返す Almide 関数で規則を表せる（AST API 公開後の話であり、別途仕様が必要）。短期的にも、HELPER primitive（`list.binary_search`、`string.run_length_encode` など）を TOML + Rust + WASM ではなく Almide で同梱できれば、Rust 実装の書換えにも利点がある。
- **自分たちで使う範囲が広がる**：examples/ の `almd-outline` demo は、`ide outline` の formatter を Almide にできると既に示している。次の段階では formatter の stdlib-outline subroutine が、compiler を経由せず同梱 Almide list primitive を使える。自己適用である。
- **Stdlib の WASM 対応が追加作業なしになる**：現状の stdlib は TOML + Rust + WASM の三か所を編集する形で、一か所を忘れやすい。同梱 Almide は一つの file から同じ source を Rust と WASM の両 target へコンパイルする。

<a id="non-goals"></a>

## 対象外

- 既存 TOML template の置換。fast-path intrinsic（`try_inline_intrinsic`、`try_lower_to_iter_chain`）は Rust 固有の最適化なので残す。
- `almide_rt_*` runtime の除去。既存関数はすべて使い続ける。この変更は同梱 Almide によって stdlib module に追加する新規関数だけに影響する。

<a id="testing"></a>

## テスト

修正後：

1. `fn binary_search_v2(xs: List[Int], target: Int) -> Option[Int] = ...` を持つ `stdlib/list.almd` を追加する（単純な dispatch test。既存 TOML 版と同じでなくてよい）。
2. `list` を `BUNDLED_MODULES` と `AUTO_IMPORT_BUNDLED` に追加する（以前の失敗した試行で既にあり、戻すか再利用できる）。
3. 利用者の code `list.binary_search_v2([1, 3, 5], 3)` は：
   - 型チェックに通ること。
   - `almide run` で正しくコンパイル・実行できること。
   - `almide build --target wasm` で有効な WASM を生成すること。
4. TOML ベースの既存 `list.binary_search` が変更なしで動き続けること（回帰防止）。

<a id="estimated-scope-revised-post-investigation"></a>

## 見積り範囲（調査後に改訂）

- **Frontend 統合**：1-2 時間。同梱 .almd 関数が TOML module 名前空間への統合に失敗する箇所を特定する。おそらく `canonicalize::registration` または `import_table`。既存の `option` / `result` 同梱方式を参照し、なぜ動くかを追うことで統合箇所を見つける。
- **Codegen 迂回**：1 時間。IR に同梱関数が入れば、`pass_stdlib_lowering` の入口で `program.functions` に `<module>.<func>` 形式の名前があるかを調べ、あれば rt_ 出力を飛ばす。
- **テスト**：1 時間。回帰防止として `stdlib/list.almd` に新しい同梱関数を追加し、型チェック、実行（Rust target）、コンパイル（WASM target）ができ、既存 TOML ベースの `list.*` 呼出しを壊さないことを確認する。

合計：専用の作業時間として 3-4 時間。これは `diagnostic-snippet-externalization` と、一般的な「stdlib を Almide で書いて自ら使う」構想のアーキテクチャ上の阻害要因の一つである。

導入時に dojo 測定値の変化は期待しない（診断ではなく基盤である）。依存する後続作業が数値を動かす。

<a id="when-to-implement"></a>

## 実装する時期

今（dojo run #10 の推奨「`almide fix` が劇的に効いたので、auto-rewrite rules を Almide で書けるようにする価値が確定」を受け優先度を引き上げ）。阻害要因：選択肢 3 と代案のどちらを使うか、利用者の承認待ち。

<a id="resolution-2026-04-16"></a>

## 解決（2026-04-16）

`llm-first-phase2` に実装した。実現した設計は仕様から二点大きく異なる。どちらも実装調査から判明した。

1. **選択肢 3（IR の `FunctionSource` tag）は採用しなかった。** `pass_stdlib_lowering` の入口で `program.modules` を走査し、`is_bundled_module(m.name)` を調べることで、より小さい差分で同じ自己記述性を実現する（IR struct 変更も関数ごとの lowering tag 引回しも不要）。pass 実行を scope とする `thread_local!` cell に保存する。同じ検索を必要とする二つ目の consumer（別 pass や target）が現れたら選択肢 3 を再検討する。
2. **仕様は option/result の基準状態を誤って説明していた。** 調査では「Tier-1 module の frontend 解決が Tier-2 / TOML stdlib と異なる経路を通るため、option/result 同梱関数は現状動く」と述べた。修正後に再確認すると、option/result の `.almd` source は **codegen 出力**を一切生成しない（全関数が TOML entry と衝突して除去される）が、型チェッカーには使われ、**TOML の signature を上書きする**。具体的には TOML は `Fn[Unit] -> X`、同梱 `.almd` は `fn() -> X` を使い、実際の呼出し元はすべて後者を書く。同梱 `.almd` を削除すると `coverage_misc_test` は直ちに失敗する。二つの側面は偶然相互依存している。同梱 = signature、TOML = runtime dispatch。この関係をほどく道筋は `roadmap/active/option-result-bundled-cleanup.md` を参照。

<a id="surfaced-gaps-not-in-the-original-spec"></a>

### 当初仕様になかった顕在化した不足

- **Verifier**（`almide-ir::verify`）は `known_module_functions` を正本とする。同梱 list を IR に lowering すると、`result.collect` から `list.is_empty` への呼出しが verify に失敗した（TOML 関数が verifier から見えない）。修正：registry で同梱 stdlib module を丸ごと飛ばす。
- **TOML 重複除去が必要。** 同梱 `.almd` と TOML 定義は両方とも、たとえば `option.map` を宣言する。両方を lowering すると `almide_rt_option_map` の定義が二つできる。`src/main.rs` の lowering loop は、生成された stdlib_sigs の `module_functions(name)` と名前が重なる IR 関数を除去する。

<a id="files-touched"></a>

### 変更した file

- `crates/almide-types/src/stdlib_info.rs` — `list` ∈ BUNDLED + AUTO_IMPORT
- `crates/almide-frontend/src/stdlib.rs` — `get_bundled_source("list")`
- `stdlib/list.almd` — smoke 関数
- `src/main.rs` — 同梱 lowering の通過 + TOML 除去
- `crates/almide-ir/src/verify.rs` — 同梱 module の除外
- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — `BUNDLED_FNS`
- `spec/stdlib/list_bundled_test.almd` — 回帰防止

<a id="estimated-3-4h-actual-3h"></a>

### 見積り 3-4h、実績 約3h。
