# Type System Architecture [ACTIVE]

## Vision

Kind = KType | KArrow Kind Kind — 全ての判断が構造から自明になるコンパイラ。

## Current: Rust 153/153, WASM 13 compile failures, 21/73 pass

## Completed

- **Union-Find 型推論**: HashMap → 等価クラスモデル。propagation hack / fixpoint 不要
- **lambda current_ret isolation**: 外側関数の戻り型が lambda の ok/err に漏洩するバグ修正
- **closure env typed zero-init**: capture 型に応じた zero 値
- **VarTable mono update**: monomorphize 後の VarTable 型を concrete に更新
- WASM compile failures: 17→13 (4件改善)

## Remaining: 13 compile failures, 2 root causes

### Root A: RecordPattern VarId mismatch [5 files] ← NEXT

**Files**: default_fields, type_system, protocol_advanced/extreme/stress

**Root cause**: lowering が match の RecordPattern で field 変数に新しい VarId を割り当てるが、
body 内の Var 参照は checker が割り当てた古い VarId を使う。

```
lowering:  Rect { width, .. }  → VarId(25) 'width' (RecordPattern で新規)
body IR:   float.to_string(width) → Var { id: VarId(2) }  (checker が割り当てた元の id)
```

scan_pattern は VarTable 名前検索で VarId(25) を見つけ local を確保するが、
emit_expr は VarId(2) で var_map を探して見つからず、ゼロフォールバック。

**Fix**: lowering の RecordPattern 処理で、body 内の Var 参照と一貫した VarId を使う。
具体的には `lower_pattern` が返す VarId を body の scope に正しく登録する。

### Root B: Codec/Value type mismatch [8 files]

**Files**: auto_derive, codec_convenience/list/nested/p0/test/weather, value_utils

Codec derive が生成する IR で Value variant の Int/Float payload を i32 slot に store。
mono とは無関係。Codec WASM 対応 or skip。

## Fix Order

| Step | What | Files | Cumulative |
|------|------|-------|------------|
| ✅ | Union-Find + mono VarTable | +4 | 13 failures |
| **NEXT** | RecordPattern VarId fix | +5 | 8 failures |
| Then | Codec WASM or skip | +8 | 0 failures |
