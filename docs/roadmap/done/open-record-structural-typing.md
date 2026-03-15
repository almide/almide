# Open Record / Row Polymorphism — 実装ガイド

**テスト:** `spec/lang/open_record_test.almd`
**ステータス:** 16 rustc エラー, 0 checker エラー
**理論:** Rémy 1989 Row Polymorphism

## 現状のアーキテクチャ

```
チェッカー (src/check/)     → OpenRecord は compatible チェックで通る ✅
monomorphizer (src/mono.rs) → generic + structural bound のみ対応 ⚠️
codegen (src/emit_rust/)    → OpenRecord / TypeVar("Named") が Rust 型に変換できない ❌
```

## テストが要求する2パターン

### パターン A: 直接 OpenRecord パラメータ
```almide
fn greet(who: { name: String, .. }) -> String = "Hello, ${who.name}!"
greet(Dog { name: "Rex", breed: "Lab" })  // Dog は name を持つ
```
**monomorphizer が認識しない** — generic がないから。

### パターン B: Generic + Structural Bound
```almide
fn describe[T: { name: String, .. }](x: T) -> String = "name: ${x.name}"
describe(Dog { name: "Rex", breed: "Lab" })
```
**monomorphizer が対応済み** — `src/mono.rs` で specialization される。

## 修正するファイル

### 1. `src/mono.rs` — find_structurally_bounded_fns を拡張

```rust
// 現状: generic + structural bound のみ検出
fn find_structurally_bounded_fns(functions: &[IrFunction]) -> HashMap<String, Vec<BoundedParam>> {
    for func in functions {
        if let Some(ref generics) = func.generics {
            // structural_bound がある generic param を検出
        }
    }
}

// 修正: 直接 OpenRecord パラメータも検出
fn find_open_record_fns(functions: &[IrFunction]) -> HashMap<String, Vec<OpenRecordParam>> {
    for func in functions {
        for (i, param) in func.params.iter().enumerate() {
            if matches!(&param.ty, Ty::OpenRecord { .. }) {
                // この関数は monomorphization 対象
            }
        }
    }
}
```

### 2. `src/mono.rs` — discover_instances を拡張

Call site で渡される具体型を収集:

```rust
// greet(Dog { name: "Rex", breed: "Lab" })
// → call target: "greet", args[0].ty = Named("Dog", [])
// → instance: ("greet", "Dog") → { param_0 → Dog }
```

IR の Call ノードを走査し、target が open record fn で args の型が Named なら instance を登録。

### 3. `src/mono.rs` — specialize_function を拡張

Open record パラメータの型を具体型に置換:

```rust
// greet(who: { name: String, .. }) → greet__Dog(who: Dog)
// 関数 body 内の who.name は Dog::name に解決される
```

### 4. `src/mono.rs` — rewrite_calls を拡張

Call site を specialized 版にリダイレクト:

```rust
// greet(dog) → greet__Dog(dog)
```

## アルゴリズムの核心 (Row Unification)

```
unify({ name: String, .. }, Dog)
  ↓
Dog を resolve → { name: String, breed: String }
  ↓
{ name: String | ρ } vs { name: String, breed: String | RowEmpty }
  ↓
共通: name: String ✓
余り: breed: String → ρ に入る
  ↓
ρ = { breed: String }
```

これはチェッカーでは **既に動いてる** (compatible チェック)。codegen 側の monomorphization が足りないだけ。

## 実装順序

1. `find_open_record_fns()` — OpenRecord パラメータを持つ関数を検出
2. `discover_instances()` — 各 call site の具体型を収集
3. `specialize_function()` — OpenRecord → 具体型 に置換した関数コピーを生成
4. `rewrite_calls()` — call site を specialized 版にリダイレクト
5. codegen の `TypeVar("Named")` → resolved named type のフォールバック

## テストの期待結果

```
spec/lang/open_record_test.almd: 16 tests pass
```

全 16 テストが Rust compilation を通過し、runtime assertion を満たすこと。
