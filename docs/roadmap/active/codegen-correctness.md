# Codegen Correctness Fixes [ACTIVE]

生成コードの正確性に関わる問題。rustc が弾いてくれるケースもあるが、TS ターゲットや将来の IR interpreter では致命的になる。

## P1: auto-`?` の二重ロジック統一 ✅

**修正済み.** `should_auto_unwrap_user()` と `should_auto_unwrap_stdlib()` に統一。両方とも `skip_auto_unwrap` を尊重する。

## P1: Range 型のハードコード ✅

**修正済み.** Range codegen が IR の `expr.ty` から要素型を取得し、`Vec<{elem_ty}>` を生成するように変更。

## P1: Box パターンデストラクトの未バインド変数 ✅

**修正済み.** 非 Bind パターン（Wildcard, nested Constructor）に対して適切な `box` キーワードまたはスキップを追加。

## P1: Do-block guard の二重 Result ラップ

**問題:** guard の else ブランチが既に Result を返す式の場合、`return Err(...)` で二重ラップされる可能性。

文字列の `contains("return ")` による判定は脆弱だが、IR ベースの判定に切り替えると `ResultOk` in effect context のケースが壊れる。

**修正:**
- [ ] ResultErr/ResultOk の codegen と guard の codegen の実行順序を整理
- [ ] guard 内の else 式を codegen する際に do-block コンテキストフラグを活用

## P1: do ブロック内の auto-`?` が `let` バインドで効かないケースがある

**問題:** `effect fn` の `do` ブロック内で `let output = process.exec(...)` のように Result を返す関数を `let` にバインドした場合、auto-`?` が挿入されず、`output` が `Result<String, String>` のまま。

**修正:**
- [ ] do ブロック内の `let` バインドで右辺が Result を返す場合に auto-`?` を挿入

## P1: do ブロック + guard で main 関数が unreachable になる

**問題:** guard が `loop` 変換され、loop 内の `return Err(...)` 後のコードが unreachable 扱いになる。

**修正:**
- [ ] guard ありの do ブロックで `loop` の後に `break` を挿入して正常終了パスを作る

## P1: effect fn 内の for ループで Result ラップが壊れる

**修正:**
- [ ] `emit_rust/ir_blocks.rs`: `do` ブロック内の `for` ループの codegen を修正

## P2: 文字列パターンの borrowed subject 不整合

**修正:**
- [ ] borrowed param が match subject のとき、適切な deref / `as_str()` を挿入

## P2: パターンデストラクトの全フィールド clone

**修正:**
- [ ] Copy 型のフィールドは clone をスキップ
- [ ] single_use_vars と連携して不要な clone を除去
