<a id="v1-heap-result-ifmatch-execution--design-de-risked-no-coq-change"></a>

# v1 ヒープ結果の `if`/`match` 実行 — 設計（リスク低減済み: Coq 変更なし）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-heap-result-control-flow.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-heap-result-control-flow.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-heap-result-control-flow.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-heap-result-control-flow.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した非規範的な読者向け翻訳です。自動同期されず、原本が引き続き権威を持ちます。歴史的な状態・実装・証明・監査・測定の主張は固定した原本の記述であり、ここで独立に実行・検証していません。未完の提案は現在の保証ではありません。コードブロックとコメントは原文どおり、原本メタデータは無効化した表示注記です。

> 原本の説明（無効化したメタデータ）: ヒープ結果制御フローの歴史的な最初の範囲と、その後の lowering の背景。

<a id="historical-first-slice-later-lowering-exists"></a>

## 歴史的な最初の範囲 — その後の lowering は存在する

以下のリテラル arm の結果と「残作業」の一覧は、MIR のヒープ結果制御フローの最初の範囲を記述しています。その後の match と非リテラル arm の作業は [MIR lowering](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-mir/src/lower/) に存在し、[Camp 4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md) に記録されています。この当初の次セッション用手順を、それらの形が今も未実装である証拠として扱わないでください。現在出荷する Wasm 経路は[構造的経路](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)です。当初の証明書に関する論証は、モデル化した範囲内で保持し、成果物全体のメモリ安全性定理に格上げしません。

状態: **リテラル arm の `if` は実装済み（commit 126921e6）、敵対的検証で健全と確認。** `fn label(c) -> String = if c then "yes" else "no"` は現在、v0 とバイト一致して実行します。以下の設計はまさに正しかったと判明しました。Coq 変更はなく、既存の checker が arm ごとの `"im"` 証明書を変更なしで受理します。3 つの独立した敵対的監査で、受理されるが安全でない穴は見つかりませんでした。corpus-wall: ownership 13121→13129、3 性質すべて ACCEPT。

この範囲での残作業: ヒープ結果の `match`（`desugar_match_to_if` による脱糖）と、非リテラル arm（直接呼び出し — 下記の手順 1）。監査で見つかった別の既存バグは**その後修正済み**（render_wasm_p2.rs）です。String/Bytes のブロックは現在 LIST と互換のサイズになっています（`cap = ceil(bytes/ELEM_SIZE)` 要素、割り当て = `LIST_HEADER + cap*ELEM_SIZE` — `$alloc` の再利用検査が再計算するものと完全に同じ）。そのため解放した String ブロックは再利用され、String を繰り返し生成・破棄するループは有界メモリで動きます（render_wasm/tests_part1_b.rs / tests_part2.rs の churn テストで固定）。

<a id="the-problem"></a>

## 問題

ヒープ結果の分岐は arm から String/データを返します。

```almide
fn label(c: Bool) -> String = if c then "yes" else "no"
```

スカラーの `if`/`match`/`while`/`for` は既に、平坦なマーカー `Op::IfThen`/`Else`/`EndIf`（ループでは `LoopStart`/… も使用）を通じて実行されます（commits a2a9f656 / 0a6db87b / 547b5efc）。健全性の論証は**arm ごとの収支均衡**です。各 arm は内部で均衡しているため、cert は平坦なマーカーストリームを、コーパスで実証された線形化とまったく同様に処理します。スカラー arm はヒープを持たないので、自明に均衡します。

ヒープ結果の arm はこれを崩します。各 arm が文字列を `Alloc` し、その値が**escape**（返り値になる）するので、arm 内部で均衡しません。平坦な所有権証明書からは、2 つの arm の割り当てと 1 回の move-out が見えることになります。

```
objA: i          (Alloc "yes")
objB: i          (Alloc "no")
ret  : m  on dst (one move-out)
```

`objA`/`objB` は対応する `m`/`d` のない `i` を受ける → checker が FAULT（リーク）→ REJECT。これが、現在の lowering がヒープ結果 arm を単一の `Alloc{Opaque}` に先送りする理由です（健全ですが、結果は Opaque = 実行不能）。

<a id="the-key-insight--no-coq-checker-change-is-needed"></a>

## 核心 — Coq checker の変更は不要

カーネルで証明された checker は、オブジェクトごとのイベントストリーム（`i`/`a`/`d`/`m`）を畳み込み、各オブジェクトの収支が均衡するかだけを検査します（`certificate.rs::ownership_certificate` がストリームを出力し、Coq checker が再検証します）。既に `"im"`（割り当て後に move-out）を受理します。したがって**コンパイラ**が arm ごとに move-out を出力し、arm ごとの均衡を復元すれば、証明変更ゼロで既存 checker が受理します。

```
objA: im         (Alloc "yes"; Consume objA)
objB: im         (Alloc "no" ; Consume objB)
dst : (none)     dst is the IfThen RESULT, never an Alloc → not in `of`
                 → func.ret = dst emits NO second `m` (no double-move)
```

各 arm は今や `"im"` = 内部で均衡しています。均衡した 2 つの arm を平坦に線形化しても均衡します。既存の arm ごとの均衡による健全性の論証は変更なしで成り立ちます。実行時には wasm の `if` がちょうど 1 つの arm を実行 → 1 回の `Alloc` + return（rc=1、呼び出し元が所有）。選ばれなかった arm は一度も割り当てないので、リークも二重解放もありません。

これにより、この範囲は「Coq checker の変更と再証明」（以前のセッションが考えていたこと）から「コンパイラ側の cert 生成と render、checker は無変更」に格下げされます。はるかに安価ですが、依然として健全性の中核です。

<a id="implementation-plan-scoped-literal--direct-call-arms-first"></a>

## 実装計画（範囲: まずリテラルと直接呼び出しの arm）

1. `lower/control.rs` の `try_lower_heap_result_if(cond, then, else_, ty)`（`try_lower_scalar_if` をなぞる）: `is_heap_ty(ty)`、スカラー lowering 可能な `cond`、**単純な所有ヒープ値の生成元**である arm を条件にします。生成元は `LitStr`（→ `Op::Alloc{Init::Str}`）またはヒープを返す直接 `Call`/`CallFn`（→ 呼び出し op。cert は既にこれを `i` と計上）です。他の arm 種類（ヒープ `Var`、入れ子の分岐、ローカル変数を持つブロック）→ `None` を返す → 現在の健全な Opaque 形式にフォールバックします。`match` 版はスカラー経路とまったく同じく `desugar_match_to_if` で脱糖します。
2. 各 arm: 生成元を `objX` に lower し、`Op::Consume{v: objX}` を追加して `objX` を `live_heap_handles` から削除します（結果として move-out するのであって、arm ごとの drop ではありません）。`Else{val: Some(objX)}` / `EndIf{val: Some(objX)}` を出力し、wasm が `dst` 用のハンドルをスタックに残すようにします。
3. `ownership_certificate`（`certificate.rs`）: 変更不要です。`Op::Consume` は既に `m` を、`Op::Alloc`/ヒープの `Call` は `i` を出力し、`IfThen.dst` は `of` に挿入されないので `func.ret` は 2 個目の `m` を加えません。敵対的検証でこれを確認してください（危険なのは、新しい所有値ではない生成元 — 例えば借用パラメータ — の arm です。黙って均衡させず REJECT しなければなりません。借用パラメータの Consume は rc 0 で `m` を出力 → checker は既に FAULT となり、正しい壁ですが、ゲートが決してそれを通さないことを確認します）。
4. `render_wasm.rs`:
   - `render_wasm_fn` の `IfThen` arm は現在 `(result i64)` を固定しています。結果型を `dst` の repr に従わせます。**`dst` がヒープなら i32**、スカラーなら i64。
   - dst-repr の難所: `value_reprs_wasm` は `IfThen` の `dst` をヒープに分類する必要があります。`EndIf`/`Else` の `val` repr から推論するか（arm のハンドルは i32）、マーカーに repr を伝えます。ここは正しく実装してください。ヒープ dst を i64 ローカルとして render すると、最善でも型エラー、最悪では無言の誤コンパイルです。
5. テスト（`render_wasm.rs`）: `label(true)="yes"`、`label(false)="no"` が v0 とバイト一致。呼び出し arm 版、リテラルパターンでヒープ結果を返す `match`。
6. ゲート: `cargo test -p almide-mir`、`proofs/corpus-wall.sh` は WALL OK / 3 性質 ACCEPT を維持しなければなりません（ownership 数に注意。ヒープ結果 if のコーパス関数は Opaque の 1 オブジェクトから arm ごとの 2 オブジェクトへ移るので、ownership は増える可能性があります。ACCEPT が続き、各々が `"im"` であれば健全です）。
7. **敵対的健全性検証**（必須 — #49 級の、受理されるが安全でない穴が生じる場所です）: 3 以上の独立エージェントを起動し、新しい lowering では checker が ACCEPT するのに実行時にリークや二重解放を起こすヒープ結果分岐の構築を試みます。破れなかった場合のみ commit します。

<a id="why-deferred-honesty"></a>

## 先送りした理由（率直に）

設計は健全で checker は無変更ですが、手順 2–4 は所有権証明書の生成と wasm render を変更します。そこは歴史的に、受理されるが安全でない 2 つの穴（#49 の推移的 caps、呼び出し省略の taint）が見つかった場所そのものです。長いセッションの終わりに実装するのではなく、新たな集中力での敵対的検証が必要です。リスクを低減した設計をここに記録することが今回の前進です。実装と敵対的検証は、次のセッションの最初の作業です。

## コード内説明・コメントの訳

最初のイベント例は “yes” と “no” を別々に割り当て、dst で 1 回移出します。均衡した例は各オブジェクトを個別に割り当てて消費します。dst は IfThen の結果であり割り当てではないため of に含まれず、dst の返却は 2 回目の移出を出力しません（二重 move なし）。

## 原本に関する注意

原本には歴史的な実装済み・健全との報告、残作業一覧、その後修正済みの割り当て問題、次セッションの計画が併存し、後続 lowering の存在も明記されています。保持した局所的な証明書の論証と敵対的監査の報告は、成果物全体のメモリ安全性定理でも、この翻訳による新たな監査でもありません。テスト・エージェント・commit に関する原本の指示は歴史的計画の内容であり、ここで実行した操作ではありません。
