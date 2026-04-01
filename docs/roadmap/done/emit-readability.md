<!-- description: Improve readability of generated Rust output -->
<!-- done: 2026-04-01 -->
# Emit Readability

**Priority:** High — Directly affects the quality of generated code that LLMs modify
**Goal:** `almide app.almd --target rust` output should be readable without rustfmt

> "Generated code readability directly impacts modification survival rate."

---

## Current State (v0.10.5)

Phase 4 の codegen 最適化（iterator chain, math inline, borrow inference）は完了。
残りは **ソース構造の保存** と **フォーマッティング品質** の 2 軸。

### 現状の問題

```rust
// 現在の emit: 全部詰まる、doc comment 消える、match は 1 行
pub fn area(width: f64, height: f64) -> f64 { (width * height) }
pub fn perimeter(width: f64, height: f64) -> f64 { (2f64 * (width + height)) }
pub fn describe(s: Shape) -> String { match s { Shape::Circle(r) => format!("..."),
Shape::Rect(dims) => format!("..."), } }
```

### 理想

```rust
/// Compute the area of a rectangle
pub fn area(width: f64, height: f64) -> f64 {
    width * height
}

/// Compute the perimeter
pub fn perimeter(width: f64, height: f64) -> f64 {
    2.0 * (width + height)
}

pub fn describe(s: Shape) -> String {
    match s {
        Shape::Circle(r) => format!("circle r={}", r),
        Shape::Rect(dims) => format!("rect {}x{}", dims.width, dims.height),
    }
}
```

---

## Phases

### Phase 1: Blank Line Preservation

ソースの空行を emit に反映する。関数間・型間の論理ブロック分離を保存。

**実装パス**: Parser → AST → IR → Walker

- [x] Parser: 空行位置を AST に記録（`blank_lines_before: u32` を Program.blank_lines_map に追加）
- [x] IR: `IrFunction` / `IrTopLet` / `IrTypeDecl` に `blank_lines_before` フィールド追加
- [x] Walker: emit 時に top-level 宣言間に空行挿入 (`parts.join("\n\n")`)
- [x] 最低限: top-level 宣言間に常に 1 空行（walker のみ）

### Phase 2: Doc Comment Preservation

`/// ...` doc comment を Rust emit に反映。

**実装パス**: Parser → AST → IR → Walker

- [x] Parser: `/// ...` コメントを AST の `Program.doc_map` に記録
- [x] IR: `IrFunction` / `IrTopLet` / `IrTypeDecl` に `doc: Option<String>` フィールド追加
- [x] Rust emit: `/// ...` として出力
- [x] TS emit: 削除済み（TS codegen は撤去）
- [x] WASM: コメント不要（バイナリ形式）

### Phase 3: Import Grouping

- [x] Rust emit: extern fn `use` 文をグループ化 + prelude 空行で分離
- [x] TS emit: 削除済み（TS codegen は撤去）
- [x] 論理順序: prelude → extern imports → types → lets → functions

### Phase 4: Formatting Quality

- [x] Iterator chain emission: `list.map/filter/fold` → `.into_iter().map().collect()` (v0.10.4)
- [x] Math intrinsics inline: `math.sqrt(x)` → `x.sqrt()` (v0.10.4)
- [x] Numeric cast inline: `float.from_int(n)` → `(n as f64)` (v0.10.4)
- [x] Borrow parameter inference: read-only String/List params → `&str` / `&[T]` (v0.10.4)
- [x] match arm を複数行に展開（各 arm を独立行、インデント付き）
- [x] 関数本体のマルチライン展開 + 4-space インデント
- [x] for/while/if-else/block のマルチライン展開 + インデント
- [x] rustfmt なしで読めるレベルの出力品質

---

## 実装戦略

**Quick win (walker のみ):**
- top-level 宣言間に空行挿入
- match arm の複数行展開
- 長い式の改行ルール

**Full (parser → IR → walker):**
- blank line / doc comment tracking を parser に追加
- IR に伝搬
- walker で emit

Quick win だけでも大幅改善。Full は Quick win の上に積む。

---

## Key Files

| ファイル | 変更内容 |
|---|---|
| `src/parser/declarations.rs` | doc comment / blank line 記録 |
| `src/ast.rs` | Decl に doc/blank_lines フィールド追加 |
| `src/lower/mod.rs` | IR に doc/blank_lines 伝搬 |
| `src/ir/mod.rs` | IrFunction に doc/blank_lines フィールド追加 |
| `src/codegen/walker/mod.rs` | emit 時のフォーマッティング |
| `src/codegen/walker/expressions.rs` | match arm / 長い式の改行 |

## Success Criteria

- `almide app.almd --target rust` 出力がソースの論理構造を保存
- Doc comment が Rust/TS 出力に反映
- match arm が複数行で出力
- rustfmt なしで人間が読めるレベル
