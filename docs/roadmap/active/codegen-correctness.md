# Codegen Correctness Fixes [ACTIVE]

生成コードの正確性に関わる問題。rustc が弾いてくれるケースもあるが、TS ターゲットや将来の IR interpreter では致命的になる。

## P1: auto-`?` の二重ロジック統一 ✅
## P1: Range 型のハードコード ✅
## P1: Box パターンデストラクトの未バインド変数 ✅
## P1: Do-block guard の break/continue ハンドリング ✅
## P1: do ブロック + guard で unreachable になる ✅
## P1: Module/Method 呼び出しの auto-`?` ✅

**修正済み.** `CallTarget::Module` / `Method` / `Computed` でも effect context かつ Result 返却時に auto-`?` を挿入。以前は `CallTarget::Named` のみ。これにより do-block let binding と for-loop body 内の auto-`?` も解消。

## P1: effect fn 内の for ループで Result ラップが壊れる ✅

**修正済み.** 上記の auto-`?` 修正により、for-loop body 内の呼び出しも effect context で正しく `?` が付く。`in_effect` は `LowerCtx` のフィールドとして全 stmt/expr に伝播済み。

## P2: 文字列パターンの borrowed subject 不整合

**修正:**
- [ ] borrowed param が match subject のとき、適切な deref / `as_str()` を挿入

## P2: パターンデストラクトの全フィールド clone

**修正:**
- [ ] Copy 型のフィールドは clone をスキップ
- [ ] single_use_vars と連携して不要な clone を除去
