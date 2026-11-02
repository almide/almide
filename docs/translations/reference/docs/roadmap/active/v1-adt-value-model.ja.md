原文の説明メタデータ：v1 値モデルでカスタム ADT（variant）を第一級の値にする
<a id="v1-①--custom-adt-variant-as-a-first-class-value"></a>

# v1 ① — カスタム ADT（variant）を第一級の値にする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-adt-value-model.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-adt-value-model.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-adt-value-model.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-value-model.md)

> 原文注記：これは段階的実装の記録であり、現在の普遍的保証ではない。初期のコンストラクタ非対応の診断や単一 arm ヒープ結果の壁と、後の完了・全位置対応の記述は歴史的な層として保持し、黙って整合化しない。生成する再帰 drop ルーチンは信頼する部分で、ownership 証明書は空。再帰の正しさゲートは記録されたリークループに依存する。コーパス witness の受理や例のバイト一致は、全誤コンパイル・二重解放・リークの不在の証明ではない。件数、commit、組織リポジトリ観測は原文の主張であり独立に再実行していない。

<a id="original-diagnosis-before-the-completed-bricks-recorded-below"></a>

## 当初の診断（以下に記録した段階の完了前）

当初のリポジトリ横断の壁を解消する重点項目 #1（約41件の壁：`type Expr = Lit(Int) | Add(Expr,Expr) | …` に対する再帰的な `to_string` / pretty-print）は、カスタム variant のレンダリング欠如を露呈した。これは実装前の診断であり、現在 ADT をサポートしていないという意味ではない。後の STATUS 節は段階1–5の完了を記録する。

<a id="evidence-byte-match-first-tmpadtalmd-tmpadt3almd"></a>

## 証拠（バイト一致優先、/tmp/adt.almd、/tmp/adt3.almd）

- `classify(e) = match e { Lit(n) => n, Add(_,_) => 100, Neg(_) => 200 }`（非再帰、スカラー）は v0 = 7/200、v1 = **WALL** `unlinked stdlib/runtime call(s): Lit, Neg` — **コンストラクタに wasm 実装がない**。
- `eval`（スカラー match）は classify で `in-profile` と数えられるが、render は同じ未リンク ctor で WALL になる。つまり **classify は ADT 対応を過大に示す**。render（バイト一致）が真実である。（これは、ADT を使うどのリポジトリについても org-trust-status の `in-profile` が楽観的であることも意味する。）
- MIR 上の根本原因：`build_record_layouts`（lower/mod.rs:142）は **variant 宣言を飛ばす**（「variant / alias 宣言はフラットな record layout を持たない」）。MIR lowering に `IrPattern::Constructor` のタグ分岐がない。既存の `try_lower_variant_*` は Option/Result 専用。

<a id="target-repr--v1s-own-uniform-slot-block-only-the-output-byte-matches-v0"></a>

## 目標表現 — v1 独自の均一スロットブロック（v0 とバイト一致するのは出力だけ）

訂正（段階1の構築中に検証）：v1 は v0 の内部バイト列を再現**しない**。v0 とバイト一致しなければならないのは、観測可能な出力（stdout に出力する `classify` / `to_string` の結果）だけである。v0 wasm（emit_wasm：`variant_alloc_size`、`collections.rs::emit_record`、`i32_store(0)` による @0 のタグ、型サイズで詰めたスロットの @4 からのフィールド、`4 + max payload` までパディング）はバイト単位で詰める。しかし v1 の record は既に別の内部モデル — フィールドごとに均一な i64 スロット（`layout::slot_offset`、`Store{width:8}`、ブロック `[rc][len][cap]` + スロット）— を使い、書き込んだスロットから読み戻す。そのため v0 のバイト列と一致しなくても出力は一致する。variant はこのモデルに加わる。

- variant 値は record に似たブロックで、**スロット0 = タグ**、**スロット1.. = 有効な ctor のフィールド**（各々均一な i64、record と同じ `slot_offset` 機構）。
- `slot_count = 1 + max arity over all ctors`。従って型のどの ctor も同じブロックサイズ（均一な確保 + 健全なブロック全体の `==`）になる。これは v0 の最大ペイロードのパディングに相当する v1 の仕組みであり、バイトコピーではない。

タグ = 宣言インデックス、タプルフィールド名は `_0.._n`（v0 の登録と一致）。従ってバイト配置は異なっても、両バックエンドのタグとフィールドの識別は一致する。

<a id="brick-sequence-each-ends-in-a-v0v1-byte-match-protected-by-proofsdiff-fuzzsh"></a>

## 段階の順序（各段階を v0==v1 バイト一致で終え、`proofs/diff-fuzz.sh` で保護）

1. **Variant layout registry** — ✅ 完了（commit `c81b0878`）。`build_variant_layouts` は `VariantLayouts` レジストリ（型 → `VariantLayout { generics, tag-indexed cases, slot_count }` + ctor名 → 型の逆引きインデックス + `lookup_ctor`）を構築し、新しい `lower_function_all_with_layouts` 経由で lowering に渡す（record 専用の `_with_types` は空の variant レジストリを渡して委譲。render_program + classify_corpus が構築して渡す）。基盤だけで、まだレジストリを消費するものはないため、**出力変化ゼロ**（diff-fuzz mismatch=0、mir スイート全体に変化なし）。単体テストはタグ順序、`_N` タプル合成、名前付き record フィールド、`slot_count = 1 + widest arity`、`lookup_ctor` を固定する。（確認した欠落：ctor は `IrExprKind::Call { target: Named(ctor) }` → 現在「unlinked stdlib/runtime call」として WALL。oracle の `classify(Lit/Add/Neg)` は v0 = 7/100/200。）

   最初のバイト一致部分（段階2+3、スカラーのみ — #51/#54 がスカラーから始めたのと同様に、段階5の再帰 drop から分離）：
   ```
   type Tok = Num(Int) | Sym(Int) | Eof
   fn val(t: Tok) -> Int = match t { Num(n) => n, Sym(s) => s * 10, Eof => -1 }
   ```
   ヒープフィールドなし ⇒ 再帰 drop なし ⇒ 段階2+3だけで検証可能。再帰 / ヒープフィールドの `Expr`（#1 の to_string 重点項目）は段階5を必要とし、その後に続く。

2. **Ctor 構築（スカラーフィールドから）** — ✅ 完了（commit `62a4a862`）。`try_lower_variant_ctor`（binds.rs）は名前が `variant_layouts.ctor_to_type` にある `Call{Named(ctor)}` を捕捉する。`slot_count` 幅のブロックを `Alloc`、スロット0にタグを `Store`、各引数をスロット `1+i` に lower する（`try_lower_scalar_record_construct` のスロット機構でスカラーをコピー）。呼出しの引数経路（`val(Num(7))`、calls.rs）と LET 束縛経路（`let t = Num(9)`、binds.rs）に接続。ヒープ / 再帰 ctor フィールドは壁になる（ADT 段階5）。誤ったバイト列のブロックには決してしない。
3. **タグ分岐 match（スカラー結果）** — ✅ 完了（commit `62a4a862`）。`try_lower_custom_variant_match`（control.rs）：対象を実体化 / 借用 → `handle → tag@slot0`、右に入れ子の `if tag==t_i { bind scalar fields from slots 1+i; arm } else …` 連鎖を出力（最終 arm / wildcard は無条件 else — 網羅性がそれを保証）。末尾、let 束縛、スカラーオペランドの match 箇所に接続（Option/Result 専用の `is_variant_ty` で制限しない）。`try_lower_variant_value_match` の N コンストラクタへの一般化。順不同の arm、wildcard、複数フィールド ctor、bind/let 位置で v0 とバイト一致。corpus-wall の証明済み checker は全 witness を受理。新しい diff-fuzz 生成 variant テンプレート + 決定的な wasmtime cargo test が保護する。スカラー結果 + スカラー ctor フィールド束縛のみ（ヒープ結果 arm = 段階4、ヒープ / 入れ子 ctor フィールド = 段階5）。
4. **ヒープ結果 match（借用対象）** — ✅ 完了（commit `f37a7314`）。借用した引数 / 変数を対象とするヒープ結果の `match t { Ctor(..) =>
   <String> }` は tag@slot0 で分岐し、各 arm を `lower_heap_result_arm` で lower する（arm は新しいヒープ値を move-out。束縛したスカラーフィールドはまだ生存する借用対象のスロットを読む）。`emit_variant_arm_chain` は `result_ty` を受け取り、スカラーとヒープの arm lowering を選択。ヒープ結果の末尾に接続。ヒープ結果を持つ所有一時対象は、証明書が失敗する MIR を出力せず WALL になる（arm 前の対象 drop が必要、段階4b）。corpus-wall ownership 15904→16046（ヒープ結果 variant match を lower でき、証明済み checker が全件受理）。
5. **ヒープ / 再帰 ctor フィールド + 再帰 drop** — 残る前線（再帰 `Expr` の to_string、#1 重点項目を解放）。3つの下位段階に分け、各々 corpus-wall ACCEPT を条件とする。

   - **5a — 葉 String の構築** ✅ 完了（commit `f2c32c90`）。`try_lower_variant_ctor` は `String` ctor フィールドをスロットへ move（cert `m`）し、`record_masks` を追跡する。従ってブロックのスコープ終了 drop は、String フィールド record と同じマスク付き DropListStr で解放する。corpus-wall ACCEPT（ownership 16046→16085）。wasmtime 上の1000×構築+drop リークループ cargo test。（構築のみ：フィールドの match はまだ WILDCARD — ヒープフィールドの束縛は5c。）
   - **5c — ヒープフィールド match の束縛** ✅ 完了（commit `b50e95a3`）。葉 String ctor フィールドを束縛する複数 arm の match を lower：スロットハンドルを**借用**（`LoadHandle`+`param_values`）。move-out arm は `lower_heap_result_arm` で自動 `Dup`、消費する再利用は `lower_owned_heap_field` で `Dup` するので、スロットを所有する対象を rc 0 で解放しない。**単一 arm のヒープ結果 match**（1 ctor の newtype `unbox`、`B(x)=>x`）は WALL：IfThen の分岐合流 `dst` がないため arm 値が直接 return し、arm の `Consume` と二重 move になる（`amm`/`aamdm`、正味 −1 — checker は REJECT）。証明書駆動で診断：正確な再現例は `spec/wasm_cross/generic_fn_in_inferred_lambda.almd` の `unbox[String]`。`Op::Consume`→`m` とヒープ `func.ret`→`m` が二重計上された（certificate.rs）。corpus-wall ACCEPT（ownership 16085→16161）、1000×リークループ、wasmtime cargo test + diff-fuzz ヒープフィールドテンプレート。（`emit_cert_from_source
     <f> <fn> mir` は今や MIR ops + cert をダンプする — 問題を発見したデバッグ補助。）
   - **5b — 入れ子 variant ctor フィールド + 再帰 drop** ✅ 完了（commit `c98486be`）— #1 重点項目が開通。`try_lower_variant_ctor` は入れ子 variant ctor フィールドを再帰構築（`Add(Lit(1),
     Neg(Lit(2)))` — ctor 呼出し引数 → 再帰、変数 → `Dup`）して move-in し、`Op::DropVariant` 用にブロックを追跡する。match のヒープフィールド束縛は variant 型フィールドへ拡張（再帰 `tos(l)`/`tos(r)` の借用渡し）。再帰解放は型ごとに生成する Almide 関数 `$__drop_<T>`（`generate_variant_drop_sources` — `$__drop_value` の形：`prim.handle`/`load*`/`load_handle`/`rc_dec` + 自己再帰）であり、render_program + lower_source の2パス `source_to_ir` で自動リンクする。`prim` のみなので ownership cert は空（信頼するルーチン — `rc_dec` ホワイトリストは `__drop_*` を許可）。`Op::DropVariant` は信頼する1個の `d`（`DropValue` と同様）。検証：`tos(Add(Lit(1), Neg(Lit(2)))) == "(1 + -2)"` が v0 とバイト一致 + 深い木。2000× build+tos+drop リークループは問題なし（再帰の唯一の正しさゲート）。corpus-wall ACCEPT（ownership 16161→16300、証明済み checker はコーパスの再帰 variant を受理し、二重解放 / リークはゼロ）。決定的 wasmtime cargo test + diff-fuzz 再帰 variant テンプレート。

   恒常的な教訓（今回再確認）：ヒープフィールド variant lowering は wasmtime でバイト一致しても潜在的二重解放を含み得る。kernel で証明された `[ownership]` checker だけが検出する（5c）。信頼する prim ルーチンについてはリークループが検出する（5b）。借用束縛も Dup した所有束縛も、証明書駆動の診断が *単一 arm の直接 return* の二重 move を特定するまでは REJECT だった。修正はそこを制限することであり、束縛モデルではなかった。checker が拒否した witness は決して出荷しない。

<a id="status-①-adt-value-model-complete-bricks-15"></a>

## 状態：① ADT 値モデル完了（段階1–5）

カスタム ADT は v1 の信頼基盤で第一級になった：構築 + N arm のタグ分岐 match（スカラー / ヒープ結果、Unit 文を含む全位置）+ スカラー / String / 入れ子 variant ctor フィールド + 再帰 drop。記録した例は v0 とバイト一致し、コーパスの ownership witness は受理された。これらの限定的なテスト / 検査は、可能なサイレント誤コンパイルや実行時二重解放がゼロであることを立証しない。PR ごとの diff-fuzz + 決定的 cargo テストで保護する。#1 のリポジトリ横断の壁解消項目（`type Expr = Lit | Add | Neg` に対する再帰 `to_string` / pretty-print）が開通した。

<a id="org-repo-verification-the-authoritative-byte-match"></a>

### 組織リポジトリの検証（基準となるバイト一致）

実際の `github.com/almide` 組織の variant 型が、v1 の信頼基盤で v0==v1 バイト一致する（②ゲートであり、単なる壁の件数ではない）：`porta` の `InstanceState = Created | … | Exited(Int) | Failed(String)`（unit + スカラー + String ペイロード ctor）、`RestartPolicy`/`ValType`（enum）、`FdKind`（String ペイロード）。いずれも `almide run`（native）== `render_program | wasmtime`。String ペイロード `FdKind` に対する2000× ctor+match+drop リークループも問題なし。porta の variant ctor はもはや「unlinked call」の壁にならない。

注意（測定であり正しさではない）：`scripts/org-trust-status.sh` が走査するのは各 repo の入口モジュールだけで、サブモジュール内の ADT コードは見えない（porta の入口は barrel — subset 内関数0個）。従って ADT 作業でもダッシュボード合計は変わらない。上記のバイト一致が実際の証拠である。組織全体の次の重点項目（ダッシュボード最上位の理由、約40件の壁）は今や **ヒープ結果式の系列**（末尾または呼出し引数に返す `match`/`if`/`Range`/`??`/`Try`/`List`/`BinOp`）で、toml/svg/aes/base64/csv を阻む。ADT ではない。

<a id="why-this-is-①-value-model-unification-not-a-one-off"></a>

## なぜ単発対応でなく①（値モデル統一）なのか

record / tuple / enum / list はどれも「タグ付き？ / マスク付きヒープブロック」。段階1-5で variant が最後の構成員としてこのモデルに加わる。完了すると、表面形ごとの段階（List[(String,Value)] の手動 drop、Option-tuple-match、ヒープ accumulator の特殊処理）は、単一のマスク付き / タグ付きブロックの構築+match+drop の実例になる。偶発的な複雑さを廃止する道筋である。全過程でバイト一致を優先し、PR ごとの diff-fuzz ゲート（commit 33e4237f）が、生成プログラムが差異に当たった瞬間に検出する。
