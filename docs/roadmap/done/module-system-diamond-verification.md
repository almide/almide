<!-- description: Verify diamond dependency handling and fix remaining module system edge cases -->
<!-- done: 2026-03-28 -->
# Module System Diamond Dependency Verification

## Status: 2026-03-28

### Verified (all tests pass)

- [x] D が1回だけロードされる（`loaded_names` 重複チェック）
- [x] B と C が同じ D の型宣言を参照できる
- [x] D の関数が codegen で重複定義されない
- [x] B が作った D.Logger を C に渡せる（型の同一性）
- [x] C が作った D.Logger を B に渡せる（逆方向も）
- [x] D のサブモジュール（`dmod_d.utils.format_level`）が3段ドット呼び出しで動く
- [x] `check_module_bodies` の `imported_stdlib` save/restore
- [x] `check_module_bodies` の `module_aliases` save/restore
- [x] `check_module_bodies` で prefix なし宣言を一時登録 → 除去
- [x] モジュール名のドット→アンダースコア変換（全4箇所）

### Test: `spec/integration/modules/diamond_test.almd` (10 tests)

```
diamond: B calls D                              ✓
diamond: C calls D                              ✓
diamond: D called directly                      ✓
diamond: D value consistent                     ✓
diamond: B creates D Logger type                ✓
diamond: C creates D Logger type                ✓
diamond: B Logger passed to C function          ✓
diamond: C Logger passed to B function          ✓
diamond: direct D Logger works with both B and C ✓
diamond: D submodule called directly            ✓
```

### Not yet implemented (requires package registry)

- [ ] 異なる major バージョンの同名パッケージ共存（B→D(v1), C→D(v2)）
- [ ] `versioned_name` を codegen で使ったリネーム
- [ ] バージョン違いの型を別物としてエラー報告

これらは `PkgId { name, major }` と `IrModule.versioned_name` で設計済みだ��、
semver 解決と package registry がないと実装できない。on-hold/package-registry.md に記載。
