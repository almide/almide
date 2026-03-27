<!-- description: Three-layer codegen architecture for multi-target extensibility -->
<!-- done: 2026-03-18 -->
# Codegen v3: Three-Layer Architecture

**優先度:** High — 1.0 後の target 拡張（Go, Python）の前提条件
**状態:** Phase 4 完了、Phase 5 進行中
**テスト:** Rust 72/72 (100%), TS cross-target 106/106 (100%)

## 実装済みアーキテクチャ

```mermaid
flowchart TB
    subgraph BuildTime["Build Time (build.rs)"]
        TOML["stdlib/defs/*.toml"]
        BUILDRS["build.rs"]
        ARGTX["arg_transforms.rs\n282 functions × ArgTransform"]
        TOML --> BUILDRS --> ARGTX
    end

    subgraph Pipeline["Runtime Pipeline"]
        IR["IrProgram (typed IR)"]

        subgraph RustPasses["Rust Passes"]
            RP1["StdlibLoweringPass"]
            RP2["ResultPropagationPass"]
            RP3["BuiltinLoweringPass"]
        end

        subgraph TSPasses["TypeScript Passes"]
            TP1["MatchLoweringPass"]
            TP2["ResultErasurePass"]
            TP3["ShadowResolvePass"]
        end

        subgraph Shared["Shared Passes"]
            SP1["FanLoweringPass"]
        end

        subgraph Annotations["Annotations"]
            ANN["clone_vars, deref_vars\nlazy_vars, ctor_to_enum\nboxed_fields, default_fields\nrecursive_enums"]
        end

        subgraph Walker["Layer 3: Template Walker (target-agnostic)"]
            TMPL["TOML Templates\nrust.toml / typescript.toml"]
            WALK["walker.rs\nis_rust() = 0\nctx.target branch = 0"]
            TMPL --> WALK
        end
    end

    IR --> RustPasses --> Shared
    IR --> TSPasses --> Shared
    Shared --> ANN --> WALK
    ARGTX -.-> RP1
    WALK --> OUT["Rust / TypeScript source"]

    style Pipeline fill:#1a1a2e,stroke:#e94560,stroke-width:2px
    style Walker fill:#16213e,stroke:#00ff00
```

## Nanopass 一覧

| Pass | Target | 責務 |
|------|--------|------|
| StdlibLoweringPass | Rust | Module→Named + arg_transforms (Borrow/ToVec/Clone/WrapSome) |
| ResultPropagationPass | Rust | auto-? for effect fn (match subject 除外) |
| BuiltinLoweringPass | Rust | assert→RustMacro, println→RustMacro, value_*→almide_rt_ |
| MatchLoweringPass | TS/Py/Go | match→if/else chain (Constructor, RecordPattern, guard対応) |
| ResultErasurePass | TS/Py | ok(x)→x, err(e)→throw, Try→identity, Result ret type→T |
| ShadowResolvePass | TS/Py | let shadowing → assignment (const/let再宣言回避) |
| BoxDerefPass | Rust | recursive pattern variable *deref |
| ClonePass | Rust | heap-type variable .clone() |
| FanLoweringPass | All | (placeholder — fan rendering is template-driven) |

## TOML テンプレート (40+ per target)

両ターゲットで同一walkerから異なるコードを生成:

| 構成 | Rust | TypeScript |
|------|------|-----------|
| if_expr | `if {cond} {{ {then} }} else {{ {else} }}` | `(({cond}) ? ({then}) : ({else}))` |
| block_expr | `{{\n{body}\n}}` | `(() => {{\n{body}\n}})()` |
| fan_expr | `std::thread::scope(\|__s\| {{ ... }})` | `await Promise.all([{exprs}])` |
| enum_decl | `#[derive(...)] enum {name} {{ {variants} }}` | `// type {name}\n{variants}` |
| ctor_call | `{enum_name}::{ctor_name}({args})` | `{ctor_name}({args})` |
| tuple_literal | `({elements})` | `[{elements}]` |
| deref_var | `(*{name})` | `{name}` |
| clone_expr | `{expr}.clone()` | `{expr}` |

## 進捗

### ✅ Phase 1: Template + Walker (完了)

- TOML テンプレートエンジン (type/attr guards, array rules)
- rust.toml / typescript.toml (60+ 構文)
- IR walker (全 IrExprKind + IrStmtKind カバー)

### ✅ Phase 2: Nanopass Pipeline (完了)

- 6 Rust passes + 3 TS passes
- build.rs arg_transforms テーブル (282 functions, WrapSome対応)

### ✅ Phase 3: Annotations + gen_generated_call 排除 (完了)

- CodegenAnnotations: clone_vars, deref_vars, lazy_vars, boxed_fields, default_fields
- walker から gen_generated_call 依存を完全排除

### ✅ Phase 4: walker target-agnostic 化 (完了)

- `is_rust()` **42 → 0** (メソッド自体を削除)
- `ctx.target ==` 分岐 **0** (fan含む全てテンプレート化)
- break-in-IIFE 解決 (contains_loop_control でIIFE回避)
- TS cross-target **106/106** (0 fail)

### ✅ Phase 5: 既存 codegen 完全置換 (完了)

- v3 がデフォルト codegen (`almide run/build/test`)
- `almide emit --target rust/ts` で v3 出力
- legacy codegen は `legacy-rust/legacy-ts` でアクセス可能

### Phase 6: 新ターゲット (post-1.0)

Go/Python targetは1.0後。アーキテクチャは準備完了（TOML + pass追加のみで対応可能）。

1. Go target: `go.toml` + Go-specific passes
2. Python target: `python.toml` + Python passes

## 成功基準

- [x] spec/lang 45/45 pass (Rust)
- [x] spec/stdlib 14/14 pass (Rust)
- [x] spec/integration 13/13 pass (Rust)
- [x] cross-target 106/106 pass (TS)
- [x] gen_generated_call 依存排除
- [x] build.rs arg_transforms テーブル生成
- [x] 9 Nanopass 実装
- [x] walker `is_rust()` ゼロ
- [x] walker `ctx.target ==` 分岐ゼロ
- [x] 既存 codegen 完全置換
- Go target → [on-hold](../on-hold/go-target.md)
