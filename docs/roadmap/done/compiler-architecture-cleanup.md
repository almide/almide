# Compiler Architecture Cleanup

**優先度:** Medium — 1.0後でもいいが、やるなら早い方がいい
**状態:** 5項目

## 項目

### ✅ 1. clone/deref IR化 (完了)

- [x] CloneInsertionPass: `Var { id }` → `Clone { Var { id } }` (heap-type variables)
- [x] BoxDerefPass: `Var { id }` → `Deref { Var { id } }` (box'd pattern bindings)
- [x] walker から `ann.clone_vars` / `ann.deref_vars` 参照を削除
- [x] annotations から `clone_vars` / `deref_vars` フィールド削除

### ✅ 4. walker HashMap allocation 削減 (完了)

- [x] `fill_template` を `&[(&str, &str)]` に変更
- [x] `render_with()` API 追加
- [x] 全89箇所の HashMap::new() を render_with に移行

### 2, 3, 5 → 個別 roadmap に分離 (post-1.0)
