<!-- description: Fix WASM validation errors from union-find generic instantiation -->
<!-- done: 2026-03-23 -->
# WASM Validation Fixes

## Status: Rust 153/153, WASM 1 compile failure (type_system_test), 8 skipped (Codec)

## 諸悪の根源: Checker の Union-Find Generic Instantiation 汚染

### 症状

generic 関数 `fn either_map_right[A, B, C](e: Either[A, B], f: (B) -> C) -> Either[A, C]` で:

```
checker が A=String, B=Int, C=Int を推論すべきところ、
Union-Find で A の fresh var が B/C の fresh var と同じ等価クラスに入り、
A=Int に汚染される。

結果: match subject.ty, arm body.ty, pattern.ty が全て汚染。
Left(a) の a が String ではなく Int として型付けされる。
codegen が i64_load (Int) を出すが、実際の payload は String (i32) → validation error。
```

### 根本原因

`check_call_with_type_args` で generic 関数を呼ぶとき:

1. `fresh_var()` で各 generic param に inference var を割り当て (?N, ?M, ?O)
2. `constrain(param_ty_substituted, arg_ty)` で引数型と unify
3. Union-Find の `bind/union` で ?N, ?M, ?O が concrete 型にバインド

**問題**: step 3 で `bind` が既存のバインドを上書き。`?N = String` がセットされた後、
別の constraint で `?N = Int` に上書きされる。または `union(?N, ?M)` で
異なる generic params が同じ等価クラスに入る。

### なぜ codegen パッチでは解決しないか

checker が `expr_types` に汚染された型を格納 → lowering が IR に汚染型を設定
→ mono が TypeVar を置換するが concrete 型は変えない → codegen が汚染型で命令を生成

**汚染は IR 全体に伝播する。** scan, emit, pattern, match result — 全てが影響。
codegen で個別に修正しても、次の式で同じ問題が再発。

### 正しい修正

checker の generic instantiation で **各 generic param の fresh var を independent に管理**。

具体的な選択肢:

**A. Scoped fresh vars**: generic 関数呼び出しごとに independent な fresh var set を作り、
call の constraint 解決が完了するまで他の constraint から分離。

**B. Bidirectional inference**: top-down で期待型を伝播し、bottom-up で推論型を返す。
generic params は top-down の期待型から先に解決。Union-Find の上書き問題が発生しない。

**C. Constraint 分離**: generic 関数呼び出しの constraint を別の solver context で解決し、
結果のみを main context に merge。HM inference の let-polymorphism と同じ考え方。

**推奨**: A が最小変更。B は理想だが checker 全体の書き直し。C は中間。

### この branch でやったこと (workaround)

- `IrPattern::Bind { var, ty }` — pattern が型を自前で持つ (VarTable 非依存)
- mono `substitute_pattern_types` — mono で pattern.ty を concrete に置換
- checker match result var 分離 — `first` arm type を直接返す (shared fresh var なし)
- propagate で match.ty を func.ret_ty で修正
- emit_match で arm body の WASM type consensus を使用

**全て workaround。** checker を直せば不要になる。
