<a id="reimpl-lint-signature-match-detection-of-stdlib-reimplementations"></a>

# 再実装 lint：シグネチャ一致による stdlib 再実装の検出

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](reimpl-lint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](reimpl-lint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](reimpl-lint.zh-CN.md)

> [英語の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/reimpl-lint.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 過去のメタデータ：stdlib fn とシグネチャが一致するユーザー fn を検出し、委譲を提案する。完了日 — 2026-04-20。

> 原文の注意点：完了節は Warning と stdlib 側 TypeVar の一方向ワイルドカードを記す一方、保存された旧計画は Info、異なるジェネリック一致規則、両方向のジェネリック除外を記す。この時点差を黙って解消しない。偽陽性がほぼゼロという主張や +1/+2 の測定仮説は原文の根拠であり、独立に示した意味的等価性や新しい測定結果ではない。委譲 shim の採用は任意のままである。

<a id="completion-status-2026-04-20"></a>

## 完了状態（2026-04-20）

`E015 Possible stdlib reimplementation` として導入済み。`check_reimpl_lint`（`crates/almide-frontend/src/check/mod.rs`）から発する Warning レベルの診断である。

検出規則：

1. トップレベルの `fn`（test / derive / 単相化済み / convention method ではない）。
2. 短い名前が stdlib の短い名前から Levenshtein 距離 ≤ 2（大文字小文字を区別しない。既存の `almide_base::diagnostic::levenshtein` helper は、crate をまたいで使うため `pub` になった）。
3. パラメーターの個数と型、戻り値型が構造的に一致する（`check/mod.rs` の `ty_reimpl_eq`）。stdlib 側の `TypeVar` はワイルドカードである（単相なユーザー fn がジェネリックな stdlib fn に一致する）。逆方向は一致しない。

三つすべてが成立する必要がある。一つでも外れれば提案を抑制し、偽陽性率をゼロに近く保つ。

出力は警告と委譲用の `try:` shim（`fn <user_name>(args) ->
T = <module>.<stdlib_fn>(args)`）である。文書ファイルは `docs/diagnostics/E015.md`。shim は自動適用されず、ユーザーまたは LLM の再試行で意識的に受け入れなければならない。

レベル：`Level` enum に `Info` を追加せず、`Warning` のままにした。ロードマップの「info/warning-level」という許容範囲に一致し、crate をまたぐ variant の連鎖変更を避ける。dojo 測定で警告が過度にうるさいと示されたら再検討する。

<a id="original-plan-retained-for-history"></a>

## 当初の計画（履歴として保存）

実装のきっかけ：SYSTEM_PROMPT の「Prefer stdlib」節を適用しても効果がなく、dojo 測定により `list.binary_search` / `string.run_length_encode` / その他の stdlib プリミティブが一から再実装されていると分かった時に実装する。

<a id="goal"></a>

## 目標

ユーザー定義のトップレベル `fn` が、名前の非常に似た stdlib 関数と**シグネチャが完全一致**する場合に、info/warning レベルの診断を出す。診断は stdlib 関数を示して委譲を提案し、LLM の再試行が短く慣用的な形に収束できるようにする。

<a id="scope-discipline"></a>

## 範囲の厳守

この lint は偽陽性を出しては**ならない**。自分なりの理由で `list.first` の特殊版を書くユーザーに、うるさく指摘してはならない。発火条件は厳格である：

- 名前の類似度：ユーザー fn 名と stdlib fn 名（短い形式）の Levenshtein 距離 ≤ 2、かつ
- パラメーターのシグネチャ：型が構造的に同一（要素ごと）、かつ
- 戻り値型：構造的に同一。

三条件すべてが必要である。シグネチャの完全一致により偽陽性率は実質ゼロとなる。ユーザーが `fn binary_search(xs: List[Int], target: Int) -> Option[Int]` と書いたなら、`list.binary_search` と*異なる*ことを意図した確率は極めて小さい。

<a id="example-binary-search-case"></a>

## 例（二分探索の場合）

```almide
// User writes (~30 lines of algorithm):
fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
    let low = 0
    let high = list.len(xs) - 1
    // ... loop implementation ...
```

出力する診断：

```
info: fn 'binary_search' has the same signature as stdlib `list.binary_search`
  --> binary_search.almd:1:4
  |
1 | fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
  |    ^^^^^^^^^^^^^^^^^
  hint: if this is the standard algorithm, delegate to stdlib:
  try:
      fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
        list.binary_search(xs, target)
```

`try:` スニペットは**委譲 shim**であり、ユーザーの本体を書き換えるものではない。既存の本体を残す価値があるか、一行版に置き換えるかは、ユーザー（または LLM の再試行）が選ぶ。

<a id="non-triggers-explicitly-must-not-fire"></a>

## 発火させない条件（明示的に禁止）

- パラメーター順序が異なる（`fn lookup(target: Int, xs: List[Int])`）
- 引数型が異なる（`fn binary_search(xs: List[Float], target: Float)`）
- 戻り値の形が異なる（見つからない場合に -1 を返す `fn binary_search(...) -> Int`）
- 名前の距離 > 2（`fn find_by_binary_search`）
- stdlib 版にはないユーザー定義のジェネリック（およびその逆）

迷ったら発火しない。これは任意の最適化であり、正しさの検査ではない。

<a id="implementation-sketch"></a>

## 実装の概略

1. **表の構築（チェッカーのインスタンスごとに一度）**：スコープ内で見えるすべての stdlib 関数について `(module, fn_name, param_tys, ret_ty, generics)` を収集する。出典は `almide_frontend::stdlib::module_functions` + `lookup_sig`。短い名前（モジュール接頭辞より後の部分）でグループ化する。

2. **各ユーザー fn**：推論後、トップレベルの `Decl::Fn` を走査する：
   - シグネチャから `user_key = (param_tys, ret_ty)` を計算する。
   - 短い名前がユーザー fn 名から編集距離 ≤ 2 の stdlib fn 候補を探す。
   - 各候補について `(param_tys, ret_ty)` と `user_key` を構造的に比較する（下記）。一致したら info 診断を出す。

3. **型の構造的等価性**：
   - `Ty::Int == Ty::Int`、など。
   - `Ty::Applied(id_a, args_a) == Ty::Applied(id_b, args_b)` は、`id_a == id_b && args elementwise ==` の場合かつその場合に限る。
   - `Ty::TypeVar("A")` は、同じ位置の別の `TypeVar` と等しいとみなす（単相シグネチャはユーザー側の多相シグネチャと一致するべき — 一般化したふりをしない）。
   - protocol / 構造的境界は無視する（stdlib fn にはまれであり、stdlib にあってユーザー側になければスキップする）。

4. **診断**：委譲 shim である `try_snippet` を付ける。重大度は `Warning` でなく `Info` とし、コンパイルを妨げないようにする。

<a id="what-this-lint-does-not-do"></a>

## この lint がしないこと

- アルゴリズムの形で検出しない（while + mid + low + high → 二分探索）。それは別のツール（本当の静的解析器として Phase 5 かもしれない）であり、Phase 3 には曖昧すぎる。
- 本体を書き換えない。
- `almide fix` で適用しない。ユーザー（または LLM の再試行）が意識的に shim を受け入れなければならない。

<a id="measurement-hypothesis"></a>

## 測定仮説

実装して有効にした場合：

- 70b：binary-search で +1（動機となったそのケース）。
- 70b：run-length-encoding で +1（シグネチャが一致すれば。現在の stdlib は `List[(String, Int)]` を返すので、ユーザー側で `String` を返す RLE は一致しない — 設計どおり。発火させない条件を参照）。
- 8b：効果は最小（8b は stdlib と完全一致するシグネチャをめったに書かず、通常は誤った型を選ぶ）。

合計予想：**70b で +1 から +2**。診断が `Info` レベルのため低リスクであり、コンパイルを止めず、再試行を促すだけである。

<a id="when-to-implement"></a>

## 実装する時期

- dojo run #10（`almide fix` 再試行ループ統合 + SYSTEM_PROMPT の「Prefer stdlib」節あり）が、現在の baseline に対し ≤ +2pt となったとき。
- または、ハーネス改善後も binary-search / 類似タスクが stdlib を再実装しているという、ログからの直接の証拠があるとき。

それ以外は延期する。測定の機会は安価であり、実装はチェッカー約 50 行とテストである。

> 保持した例のコメントは、ユーザーが約 30 行のアルゴリズムを書き、ループ実装部分を省略していることを示す。旧診断例は binary_search が標準ライブラリの list.binary_search と同じシグネチャであると述べ、「標準アルゴリズムなら標準ライブラリへ委譲する」という条件付きヒントを示す。
