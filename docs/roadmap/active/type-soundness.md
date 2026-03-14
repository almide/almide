# Type Soundness Fixes [ACTIVE]

Type checker の健全性に関わる問題の修正。現在 rustc が第二の型チェッカーとして機能しているため表面化しにくいが、TS ターゲット・IR interpreter・LLM → IR 直接生成では安全ネットがない。

## P0: expr_types のスパンキー → ExprId ✅

**修正済み.** AST ノードに `ExprId(u32)` を追加、`HashMap<ExprId, Ty>` に変更。`skip_span_lookup` ハック除去。

## P0: Occurs Check ✅

**修正済み.** `unify()` で TypeVar バインド前に `occurs_in()` で直接再帰を検出。`T = List[T]` を拒否。
スコープ付き TypeVar ID が未実装のため、深い occurs check は future work（同名の TypeVar が異なるスコープから来る場合を区別できない）。

## P0: Named 型の循環解決 ✅

**修正済み.** `resolve_named()` に `seen: HashSet<String>` による循環検出を追加。

## P0: Unknown 型の黙続行

**問題:** 型が見つからない場合 `Ty::Unknown` で続行し、以降の型チェックをすり抜ける。

```rust
.unwrap_or(Ty::Unknown)  // 15箇所以上
```

`Unknown` は全ての型と互換なので、型エラーが検出されないまま codegen に流れる。

**修正:**
- [ ] `unwrap_or(Ty::Unknown)` を段階的にエラー報告に置換
- [ ] Unknown の発生源を分類: (a) 意図的なワイルドカード (b) 推論失敗 (c) 内部エラー
- [ ] (b)(c) はエラーまたは warning として報告
- [ ] codegen が Unknown を含む IR を受け取った場合に ICE (internal compiler error) を出す

**Note:** `substitute()` で未束縛 TypeVar → Unknown は維持（codegen が TypeVar を Rust 型に変換できないため）。根本修正にはスコープ付き TypeVar ID が必要。

## P0: TypeVar の無条件一致

**問題:** `unify()` で actual が TypeVar のとき無条件で成功する。

```rust
if matches!(actual_ty, Ty::TypeVar(_)) {
    return true;
}
```

**現状:** TypeVar のスコープ ID がないため、同名 TypeVar の区別ができない。この修正にはスコープ付き TypeVar ID（`TypeVarId(scope, name)`）の導入が前提条件。

**修正:**
- [ ] TypeVar にスコープ ID を追加: `TypeVar { name, scope_id }`
- [ ] actual 側の TypeVar は同スコープの場合のみバインド
- [ ] 異なるスコープの TypeVar 同士はエラー

## P1: Result の Unknown 半分

**問題:** `ok(42)` → `Result[Int, Unknown]`、`err("fail")` → `Result[Unknown, String]`。Unknown 半分が型チェックをすり抜ける。

**修正:**
- [ ] 双方向型推論: `ok(x)` の呼び出し文脈から期待される Result 型を取得し、Unknown 半分を埋める
- [ ] 文脈がない場合は TypeVar を使い、後で解決

## P1: ラムダ引数の TypeVar → Unknown 降格

**問題:** ラムダの引数型推論で TypeVar が Unknown に降格される。

**修正:**
- [ ] TypeVar を保持し、ラムダの body 型チェック中に他の引数から制約を収集して解決
- [ ] 2パス推論を改善: 非ラムダ引数で TypeVar をバインド → ラムダ引数に適用

## P1: パターンマッチの Unknown 伝播

**問題:** `match` の subject が Unknown 型のとき、パターン変数が Unknown でバインドされる。

**修正:**
- [ ] subject が Unknown の場合、パターンバインドをエラーにするか、制約を収集して遅延推論
- [ ] `ok(x)` / `err(e)` パターンで subject が Result でない場合にエラーを出す
