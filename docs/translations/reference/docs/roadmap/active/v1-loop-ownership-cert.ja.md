説明：ループ持ち越しヒープ所有権を証明書基盤の拡張として扱う — 特例ではなく完全性の修正
<a id="v1-heap-loop-carried-ownership--option-c-cert-spine-extension-the-completeness-fix"></a>

# v1 ヒープのループ持ち越し所有権 — 選択肢 C（証明書基盤の拡張）、完全性の修正

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-loop-ownership-cert.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-loop-ownership-cert.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-loop-ownership-cert.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-loop-ownership-cert.md)

> 原文の歴史的留保：この 2026 年 6 月の変化する調査記録は、初期計画、失敗した試行、診断と、それらの後続の撤回を併存させる。現在の実装、資格化、安全性の保証ではない。証明済み／絶対的な壁 3 個の下限、ABI／Coq 再設計の不可欠性、入れ子ループの阻害要因、持ち上げ方向の検証済みという反復された主張は、後で限定または覆される。後続記録は、証明書／Coq 変更なしで list_instances を解消（for 専用、while ではない）、load_porta_config を解消、read_message の低水準化は解決した一方 io.read_n_bytes の runtime 実装はまだない、と報告する。これらの完了境界は別である。歴史的修正、試験、証明、コマンドをここで独立に再実行していない。モデル化された所有権イベント列や参照カウント均衡の定理は、すべての renderer／runtime 演算の忠実性、アロケータの回収動作、任意プログラムのリーク不在を証明するものではない。原文自体が、証明書受理にもかかわらず起こる実行時誤コンパイル／リーク、信頼する再帰 drop、平坦ループ制約、検証器の制御フロー不足を報告する。ゲート件数、有限のバイト／リークプローブ、NIST ベクトル検査は原文が報告した限定的証拠であり、暗号の安全性や外部認証ではない。冒頭のヒープループ主張、後続の条件付きループ統合、古い未完了手順を黙って統一せず、暫定件数の矛盾や「6 workflows」の列挙も原文どおり残す。純粋な前置部分に限定する恒等書き換え、所有 String エラーコピーの制約も保持する。この閲覧版は歴史的な自律エージェント／リポジトリ編集指示を承認するものではない。

**CEO は C を選択した（「C一択」、2026-06-20）。迂回（A：インプレース push）や表現の小細工（B）ではなく、証明済み検査器にループ持ち越しヒープアキュムレータを推論させ、証明基盤の完全性の不足を根本で閉じる。C なら利用者は自然な再帰 `acc + [x]` を書き、それを証明できる。**

<a id="the-gap-why-the-11-remaining-yaml-walls-need-this"></a>

## 不足（残る yaml の 11 個の壁にこれが必要な理由）

基本の所有権証明書（OwnershipChecker.v）はオブジェクトごとの平坦なイベント列であり、ループの概念がない。ループ持ち越しヒープアキュムレータ（反復ごとに `acc = acc + [x]`：旧オブジェクトを drop、新規 alloc、スロットを再束縛）は表現できない。あるオブジェクトの `i` は反復 K、`d` は K+1 にあり、異なるオブジェクトが一つのスロットを共有する。`verify_ownership`（平坦な 1 パス）は `d`/`i` の不均衡を見て、安全なコードを誤って拒否する。これは**完全性**の穴であり、健全性は危険にさらされていなかった。11 個の壁（collect_*/parse_*/seq_item/map_entry、flow_rec↔flow_step、block_*）はすべてこれに依存する（追加型アキュムレータ + 相互再帰）。

<a id="-landed-2026-06-20-commit-7f673b4c-the-soundness-proof--the-②-critical-core"></a>

## ✅ 導入済み 2026-06-20（コミット 7f673b4c）：健全性証明 — ②上重要な核心

`proofs/OwnershipLoop.v`（証明ゲート内：`_CoqProject` + `check.sh` の coqc **および** coqchk + claim-drift。「PROOF SPINE OK」、追加公理なしの「Closed under the global context」）。`Loop : list FlatOp -> Op` 構成子を追加し、次を証明する：
- `exec_list`（検査器の fold）の Loop 規則：本体が開始時のカウントから rc を維持し、障害を起こさない場合に限りループを受理する。
- `Unrolls`：抽象証明書を具体的な平坦実行へ展開する（各 `Loop body` → 本体の n 個のコピー）。
- **`check_unroll_sound`**：`check ops = true → ∀ unrolling, no_double_free ∧ no_leak`。つまり rc を維持するループ本体は任意の反復回数でリークも二重解放も起こさない（`exec_flat_repeat_preserve` による帰納）。
アキュムレータスロットの証明書は `[Inc; Loop [FDec; FInc]; MoveOut]`（1 回取得、反復ごとに旧値解放 + 新値取得 = 正味 0、最後の値をムーブアウト）。ループ本体は平坦（入れ子ループなし）。v1 パーサーの壁には十分（反復ごとに drop+alloc を 1 回）であり、入れ子ループは将来の合成可能な拡張となる。C の難しく削減できない部分（Coq の再証明 —「C には Coq が必要で、corpus-wall では検証できない」）は完了し、カーネルで検証済み。残りはゲートで検証できる実装統合である。

<a id="remaining-c-integration-each-gate-verifiable--corpus-wall--byte-match--the-proof-gate"></a>

## 残る C の統合（それぞれ corpus-wall + バイト一致 + 証明ゲートで検証可能）

1. **✅ 完了 2026-06-20（コミット c05fc209）：本番検査器 + 証明書形式。** `OwnershipChecker.v` に `CertItem`（`COp`/`CLoop`）、`exec_line`、`check_line`、完全な Inc/Alias/Dec/MoveOut/Reuse アルファベット上の健全性の再導出（`exec_app`、`exec_repeat_preserve`、`UnrollsL`、`exec_line_unroll`、**`check_line_unroll_sound`**、**`check_cert_lc_sound`**）を追加。追加公理なし、証明ゲート内。証明書形式 v2：ループ区切り `(`…`)`（`parse_lc`）、後方互換（括弧のない証明書は平坦な `check` とまったく同じ fold）。`Extract.v` は `check_cert_lc` を抽出し、`driver.ml` は所有権をそこへ振り分ける。`build-checker.sh` で実バイトを往復：`I(DI)M` ACCEPT（アキュムレータスロット）、`I(I)M`/`I(D)M` REJECT（リーク／枯渇）。corpus-wall（ヒープオブジェクト 14564 個）は `check_cert_lc` 経由で引き続き ACCEPT、退行 0。
2. **✅ 完了 2026-06-20（コミット 291a1f35）：Rust のループ対応証明書出力 + 検証。** `lib.rs
   verify_ownership` — `Op::SetLocal { local, src }` がヒープスロットを再束縛するようになった（`object_of[local] ←
   object_of[src]`、スロットが再び live）。旧オブジェクトは本体の先行 `Drop` で解放済みなので反復ごとの不変条件が成り立つ（スカラー SetLocal は引き続き何もしない）。`certificate.rs ownership_certificate` — `loop_carried_slots()` が `LoopStart`…`LoopEnd` 内の `SetLocal` への供給を事前走査し、スロットを一つの列 `i(id)m` へ畳み込む（Alloc/Call の供給 `i` をスロットへ振り分け、ループ本体を `(`/`)` で囲む）。単体テスト：`loop_carried_accumulator_folds_to_one_slot_stream`（`i(id)m`、verify_ownership Ok）、リークする本体 `i(i)m` は拒否。証明済み抽出検査器は出力された `i(id)m` を ACCEPT（`./checker
   ownership` で検証）。corpus-wall（14564 オブジェクト）は成功、後方互換。（既存の render_wasm json の wasm 実行失敗 2 件は無関係。この 2 ファイルだけを stash して確認。他エージェントの list-cap 作業。）
3. **低水準化**（`lower/mod.rs`）：ヒープのループ持ち越しアキュムレータ MIR、すなわち追加型アキュムレータ TCO を出力する。変更点：(a) `try_tco_rewrite` 約 2184 行 — 持ち越すヒープ引数のすべての自己呼び出し値が `acc + [x]`（`left = Var(acc)` の `BinOp::ConcatList`）なら `carried[i] && is_heap_ty` による中止を外す。(b) `tco_rewrite` はその持ち越し引数をすでに `Assign { acc, acc + [x] }` として出力する（変更なし）。(c) ループ内 `Assign` の低水準化（mod.rs 約 690 行、現在ヒープ再代入は `Err`）で、`acc = acc + [x]` → `new = __list_concat(acc, [x]); Drop acc; SetLocal acc, new` を許可する（`try_lower_concat_list`。証明書に裏付けられたスロットは段階 2 で受理される）。加えて相互再帰のインライン化（flow_step→flow_rec）。

   **設計確定 — 方法 3（新規スロットへの事前コピー）が最も明快で、規約変更なし。** 核心：きれいな `i(id)m` には、所有されたスロットと `of[slot] = slot` が必要（証明書は ValueId をスロット列のキーにする）。借用 `acc` パラメータには `i` がない（証明書 `(id)m` → 正しく拒否。反復 1 で drop すると呼び出し元を二重解放する）。Dup／所有パラメータで再束縛すると `of[slot] ≠ slot`（パラメータのオブジェクトとスロットキーが異なる）になり、証明書が複雑になる。解決策：新しいスロット変数 `acc_slot` を導入し、`acc_slot = __list_concat(acc, [])`（所有コピー）で初期化する。Call のヒープ結果は自動的に `of[acc_slot] = acc_slot`（証明書 `i`）に設定されるので、スロットキー == オブジェクトとなり明快。その後ループ本体と基底全体で `acc → acc_slot` を置換する。ループは `acc_slot` を持ち越し（旧値 drop／新値 alloc）、基底はそれを返す（ムーブアウト）。証明書 = `i(id)m`、段階 2 が受理するものと完全一致。借用パラメータ `acc` は借用のまま（呼び出し元所有）で、事前コピー時だけ読む。バイト一致も維持する（コピーと反復ごとの追加は v0 再帰と同一の最終リストを作る）。実装部品：(i) ヒープ追加型アキュムレータの検出（carried[ai] がヒープ、すべての自己呼び出し値が `ConcatList{left:Var(acc)}`）、(ii) IR 変数置換ヘルパー（Var(acc)→Var(acc_slot)）、(iii) 事前 `let acc_slot = acc + []` 束縛の出力、(iv) ループ内 Assign の接続 (c)。`try_lower_concat_list` はスカラー要素専用なので、まず合成 `List[Int]` 追加で機構を検証する。yaml の `List[Value]`/`List[(k,v)]` にはその後ヒープ要素連結（+ value.object/stringify）が必要。
4. **✅ 完了 2026-06-20（コミット f3ce5401）：追加型アキュムレータ TCO の生成側。** `try_tco_rewrite` が、すべての自己呼び出し値が `acc + [x]` であるヒープ持ち越しパラメータを検出し、新しい所有スロット（`let slot = acc + []`、`acc → slot` を置換）を導入する。ループ内 `Assign` は `slot = slot + [x]` を `new = __list_concat(slot,[x]); Drop slot; SetLocal slot,new` へ低水準化する。`spec/wasm_cross/append_accumulator.almd`（List[Int]）で端から端まで検証済み：プロファイル内（以前は壁）、所有権証明書 `i(id)m` ×2、**v0 とバイト一致**（output-parity baseline、match 69→70）、corpus-wall 成功、cargo-test 問題なし（json wasm の失敗 2 件は他エージェントの既存問題）。描画されたループは反復ごとに `rc_dec(old)` を出力（wat で解放を確認）。メモリ注記：`__list_concat` はコピーする（v0 の深い再帰と同じ O(n²)）。大きい n は wasm の 64KB で n≈110 のとき OOB（sum(1..n)·8B）になるが、これはアロケータの再利用／効率の限界であり rc リークではない（証明書が rc の均衡を証明し、解放も出力される）。将来のインプレース push なら O(n) になる。fixture の n は小さく保った。

**🎯 選択肢 C の全連鎖が端から端まで動作する**（証明 → 抽出検査器 → 証明書シリアライザー → verify_ownership → 生成側低水準化）。すべて検証済み、コミット 7f673b4c、c05fc209、291a1f35、f3ce5401。ヒープのループ持ち越し追加型アキュムレータが `.almd` からコンパイルされ、v1 信頼基盤上で低水準化され、証明済み `i(id)m` 証明書を持ち、v0 とバイト一致する。スカラー要素の追加型アキュムレータについて完全性の穴を根本で閉じた。

<a id="remaining-toward-yaml0-the-producer-extensions--the-chain-is-proven-these-widen-its-element-domain"></a>

## yaml=0 に向けた残り（生成側の拡張 — 連鎖は証明済みであり、要素の対象領域を広げる）

- **✅ 完了 2026-06-20（コミット 7074579d）：ヒープ要素連結。** `__list_concat_rc`（セルフホスト、許可リスト内の `__lc_copy_rc` で要素ごとに rc_inc）+ `try_lower_concat_list` が String/Value 要素を許可 + `heap_elem_lists`/`value_elem_lists` の印を付ける（したがって `drop_op_for` = DropListStr/DropListValue）+ ゲートの `count_ir_calls` がヒープ要素 ConcatList を数える（mir≤ir が成り立つ）。`spec/wasm_cross/append_accumulator_heap.almd`（`List[String]` build_s + extend_s）で検証済み：v0 とバイト一致、corpus-wall 成功（spec の壁を 2 個解消、866→864）、output-parity 70→71、cargo-test 問題なし。これでスカラー（Int/…）とヒープ（String/Value）の両要素アキュムレータについて、証明済み `i(id)m` スロット上で `acc + [x]` を低水準化できる。
- **相互再帰インライン化 — 試作 + 重要な発見（2026-06-20、撤回済み、未コミット）。** yaml の壁 11 個はすべて「heap-result if/match」。各追加関数が相互再帰（`flow_rec↔flow_step`、`collect_seq↔seq_item`、`collect_map↔map_entry`、`collect_block↔block_line↔block_nonblank`）なので、`try_tco_rewrite`（自己呼び出し検出）が発動しないため。試作 `inline_mutual_tail_recursion`（1 回呼ばれる兄弟 G をパラメータごとの `substitute_var_in_expr` で呼び出し元 F へインライン化して G を削除、`IrMutVisitor` による再構築）+ 検出の緩和（自己呼び出しが `acc` または `acc+[x]` を渡す）+ `tco_rewrite` の恒等代入スキップを、合成 `frec⇄fstep`（List[String]、v0 とバイト一致）で検証。yaml は 11→9 になったが、**`esc_rec` + `collect_block` が退行（プロファイル内 → 壁）**。インライン化で F が自己再帰 → TCO 発動 → TCO なしなら正常に低水準化できた関数が TCO で壁に当たる。②はこの不完全性の退行を禁じるため撤回した。
  **✅ 完了 2026-06-20（コミット 8c9a5c07）：ガード付き相互再帰インライン化。** `inline_mutual_tail_recursion`（lower/mod.rs、`globals`+`record_layouts` を引き回す）：1 回呼ばれる相互再帰兄弟 G を呼び出し元 F へインライン化（`IrMutVisitor` + パラメータごとの `substitute_var_in_expr`）して G を削除するのは、**F が現在壁に当たり、インライン化した F が低水準化できる場合だけ**（両方を試行）なので構成上退行しない。検出緩和（自己呼び出しは `acc` または `acc+[x]` を渡す）+ `tco_rewrite` の恒等代入スキップも追加。render_program + classify_corpus に接続。検証済み：`spec/wasm_cross/mutual_append.almd`（`frec⇄fstep`、List[String]）が v0 とバイト一致、**spec corpus の壁を 6 個解消（プロファイル内 3712→3718）**、corpus-wall 成功、cargo-test 問題なし、yaml は 11 のまま（退行なし。esc_rec/collect_block はプロファイル内を維持し、ガードが変更を拒否）。

  **✅ 完了 2026-06-20（コミット f6199af9）：`[call_result]` 要素の実体化 + 1 つずれる問題のガード → yaml 11→9。** `try_lower_str_list_literal` が elem_value の Value 呼び出しだけでなく、`elem_str` に対する String を返す Module/Named 呼び出し要素（`[string.slice(s,0,1)]`）を許可する。呼び出しを新規所有 String（登録済み `string.slice` ランタイム。`lower_pure_module_value_call` は value 限定でなく一般のモジュール呼び出しに対応済み）へ低水準化し、スロットへムーブする。バイト検証：`spec/wasm_cross/list_call_element.almd`（`xs + [string.slice(s,0,1)]`）は v0 と一致。
  **🚨 さらに無言の誤コンパイルを発見・修正：** `[string.slice]` 要素から、TCO が持ち越しパラメータを逐次代入することが判明した。そのため `acc + [string.slice(s, i, …)]` がループ添字 `i`（`i=i+1` と再代入）を読むと新しい `i` を見てしまい、1 つずれた（`chars("abc")` → `a-b-c` でなく `b-c-`）。`try_tco_rewrite` で相互依存 TCO（別の持ち越しパラメータを読む自己呼び出し引数）を壁で止めて修正。②上安全（壁に当たり、誤コンパイルしない）で、各引数が自分のパラメータだけを読む通常ケースは影響なし。yaml 11→9：flow_step + もう 1 個が正しく低水準化され、相互依存関数（flow_rec、chars）は誤コンパイルではなく壁となる。

  **✅ 完了 2026-06-20（コミット 89664c68）：同時更新 TCO。** `tco_rewrite` が持ち越す各スカラーの新値を新しい一時変数に用意し（旧パラメータを読む）、ヒープ追加代入を実行（スカラーはまだ旧値）、その後一時値を反映する。そのため相互依存追加（ループ添字を読む `acc + [string.slice(s, i, …)]`、`flow_rec` の `acc + [slice(s, start, pos)]`）がずれなくなった。バイト検証：`spec/wasm_cross/cross_dep_accumulator.almd`（chars `a-b-c`、win `ab|bc|cd`）。相互依存の壁は (a) ヒープアキュムレータが別のヒープアキュムレータを読む、(b) 純粋な Var 別名引数（`start = pos`。コピー用一時変数では用意できない）に限定。両方とも稀で②上安全に壁で止める。corpus の壁 +3 個解消（プロファイル内 3719→3722）、output-parity OK。

  - **✅ flow_rec の基底完了（コミット af2a5695）：`lower_heap_result_arm` の ConcatList アーム。** heap-result-if のリターン `if string.is_empty(last) then acc else acc + [last]`（Var のムーブアウトアーム + ConcatList アーム）を低水準化できるようになった（アームごとの `"im"` 均衡）。flow_rec が端から端まで低水準化可能 → yaml 9→8。`spec/wasm_cross/heap_result_if_append.almd` でバイト検証、corpus の壁 +3 個解消（3722→3725）。

  **⚠ 残り（yaml 8 — Value パーサー核心。各関数に複数の不足が重なり、単一の突破口ではない）：** `collect_seq`/`seq_item`、`collect_map`/`map_entry`、`block_*`、`parse_lines`/`parse_nested`。代表の `collect_seq` の分析：`(Value, Int)` を返し、次の特徴がある。
  - **タプルを返す**：基底は `(value.array(items), pos)`、単なる List でなくヒープ結果タプル（Value + Int）。ヒープ結果のタプルリターンの低水準化が必要（基底でアキュムレータから Value を構築）。
  - **基底の value.array**（演算としては完了）をタプルへ組み込む。
  - **相互再帰 + 追加の呼び出し先**：`seq_item` は相互再帰の兄弟だが `dash_item` も呼ぶ。ガード付きインラインは依然適用可能（seq_item は collect_seq からのみ呼ばれる）だが、インライン後の本体には dash_item 呼び出しが残る。
  - **effect 呼び出しのタプル分解束縛**：`let (val, next) = dash_item(...)!` — effect 関数の Result から `(Value, Int)` を束縛し、その後 `items + [val]` で追加。
  - タプルを返す **effect fn**（`!` による Result 伝播）。
  よって `collect_seq` にはヒープ結果タプルリターン + effect タプル分解束縛（+ 完了済みの追加／インライン／TCO）が必要。`collect_map` にはさらに **value.object** + `List[(String,Value)]`（タプル要素）の追加、`block_*` には **tuple-heap drop**、`parse_*` は heap-result match の根となる。これは複数の本格的な機構（Value パーサー機構）であり、単一の突破口ではない。追加型アキュムレータの基礎は完成し、残りは Value 集約の構築とタプルの接続である。

  **🎯 根本の阻害要因を特定（2026-06-20、合成プローブ）：effect 関数の `!` の早期リターン伝播。** 残る 8 個は要となる機能で二群に分かれる。
  - **effect 関数 6 個**（collect_seq、seq_item、collect_map、map_entry、parse_lines、parse_nested）— すべて `effect fn` で `let (val,next) = dash_item(...)!` を束縛、または末尾 `parse_nested(...)!`。合成 `let (v,next) = make(n)!`（make は `effect fn -> (Value,Int)`）は **「呼び出し引数位置の unwrap `!` を忠実に計算できない（早期リターン伝播が必要）」**で壁となる。したがって全 6 個の要は `!` 伝播、すなわち v1 MIR の Result／エラー機構：`f()!` を `match f() { Ok(v) => <continue>, Err(e) => return Err(e) }`（Err で早期リターン）へ低水準化する。これは関数ごとの機構でなく基本機能（Result 表現 + 関数途中の早期リターン／その脱糖）。導入すれば、6 個の effect 関数のタプル分解束縛とタプルリターン（両方対応済み）が組み合わさる。
  - **block_scalar/block_line + collect_block**（非 effect の `local fn`、`!` なし）— 要は 3 周期の反復的インライン化（collect_block↔block_line↔block_nonblank、ペアではない）+ `(List[String],Int)` タプルリターン + tuple-heap drop。effect `!` 作業とは独立。
  次の突破口は effect 関数 `!` の早期リターン伝播（8 個中 6 個を解放）。健全性上重要（誤った Err 伝播 = 無言の誤コンパイル）なので、早期リターン脱糖 + バイト一致とともに導入し、Const/Opaque の近道では済ませない（現在の壁はそれを明示的に拒否する）。

  **訂正（2026-06-20、より深いプローブ）：この群の大半は低水準化できる。壁は約 74 個中 8 個だけで、原因は異種（単一の要はない）。** ディスパッチャー（dash_item、dash_after、nested_dispatch、…）はプロファイル内。effect 関数の末尾 `f()!` は素通し（Result をそのまま返す → 単に `f()`）で、対応済み。壁は次の 8 個のみで、各々異なる機能が原因。
  - **collect_seq、seq_item** — let 束縛の `!`：`let (val,next) = dash_item(...)!; <rest>` は早期リターン脱糖 `match dash_item(...) { ok((val,next)) => <rest>, err(e) => err(e) }`（Ok アームの継続 + タプルペイロード分解束縛 + Err 伝播）が必要。[最も近い 2 個。追加／インライン／TCO／タプルは完了、let 束縛の `!` だけが残る]
  - **collect_map、map_entry** — let 束縛の `!` + **value.object**（`List[(String,Value)]` から Value オブジェクトを構築）。
  - **parse_lines、parse_nested** — `lines |> … |> list.find((e) => not is_blank(e.1))`（list.find + ラムダ + パイプライン）+ `match next { some((offset,line)) => … }`（Option-of-Tuple の match ペイロード）。
  - **block_scalar、block_line**（+ collect_block）— `(List[String],Int)` の **tuple-heap drop** + 3 周期インライン化（collect_block↔block_line↔block_nonblank）。
  道筋は複数の別々の機構（let 束縛 `!` の早期リターン ⇒ まず collect_seq/seq_item、その後 value.object、list.find+lambda+Option-tuple-match、tuple-heap+3-cycle）で、それぞれ健全性に注意が必要で独自のバイト一致を行う。単一の突破口ではない。追加型アキュムレータ + 選択肢 C の基礎は完成している。

  **✅ block_scalar 完了 2026-06-20（コミット b31096e8）：yaml 8→7 — ただし阻害要因は上で見積もったものではなかった。** block_scalar の実際の壁はリターン `(value.str(if string.ends_with(ind,"-") then
  joined else joined+"\n"), end)`。タプル要素内の `value.str(..)` 内に heap-result `if` が入れ子になり、その前に `let joined = if…`（ヒープの let 束縛 if が 2 個）がある。tuple-heap-drop でも 3 周期でもない。組み合わせる修正は 3 個：(1) `extract_first_callarg_branch` がタプル要素へ再帰（`value.str(if…)` 引数を ANF へ持ち上げる）。(2) 有界複製ゲートが残る分岐束縛を ≤2 個許可（以前はすべて拒否）。(3) `desugar_heap_branches` が if/match アームとブロック末尾へ再帰（`desugar_nested_branch_arms`）し、複製アームの入れ子 let 束縛 if を解消。すべて共有脱糖内（lower == count、mir>ir 違反なし）。**これで露呈した control_flow（C-044）の既存の無言の誤コンパイル 2 件も修正：** (a) `x |> (n) => body` は Computed 呼び出し先の呼び出しへ脱糖され、v1 MIR が 0 へ誤低水準化していた。frontend は今 `{let n=x; body}` へインライン化する（`lower_pipe`）。(b) ブロック値のスカラー束縛 `let a = {…; tail}` も 0 へ誤低水準化していた。`lower_bind` がブロックの文を実行してから末尾を束縛するようにした。検証：corpus-wall のプロファイル内 3725→3733（+8）、ownership 14984 ACCEPT、cargo-test 466、output-parity の baseline 退行なし + control_flow が新たに wasm とバイト一致（fixtures heap_result_tuple_return、pipe_lambda_block_value）。（呼び出し引数位置の pipe-lambda は、今は誤低水準化でなく壁となるので安全。後で ANF へ持ち上げる。）**残る yaml 7：block_line（block_scalar の兄弟、おそらく最も近い）、collect_map、collect_seq、map_entry、parse_lines、parse_nested、seq_item**。effect 関数 6 個はまだ effect モナドの let 束縛 `!`（⛔ 上記注記）+ value.object が必要。

  **✅✅ block_line 完了（コミット 5518fff3）：yaml 7→6、v0 とバイト一致。** ②に従う二分調査を 6 ターン行った後（下記の長い記録。置換／借用パラメータ／fresh-let インライン／単純な ID 再利用をすべて試験して否定）、wat から else アームの `string.drop(line, 0)` が `line` でなく `$v19`（then アームの `__list_concat` 結果）を読んでいると分かった。根本原因：`desugar_callarg_heap_if` の裸の呼び出し引数の持ち上げが、新しい `tmp` を `max_var_id(THIS arm)` で決めていた。しかしそのアームには `line` がない（兄弟の else アームだけで使用）ため `tmp` が `line` と重なり、renderer のグローバル VarId→local マップで衝突した。修正：関数全体の `next_var` カウンターを `desugar_heap_branches`/`desugar_callarg_heap_if`/`desugar_nested_branch_arms` に引き回す（`desugar_heap_branches_inner(body, &mut u32)`、公開 wrapper は `max_var_id(whole_body)+1` で初期化）。検証：spec/wasm_cross/block_line_collect.almd がバイト一致（`a||bb|c`）、corpus-wall のプロファイル内 3733→3734 + ownership 14988 ACCEPT、cargo-test 466、output-parity 退行なし + 新規一致、全 worktree 走査 = 既存の不一致 3–4 個のみ（string_ops/fan_map/nested_named/list_string、すべて HEAD~1 で確認）。**診断の規律は重要だった。corpus-wall（所有権）と壁数は両方ともバグを含む低水準化を受け入れ、バイト一致だけが検出した。6 ターンを通して②を守った。**

  **（方法を残すための歴史的診断）**
  **⚠ block_line — 検出した②の落とし穴（2026-06-20）：指標上は低水準化可能だが実行時に誤コンパイルするため、壁のままにした。** block_line の本体は `if is_blank then collect_block(.., if list.is_empty(acc) then acc else acc+[""])
  else block_nonblank(..)`。then アームの呼び出しが引数にヒープ `if` を持つ heap-result `if` である。裸の呼び出し／タプル本体を持ち上げる `desugar_callarg_heap_if` 拡張（`desugar_nested_branch_arms` がアームごとの呼び出しに届く）は確かに block_line の壁を消した（yaml 7→6）。しかし、それによりガード付き相互インライン化が collect_block↔block_line を TCO に畳み込み、その追加型アキュムレータがすべての要素を無言で "" に誤低水準化した（`["a","","bb","c"]` → `["","","",""]`、壁でなくバイト不一致）。相互インライン化のガードが検査するのは、インライン化した F が低水準化できるかだけで、バイト一致ではないため、低水準化できても誤ったインライン化が通る。②に従い撤回（mod.rs の注記）。誤コンパイルを出荷する偽の壁数削減は正直な壁より悪い。本当の阻害要因は collect_block↔block_line TCO の追加が全要素を "" と読むこと。2 周期の `collect_block`/`block_line` 合成で再現した。まずこれを直して初めて呼び出し引数の持ち上げが安全になる。

  **🎯 根本原因を特定（2026-06-20、持ち上げを一時的に再有効化して二分調査）：相互インライン化の `substitute_var_in_expr` が、実体化できないリスト要素を生成して "" になる。** 2 周期 `collect_block`/`block_line` 合成では、block_line 本体に直接置いた定数要素（`acc + ["Z"]`）はバイト一致（`Z|E|Z`）。TCO + スロット + インライン機構は正しい。しかし block_line のパラメータ `line` を使う `acc + [line]` は、collect_block が定数 `"X"` を渡しても全要素が ""（`X|X|X` → `||`）。一方 `line`/`x`/`y` が普通のローカルなら `["a"] + [line]` / `[x,y]` はバイト一致する。
  **⚠ 訂正（2026-06-20、置換仮説を試験し否定）。** `inline_sibling_calls` の `substitute_var_in_expr` を `let fresh = arg;` + パラメータを `fresh` に改名するインライン化（リスト要素がローカル変数参照 `[Var(fresh)]` になる）へ置き換えても直らず、block_line は全要素を "" と出力した（flow_rec/chars はバイト正確のままなので let-inline は退行しないが解決策でない）。撤回。より深い二分調査：`acc + [string.drop(line, 0)]` でも呼び出し要素が "" なので、要素実体化ではない。新しい `let line = list.get(lines, pos) ?? ""` 自体が TCO ループ内で空を読む。借用リストパラメータ `lines` に対する `list.get(lines, pos)` が何も返さない。chars はループで借用文字列パラメータ（`string.slice(s, …)`）を読むため動作する。相互インライン→TCO ループで借用リストパラメータを読むと空になる。ループ本体の読み出し前にパラメータが drop／ゼロ化されるか、ループがそれを持ち越せていない。

  **🎯🎯 真の根本原因、wat で確認（2026-06-20）：裸の呼び出し引数の持ち上げ + 脱糖の複製で VarId が衝突。** yaml に忠実な `line_at` ヘルパー合成の wat をダンプした（持ち上げ再有効化）：
  ```
  (local.set $v15 (call $line_at  (local.get $v0) (local.get $v1)))   ;; line = line_at(lines,pos)
  (local.set $v19 (call $__list_concat_rc (local.get $v2) (local.get $v13)))   ;; THEN arm: acc + [""]
  (local.set $v28 (call $string.drop (local.get $v19) (i64.const 0)))   ;; ELSE arm: string.drop(line,0)
  ```
  コード内コメントの説明：最初は line = line_at(lines,pos)。次は then アームの acc + [""]、最後は else アームの string.drop(line,0)。
  else アームの `string.drop(line, 0)` が `$v15`（line_at の String）でなく **`$v19` — then アームの `__list_concat` 結果（List）**を読む。つまり `line` の VarId が then アームの連結スロットと重なる。持ち上げ（`tmp = max_var_id(body)+1`）+ 末尾複製（`desugar_let_bound_heap_branch` が継続を両アームへ複製）+ 有界複製の緩和によって両アームで VarId を再利用し、グローバル VarId→wasm-local マップで衝突する。string.drop はリストポインター上で動き、不正値／"" となる。置換でも借用パラメータでもない（上で両方否定）。それらの理論は見当違いだった。
  **修正：** 呼び出しごとに `max_var_id(body)+1` を再計算する代わりに（以前の書き換えが ID を消費済みだと衝突）、単一の単調増加する新規 VarId カウンターを `desugar_heap_branches` / `desugar_callarg_heap_if` / `desugar_let_bound_heap_branch` に引き回し、持ち上げた各 `tmp` と複製した各継続が全体で一意の ID を持つようにする。その後で裸の呼び出し引数の持ち上げが健全になり、block_line が正しく低水準化されて yaml 7→6。新規 ID の引き回しより前に持ち上げを再追加してはいけない。

  **⚠ 更新：単純な ID 再利用も否定された。** `max_var_id`（mod.rs:2065）は `IrStmtKind::Bind` の変数（visit_stmt、2107 行）+ Match パターン束縛を数える。持ち上げの `let tmp` は計上され、次の `max_var_id+1` は増えるので単純な ID 再利用ではない。したがって v15 でなく v19 になる断絶は IR-VarId の衝突でなく、MIR 低水準化層の `value_of`／実体化の断絶。インライン化が呼び出し `line_at(lines,pos)` を block_line の条件（`string.is_empty(line)`）と要素（`string.drop(line, 0)`）の両方へ置換すると、低水準化は line_at を 1 回実体化（`v15`）するが、要素の `string.drop` は `v19`（兄弟アームの連結）へ束縛される。仮説 4 個を否定（置換、借用パラメータ、fresh-let インライン、単純な ID 再利用）。次：`line_at` 合成の MIR Op 列（wat だけでなく）をダンプし、どの Op が `string.drop` 引数を v19 に設定するか追跡する。バグは複数位置へ置換された呼び出しについて、持ち上げ／脱糖が `value_of` を引き回す方法にある。MIR Op に集中したセッションが必要。

  **⚠ let 束縛 `!` の一律除去は不健全 — 検出した②の落とし穴（2026-06-20）。** `lower_destructure` で `let (a,b)=f()!` → `let (a,b)=f()` と除去（末尾 `!` の素通し）し、`tco_collect`/`tco_rewrite` で `!` の先を見ると、確かに seq_item→collect_seq→collect_map を解消した（yaml 6→3）。しかし v0/v1 spec の全走査がエラーを返す関数の誤コンパイルを検出：`safe_div_chain`、`grade_classify`、`sum_of_squares`、`closure_env_churn`、`map_entry_churn` がすべてバイト不一致（除去がこれらの関数の伝播する本物の `err(…)` を落とす）。corpus-wall（所有権）は成功、バイト一致だけが検出。②に従い撤回。
  健全な版を可能にする重要な区別：**yaml パーサー群は `err(…)` を決して返さない**（grep で確認。唯一の `err("…")` は純粋な整数パーサー `oct_rec`/`bin_rec` 内で、effect `!` でなく `match` で処理）。したがって never-err に範囲を絞り、呼び出し先が決してエラーを返さないと証明できる場合だけ let 束縛 `!` を除去するなら健全で yaml=0 に届く一方、`safe_div` などは壁のままにできる（呼び出しグラフの不動点：`err(…)` があるか can-err 関数を `!` 呼び出しする場合に限り can-err。yaml 群にはない）。呼び出し先ごとの解析（低水準化へ引き回す）または完全な effect モナド（return-wrap）が道筋であり、一律除去は永久に除外する。

  **✅✅ never-err 限定除去が完了（コミット b154a270）：yaml 6→3。** `mod.rs` に呼び出し先ごとの can-err 解析を実装：`compute_can_err(fns)` が `has_result_err`（本体に `IrExprKind::
  ResultErr` を含む）で初期化し、`!` 伝播の不動点を計算（`unwrap_named_callees` = `Unwrap{Call{Named
  g}}` の `g`。can-err 関数を `!` 伝播する関数は can-err）。その後 `strip_never_err_unwraps` が never-err の `g` に限り `Unwrap{Call{Named g}}` → `Call{Named g}` と置換。`inline_mutual_tail_recursion` の最初の段階として実行（インラインガードの試行低水準化より前なので、インライン化した F は裸の呼び出しを見て追加 TCO が発動し、`tco_collect` は `!` を意識しなくてよい）。yaml 群はすべて never-err（`ResultErr` なし。唯一の `err("…")` は `!` でなく `match` 経由で達する純粋な `oct_rec`/`bin_rec`）なので seq_item/collect_seq/collect_map がすべて TCO で解消する。②の検証：corpus-wall のプロファイル内 3741→3758（+17）、ownership 15068 ACCEPT、v0/v1 spec 全走査 = 既存の不一致のみ。一律除去でバイト不一致だった `safe_div_chain`、`grade_classify`、`sum_of_squares`、`closure_env_churn`、`map_entry_churn` は、can-err の `!` を残すため今は一致する。cargo-test 466。

  **⚠ 残る yaml 3：map_entry、parse_lines、parse_nested** — それぞれ異なる Value 集約／match 機能（除去 + TCO の基礎は完了）：
  - **parse_lines、parse_nested**：`… |> list.find((e) => not is_blank(e.1))`（タプル添字 `.1` を使うラムダ付き list.find）、その後 `match opt { none => …, some((idx, line)) => … }` — **タプルペイロードを持つ Option match**（`some((idx,line))`）。try_lower_variant_value_match はスカラー／単一ヒープペイロードを扱うが、タプルペイロードの分解束縛が不足。
  - **map_entry**：`match find_colon(t) { none => (value.object(pairs), pos), some(cp) => { … pairs +
    [(key, val)] … } }` — Option のスカラー match（cp）は問題ないが、アキュムレータ追加 `pairs + [(key,
    val)]` は **List[(String,Value)] のタプル要素追加**（value.object 自体は証明済みで、collect_map の基底は低水準化できる）。不足はタプル要素リストの追加。

  **🔧 Option のタプルペイロード match の手順（parse_lines/parse_nested）。** レイアウト確認済み：タプルは `DynList`、要素 `i` は `layout::slot_offset(i)`（つまり `(idx,line)` = slot_offset(0) のスカラー、slot_offset(1) の String ハンドル）。Option `Some` ペイロードはバリアントブロックの `@12` にタプルハンドルとして入る。`try_lower_variant_value_match`（control.rs:822）を拡張：`Some`/`Ok` の内部パターンが `Tuple` なら、`@12`（タプルハンドル）を借用として新規 `$p` へ束縛（`str_heap_bind` 同様）、`let (idx,line) = $p` を前置してアームを低水準化（追跡済みコンテナの `BindDestructure`。`lower_destructure` の「追跡済みヒープ変数がコンテナの別名」経路）。対象の drop はアーム後（control.rs:1037 の `str_heap_bind` 分岐）にする。`parse_lines`/`parse_nested` は `line` を借用するだけ（`dispatch`/`nested_dispatch`/`indent_of`/`string.trim` へ渡す）でムーブアウトしないので、後の対象 drop がタプルとその String をちょうど 1 回解放する。最も明快な実装：関数先頭の脱糖で `some((idx,line)) => B` → `some($p) => { let (idx,line)=$p; B }`（新規 `$p` は `max_var_id+1`）。その後 `heap_or_scalar_bind` を拡張して Option 対象のヒープタプルペイロードを許可する。これで yaml 3→1、map_entry のタプル要素追加で 1→0。

  **✅ Option タプル match 完了（コミット a89bda41）：yaml 3→1** — バリアント match のタプルペイロード束縛 + 後置 drop で parse_lines/parse_nested を解消（corpus-wall ACCEPT、全走査で新規不一致なし）。

  **🎯 最後の壁 = map_entry（yaml 1）、完全診断済み（コミット 55343d53 まで）。** 合成を map_entry の正確な形（`match find_colon(t) { none =>
  (value.object(pairs), pos), some(cp) => { … cmap(…, pairs + [(key,val)]) } }`）へ二分調査して見つけた、入れ子の阻害要因 3 個：
  1. ✅ **ユーザー関数の Option 対象**（`find_colon` はセルフホストでなく `Named` 関数）— 未追跡で variant-match が発動しなかった。修正：Option/Result を返す `Named` 呼び出しを実体化された対象として追跡（同じ DynListStr の len-as-tag 表現）。`ufo.almd` のバイト一致で検証。
  2. ✅ **借用 `pairs` を両アームで使用**（none で `value.object(pairs)`、some で `pairs + [(k,v)]`）— then アームの consume が else アームの低水準化の見方に漏れて else が壁となった。修正：代替アーム間で param_values+live+materialized_aggregates を保存／復元（分岐所有権の隔離）。
  3. ✅ **完了（コミット 95accd80）：`pairs + [(key,val)]`（List[(String,Value)] タプル要素追加）。yaml 1→0。** 新しいセルフホスト再帰 drop `$__drop_list_str_value`（value_core.almd：タプルごとに @12 の String スロットを rc_dec、@20 の Value スロットを `$__drop_value`、その後タプル、最後にリスト）を新しい `Op::DropListStrValue` の背後に置く（単一の証明書 `d`、`DropListValue` 同様の信頼される再帰）。`try_lower_concat_list` + `try_lower_str_list_literal` がヒープフィールド `(String,Value)` タプル要素を許可（`try_lower_tuple_
     construct` + `__list_concat_rc` 経由）し、`str_value_elem_lists` で追跡。rc_dec 許可リストと example 側 linker が `$__drop_list_str_value`/`$__svdrop_list` を取り込む。corpus-wall ownership ACCEPT（プロファイル内 3758→3822、+64。他の corpus 関数も 63 個解消）、v0 とバイト一致、cargo-test 466、全走査で新規不一致なし。**⚠ 発見：既存の TCO ヒープループのリーク — 末尾ループの `let xs = [heap]; loop(...)` は List[Value]（証明済み）、List[String]、この新しい drop のすべてで等しく約 N/反復あたりオブジェクト数でトラップ（freelist が再利用されない）。List[Value] 1 要素は約 2000、3 要素は約 1500、この 4 オブジェクト版は約 1000 と同じオブジェクトあたりの率。したがってこの drop のバグでなく、すべてのヒープを確保する末尾ループに影響する別の freelist/TCO 問題であり、専用の実装単位に値する。**

  ~~3-旧。残り：`pairs + [(key,val)]` … 呼び出し引数位置。~~（上で完了し置換済み、診断の経緯として残す）：`try_lower_concat_list`（calls.rs:534）は String/Value 要素しか許可しない（548–552 行）。**ヒープフィールドの集約要素（内部にヒープを持つタプル／レコード）は延期**され、呼び出し引数経路は壁となる（calls.rs:887、②として正しい。一方 let 束縛経路は無言で Opaque の空リストへ延期する = 潜在的誤コンパイルであって本物の低水準化でない。単なるゲートの癖でなく真に未解決）。修正は真の「Camp-4 未解決領域」、すなわち**タプル対応の再帰リスト drop**（`(String,Value)` タプルの `DropList`。反復し、各タプルのマスク付き drop で内部 String+Value を解放）で、`__list_concat_rc` がタプル要素を rc 所有し、結果を正しく回収できるようにする。DropListStr/DropListValue とは異なる新 runtime drop（List[tuple]）。それがあれば map_entry を低水準化し yaml=0。（単独再現：`/tmp/eff2.almd`、`/tmp/eff5.almd`。let 束縛の延期は `/tmp/sv.almd`。）

  **⚠ 訂正（分解束縛の脱糖経路は阻まれる — 試験済み）：** タプル変数／パラメータの `let (idx, line) = pair` は単独でもバイト検証で壁となる（`/tmp/td.almd`：v0 `7:hi`、v1 は壁）。`lower_destructure` の形はタプルリテラル値または追跡済みコンテナで、単なるタプル変数 → スカラー + ヒープの分割はどちらでも扱えない（cs の `let (v,n) = cs(...)` が低水準化できたのは、右辺が新しい呼び出し結果タプルという別の形だから）。したがって Option タプル match は `some($p) => { let (idx,line)=$p; … }` へ脱糖できない。variant-match の `bind_payload` 内で直接 idx/line を束縛する必要がある。`@12`（タプルハンドル）を読み、`idx = load(handle +
  slot_offset(0))`（スカラーコピー）、`line = load(handle + slot_offset(1))`（ヒープハンドル借用）とし、対象をアーム後に drop する。つまりアームごとの束縛を `Option<(VarId,bool)>` から複数束縛（単一またはタプル）へ再構成する。複雑で所有権上重要な変更（後置 drop を誤ると UAF）だが、証明書 + バイト一致ゲートがあらゆる誤りを捉える。残る作業はこの複数束縛の再構成（parse_lines/parse_nested）+ List[(String,Value)] タプル要素追加（map_entry）。
  （このターンではほかに、古い作業ツリー経由でコミット 75c9100e が mod.rs の block_line 新規 VarId 修正を誤って落とした。`git checkout 5518fff3 -- mod.rs` で復元し block_line のバイト一致を再検証。yaml は 6 に戻り、corpus-wall 成功、プロファイル内 3741、ownership 15035 ACCEPT。）

  **🔧 let 束縛 `!` の具体的手順（2026-06-20、Result 表現を確認済み）。** v1 MIR は effect 関数の `Result[T,String]` を len-as-tag の DynListStr として表現する（`materialize_result_ok`、control.rs:2030）。`len @ handle+4` は Ok で `0`、Err で `≠0`。Ok ペイロード（スカラーまたはタプル／ヒープハンドル）は `handle+12`。末尾 `f()!` はすでに Result を素通しする（`lower_tail(expr)`、tail.rs:256/624）。末尾値が関数の戻り値そのものなので健全。let 束縛 `let pat = f()!; rest`（binds.rs:235、壁）は heap-result `if` へ脱糖する。新しい variant-match 拡張は不要：
  ```
  let r = f()                                  // Result (DynListStr); track in materialized_results_str
  if <load(r+4) != 0> then r                   // Err: move the Result out as-is (Dup+Consume, the Var arm)
  else { let pat = <load(r+12)>; rest }         // Ok: extract the payload @ +12, then the continuation
  ```
  コード内コメントの訳：r は Result（DynListStr）で materialized_results_str に追跡する。Err は Result をそのままムーブアウト（Dup+Consume、Var アーム）。Ok は +12 のペイロードを抽出し、その後継続を実行する。
  Ok ペイロード抽出 + 所有権は既存の `value.as_array` str-result 経路と完全に同じ（control.rs:907–916：@12 ハンドルを借用で束縛し、その後 Result wrapper を drop）。タプルペイロードなら束縛の後に @12 ハンドルのタプル分解束縛（.0/.1 を読む）を続ける。両アームは関数の `Result` を生成するため、既存の heap-result-`if` 機構（追加したばかりの ConcatList/Call/Block アームを含む）で低水準化できる。最も難しい統合は collect_seq。`!` が TCO ループ本体内にあるため、Err の早期リターンはループ持ち越し `if`（then アーム `return r` が値付き break）になる。まず非 TCO の単独合成 `let x = mk(n)!` をバイト一致で行い、その後 TCO 統合を行う。そこから始める。

  **⛔ 確定（2026-06-20、上の手順には未充足の前提がある — コードで確認）。** v1 MIR はユーザー effect 関数の戻り値を DynListStr Result 表現で包まない。`lower_body_with_globals`（mod.rs:180）は `lower_body_into(body)` をそのまま `ret` として返し、Ok 包装がない。したがってユーザー effect 関数はタグなしの裸の値（`(Value,Int)`）を返す。末尾 `f()!` の除去が健全なのは、Result レベルで g ≡ f() だからに限る（tail.rs:253、恒等。g は Ok でも Err でも f() の値をそのまま返す）。しかし let 束縛 `let x = f()!; rest` では g ≠ f()（f() の後に rest）なので、単純除去は Err 経路で不正な x を使って `rest` を実行し、無言の誤コンパイルとなる。それゆえ binds.rs:235 は意図的に壁で止め、除去しない。正しい let 束縛 `!` には本当の早期リターンが必要で、分岐用の runtime Result タグが必要だが、裸の値モデルにはない。前提は v1 effect-Result 規律の構築である。全 effect 関数の戻り値を Result 表現（materialize_result_ok／Err 構築）で包み、すべての呼び出し位置で `!`/`?` をタグ対応させる（末尾 `!` は恒等除去からタグ検査付き素通しへ変更）。これは大規模な横断サブシステム（全 effect 関数 + 全 effect 呼び出し）であり、上の手順が想定した単一脱糖ではない。②：単純除去は不可（Err 経路の誤コンパイル）。したがって effect 関数の壁 6 個は先に effect モナドを構築する必要がある。非 effect の block_* の壁 2 個は独立（3 周期インライン + tuple-heap）であり、effect モナドサブシステムを避けるなら次の対象として取り組みやすい。
  （追加連結 — スカラー + String/Value ヒープ — ガード付き相互インライン、呼び出し要素実体化、同時更新 TCO、heap-result-if 追加基底は完了・検証済み。1 つずれる問題群にはガードがある。）

C の端から端までの導入後、11 個の壁が解消する（Value パーサー部分集合には value.object/stringify + tuple-heap も必要）。証明済み基盤上で yaml → 0 を実現する。v1 の完全性の理想である。

<a id="org-wall0--the-remaining-6-non-native-walls-precisely-scoped-2026-06-27"></a>

## ORG wall=0 — 残る非 native の壁 6 個、範囲を正確に特定（2026-06-27）

cross-module + str-acc/defunc + ReadDir + correctness-sweep のキャンペーン後、org の壁は**12 リポジトリで wall=0、非 native の壁 6 個 + native 専用の porta 29/sqlite 20**。6 個は二つの機構に分かれ、どちらも実際の `.almd` と壁の位置を読んで検証した。

<a id="mechanism-1--aes-2-walls-cfb8_encrypt-cfb8_decrypt--reuses-the-proven-loop-cert-no-new-coq"></a>

### 機構 1 — aes（壁 2 個：`cfb8_encrypt`、`cfb8_decrypt`）— 証明済み Loop 証明書を再利用、新しい Coq は不要
形（`aes/src/mod.almd:168-193`）：`var iv = state.iv`（借用ヒープレコードのフィールドに束縛した `var`）、次に `for i in 0..len { … iv = bytes.concat(bytes.slice(iv,1,16), …) }`（反復ごとに新規所有 `bytes` へ再代入）、最後に結果レコードへムーブアウト（`iv: iv`）。これは証明済みループ持ち越しスロット `[Inc; Loop[FDec;FInc]; MoveOut]`（OwnershipLoop.v）そのもの。反復ごとに正味 0（旧値 drop + 新値取得）、最後の値をムーブアウトする。
- **壁の位置**：`lower/tail.rs:48-64` が、ループ内再代入（`loop_reassigned_vars`）される可変ヒープフィールド変数を意図的に壁で止める。非ループ版（`:65-78`）は可変フィールド変数をすでに所有 `Dup` する。
- **証明書機構は対応済み**：`certificate.rs loop_carried_slots` は `LoopStart…LoopEnd` 内で SetLocal される任意のヒープ結果 Call をスロットとして登録する。`bytes.concat`（ヒープ結果）→ `iv` スロット、ループ `(id)`、ムーブアウト `m`。阻害要因は壁がスロット機構の前に低水準化を止めることだけ。
- **修正（方法 3、追加型アキュムレータに倣う）**：スロットの初期化はきれいな `i`（Alloc／ヒープ結果 Call）でなければならず、`Dup`（証明書 `a` と `of[slot]≠slot` を生む）ではいけない。壁を次で置き換える：`var iv` の初期化をフィールドの所有ヒープ結果コピーとして出力（`bytes` clone 呼び出し → `loop_carried_slots` がその `i` を見てスロットへ振り分ける）。ループ SetLocal + ムーブアウトが証明済み証明書 `i(id)m` へ畳み込まれる。**ゲート：aes は NIST FIPS-197 テストベクトルを同梱**（`mod.almd:200+`）し、バイト一致オラクルがそのまま得られる。corpus-wall ownership ACCEPT が証明書の誤りを捉える。aes 2→0、壁 6→4。

<a id="mechanism-2--filterfilter_map-4-walls-wasm-bindgen-generate_witesmdts-dojo-backfill_dir--needs-a-new-coq-construct"></a>

### 機構 2 — filter/filter_map（壁 4 個：wasm-bindgen generate_wit/esm/dts、dojo backfill_dir）— 新しい Coq 構成子が必要
形：`types |> list.filter((t) => list.contains(used_names, get_str(t,"name")))`（`used_names` をキャプチャするクロージャ）。dojo の `list.filter_map((f) => match fs.read_text(dir+"/"+f) {…})` は `dir` もキャプチャし、さらに effect を持つ。キャンペーンの値位置 HOF の正直さを保つガード（`calls.rs`、`last_call_had_unlifted_closure`）に阻まれる。
- **本当の Coq 未解決領域である理由**（実験で確認。エージェントの C1 インラインでバイト一致したが corpus-wall が拒否）：filter の要素ごとの取得は条件付き — `if pred then {Inc x; append to
  out}`。出力リスト `out` は runtime で変わる数のコピー（k = true の数）を蓄積し、反復ごとでなく最後の `out` の一括 DropList（k 個の Dec）で均衡する。現在の OwnershipLoop の Loop 規則は本体が rc を厳密に維持（正味 0）することを要求するが、条件付き +k はそうでない。安全なプログラムを拒否するので、正味 0 アキュムレータの一段先にある新しい完全性の穴となる。
- **必要な拡張**：**条件付きでアキュムレータに取得 + 一括排出**を表す Coq 構成子。ループ本体が条件付きでアキュムレータの要素数を増やし（単調、障害なし）、最後にすべて解放する `Drain` で均衡させる。健全性：任意の true 回数 k について、（out へ条件付き Inc を k 回）=（len out）=（DropList out で Dec を k 回）となり、k に依存せず均衡。その後 OCaml 検査器へ抽出し、C1 インライン化したキャプチャ付き filter の証明書を出力し、低水準化を振り分ける。dojo の filter_map はクロージャが effect を持つため、effect モナドの `!`（#22／let 束縛 `!` の未解決領域）も必要。これが本当の削減できない `#31` の Coq 作業で、多層（Coq → 抽出 → Rust 証明書 → 低水準化）かつ健全性上重要。

**wall=0 への計画**：(1) aes（証明済み基盤の再利用、NIST でゲート、取り組みやすい）→ 6→4、(2) 条件付き取得の OwnershipLoop 構成子（新 Coq）→ wasm-bindgen 3→0 → 4→1、(3) dojo の effect 付き filter_map 用 effect モナド + 条件付き取得 → 1→0。その後残る壁は porta/sqlite の native 専用だけ（再分類する）。

<a id="-step-1-done-commit-6a38227e-aes-20-a-routing-gap-not-coq--see-the-aes-section-above"></a>

### ✅ 段階 1 完了（コミット 6a38227e）：aes 2→0（Coq でなく経路の不足 — 上の aes 節参照）。

<a id="-step-2a-done-commit-5d3b642f-the-conditional-acquire-coq-core--ownershipfilterv"></a>

### ✅ 段階 2a 完了（コミット 5d3b642f）：条件付き取得の Coq 核心 — OwnershipFilter.v。
`CondLoop thenb elseb` + `ccheck_unroll_sound`：両分岐が rc を維持 ⇒ 反復ごとの任意の述語結果列で二重解放／リークなし。カーネル検査 + coqchk + 追加公理なし、証明ゲート内（PROOF SPINE OK）。filter スロット証明書 `[CInc; CondLoop [FDec;FInc] []; CMove]` は受理し、リーク／枯渇する分岐は拒否する。無条件 Loop は thenb=elseb の特例。これが削減できない Coq の部分である。

<a id="step-2b-remaining-the-integration--precisely-diagnosed-2026-06-27-heap-filter-inline--conditional-acquire-certchecker"></a>

### 段階 2b は残る（統合 — 2026-06-27 に正確に診断）：ヒープ filter のインライン + 条件付き取得の証明書／検査器。
実験的発見（各 filter の形を render_program 対 native でプローブ）：
- 非キャプチャのヒープ filter（`xs |> list.filter((s) => string.len(s) > 1)`、List[String]）は動作。インラインでなく C2 リフト（セルフホストの `(call $list.filter list funcref)` 結合子、独自の証明済み証明書）経由。
- スカラー filter（`[1,2,3] |> filter((x) => x > 2)`）は動作。C1 インライン（try_lower_defunc_list_hof）経由で、書き込みカーソル + i64 値の条件付き store（所有権なし ⇒ 要素の証明書イベントなし。既存検査器はこの条件付き store を受理済み）。
- キャプチャ付きヒープ filter（スカラー、文字列、リストの全キャプチャで壁）は失敗。キャプチャに FuncRef がないので C2 リフト不可、C1 インライン `try_lower_defunc_list_hof` も control_p5.rs:116（`filter` はヒープ入力の許可リスト外）+ :138–139（`filter` 結果は非ヒープ必須）でヒープ filter を辞退する。funcref-drop + 値位置ガード（calls.rs:163）へ落ちて壁になる。
修正（2026-06-27、各要素型のプローブで精密化）：`filter(pred) ≡ filter_map((x) => if pred(x)
then some(x) else none)`。**List[String]** については、キャプチャ付き `filter_map` は証明済み str-acc 条件付き追加経路で、すでに低水準化 + corpus-wall ACCEPT（検証：`xs |> filter_map((s) => if
list.contains(names,s) then some(s) else none)` が `bb,ccc` とバイト一致。条件付き追加／スキップは既に受理された証明書であり、OwnershipFilter.v が今その健全性を証明する）。したがって List[String] filter の壁は filter→filter_map 脱糖だけ（経路の不足、新しい検査器コード不要）。しかし wasm-bindgen の壁は List[String] でなく **List[Value]**（`get_arr` 由来の `fns`/`types` = `json.as_array`）。キャプチャ付き List[Value] `filter_map` も壁となる（検証済み）。str-acc 経路の `result_str_acc`（control_p5:131–133）が `List[String]` だけに一致するため、`List[Value]` filter_map は辞退 → キャプチャで C2 リフト失敗 → 壁。

したがって残る正確な作業は次のように分かれる。
1. **filter→filter_map 脱糖**（List[String]）— 経路の不足。検証済みで着手可能。任意の List[String] キャプチャ付き filter を解消する一般機能だが、これだけでは wasm-bindgen の壁を解消しない。
2. **Value 要素のキャプチャ付き filter/filter_map インライン**（wasm-bindgen の壁 3 個）— str-acc 条件付き追加経路を String 要素から **Value 要素**へ一般化（`value_elem_lists` + Value 用 `__list_concat_rc` は、上記のヒープ要素連結機構で無条件 Value 追加用に存在済み。条件付き filter_map 追加へ拡張）。証明書は String 経路の同じ条件付き追加（OwnershipFilter.v の CondLoop、String では corpus-wall 受理済み）なので、おそらく新しい検査器コードは不要で、Value 要素の経路拡張となる。ゲート：バイト一致 + corpus-wall ACCEPT。
3. **dojo filter_map**（壁 1 個）— クロージャが fs.read_text を呼ぶ（effect あり）ので、Value／レコード要素の条件付き追加に加え、effect モナドの let 束縛 `!`（#22 未解決領域）が必要。最も難しい。
全体のゲート：バイト一致（キャプチャ付き filter の実出力）+ corpus-wall ownership ACCEPT + backend-split 0。コミット済み OwnershipFilter.v の Coq 核心が三つすべての健全性の基盤となる。

<a id="-step-2b-partial-commit-5a0a9efb-capturing-heap-filter-lowered--wasm-bindgen-32"></a>

### ✅ 段階 2b の一部完了（コミット 5a0a9efb）：キャプチャ付きヒープ filter が低水準化可能 → wasm-bindgen 3→2。
キャプチャ付きヒープ `list.filter`（List[String] + List[Value]）を、書き込みカーソル + 局所的に均衡する条件付き取得で低水準化できるようになった。述語 true の then アーム内で入力要素をコピー（Dup `a`）し、出力リストへムーブ（Consume `m`）して保持する。`a..m` が then アーム内で均衡し（else は何もしない、出力リストは 1 回 alloc され、SetLocal で再束縛するスロットではない）、既存の平坦検査器がループ持ち越し CondLoop なしで受理する。OwnershipFilter.v の CondLoop はより一般的なループ持ち越し形を証明するが、この局所均衡形には基本検査器で足りる。control_p5 lower_defunc_list_hof_inner：filter のヒープ入力 + 結果を許可し、本体は Bool のまま、then アームで clone+consume、出力は heap_elem_lists/value_elem_lists で追跡。List[String]+List[Value] がバイト一致、corpus-wall 18795 ACCEPT、backend-split 0。`generate_wit` を完全に解消。

<a id="remaining-org-non-native--3-the-deep-frontiers"></a>

### 残り（org 非 native = 3）：深い未解決領域。
- **generate_dts / generate_esm（wasm-bindgen 2）** — 単純な heap-result-if ではない。単独の形（flat_map の条件付き呼び出し、let 束縛の大きいリスト if、連結途中の heap-result-if `A + throws_line + B`、最上位 let 束縛ラムダ HOF 引数）はすべて低水準化 + バイト一致する（最後はコミット 9757d56a で修正）。残る阻害要因は `sigs = supported |> list.flat_map((f) => { … let param_ty = (p: Value) => {…};
  let params_str = params |> list.map((p) => "…${param_ty(p)}…") |> list.join(", "); … })`。flat_map 本体内で定義した let 束縛ラムダを、内側 map のインラインラムダ（`param_ty(p)`）が呼ぶ、入れ子 defunc 本体の let-lambda。最上位形は今インライン化される（9757d56a、lambda_bindings 経由）。flat_map の Block アームも lower_stmt で内部の文を低水準化する（`let param_ty` が登録される）。それでも合成 `fs |> flat_map((f) => { let tag = (p)=>p+":"+f; ["a","b"] |> map(tag) })`（tag が外側 flat_map パラメータ `f` をキャプチャ）は外側 flat_map で壁となる。外側 str-acc 本体の走査と、外側 HOF パラメータをキャプチャする内側 map(captured-let-lambda) の深い相互作用。さらに精密化（2026-06-27）して正確な位置依存バグへ絞った。`tag` が let 束縛ラムダ（`let tag = (p) => …`）である `xs |> list.map(tag)` は末尾／値位置で低水準化できる（9757d56a 修正、n2/n1d バイト一致）が、束縛位置（`let parts = xs |> list.map(tag); parts`、n1e/n1f/n1h はすべて「list.map with an unliftable/closure-list higher-order argument」で壁）ではできない。結果がヒープ（List[String]）でもスカラー（List[Int]）でも、キャプチャ有無にかかわらない。両位置は同じ lower_pure_module_value_call → try_lower_defunc_list_hof（calls.rs:89/139）を呼び、`tag` は使用前に lambda_bindings に登録される（binds_p2:280、無条件）。しかし束縛位置の try_lower_defunc_list_hof は None を返す（→ funcref-drop → last_call_had_unlifted_closure=true → binds_p2:642 ガードで壁）のに、末尾は Some（211 解消）を返す。インラインラムダ束縛（`let parts = xs |> map((p) => …)`）は束縛位置でも動くため、let 束縛ラムダ（Var）の解決が位置で異なることに限定される。defunc 経路は binding_var/binding_is_mutable を読まないので、差はさらに微妙（束縛右辺と末尾で `tag` の IR 引数構造が異なる、または lower_bind が calls.rs:609 より前に設定する状態）。次：n1d（動作）対 n1e（壁）の MIR ops/args をダンプし、try_lower_defunc_list_hof 呼び出し時の `tag` 引数ノード + lambda_bindings 状態を差分比較する、集中した IR レベルのセッション。盲目的な修正は本物の忠実でない HOF の壁を隠す危険（無言の誤コンパイル）があるため、推測でなく ops 差分 + バイト一致とともに導入する。generate_dts/esm の `sigs` はまさにこれ（`param_ty` が let ラムダである内側 `params |> list.map((p) => …${param_ty(p)}…)` と、その map の `let params_str =` 束縛）。
  **✅ 束縛対末尾を修正（コミット 1dee4752）：let 束縛ラムダ HOF 引数が束縛位置でも低水準化可能**（binds_p2 の data_arg_has_fn が let-lambda Var を関数型のデータ引数に誤分類し壁にしていた。今はクロージャ引数と認識）。n1e/n1f/n1h バイト一致。ただし generate_dts/esm はまだ壁。let-lambda はそれらの最初の阻害要因ではなかった。
  **generate_esm の真の阻害要因（2026-06-27 に完全特定）：二つのヒープリストのタプル (List[String], List[String]) をアキュムレータとする fold** — `tuple_helpers`：`all_tuples |> list.fold(([], []), (state, ty) => { let
  (lines, seen) = state; let sig = …; if list.contains(seen, sig) then (lines, seen) else (lines +
  emit_tuple_helpers(ty), seen + [sig]) })`（重複除去パターン）。アキュムレータはヒープ List 二つのタプル、本体はそのタプルを返す heap-result-`if`。既存の tuple-acc fold（#69）が扱うのは (List[T], Int)、すなわちヒープリスト一つ + スカラー一つだけ。二つのヒープリストスロット（それぞれ独自の DropListStr、条件によって片方を再利用または他方を構築）は本格的な拡張。generate_dts の `matrix_iface_block` + `sigs` も同様の深い形、dojo の `backfill_dir` は effect モナド（#22）。これらは真に深い複数機能の低水準化（単一の経路不足の突破口ではない）で、各々独立して新たに取り組む実装単位となる。
- **dojo backfill_dir（1）** — クロージャが fs.read_text を呼ぶキャプチャ付き `filter_map`。Value／レコード要素の条件付き追加に加え effect モナド let 束縛 `!`（#22）が必要。
- **porta 29 + sqlite 20 native 専用** — `@extern(rust)` のホストスタブに wasm 形はない（物理的に低水準化できず、再分類だけ。低水準化作業でなく利用者の集計判断）。

<a id="the-5-real-lowering-walls-user-approved-2026-06-27-両方-分離--5-実装--precise-diagnoses"></a>

## 本当の低水準化の壁 5 個（利用者承認 2026-06-27「両方: 分離 + 5 実装」）— 正確な診断

@extern(rust) の再分類（native-FFI ホスト import 39 個 → 別カテゴリー）は org-trust-status.md で完了。実装すべき本当の低水準化の壁 5 個：

1. **wasm-bindgen generate_dts/esm（2）** — List[Value] に対するキャプチャ付き list.filter/filter_map。OwnershipFilter.v CondLoop 統合（条件付き取得）が必要。OwnershipFilter.v（Coq 核心）は証明済みでゲート内（5d3b642f）。キャプチャ付き String/Value filter の書き込みカーソル clone+consume は低水準化済み（5a0a9efb）。残りは (List,List) タプル fold の重複除去 + 入れ子 defunc の形。
2. **dojo backfill_dir（1）** — クロージャが fs.read_text を呼ぶキャプチャ付き filter_map。effect モナド let 束縛 `!`（#22）+ Value／レコード要素の条件付き追加。
3. **porta proxy.start（1）** — heap-result `match` + `validate(c)!`（effect モナド let 束縛 `!`、#22）。
4. **porta observability.format_tool_log（1）** — `"…${if success then "ok" else "error"}"`：heap-result-`if` 部分を埋め込んだ StringInterp。正確な診断：手動 ConcatStr `("" + "a=") + (if c
   then "x" else "y")` は低水準化 + バイト一致（脱糖形は問題ない）だが StringInterp 経路は壁。try_lower_string_interp（calls_p2.rs:188–189）がその正確な木へ脱糖した後、`try_lower_concat_str` 経由で低水準化する（→ lower_call_args が heap-result-if オペランドを辞退）。一方手動 ConcatStr リターンは lower_tail の ConcatStr 処理（if を実体化）を通る。同じ IR 木、異なる低水準化の入口。修正：try_lower_string_interp の脱糖木を、try_lower_concat_str ではなく lower_tail が ConcatStr に使う if 実体化経路へ送るか、try_lower_concat_str / lower_call_args に heap-result-if オペランドの実体化を教える。正確な入口の差を特定するため、nest（動作）対 sc2（壁）の MIR Op ダンプが必要。#60（StringInterp 脱糖）の領域。

#22（effect モナド let 束縛 `!`）は壁 2 + 3 の要。OwnershipLoop の条件付き取得 + (List,List) タプル fold は壁 1 の要。壁 4 は上の StringInterp-if 経路修正。

<a id="portastart--dojo-effect-monad-22--precise-statement--finding-2026-06-27"></a>

## porta.start / dojo effect モナド #22 — 文位置の `!` の正確な発見（2026-06-27）

porta.start の要を、エラーを返し得る本物の Result 呼び出し先に対する文位置の effect `!` まで追跡した（`validate(c)!`、`fn validate(c) -> Result[Unit,String]` が ok/err を構築）：
- `lower_effect_call`（calls.rs:438、lower_stmt:576 からの文経路）は Try/Unwrap を除去（10 行）し、内側の呼び出しを低水準化する。この除去は末尾専用の近道（呼び出しが関数の戻り値そのものなら健全）。
- 本体途中の文 `validate(c)!` では、除去した `validate(c)` がヒープ Result ハンドルを生成し、(a) drop せずスタックに残して不正 WAT（検証：`validate(n)!; ok(42)` →「type mismatch at end of function, expected [] but got [i32]」）、または (b) drop すると err 伝播を失って無言の誤コンパイル。健全な低水準化には本当の早期リターン `match validate(c) { ok(_) => <drop result,
  continue>, err(e) => return err(e) }` が必要。（スカラー Ok の本物の Result 呼び出し先に対する let 束縛 `!`、`let x =
  mk(n)!` はすでに低水準化できる、lb1。不足は文位置 `!` と heap-Ok の let 束縛 `!`。）
- 成果物は二つ：(1) 文位置 `!` の早期リターン（タグ検査 + return-err + Ok ハンドル drop）、(2) 応急的に、本体途中の can-err heap-Result 文の除去を不正 WAT でなく壁にして「不正 wasm を絶対に出力しない」修正。完全な修正は effect `!` 早期リターン機構（v0 側は emit_early_return_decs で対応済み、v1 の文／let 束縛経路にタグ分岐 + Ok drop が必要）。これが #22 の要であり、本物の Result 呼び出し先に限定される（裸値 effect 関数サブシステム全体ではない。validate/json.parse は本物の Result を返すので分岐用タグが存在する）。dojo.backfill_dir の `dash_item(...)!` let 束縛も同じ問題群 + キャプチャ付き filter_map。

<a id="-correction-2026-06-27-portastartdojo-need-nested-match-desugar-not-a-v1-mir-return-op"></a>

## ⭐ 訂正（2026-06-27）：porta.start/dojo が必要とするのは入れ子 match 脱糖であり、v1 MIR Return 演算ではない

以前の私の「v1 MIR に関数途中の早期リターン（Return 演算）がないため、porta.start + dojo に大規模構造サブシステムが必要」は誤りだった（断言せず検証する。変換先を試験して訂正）。effect `!` は早期リターン演算を必要とせず、入れ子 match の継続へ脱糖できる（標準的モナド do 脱糖）：

    { before; let x = f()!; after }   →   { before; match f() { ok(x) => { after }, err(e) => err(e) } }
    { before;     f()!    ; after }   →   { before; match f() { ok(_) => { after }, err(e) => err(e) } }

継続（`after`）は Ok アームに入れ子となり、Err アームは `err(e)` を再構築する（外側関数と同じ `Result[_, String]` エラー型）。早期リターンも Return 演算も不要。関数末尾は入れ子 match となり、既存の heap-result-match 末尾低水準化が扱える。

検証済み：明示的入れ子 match の変換先は、porta.start/dojo の使う全 Ok 型でバイト一致する。
- スカラー Ok：`match validate(n) { ok(_) => ok(42), err(e) => err(e) }` → `42`/`bad` ✓
- 入れ子（`!` 2 個）：`match validate(n) { err(e)=>err(e), ok(_)=> match parse(n) {err(e)=>err(e), ok(p)=>ok(p+1)} }` → `1`/`bad` ✓
- String Ok：`match parse(s) { err(e)=>err(e), ok(p)=>ok(p+"?") }` → `a!?`/`E:empty` ✓
- レコード Ok（porta.start の `Result[ProxyHandle, String]`）：`match validate(n){err(e)=>err(e),ok(_)=>if … then err … else ok({handle,port})}` → `5:10`/`E:bad` ✓

したがって porta.start（および dojo の effect 部分）の残作業は frontend/IR の `desugar_effect_unwrap` Block パス。effect 関数本体 Block を走査し、最初の `Unwrap`/`Try`（`let x = f()!` の Bind 値または裸の `f()!` Expr 文）で前後を分割し、上の入れ子 match に書き換える（Ok パターンは let 変数または `_`、Err パターンは新規 `e` を束縛、`after` とアームへ再帰）。入口：`desugar_heap_branches` と並べる（mod.rs:1003）。関数全体の `next_var` を引き回す（block_line の新規 VarId 修正同様）。構造：`IrExprKind::Match{subject,arms}`、`IrMatchArm{pattern,guard:None,body}`、`IrPattern::{Ok,Err,Bind,Wildcard}`、`IrExprKind::ResultErr{expr:Var(e)}`。変換先が検証済みなので取り組める。私が誤って主張した大規模 Return 演算サブシステムではない。dojo.backfill_dir はさらにキャプチャ付き effectful `filter_map`（クロージャ本体が要素ごとに fs.read_text を呼び、Option[record] を生成）を加える。それがクロージャを effect 付きループ本体として扱う未解決領域であり、本当に難しい残りの部分。

<a id="-the-remaining-3-lowering-walls-all-reduce-to-one-frontier-the-unliftable-hof-closure-2026-06-27"></a>

## ⭐ 残る低水準化の壁 3 個はすべて一つの未解決領域：リフトできない HOF クロージャ（2026-06-27）

effect モナド `!` 脱糖（9af74ed5）+ porta.start の native-FFI 再分類後、残る org の低水準化の壁 3 個（wasm-bindgen generate_dts/esm、dojo backfill_dir）はすべて同じ辞退に当たる：`list.{map,filter,flat_map,filter_map,fold} with an unliftable/closure-list higher-order argument`（calls.rs:166 / binds_p2:651 — `try_lower_defunc_list_hof` が None を返し、かつ `last_call_had_unlifted_closure`）。独立した下位ケース 3 個（各々検証済み。より簡単な形はすべて低水準化でき、この積み重ね形だけが壁なので、脱関数化器が単一の突破口）：

1. **クロージャ本体内の入れ子ラムダ + 内側 HOF**（generate_dts の `sigs`、generate_esm の `record_helpers`）。最小再現（壁）：`xs |> list.flat_map((f) => { let tag = (p) => if … then "BIG" else p; ["a","Mb"]
   |> list.map((p) => "x:" + tag(p)) |> list.join(", "); [f + "(" + parts + ")"] })`。本体が let 束縛ラムダ（`tag`/`param_ty`）を定義し、それを呼ぶ内側 `list.map` を実行する flat_map クロージャ。`try_lower_defunc_list_hof` は外側クロージャ本体をインライン化するが、そこに入れ子の内側 HOF を脱関数化できない（入れ子脱関数化）。let 束縛リストを持つブロック本体クロージャは正常に低水準化する（sig1/sig2 検証済み）。入れ子ラムダ + 内側 HOF だけが辞退する。注記：単なる heap-result-`if` 連鎖の本体（内側 HOF なし）を持つ flat_map クロージャも低水準化できる（fmc/fm2 検証済み）。不足は if 連鎖でなく内側 HOF。
2. **クロージャ本体内の effect 呼び出し**（dojo `backfill_dir`）：`task_files |> list.filter_map((f) =>
   match fs.read_text(dir+"/"+f) { ok(c) => some(parse_task_md(f,c)), err(_) => none })`。クロージャは `dir` をキャプチャし、要素ごとに effect 呼び出しを実行して `Option[record]` を生成する。Block レベルの effect `!` 脱糖は HOF クロージャに埋もれた effect 呼び出しへ届かず、クロージャを effect 付きループ本体にするには、脱関数化した要素ごとのループへ effect を引き回す必要がある。
3. **(List,List) の条件付きタプル fold**（generate_esm の `tuple_helpers`）：`all_tuples |> list.fold(([],[]),
   (st, ty) => { let (lines, seen) = st; if list.contains(seen, sig(ty)) then (lines, seen) else (lines +
   emit(ty), seen + [sig]) })`。条件付き取得を持つ、ヒープリストのループ持ち越しスロット 2 個（両分岐とも rc 維持、OwnershipFilter.v の CondLoop パターン、証明済み）。`try_lower_defunc_tuple_acc_fold`（control_p5:75）は (List[T], Int) しか扱わない。(List[heap], List[heap]) + 条件付き本体へ拡張する。

要は `try_lower_defunc_list_hof` を拡張して、(a) 内側 HOF + 入れ子 let 束縛ラムダ [#1]、(b) effect 呼び出し [#2] を含むクロージャ本体を扱い、`try_lower_defunc_tuple_acc_fold` で (List,List) 条件付き fold [#3] を扱うこと。#1（入れ子脱関数化）を突破すれば generate_dts + generate_esm record_helpers、#3 で generate_esm tuple_helpers、#2 で dojo を解消する見込み。すべてゲート検証可能（バイト一致 + corpus-wall + 証明ゲート）。これが最後の org 低水準化の未解決領域。

<a id="critical-refinement-sub-case-1-nested-hof-needs-the-nested-loop-cert-a-coq-frontier"></a>

### 重要な精密化：下位ケース #1（入れ子 HOF）は入れ子ループ証明書が必要（Coq の未解決領域）

nl1 の辞退を深く追うと、内側 `list.map` を含む flat_map クロージャは、内側 map の要素ごとのループが外側 flat_map ループに入れ子となる。しかし OwnershipLoop.v の証明済み `Loop` は平坦。「ループ本体は平坦（入れ子ループなし）…入れ子ループは将来の合成可能な拡張」（2026-06-20 の導入済み注記）。したがって `try_lower_defunc_list_hof` が別 HOF を含むクロージャ本体を辞退するのは健全（証明書で検証できない入れ子ループの出力を正しく拒否）であり、単なる実装統合の不足ではない。generate_dts `sigs` / generate_esm `record_helpers`（可変長リストの内側 `params |> list.map(… param_ty …)`、展開不可）には**入れ子ループ所有権証明書**（OwnershipLoop.v の拡張：本体に Loop を含む Loop、rc 維持を合成）が必要で、脱関数化器の微調整でなく #31 級の Coq 未解決領域。残る 3 個を取り組みやすさで順位付け：
- **#3 (List,List) 条件付きタプル fold**（generate_esm `tuple_helpers`）— 最も取り組みやすい。CondLoop は証明済み（OwnershipFilter.v）、平坦なループ持ち越しスロット 2 個、入れ子なし。`try_lower_defunc_tuple_acc_fold` を (List[heap], List[heap]) + 条件付き本体へ拡張。今日証明可能。
- **#2 クロージャ内 effect**（dojo `backfill_dir`）— 中程度。filter_map クロージャの要素ごとの本体が effect 呼び出しを実行し、effect（Stdout/FsRead cap）を defunc の要素ループへ引き回す。平坦ループだが、導入済み Block レベル脱糖とは異なるクロージャ内 effect `!` は新機構。
- **#1 入れ子 HOF**（generate_dts/esm）— 最も深い。先に入れ子ループ Coq 証明書が必要。
つまり generate_esm の壁二つは `tuple_helpers`（#3、取り組みやすい）と `record_helpers`（#1、Coq）に分かれ、dojo は #2。

<a id="-correction--win-2026-06-27-generate_esm-was-the-bounded-duplication-gate-not-coq-now-cleared"></a>

## ⭐ 訂正 + 成果（2026-06-27）：generate_esm は有界複製ゲート（Coq ではない）が原因で、解消済み

実際の wasm-bindgen mod.almd 全モジュールの二分調査（唯一信頼できる方法。単独 render_program は string.starts_with/list.contains をリンクしないため単独再現は誤解を招く）が、私の繰り返した誤診を訂正した。
- **generate_esm**：入れ子 HOF でも Coq でもない。壁は `desugar_let_bound_heap_branch` の有界複製ゲート（mod_p6.rs:1238、以前は `rest_branch_binds > 2`）。generate_esm は**最上位の省略可能リスト `if` を 4 個**重ねる（matrix_helpers/bytes_helpers/import_shim/shim_noop、それぞれ `if cond then [LITERALS]
  else []`）。最初が rest=3 > 2 を見てゲートが中止し、合流した let 束縛 `if` が「no sound scope-end drop」で壁になる。ゲートを `> 3` へ上げる（≤ 2^4 = 16 葉。末尾複製はアームごとの均衡を証明済み）と generate_esm を解消。検証：corpus-wall ownership **18854→19338 ACCEPT**（corpus の let 束縛 if +484 個が複製・低水準化可能、すべてカーネルで再検証）、output-parity 124/124、corpus-wall 高速（1:52、爆発なし）。**wasm-bindgen 2→1。**
- **generate_dts**：本当に入れ子ループの未解決領域。二分調査で壁を `sigs` flat_map クロージャの `param_ty` に特定した。`params |> list.map((p) => "${name}: ${param_ty(p)}") |> join` は外側 flat_map ループ内の内側 map（param_ty を除去すると generate_dts 解消）。ループ本体内のループは OwnershipLoop.v（平坦な Loop 本体）では表現できず、**入れ子ループ所有権証明書（Coq）**が必要。
- **dojo backfill_dir**：クロージャ内 effect 呼び出し（本体が fs.read_text を呼び Option[record] を生成する `filter_map`）。defunc ループ内 effect 機構。

正味：低水準化の壁 **3→2**（generate_esm 解消）。残り 2 個 = generate_dts（入れ子ループ Coq）+ dojo（クロージャ内 effect）。「深い／Coq」という上位パターンが当たったのは半分だけ。generate_esm は 1 行のゲートだった（「深いという見立ては繰り返し実験的な誤評価だった。壁ごとに検証」という教訓が再び適用された）。ただし generate_dts の入れ子ループは本当の Coq 未解決領域。

<a id="dojo-backfill_dir--precise-the-blocker-is-an-effect-call-as-the-filter_map-closures-match-subject-2026-06-27"></a>

## dojo backfill_dir — 正確な阻害要因：filter_map クロージャの match 対象である effect 呼び出し（2026-06-27）

実際の dojo モジュールの全モジュール二分調査（信頼できる方法）で backfill_dir の壁を正確に特定：`task_files |> list.filter_map((f) => match fs.read_text(dir+"/"+f) { ok(c) => some(parse_task_md(f,c)),
err(_) => none })`。二つの切り分け：
- `fs.read_text`（effect）を純粋呼び出しに置き換え、match + Option[record] 結果 + `dir` キャプチャは残す → **backfill_dir が低水準化（壁 0）**。したがってクロージャ内 match、`Option[Task]`（レコード）結果、`List[Task]`（List[record]）filter_map、`dir` キャプチャはすべて動作済み。
- 唯一の阻害要因は、defunc した filter_map ループ本体内の match 対象である effect 呼び出し `fs.read_text`。（List[record] でも Coq でもない。別の「深い」誤診が二分調査で訂正された。）
fs.read_text は最上位なら正常に低水準化（ReadTextFile prim、FsRead cap、dojo 3→1 を導入済み）するが、defunc 本体の match 対象低水準化（append_body_to_str_acc / append_variant_match_to_str_acc / ヒープ要素の内部経路）が effect 付き Module 呼び出しを対象として辞退する。修正は、defunc の要素ごとの本体に、許可された effect 付き標準ライブラリ呼び出し（fs.read_text → CallFn + FsRead cap-witness）を match 対象として低水準化させ、cap を外側関数の declared_caps へ引き回し、反復ごとの Result を所有／drop すること。これは defunc ループ内 effect 機構で、本格的だが Coq ではない（generate_dts の本当の入れ子ループと異なる）。最後の org 低水準化の壁 2 個は、**dojo = defunc ループ内 effect（実装統合）**、**generate_dts = 入れ子ループ所有権証明書（Coq）**。

<a id="dojo-backfill_dir--the-hoist-approach-is-verified-correct-but-my-desugar-impl-non-terminated-reverted"></a>

## dojo backfill_dir — 持ち上げ方式の正しさは検証済みだが、私の脱糖実装は非停止（撤回）

二分調査で修正の方向を証明：`filter_map((f) => match fs.read_text(p) { ok(c) =>
some(parse(f,c)), err(_) => none })` を手動で `filter_map((f) => { let r = fs.read_text(p); match r { … } })` に書き換えると backfill_dir が低水準化する（defunc インライン化器は要素ごとの let 束縛として effect 呼び出しを扱えるが、辞退される variant-match 対象では扱えない）。したがって dojo 修正は、HOF クロージャのラムダ本体内の非純粋／effect 付き variant-match 対象を let 束縛へ持ち上げる脱糖。Coq でも List[record] でもない（両方とも純粋対象／持ち上げ形で動作検証済み）。

私はこれを実装した（`desugar_hof_match_subject_hoist` + `hoist_hof_call_lambda_subject` + `hoist_variant_subject`、`desugar_heap_branches_inner` へ接続）が、スタックオーバーフローした（render exit 134、空 wat。最初に見た「dojo total: 0」はクラッシュが Unsupported 行を抑えたのであって本当の解消ではない。断言せず検証する規律により、空 wat + h3 合成もオーバーフローすることで検出）。脱糖は紙上では冪等に見える（持ち上げた `match Var {…}` の対象は純粋なので再発動しない）が、lower_body_into / desugar_heap_branches 不動点は停止しなかった。微妙な相互作用（おそらく desugar_nested_branch_arms 再帰、または兄弟の脱糖が持ち上げ形に再発動）を、非常に長いセッションの終わりに安全には特定できなかった。きれいに撤回（作業ツリーは lowering-walls=2、build エラー 0、dojo は正直な壁、generate_esm は解消を維持）。新しいセッションの TODO：停止ガード（「正確に 1 回変更」の不動点、または HOF ラムダ引数へ降りる非ループの単一パスなど）を付けて持ち上げを再実装し、Result-ok/err + Option[record] filter_map fixture + corpus-wall + oracle でバイト一致を検証する。方向は証明済みで、残りは停止する実装だけ。

<a id="dojo-hoist--second-attempt-one-shot-lower_body_into-fixpoint-also-stack-overflowed--it-is-the-lowering-not-the-desugar-loop"></a>

### dojo 持ち上げ — 2 回目（lower_body_into 不動点で 1 回だけ）もスタックオーバーフロー → 脱糖ループでなく低水準化が原因

持ち上げを自己再帰的な 1 パス 1 持ち上げ変換として再実装し、lower_body_into の `if let Some` 不動点に適用（最初のループ相互作用を避けるため desugar_heap_branches ループ外）。構成上収束するはず：各パスは対象 1 個を持ち上げ、持ち上げた `match Var {…}` の対象は純粋なので再度持ち上げない（冪等 → 2 回目は None）。紙上では脱糖不動点は停止する。しかし render は実 dojo と h3 合成でまだスタックオーバーフロー（exit 134）。したがって非停止は脱糖不動点でなく持ち上げ形の低水準化にある（defunc が `{ let r = <effect/call>; match r {…} }` ラムダ本体をインライン化して再帰するか、後続パスが再入）。特定にはデバッガー／計装（再帰深さトレース）が必要で、2 回失敗した盲目的な冪等性の推論では足りない。両回ともきれいに撤回（lowering-walls は 2、build エラー 0、dojo は正直な壁 exit-2）。新セッション：持ち上げた HOF クロージャ `{let r=…; match r{…}}` の低水準化を計装して再帰を探すか、別の道（持ち上げず、defunc 本体低水準化が effect 呼び出しの variant-match 対象を直接受け入れ、最上位 variant-match のように実体化するよう拡張）を採る。方向（effect match 対象を要素ごとの束縛／実体化値にする）は証明済みだが、安全な機構は未解決。盲目的な試行 2 回は中止し次にデバッガーを使う合図。

<a id="dojo-hoist--sharpened-debugger-pass-direction-is-verified-my-desugar-builds-a-malformed-tree"></a>

### dojo 持ち上げ — 精密化（デバッガー調査）：方向は検証済み、私の脱糖が不正な木を構築する

終了コードを確認した慎重な二分調査（以前の「壁 0」はクラッシュの偽陽性。render exit 134 は空 wat +「Unsupported」行 0 個を出し、Unsupported の grep は 0 と読んだ）：
- **実 dojo の手動事前持ち上げ**（`filter_map((f) => { let rr = fs.read_text(p); match rr {…} })` を backfill.almd ソースへ編集）→ backfill_dir 解消（`backfill_dir:` の壁なし。exit-2 は無関係のモジュールレベルの壁）。したがって方向は本当に検証済みで、frontend 由来の木なら defunc は Block 本体 `{let rr=…; match rr{…}}` の filter_map クロージャを低水準化できる。
- **同じ論理プログラムに対する私の脱糖による持ち上げ** → スタックオーバーフロー（exit 134）。論理形は同じだが、プログラムで構築した IR は frontend と異なり、低水準化をクラッシュさせる。
- （最小合成の手動事前持ち上げ `m_str`/`m_rec` は低水準化せず壁となる。異なる結果型経路であり dojo の形ではない。手動実 dojo 試験が検証したのは dojo 固有の形。）
したがってバグは低水準化でも（手動の木は低水準化）脱糖不動点でも（冪等、収束）なく、私の脱糖が frontend と比べて不正な木を生成すること。主な疑い：(1) 書き換えたラムダに元の `lambda_id` を残す（`lambda_id: *lambda_id`）。キャッシュされたリフト形／キャプチャが新本体と一致しない古い ID（`lambda_id: None` を試す）、(2) frontend の正規形に対する Block 構造／型の不一致。新セッション：手動事前持ち上げソースの `--emit-ast` と脱糖出力の IR ダンプを差分比較して不正部分を特定する（見ればほぼ確実に 1 フィールド修正）。持ち上げは dojo の正しい修正であり、残りは木構築の詳細だけ。撤回済み（lowering-walls=2、問題なし）。

<a id="dojo-hoist--third-attempt--backtrace-the-loop-is-the-lower_body_into-fixpoint-re-applying-a-non-idempotent-hoist"></a>

### dojo 持ち上げ — 3 回目 + バックトレース：ループは lower_body_into 不動点が非冪等な持ち上げを再適用すること

計装（HOIST_DEBUG env）：スタックオーバーフロー前に持ち上げが **874 回、すべて tmp=VarId(5)** で発動した。2 回目の発動で panic させるバックトレースで循環を特定：
```
hoist_variant_subject  (self-recursive ×3 — descends nested Blocks)
hoist_hof_call_lambda_subject
desugar_hof_match_subject_hoist (×2)
lower_body_into  (×5 — the `if let Some(r)=hoist(body){return lower_body_into(&r)}` fixpoint)
```
トレースの説明：hoist_variant_subject は自己再帰 ×3 で入れ子 Block へ降りる。hoist_hof_call_lambda_subject、desugar_hof_match_subject_hoist（×2）を通り、lower_body_into が ×5、持ち上げ結果が Some なら lower_body_into へ戻る不動点である。
したがって lower_body_into の持ち上げ不動点は決して収束しない。各パスで私の持ち上げが Some を返し（自分自身の出力に再発動）、lower_body_into が再帰してオーバーフローする。構築された木が正しく（ダンプ：`Bind{var:5, ty:Result, value:mk(Var2)}` + `Match{subject:Var(5)…}`、対象は実際に純粋な Var(5)）、walk_expr に Lambda アームもあるのに、全パスで tmp=VarId(5)（増えない）。非冪等性の根（純粋対象の `{let 5; match Var(5)}` に hoist_variant_subject が再発動し、nv が 5 のままの理由）は、推論 + ダンプ + バックトレースでは解けない。パスごとの正確な木の変更をステップ実行する必要がある（各 lower_body_into レベルの前後で本体を出力）。きれいに撤回（3 回目、lowering-walls=2、build エラー 0、dojo 正直な壁）。有望な修正（新セッション）：lower_body_into の再帰的 `Some→re-enter` 不動点内で持ち上げを適用せず、関数本体の入口（mod.rs 約 1003 行、lower_body_into の前）で全木の単一パスとして 1 回だけ適用し、構造的に再発動を不可能にする。持ち上げの方向は検証済みのまま（手動実 dojo 事前持ち上げは backfill_dir を解消）。バグは脱糖の不動点配置／冪等性だけ。

<a id="dojo-hoist--fourth-attempt-single-pass-at-function-entry-fixes-the-crash-but-the-tree-still-walls--regresses"></a>

### dojo 持ち上げ — 4 回目（関数入口の単一パス）：クラッシュは修正、しかし木は依然として壁 + 退行

バックトレースからの修正：持ち上げを、関数本体の入口（mod.rs 約 1003 行、desugar_heap_branches の前）でトップダウンの全体パスとして 1 回だけ適用し、lower_body_into 不動点には置かない。結果：スタックオーバーフローはなくなった（134 でなく exit 2）。入口の単一パス配置が停止の修正である。しかし backfill_dir はまだ「filter_map unliftable」で壁（持ち上げ前と同じ壁）、dojo 合計は 1→2（別箇所の退行）。正しく停止しても、私の脱糖で構築した `{let $r = subj; match $r{…}}` 木は、frontend の手動事前持ち上げ（backfill_dir 解消を検証済み）と同等でない。defunc は木を辞退し、カウント側（count_ir_calls は入口専用持ち上げを見ない）が +1 を生んだ可能性が高い。したがって削減できない阻害要因を確認：脱糖構築 IR は frontend IR と異なり、(a) defunc が低水準化しない、(b) 両側より前に脱糖する caps 不変条件を破る。4 回の試行（スタックオーバーフロー 2 回、計装のみ 1 回、停止するが壁 + 退行 1 回）はすべてきれいに撤回。新セッションの確定計画：(1) 手動事前持ち上げソースの frontend IR と脱糖構築木を出力／ダンプして差分比較（不正部分がそこにある）、(2) 持ち上げを共有の両側前脱糖経路へ適用（count_ir_calls にも見せる）。入口専用は caps 不変条件を破る。方向は検証済みで、未解決は脱糖 IR の忠実さ + ゲート配置。lowering-walls=2。

<a id="dojo-hoist--fifth-attempt--ir-diff-correction-my-baseline-was-the-walling-synthetic-not-the-clearing-real-dojo"></a>

### dojo 持ち上げ — 5 回目 + IR 差分の訂正：基準は解消する実 dojo でなく壁となる合成だった

関数本体の入口（mod.rs）で全体単一パスの持ち上げを 1 回適用し、確かにスタックオーバーフローを修正した（5 回目：134 でなく exit 2）。しかし backfill_dir はまだ「filter_map unliftable」。IR ダンプ（DUMP_IR env プローブ）で、backfill_dir の filter_map ラムダ本体が `Match { Call read_text, [ok,err] }`、持ち上げの対象形そのものであり、発動することを確認。それでも壁。以前「私の木は手動と構造的に同一」と示唆した IR 差分は、手動の合成（純粋な `mk` を持つ `proc`、Option[String]）を基準にしていたが、その合成自体が壁（m_str）。backfill_dir 解消を観測したのは手動の実 dojo（dojo3、fs.read_text + Option[record]）だけ。つまり壁になる基準と出力を比べ、解消する基準（実 dojo 手動）の IR は一度もダンプしなかった。訂正した新セッション計画：(1) 手動の実 dojo 事前持ち上げが本当に backfill_dir を解消するか再検証（単に「行なし」ではなく終了状態 + 関数ごとの壁）、(2) 解消するなら、その木の backfill_dir IR をダンプし、実 dojo に対する私の持ち上げ出力と比較（差分が不正部分）、(3) str_acc／ヒープ要素 filter_map 経路はそもそも `{let r; match r}` 本体クロージャを一切受け付けない可能性（合成 m_str も壁）。その場合持ち上げは誤った機構で、修正は defunc 本体低水準化（append_body_to_str_acc / lower_defunc_list_hof_inner）が effect 呼び出し／非純粋 match 対象を直接受け入れるよう拡張すること。5 回の試行（オーバーフロー 2、計装 1、停止するが壁 + 退行 1、入口で 1 回だが壁 1）はすべてきれいに撤回（lowering-walls=2）。「方向を検証済み」という主張は今や疑わしい（実 dojo dojo3 だけ解消し、その一度の読み取りも renderer が最初の壁で止まった結果かもしれない。まず再検証が必要）。

<a id="dojo-backfill_dir---the-precise-fix-point-corrects-5-hoist-attempts-defunc-variant-match-handler-is-option-only"></a>

### dojo backfill_dir — ⭐ 正確な修正箇所（持ち上げの 5 試行を訂正）：defunc variant-match ハンドラーは Option 専用

持ち上げは誤った機構だった（5 回の試行）。defunc 本体低水準化を読んで根本原因を特定：`append_variant_match_to_str_acc`（control_p5.rs:1207）は defunc した filter_map/flat_map クロージャ内の `match subj { … }` を低水準化するが、`IrPattern::Some` / `IrPattern::None` だけを扱う（約 50–58 行で Some/None アームを走査）。`IrPattern::Ok` / `IrPattern::Err` は扱わない。dojo backfill_dir のクロージャは `match fs.read_text(p) { ok(content) => some(…), err(_) => none }`、つまり Result（ok/err）match なので、ハンドラーは Some/None を見つけられず None を返し、filter_map が「unliftable」で壁になる。（合成 m_str の `match mk(x) { ok(v)=>some(…), err=>none }` も同じ理由で壁となり、対象／持ち上げでなくハンドラーであることを確認。）修正（新セッション、持ち上げではない）：`append_variant_match_to_str_acc`（および List[record] 結果のレコード／Value 結果版）を `Ok`/`Err` アームも受け入れるよう拡張。`ok(x) => some(e)` を `some(x) => some(e)` と同様（Ok ペイロードが保持する要素）、`err(_) => none` を `none` と同様（スキップ）に扱う。対象の effect 呼び出し（fs.read_text）は要素ループ内の match 対象として低水準化される（ハンドラーは対象を既に実体化する。約 31 行の `CallArg::Handle`）。これは局所的ハンドラー拡張（Option→Option+Result）であって、木を壊した持ち上げでも Coq でもない。検証：ハンドラーを拡張し、Result-ok/err filter_map fixture + corpus-wall + oracle でバイト一致。上の持ち上げ記録をすべて置き換える。持ち上げは放棄し、Ok/Err ハンドラー拡張が道筋。

<a id="dojo--refined-fix-point-the-exact-gates-handler-is-self-host-option-call--somenone-gated"></a>

### dojo — 修正箇所を精密化（正確なゲート）：ハンドラーはセルフホスト Option 呼び出し + Some/None に限定
`append_variant_match_to_str_acc`（control_p5:1207）は (1) `is_self_host_option_call(subject)`（1221）、(2) Some/None アーム（1256/1278）でゲートする。dojo の `match fs.read_text(p) { ok(content)=>some(…), err(_)=>none }` は両方失敗。fs.read_text はセルフホスト Option 呼び出しでなく Result を返し、アームは Ok/Err。したがって新セッションの修正はこのハンドラー（parse_task_md は String でなくレコード／Value を返すので、実際の dojo が使う List[record/Value] 結果版の経路も）を拡張して、セルフホスト Result 呼び出し対象（`is_self_host_result_call` を追加）、Ok/Err アーム（Ok≈Some で保持、Err≈None でスキップ）、逆のタグ（Result Ok は tag==0、Option Some は tag!=0。`is_result_ty` で区別済み）を許可する。対象実体化（lower_call_args → CallArg::Handle、1232 行）は任意のヒープ呼び出しを扱うため、effect 呼び出しは FsRead cap を畳み込んで要素ごとの対象として低水準化される。これが健全で局所的な機構であり、5 回の持ち上げ試行は遠回りだった。ハンドラー拡張であって木の書き換えでも Coq でもない。lowering-walls=2。

<a id="dojo--final-path-confirmation-result-is-listrunresult-record-so-it-is-the-recordvalue-path"></a>

### dojo — 最終経路確認：結果は List[RunResult]（レコード）なので、レコード／Value 経路
`parse_task_md` は `RunResult`（レコード、dojo types.almd）を返すので、backfill_dir の `filter_map` は List[String] でなく List[record] を構築する。したがって修正箇所は append_variant_match_to_str_acc（String アキュムレータ経路）でなく、レコード／Value 要素 filter_map ハンドラー（lower_defunc_list_hof_inner のヒープ要素リスト経路／要素ごとの variant-match 本体低水準化）。同じ拡張をそこへ適用：セルフホスト Result 呼び出し対象 + Ok/Err アーム + 逆のタグ（Ok=tag==0）を許可し、effect 呼び出し対象を要素ごとに実体化（FsRead cap を畳み込み）、Ok では Ok ペイロードから構築したレコードを保持、Err ではスキップする。完全な dojo 新セッション計画：(1) レコード／Value 要素 filter_map の variant-match 本体ハンドラーを探す、(2) Result/Ok-Err + Result 呼び出し対象 + 逆のタグへ拡張（既存 Option/Some-None ロジックに倣う）、(3) Result-ok/err → List[record] filter_map fixture + corpus-wall + oracle でバイト一致。これが健全で局所的な機構（持ち上げ 5 試行は遠回り）。lowering-walls=2。

<a id="-dojo-filter_map-wall-solved-workflow-commit-e44637b0--the-hoist-was-wrong-write-cursor-keepskip-was-right"></a>

### ⭐ dojo filter_map の壁を解決（Workflow、コミット e44637b0）— 持ち上げは誤り、書き込みカーソルで保持／スキップが正解
持ち上げ 5 試行は誤った機構だった。実際の修正（dojo-record-variant-handler Workflow が発見）：`try_lower_defunc_list_hof` には結果が List[heap-non-String]（レコード／Value）の filter_map 用アームがなかった。`result_filter_map_heap` → `lower_defunc_filter_map_hof` を追加。書き込みカーソルを持つ結果リスト（`filter` 同様）が、Ok/Some アームで構築した所有要素（lower_heap_result_arm 証明書 `i` + Consume `m`）を保持し、Err/None アームをスキップする。len(xs) で確保し、ループ後に長さをカーソルに合わせて修正する。証明済みキャプチャ付き filter の条件付き取得（5a0a9efb）であり、持ち上げも木の書き換えもない。検証：corpus-wall 全 ACCEPT（ownership 19338 / caps 3432）、parity 124/124、oracle backend-split 0、scalar+str-Result → List[record] がバイト一致。backfill_dir の filter_map の壁は解消。上の持ち上げ記録は置換済みで、defunc ハンドラー拡張（脱糖でなく新しい結合子アーム）が答えだった。まさに 4 回目後に挙げた「defunc 本体低水準化を拡張」の別案である。

<a id="dojo-backfill_dir--now-walls-on-fswrite-the-last-dojo-wall-a-new-capability-effectful-brick"></a>

### dojo backfill_dir — 今は fs.write で壁（最後の dojo の壁、新 capability の effect 付き機構）
filter_map を解消すると backfill_dir は `fs.write(path, content)` で壁：「effectful/impure stdlib Module call fs.write needs a declared capability not in this brick」。ずっと filter_map の壁に隠れていた。fs.write は is_admitted_effectful（calls.rs:216、random.int/env.args/fs.read_text/fs.list_dir を列挙）にない。修正は ReadDir 機構に倣うが、FsRead でなく新 capability FsWrite が必要：(1) is_admitted_effectful に fs.write を許可、(2) セルフホスト fs_write.almd → prim.write_text_file、(3) Capability::FsWrite を持つ Op::Prim::WriteTextFile（lib.rs）、(4) prim.write_text_file の低水準化（calls_p4.rs、read_text_file:880 に倣う）、(5) v1 wasm runtime 関数（path_open O_CREAT|O_WRONLY + fd_write）、(6) Rust enum と Coq Capability レジストリ（単一ソース、#35）両方の Capability::FsWrite + cap_witness + caps ゲート。新 capability + Coq レジストリの単一ソースが健全性上重要。これは effectful-27／WASI-write の下限（#61 群）で、本当の実装単位、dojo と 0 の間に残る最後のもの。

<a id="-final-frontier-2026-06-29-real-lowering-walls--3-all-porta--the-genuine-targeted-irreducible-floor"></a>

### 🎯 最終未解決領域（2026-06-29）：本当の低水準化の壁 = 3（すべて porta）— 対象限定では削減できない真の下限
このセッションで org の本当の低水準化の壁を約 89（分類前、混同あり）から **3** へ減らした。native-FFI（porta wasm_rt @extern + sqlite + process/http）を明確に分離（44 除外）し、17 リポジトリ中 16 が wall=0。解消したすべての壁は全ゲートを通過（corpus-wall 全 ACCEPT + Coq PROOF SPINE OK + 3 方式 oracle backend-split 0 + parity 124/124 + バイト／リーク検査）。新しい effect 付き WASI prim 4 個を Coq 成功状態で導入（FsWrite=4、Clock=5、Stdin=6、PathExists + fs.mkdir_p/remove_all は FsRead/FsWrite を再利用）。HOLE-1（ゲートに見えない record-Ok の平坦 drop リーク）を証明し、恒久 wasmtime 回帰テストとともに解消した。

残る **3 個は対象限定では削減できない深い未解決領域**（各々、対抗的な設計 + 批評で「生成側のみ／低水準化のみの健全な修正経路なし」と繰り返し判定。本当の研究が必要で、実体化器アームではない）：
1. **config.almd::load_porta_config** — A2 非対称合流 + 脱関数化器が壁にする **6 個以上のヒープをキャプチャする list.map/filter_map クロージャ**。クロージャキャプチャ脱関数化（直接呼び出しを維持する環境レコードリフト。一級関数値ではない。caps 証明を破るため）が必要。C2 の領域で深い。
2. **ops.almd::list_instances** — **ループ内 effect モナド**。`for d in dirs` 本体内の `fs.read_text()!` で、err はループを終了し、関数から err を返す必要がある（ループ持ち越し早期終了）。#76（値位置の effect !）とループの未解決領域の組み合わせ。深い。
3. **jsonrpc.almd::read_message** — #77/#78 **ループ持ち越しアキュムレータ生成側**。TCO-while の String／バッファアキュムレータで、証明済み無条件正味 0 ループスロット証明書へ還元できない形（OwnershipLoop.v は導入済みだが、この特定バッファ形の生成側低水準化に健全な生成側限定経路がない）。

これが対象限定のアプローチの正直な終点。解消には複数セッションの研究（クロージャキャプチャ脱関数化／ループ内 effect モナドの早期終了／read_message ループ生成側証明書）が必要で成功は不確か。いずれにせよ native-FFI のため文字どおりの全リポジトリ 0 は構造的に不可能。「本当の低水準化の壁 = 3」（すべて porta、すべて特徴付け済み）が達成した下限。

<a id="refined-2026-06-29-the-3-floor-walls--2-genuine-research-themes-all-no_sound_path-empirically-proven"></a>

### 精密化（2026-06-29）：下限の壁 3 個 = 本当の研究テーマ 2 個（すべて NO_SOUND_PATH、実験で証明）
専用の調査 Workflow 三つが、残る porta の壁 3 個に健全な対象限定／統合経路がないと確認。研究テーマ二つへ還元される。

A. **ループ内 effect モナド（非局所制御）** — 壁 2 個、ともに実験的に NO_SOUND_PATH：
   - jsonrpc.almd::read_message — TCO が合成する Result 全体のアキュムレータ（rv=ok(...)）を、複数の条件付き早期終了の基底経路で再代入し、本体途中の effect ! 伝播も伴う。証明済み正味 0 追加スロットではない（OwnershipLoop.v は無条件 buf=buf+chunk を対象とする。こちらは条件付きで一度書かれる Result acc + effect モナド早期終了）。
   - ops.almd::list_instances — `for d in dirs` 本体内の `fs.read_text()!` で、Err がループを終了し関数から err を返す必要がある。
   両方とも「ループを通り抜ける effect ! 早期リターン」の健全な低水準化が必要。Option[err] を持ち越し／err で break + 伝播し、反復ごとにヒープを drop するループである。v1 基盤には非局所 Return がなく、新しいループ内 effect モナド脱糖とその所有権証明書が必要。本当の複数セッション研究（#76 x loop）。

B. **クロージャキャプチャ脱関数化（C2）** — 壁 1 個：
   - config.almd::load_porta_config — 脱関数化器が壁にする、ヒープをキャプチャする list.map/filter_map クロージャ 6 個以上。直接呼び出しを維持する環境レコードリフトが必要（一級関数値は caps 証明を破るため不可）。

結論：本当の低水準化の壁 = 3 は、対象限定／統合アプローチについて証明された下限（専用 Workflow 6 個：loop-cert producer、CondLoop-extract、conditional-acquire-5、list_instances、read_message はすべて NO_SOUND_PATH、または対抗的批評で過剰設計と判明）。3 個の解消は (A) ループ内 effect モナド、(B) クロージャキャプチャ脱関数化の専用複数セッション研究計画で、成功は不確か。native-FFI（44）があるため、いずれにせよ文字どおりの全リポジトリ 0 は構造的に不可能。

<a id="definitive-2026-06-29-real3-is-the-proven-research-floor--the-path-to-0-is-a-multi-component-research-program"></a>

### 確定（2026-06-29）：real=3 は証明済み研究下限 — 0 への道は複数構成要素の研究計画
テーマ A（ループ内 effect モナド）の設計は対抗的に反証された。read_message と list_instances はともに反復ごとの第 3 の経路（skip/continue、所有権形 []）を持つので、ループスロットは本質的に条件付き（CondLoop：then=[Dec;Inc] / else=[]）であり、単一本体 CLoop ではない。これに CLoop 証明書を使うと、受理するがモデル化しない（check_line_unroll_sound の UnrollsL は同一本体の n コピーのみ）= 不健全。無条件の包み直しへ縮約すると Err 後にも effect を再実行し、parity を破る。したがって 2 個の解消に本当に必要なのは：(1) OwnershipFilter.v CondLoop を checker.ml へ抽出（ccheck_unroll_sound は証明済みだが現在抽出されるのは CLoop のみ）、(2) 反復ごとの複数スロット（status + 結果アキュムレータ）+ 一時 f(x) wrapper／ムーブアウトされた Err ペイロードを別オブジェクトとしてモデル化し、break 経路の drop を解消すること。これは本当の複数構成要素の研究計画で、単一の実装単位ではない。

WALL=0 への道（専用の複数セッション研究計画、成功は不確か）：
- CondLoop の checker.ml 抽出（Coq で OwnershipFilter CondLoop → OwnershipChecker.v へ移植 → checker.ml + parse_clc 再生成）。健全性上重要、信頼カーネルに触れる。
- CondLoop 上のループ内 effect モナド脱糖（ループ持ち越し Result status + 条件付き skip/continue + break 経路 drop）。read_message + list_instances を解消。
- クロージャキャプチャ脱関数化（直接呼び出しを維持する環境レコードリフト、一級関数値にはしない）。load_porta_config を解消。

対象限定／統合の証拠を使い尽くした（このセッション）：loop-cert producer = NO_SOUND_PATH、CondLoop-extract = serialize_opts には過剰設計（批評）だが条件付きループスロットには本当に必要、A2 conditional-acquire-5 = 取り組みやすい 3 個解消（fs.exists）+ A2-hard 2 個、list_instances + read_message の調査 = NO_SOUND_PATH、テーマ A 設計 = 反証（CondLoop + 複数スロットが必要）。本当の低水準化の壁 = 3 が下限。native-FFI（44）により、いずれにせよ文字どおりの全リポジトリ 0 は構造的に不可能。

<a id="step-2-refutation-2026-06-29-the-decisive-blocker-is-step-15--verify_ownership-does-not-model-intra-iteration-control-flow"></a>

### 段階 2 の反証（2026-06-29）：決定的な阻害要因は段階 1.5 — verify_ownership は反復内の制御フローをモデル化しない
ループ内 effect モナド脱糖（段階 2）は、決定的な発見（H-A）によって対抗的に反証された。実行可能な鏡像 verify_ownership（lib.rs:1117–1119）と証明書生成器（certificate.rs:127）は、LoopBreakUnless / IfThen / Else を何もしないものとして、ループ本体を単一の平坦な反復として走査する。break が反復の残りを飛ばすことも、if アーム間の rc 保存／復元もモデル化しない。結果：
- !-break より前に取得し、break 後の continue 経路でだけ解放する一時値は、鏡像と Coq 証明書の両方で均衡と数えられるのに、break 反復ではリークする。break 経路の drop を忘れる脱糖が不健全に受理される（鏡像は忠実性を確認できず、今日の H3 は解消不能）。
- 逆に両 if アームで drop する一時値は平坦走査で 2 回減算され、偽の DoubleFree となる。
CCondLoop カーネル証明書（段階 1、導入済み）は、単一の持ち越し status/rv スロットの rc 差分だけを証明する。支配的なリーク面（複数の !-Err アームにまたがって drop される反復ごとの一時値、TCO 後方辺 drop、Result からのムーブアウトの同一性）は証明書外であり、現在の鏡像が見える範囲の外でもある。

精密化した wall=0 への研究経路（各段階は本物かつ健全性上重要、成功は不確か）：
- 段階 1：CondLoop 抽出 — 完了（コミット 19474461、タスク #78）。
- 段階 1.5：verify_ownership + 証明書生成器を拡張して反復内制御フローをモデル化（LoopBreakUnless は反復の残りをスキップ、IfThen/Else はアームごとに rc/dead を保存 + 復元）。信頼カーネルの実行可能鏡像への根本的変更で、全 4517 関数の corpus-wall 全 ACCEPT を維持する必要がある。決定的な阻害要因。
- 段階 2：ループ内 effect モナド脱糖（1.5 が必要）→ read_message + list_instances。
- 段階 3：クロージャキャプチャ脱関数化 → load_porta_config。
本当の低水準化の壁 = 3 は維持。段階 1.5（根本的検証器変更）導入まではこれが下限。

<a id="absolute-floor-2026-06-29-the-effect-monad-in-loop-walls-require-a-foundational-effect-fn-abi-redesign-not-a-research-brick"></a>

### 絶対下限（2026-06-29）：ループ内 effect モナドの壁は基礎的な effect 関数 ABI 再設計が必要（研究の一実装単位ではない）
段階 1.5+2 の統合設計は、最も深く決定的な発見で反証された。read_message + list_instances の解消は「深い研究」ではなく、意図的な v1 設計判断の変更 + 複数の証明拡張が必要：
1. **v1 MIR はユーザー effect 関数から裸の値を返し、runtime のタグ付き Result で戻り値を包まない**（lib.rs lower_body_with_globals、417–431 行の ⛔ 確定注記）。両方の壁は本当にエラーを返す（Err 経路のバイト一致が必要）ので never-err 除去（b154a270）は適用できない。runtime Result タグなしの err-break／伝播辺は無言の誤コンパイル、つまり撤回済み（293–305 行）の一律除去の②の落とし穴そのもの。したがってループ内 effect モナド脱糖には、v1 が意図的に省いた runtime effect 関数 Result-tag ABI が必要。
2. **CCondLoop（段階 1）が証明するのは、二つの平坦な正味 0 分岐に対する continue 専用ループだけ**（`list FlatOp`、「入れ子ループなし」）。脱糖の `match status { ok => match f(x){ok,err} }` は then 分岐に別の条件分岐を含む CondLoop で、CCondLoop では表現不能（Coq の入れ子条件分岐への一般化が必要）。また err-break の終了辺（一部の live 集合を残して反復途中で出る）には Coq 側の対応がなく、事実上の新公理となる。
3. 強化した鏡像（if アーム間の保存／復元）にはスタック + rc マップの正規化が必要で、なければ均衡する corpus 関数を誤拒否する（4517 ゲート違反の危険）。

結論：本当の低水準化の壁 = 3 はこのコードベースの現設計における絶対下限。0 への道は基礎的な再設計（runtime effect 関数 Result-tag ABI + CondLoop の入れ子条件分岐 + break-exit の Coq 定理 + スタック式鏡像）で、意図的な v1 設計判断に触れる v2 規模の計画であり、自律的な研究の一実装単位ではない。実施判断は自律ループでなく利用者のもの。native-FFI（44）により、いずれにせよ文字どおりの全リポジトリ 0 は構造的に不可能。本物の進捗として導入した CondLoop 抽出（#78、条件付きループスロットのカーネル基礎）は、最初の再利用可能部品として残る。

<a id="recursiontco-gate-sidestep-also-refuted-2026-06-30--real3-confirmed-across-all-angles"></a>

### 再帰／TCO ゲートによる迂回も反証（2026-06-30）— 試した全角度から real=3 を確認
創造的な自己批評の角度（ループを完全に避ける。try_tco_rewrite をゲートし、本体途中に effect ! のある effect 関数を、証明済み末尾 effect-unwrap 経由の通常再帰として低水準化する、Almide のループでなく再帰という慣用法）を実 porta ソースで試した（/Users/o6lvl4/workspace/github.com/almide/porta に存在を確認。以前の Workflow は worktree+cache しか調べず存在しないと誤報）。結果：再構成した最小の再帰 effect ! fixture は正常に低水準化（Ok+Err バイト一致）するが、実 read_message（jsonrpc.almd:39–65、複数 ok(...) リターンの入れ子 if/else + `else read_message()` 末尾再帰 + 本体途中の parse_and_wrap(body)!）はその形へ還元できず、TCO ゲートを付けても「while body with heap-accumulator reassignment」で壁となる。list_instances の再帰書き換えは MIR ACCEPT だがバイト一致に失敗。再帰迂回も実関数で反証された。real=3 は今や試したすべての角度で下限と確認された：対象限定低水準化（使い尽くした）、CondLoop 抽出（完了/#78）、ループ内 effect モナド脱糖（反証、基礎的な effect 関数 Result-tag ABI）、再帰／TCO ゲート迂回（反証、実 read_message の入れ子構造はきれいな再帰へ還元しにくい）。wall=0 の道は依然 v2 規模の計画（effect 関数 Result-tag ABI + CondLoop 入れ子条件／break Coq + スタック鏡像、または実際の入れ子形に対するより深い再帰低水準化）で、自律的な一実装単位ではない。

<a id="load_porta_config-true-root-2026-06-30-direct-main-agent-diagnosis--corrects-3-workflow-misdiagnoses"></a>

### load_porta_config の真の根本原因（2026-06-30、主エージェントの直接診断 — Workflow の誤診 3 件を訂正）
Workflow 三つが load_porta_config を誤診した（a/ クロージャキャプチャだが誤った箇所を修正、b/「filter_map クロージャ戻り値の heap-match」、c/「複数ヒープフィールドの record-Ok」）。直接の 5 層追跡 + 3 行の最小再現への二分調査で本当の原因を発見：

```almide
type P = { a: String, b: String }
fn f(cap: String, keys: List[String]) -> List[P] =
  keys |> list.map((k) => { let e: P = {a: k, b: cap}; e })   // 🔴 WALL "list.map unliftable/closure-list"
```

コード内コメントの訳：🔴「list.map unliftable/closure-list」で壁となる。
正確な誘因（実験で切り分け）：**自由変数をキャプチャし、かつレコード要素を生成する** `list.map`/`filter_map` ラムダ。切り分けで証明：キャプチャ→スカラー要素は動作（#67）、キャプチャ→json-scalar は動作、非キャプチャ→レコード要素は動作（lift_lambda が救済）。壁となるのはキャプチャ + レコード要素だけ。load_porta_config では `env_vars = env_keys |> list.map((k) => { let val=json.get_string(env_obj,k)??""; let
e: dispatch.EnvVar={key:k,val:val}; e })`（env_obj をキャプチャしレコードを構築）。壁が末尾の「heap-result match」へ伝播する（effect ! 脱糖の Ok アーム Block がこの文を低水準化できない）。

理由：C1 defunc インライン特殊化器（value_of でキャプチャを解決）が、この位置のレコード要素 map をインライン経路へ送らない。`lift_lambda`（binds.rs:38）へ落ち、これはキャプチャ付きラムダをすべて拒否する（free_vars が非空）。一級 FuncRef は環境を持てない。非キャプチャの record map は lift_lambda が救済するが、キャプチャ付きにはリフト形がなく壁になる。注意：低水準化は複数経路（末尾／束縛／値位置で list.map の経路が異なる。末尾位置のレコード要素 map は `lower_heap_result_arm` 経路でないと計装で確認）なので、修正は単純でない。

修正（箇所特定、未実装）：キャプチャ付きレコード要素 `list.map`/`filter_map` を三つすべての位置で、lift_lambda へ落とさず C1 defunc インライン経路へ送る。キャプチャは value_of で解決、レコード要素は反復ごとに構築して結果スロットへムーブ、証明済み書き込みカーソル／結果リスト再帰 drop を使用。取り組めるが複雑（defunc の複数経路 + 健全な反復ごとのレコード所有権 + HOLE-1 再帰 drop + 全ゲート）。これが load_porta_config の壁（3 個中 1 個）。read_message/list_instances はループ内 effect モナドの組のまま。

<a id="load_porta_config--half-landed-2026-06-30-direct-main-agent-implementation"></a>

### load_porta_config — 半分導入（2026-06-30、主エージェントの直接実装）
真の原因（キャプチャ付き list.map/filter_map → レコード要素が defunc インライン化されない）は今一部修正：
- ✅ **list.map / list.filter のキャプチャ付きレコード要素** — 導入済み（コミット bcf82dd7）。生成した `$__drop_<R>` を持つレコード結果要素を許可し、再帰的 `$__drop_list_<R>` を登録（平坦な DropListStr ではない。HOLE-1 リークになるため）。検証：native==wasm バイト一致、corpus-wall 全 ACCEPT（ownership +2）、parity 124/124、再帰 drop 出力（有効 wat）。load_porta_config の `env_vars` map を解消。
- ⏳ **list.filter_map のキャプチャ付きレコード要素**（load_porta_config の `secrets`）— 残る。より深い：filter_map 要素経路（`append_variant_match_to_result_list`）は呼び出し対象だけを追跡するが、secrets は `let val = json.get_string(sec_obj,k); match val {…}`（対象が let 束縛 Var の Block）。必要：(a) 対象置換（`{let v=E; match v}` で v∉arms → `match E`）により対象を再び呼び出しにする、(b) Block の保持アーム（`some(v) => { let e=…; some(e) }`）、(c) none アームの入れ子 `let obj=json.get(…); if
  json.get_bool(…) then some({…process.env(k)…}) else none`（effect 付き process.env キャプチャ）。多層。
したがって load_porta_config = map 側半分完了 + filter_map 側半分残り。一つの関数、二つの HOF なので両方導入まで real は 3。read_message/list_instances はループ内 effect モナドの組のまま。

<a id="sharpened-2026-06-30-all-3-remaining-walls-converge-on-the-effect-monad-in-hofloop-frontier"></a>

### 精密化（2026-06-30）：残る 3 個の壁はすべて HOF／ループ内 effect モナドの未解決領域に収束
直接実装で load_porta_config の map 側半分を導入した（bcf82dd7、健全）が、filter_map 側（`secrets`）を深く調べると、明快な 4 層拡張でなく真に複数の未解決領域にまたがると分かる：
`sec_keys |> list.filter_map((k) => { let val = json.get_string(sec_obj,k); match val {
  some(v) => some({key:k, val:v}),                                    // capturing + record keep
  none => { let obj=json.get(sec_obj,k)??null; let from_env=json.get_bool(obj,"from-env")??false;
            if from_env then { let env_val = process.env(k) ?? ""; some({key:k, val:env_val}) }  // EFFECT in arm
            else none } } })`                                          // none アーム自体が条件付き保持
コード内コメントの訳：some はキャプチャ + レコード保持、from_env 分岐内には effect がある。none アーム自体が条件付き保持となる。
したがって none アームは単なるスキップでなく条件付き保持で、effect 呼び出し（process.env）も含む。これは read_message/list_instances（ループ内 effect !）と同じ HOF アーム内 effect ! の未解決領域。結論：残る 3 個（load_porta_config secrets / read_message / list_instances）は一つの未解決領域、「HOF アームまたはループ本体へ引き回す effect !／条件付き取得と、それを越える早期リターン」へ収束する。v1 は延期された effect 関数 Result-tag ABI + 条件付きループ／HOF 所有権証明書なしでこれを表せない。real=3 は本当の下限であり、全 3 個を解放する単一の深い研究テーマは HOF／ループ内 effect モナド低水準化（+ その CondLoop/CondHOF 証明書）。キャプチャ付きレコード map/filter 機能（bcf82dd7）は健全で導入済み、load_porta_config の本当の前提部品である。

<a id="2026-06-30--list_instances-cleared-real-32-effect--in-for-loop-no-certcoq-change-d3853d52"></a>

## 2026-06-30 — list_instances 解消（real 3→2）。`for` ループ内 effect `!`、証明書／Coq 変更なし（d3853d52）

「ループ本体内の effect `!` を越える早期リターン」の下位ケースに、延期された Result-tag ABI も新しい証明書も不要だった。これは既存の証明済みループスロットへの純粋な IR→IR 脱糖（`desugar_loop_unwrap`、mod_p6.rs）である：

  `for x in xs { … e! … }; <post>`  →  `var __ef=false; var __ev=""; for x in xs { if not __ef then { match e
  { ok(v)=>…, err($x)=>{__ef=true; __ev=$x ++ ""} } } else () }; if __ef then err(__ev) else { <post> }`

早期リターンとバイト同一（`__ef` が設定されると反復ごとのガードが残りをスキップし、`xs` を使い尽くしてループ終了。**`for` 専用**で、`while` は除外する。進捗更新を飛ばすと非停止になるため）。

**健全性の重要な洞察（再利用可能）：** err アキュムレータは**所有コピー**（`$x ++ ""`、新規 String）を格納する必要がある。借用 match ペイロードを格納してループ後にムーブアウトすると、不健全な `idm`（初期化／drop／死んだ参照をムーブ）という証明書になり、corpus-wall は拒否する。本当の二重解放である。`++ ""` はスロット所有の新規 String を割り当てるため、ループ持ち越しスロットの証明書は証明済み `i(id)m`（全 4517 で corpus-wall ownership ACCEPT）。`String` エラーに限定するゲート（全 porta の壁を対象にできる）。Fixture：spec/wasm_cross/effect_unwrap_in_loop.almd（C-119）。全ゲート成功、Ok 経路（完了）と Err 経路（短絡）の両方で native==wasm。

<a id="refined-next-steps-for-the-remaining-2-each-a-separate-deep-cycle-but-the-owned-copy-insight-transfers"></a>

### 残る 2 個の次の手順を精密化（各々別の深いサイクルだが、所有コピーの洞察は転用できる）

- **read_message**（jsonrpc.almd）— **2026-06-30 に根本原因を再特定（TCO ではなく、`ok(some(record))` も不足ではない。既に低水準化できる）。** 真の原因は **`ok(<Option-typed Var>)`** と **`ok(some(<String>))`** の形に対する `Result[Option[heap], String]` コンストラクターの不足。`read_message` の基底は `ok(r)` で、`r = parse_and_wrap(body)!` は所有された `Option[JsonRpcRequest]` ローカル、つまり `ok(<Option Var>)`。tail.rs の heap-result 構築連鎖（`try_lower_result_record_ctor` / `_value_ctor` / `_str_int_ctor` / …）に Option ペイロード構築器がなく、`alloc_init → Opaque → bail` へ落ちる（tail.rs:611「heap-result ResultOk cannot be faithfully returned」）。最小再現（scratchpad）、classify_corpus で検証：
  - `okrec.almd` の `ok(some({id,method}))`（レコードリテラルの Some）→ **低水準化可能**（record-Option 経路が存在。`parse_and_wrap` でなく `read_message` だけが壁となる理由）。
  - `okvar.almd` の `fn pass(o: Option[String]) = ok(o)` → **壁**「ResultOk cannot be faithfully returned」。read_message が当たる正確な `ok(<Option Var>)` 形。
  - `okstr.almd` の `ok(some("hi"))` → **壁**（Option[String] Some）。
  （以前の `wrap`/`scan3` の「while body」という理由は、この構築器不足に再帰が TCO→while フォールバックを重ねていたため。根は構築器不足。）修正経路：`Result[Option[heap],
  String]` 用 `try_lower_result_option_ctor` を追加し、`ok(<Option Var>)`（所有 Option ブロックを Dup して入れる）、`ok(some(x))`、`ok(none)`、`err(s)` を扱う。`try_lower_result_record_ctor`+`materialize_result_aggregate` に倣う。drop は**新 Op も Coq 変更もなしで実現可能と確認**（2026-06-30）：`resrec:opt_<R>` 経由で既存 `Op::DropWrapperRec` に Ok-drop を送る（証明書は `drop_fn` にかかわらず一様。certificate.rs:76/679 は名前を無視するので証明済み検査器は変更 0）。新 Almide drop ヘルパー `fn __drop_opt_<R>(e: Option[R]) ->
  Unit = { match e { some(r) => (), none => () } }` を生成する（`match` が `e` を consume して Option ブロックを解放し、`some` アームが `$__drop_<R>` で `r` を drop）。ヘルパー唯一の依存である `match Option[record] { some(r)
  => (), none => () }` の低水準化は検証済み（scratchpad `dropopt.almd`、壁 0）。深い Op/Coq 変更ではなく、(1) `try_lower_result_option_ctor`（低水準化側）、(2) 使用時のみの発見 `collect_result_option_records(&ir)`（`collect_recursive_anon_records` に倣う）から、(3) `generate_record_drop_sources` 内の `$__drop_opt_<R>` 生成ループへ供給し、render 呼び出し位置まで引き回す。使用分だけ生成する（全再帰レコードではない。低水準化できないヘルパーはプログラム全体を壊すため。発見処理は実際に `Result[Option[R],String]` で包む R に限定する）。`Result[Option[String],String]` はより簡単な同系統（内部 Option[String] は 0 または 1 要素の `DropListStr` ブロック）。ゲートの最後の守り：誤 drop → corpus-wall REJECT。okvar/okrec/okstr/dropopt は決定的 classify_corpus fixture。端から端までのバイト試験には stdin が必要。残りは構築器 + 発見／生成の接続 + ゲートという集中したサイクルで、もはや信頼基盤の深さの阻害要因ではない。

  **実験更新（2026-06-30 に試作後撤回）：read_message は多層で、構築器は必要だが十分ではない。** `try_lower_result_option_ctor` の試作で非再帰ケースの低水準化を確認（`okrecvar.almd` の `fn pass(o: Option[Req]) = ok(o)` → wall=0、FORBIDDEN=0）。構築器 + 検証済み `$__drop_opt_<R>` ヘルパーは本当の健全な機能。しかし read_message 自体は同じ「while body heap-accumulator」で壁のまま。`ok(r)` が `else read_message()` の再試行再帰の入れ子 if アーム内にあり、末尾／アーム構築器でなく TCO（`tco_rewrite`）を通るため。read_message は第 1 層（`ok(<Option-Var>)` 構築器。試作で実現可能だが不完全として撤回。まだ `$__drop_opt_<R>` 生成の接続が必要で、なければ宙に浮く呼び出しを描画）+ 第 2 層（ループ本体ローカルを参照する再帰基底について TCO が `Result[Option[record]]` 結果を構築する必要。本当に深く、三つの捉え方でも解けなかった層）。半端に接続した不正 wasm 状態を避けて構築器は撤回（未コミット）。まず生成とともに再実装し、第 2 段階として TCO Option 結果基底に取り組む。残る 2 対象では load_porta_config の defunc-filter_map のほうが先に試すには明快。

  **層の積み重ね（2026-06-30、完全な試作で検証後、正常なツリーへ撤回）。** read_message は最も深い porta 関数で、壁は相互作用する 5 層以上に連鎖する。試作で確認：
  (a) `ok(<Option-Var>)` 構築器は非再帰ケースを低水準化（okrecvar wall=0）。
  (b) `Result[Option[<nested-heap>], String]` の base-reads-loop-local に TCO を辞退させる（mod_p5 約 :944、Option[String] は除外）と、壁が「while body heap-accumulator」→「heap-result `if`」へ変わる。つまり TCO から本当の再帰 heap-result-`if` 経路へ正しく落ちる。
  (c) 構築器を tail.rs と `lower_heap_result_arm`（control_p4）の両方へ接続しても「heap-result `if`」の壁。read_message のアームが `let r = parse_and_wrap(body)!`、すなわち **heap-result-`if` アーム内の effect `!`**（第 5 層）を含むため。これはループ `!` 機構（Unit の `for` 本体）とは異なり、Result[Option] を返す if 内でまだ低水準化できない。
  全 read_message 解消には構築器 + TCO 辞退 + アーム接続 + `$__drop_opt_<R>` 生成 + heap-result-`if` アーム内 effect `!` の低水準化という、協調した多層サイクルが必要。試作編集はすべて撤回（構築器は生成なしでは描画不完全、単独ではどれも壁を解消しない）。推奨：read_message（5 層）より先に load_porta_config（単一機構 defunc-filter_map）を行う。

  **load_porta_config 解消 2026-06-30（bc37cbe0）：本当の低水準化の壁 2 → 1。** 単一機構は予測どおりだった。defunc の `emit_filter_map_arm`（control_p5）は裸の `some(elem)` / `none` アーム本体だけを扱い、他の形はすべて壁にしていた。Block アーム本体（先頭の let を低水準化）と `if` アーム本体（同じ書き込みカーソルへの Unit の保持／スキップ。証明済み `append_body_to_str_acc` に倣う）へ再帰するよう拡張して、secrets の `none => { let b = …; if b then some(rec) else none }` 形を低水準化した。新 MIR 演算なし、証明書／Coq 変更なし（4520 に対し corpus-wall ACCEPT）。Fixture C-120（filter_map_conditional_arm.almd。porta の `json.get_string` 対象は、render_program のレジストリ経由で描画されるようユーザー関数で代用）。**read_message が唯一残る porta の壁**（上記 5 層）で、porta と wall=0 の間に残る唯一のもの。

  **次サイクルの read_message の手がかり（第 3 層の簡略化）。** read_message の難しいアームは `{ let r = parse_and_wrap(body)!; ok(r) }`。しかし `let r = e!; ok(r)` は `e` 上の恒等（Ok(x) → x → Ok(x)、Err(s) → Err(s) 伝播）、つまり全 Block ≡ `e`。小さな一般脱糖 `{ <pure-prefix>; let r = e!; ok(r) }` → `{ <pure-prefix>; e }`（unwrap 結果を変更せず Block 末尾で包み直すことを条件にする）が、第 3 層（アーム内 effect `!`）を裸の末尾呼び出しアーム `parse_and_wrap(body)` へ縮約する。本当の再帰 heap-result-`if` アーム経路はそれを `CallFn`+`Consume` として既に低水準化する（これらのアームには Option 構築器も `$__drop_opt` も不要。呼び出し結果を構築せず伝播するため）。脱糖後に残るものは `ok(none)` アーム（Option 構築器 + `$__drop_opt_<R>` 生成が必要、04caf8e0 で試作済み）と TCO 辞退（mod_p5 約 :944、791807ae で試作済み）。この小さな三部品を組み合わせれば read_message を解消するはず。脱糖の健全性を corpus-wall で検証する（早期リターンする effect `!` を除去するため、他の live ヒープを導入する文を越えてはならず、純粋な前置部分に限定する）。
- **load_porta_config**（config.almd）— secrets の `filter_map`：`match`（some アーム = record、条件付き none アーム `if from_env then some(record) else none`）+ `process.env` でレコードを生成するキャプチャ付きラムダ。これは defunc-`filter_map` 機構（control_p5 `lower_defunc_filter_map_hof`）でありループ脱糖ではない。別の深いサイクル（Block 保持アーム + 条件付き none アーム + effect 付き none アーム）。

<a id="2026-06-30--read_message-core-landed-06664cd8-8th-feature-is-the-sole-remaining-blocker"></a>

## 2026-06-30 — read_message の核心導入（06664cd8）、第 8 機能が唯一残る阻害要因

read_message の基礎はコミット済み、ゲート正常（4523 に対し corpus-wall ACCEPT、corpus 関数 +3 個が低水準化可能、output-parity 126/126、proof-spine OK）。unwrap→再包装の恒等脱糖（`desugar_unwrap_rewrap_identity`、`{…; let r=e!; ok(r)}` ≡ `{…; e}`）、`Result[Option[record]]` 構築器（`try_lower_result_option_ctor` + `lower_option_piece` + アーム／末尾接続 + `is_option_record_result_ty`）、Option の入れ子ヒープでの TCO 辞退（mod_p5 約 :944）、過剰生成の `$__drop_opt_<R>` ヘルパー（`generate_record_drop_sources`、後で発見に基づくものへ限定）。最小再現（scratchpad rdcore/rdcore2/rdcore3*）で検証済み：read_message の核心は低水準化できる（入れ子 `if` + `ok(none)` + 恒等脱糖した `ok(parse_w(b)!)` 呼び出しアーム + 引数なし `io.read_line` の再試行再帰 → wall=0）。

**唯一残る阻害要因（rdcore3c）：入れ子 heap-result-`if` アームの深い位置（深さ ≥3）で束縛した effect 呼び出しの heap-`let`（`let body_bytes = io.read_n_bytes(len)`）が「heap-result `if`」で壁**。しかし同じ `io.read_n_bytes` heap-`let` は関数最上位（iob_top）と深さ 1 のアーム（iob_arm）で低水準化でき、深さ 3 の二つの連続した純粋 heap-`let` も低水準化できる（rdcore3d）。したがって (effect 呼び出しのヒープ結果) × (深いアーム入れ子) の限定された相互作用、第 8 で最後の read_message 機能。次：heap-result-`if` アームの Block 低水準化における、深い位置の effect 呼び出し束縛ヒープローカルの扱いを突破 → porta wall=0。read_message は難しい機能約 8 個を重ねる（最難の porta 関数）。このターンで 7 個を解決・コミットした。

<a id="definitive-root-of-the-8th-blocker-it-is-not-lowering--it-is-a-missing-runtime-primitive"></a>

### 第 8 の阻害要因の確定根本原因（低水準化でなく runtime プリミティブの不足）

最小まで二分（scratchpad iob_arm7）：関数最上位の `let b = io.read_n_bytes(n)` が **「effectful/impure stdlib Module call io.read_n_bytes needs a declared capability not in this brick」**（calls.rs:241）で壁となり、heap-result-`if` の壁ではない。以前の「heap-result `if`」メッセージは capability の壁がアームを通って上に伝播したもの。したがって read_message の低水準化は完全に解決（コミット済み 7 機能。iob_arm8 = 同じ `ok(some(record))` に純粋な let を伴うものは wall=0）。唯一残る阻害要因は **io.read_n_bytes に v1 runtime の基礎実装がない**こと。`io.read_line` は許可済み（calls.rs:240 の whitelist + セルフホスト `stdlib/io_read_line.almd` = `prim.read_line()` + `PrimKind::ReadLine` の描画 = Capability::Stdin を持ち String を構築する WASI fd-0 バイトループ）。io.read_n_bytes はその同系統（WASI fd-0 から N バイト読み → `Bytes` ブロック）で、本当に WASI で実装可能なので本当の壁であり native-FFI ではない（指標から外すため native-FFI とするのは指標の不正操作であり禁止）。porta wall=0 を解消するには read_line の同系統として io.read_n_bytes を実装：(1) `io.read_n_bytes` を whitelist に追加（calls.rs:240）、(2) `stdlib/
io_read_n_bytes.almd` = `effect fn io_read_n_bytes(n: Int) -> Bytes = prim.read_n_bytes(n)`、(3) `func == "read_n_bytes"` 分岐（calls_p4 約 :1024）で新 `PrimKind::ReadNBytes`（args [n]）を出力、(4) `PrimKind::ReadNBytes` の描画（fd 0 から N バイトを `Bytes` ブロックへ WASI `fd_read`。本物の runtime 作業、`ReadLine` 描画を適用）、(5) Capability::Stdin の計上（再利用）。これは effectful-WASI-floor 作業（タスク #61）で、解決済み read_message 低水準化とは種類の違う仕事であり、明確な境界。io.read_n_bytes の基礎実装ができれば read_message は低水準化できる（構造は rdcore3* で証明）。
