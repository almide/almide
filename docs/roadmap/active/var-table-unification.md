<!-- description: Unify program/module var_tables into a single program-level table
-->
# VarTable Unification

`IrProgram.var_table` と `IrModule.var_table` の 2 層構造を 1 層に畳む。
`codegen-ideal-form` #5 の残務を独立アークに切り出したもの。

## 動機

- 現状 `IrModule` ごとに `var_table` を持ち、`VarId` の意味が module によって変わる。
- Emit 時に「この関数の VarTable はどれ?」を `for module in &mut program.modules` の
  iteration から判断し、`&mut module.var_table` を経由して pass を呼ぶ。
- `emit_wasm::WasmEmitter::top_let_globals_by_name` は cross-module `ALMIDE_RT_<MOD>_<NAME>`
  を **名前経由**で引ける band-aid で、VarId の region 不一致を回避するための workaround。
- `codegen-ideal-form` で記録された元の pain point (lifted closure が module/program
  を跨いで VarId が壊れる) は `pass_closure_conversion` が lifted を module 内に
  留める形で緩和済みだが、**二重 VarTable という構造的 smell は残存**。

## 目標

- `IrModule.var_table` を廃止。`IrProgram.var_table` 一本に統合。
- Attach 時 (`ir.modules.push(m)` 相当) に `m` 内の全 VarId を `IrMutVisitor` で
  offset 分 remap、`m.var_table.entries` を `program.var_table` に append。
- `top_let_globals_by_name` (名前経由の cross-module 引き) を削除。VarId で直引き。
- `module.var_table` に依存する 15 ファイル ≈28 箇所を `program.var_table`
  destructure パターンに移行:
  ```rust
  let IrProgram { modules, var_table, .. } = &mut *program;
  for module in modules { pass(&mut module.functions, var_table); }
  ```

## 非ゴール

- VarId の内部表現変更 (`VarId(ModuleId, u32)` 化) は行わない。globally unique な
  u32 のままで十分。
- Serialization format の大幅変更は行わない。`IrModule.var_table` の skip 化で済ます。

## 影響範囲

- 6 つの `lower_module` caller (CLI 側): `main.rs`, `cli/{emit,commands,build}.rs`
- 15 pass: `pass_rust_lowering`, `pass_list_pattern`, `pass_lambda_type_resolve`,
  `pass_tco`, `pass_clone`, `pass_shadow_resolve`, `pass_licm`, `pass_capture_clone`,
  `pass_closure_conversion`, `pass_concretize_types`, `pass_match_lowering`,
  `pass_box_deref`, `pass_auto_parallel` 等
- `emit_wasm/mod.rs`: 4 箇所 (module iteration 内の var_table 参照)
- `walker/mod.rs`: 1 箇所 (同上)

## 測定

- 全 spec/ + nn Rust/WASM green (219 / 213+ baseline)
- `top_let_globals_by_name` 削除後も cross-module top-let reference が動くこと
- IR serialization 互換 (既存 fixture は root program のみのため影響なし)

## 見積

1-2 日。mechanical change が主体で、borrow-check の destructure と pass-by-pass
更新が手数の中心。

## 前提

- `codegen-ideal-form` で #1/#3/#4/#6/#7 を完遂済み (2026-04-19 時点)。
  本 arc はそれらに続く構造 cleanup。
