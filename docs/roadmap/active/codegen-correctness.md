# Codegen Correctness Fixes [ACTIVE]

生成コードの正確性に関わる問題。rustc が弾いてくれるケースもあるが、TS ターゲットや将来の IR interpreter では致命的になる。

## P1: auto-`?` の二重ロジック統一 ✅

**修正済み.** `should_auto_unwrap_user()` と `should_auto_unwrap_stdlib()` に統一。両方とも `skip_auto_unwrap` を尊重する。

## P1: Range 型のハードコード ✅

**修正済み.** Range codegen が IR の `expr.ty` から要素型を取得し、`Vec<{elem_ty}>` を生成するように変更。

## P1: Box パターンデストラクトの未バインド変数 ✅

**修正済み.** 非 Bind パターン（Wildcard, nested Constructor）に対して適切な `box` キーワードまたはスキップを追加。

## P1: Do-block guard の break/continue ハンドリング ✅

**修正済み.** guard の else ブランチの IR ノード種別 (Break/Continue/ResultErr) を検査し、適切な Rust コードを生成。

## P1: do ブロック + guard で unreachable になる ✅

**修正済み.** guard を含む do ブロックを `loop { ... break; }` で囲むことで、guard の `break`/`return` 後のコードが正常に到達可能に。TS codegen の `DoLoop` パターンと同じアプローチ。

## P1: do ブロック内の auto-`?` が `let` バインドで効かないケースがある

**問題:** `effect fn` の `do` ブロック内で `let output = process.exec(...)` のように Result を返す関数を `let` にバインドした場合、auto-`?` が挿入されず、`output` が `Result<String, String>` のまま。

**修正:**
- [ ] do ブロック内の `let` バインドで右辺が Result を返す場合に auto-`?` を挿入

## P1: effect fn 内の for ループで Result ラップが壊れる

**修正:**
- [ ] `emit_rust/lower_rust.rs`: for ループ body 内のステートメントに in_effect コンテキストを伝播

## P2: 文字列パターンの borrowed subject 不整合

**修正:**
- [ ] borrowed param が match subject のとき、適切な deref / `as_str()` を挿入

## P2: パターンデストラクトの全フィールド clone

**修正:**
- [ ] Copy 型のフィールドは clone をスキップ
- [ ] single_use_vars と連携して不要な clone を除去
