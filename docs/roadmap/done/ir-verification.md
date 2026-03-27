<!-- description: Debug-time IR integrity checks and self-describing IR nodes -->
# IR Verification & Self-Describing IR

Debug-only integrity checks + IR self-description improvements. Verification runs after optimization, before monomorphization. Self-describing IR ensures every node's meaning is unambiguous without type inspection.

## Implemented

### IR Verification (19 tests)

- [x] **VarId bounds** — every variable reference maps to a valid VarTable entry
- [x] **Parameter VarId uniqueness** — no two params in a function share the same VarId
- [x] **Loop context** — Break/Continue only inside ForIn, While, or DoBlock
- [x] **BinOp/UnOp type dispatch** — operator variant matches operand types (all 22 variants)
- [x] **MapAccess/IndexAccess type constraints** — MapAccess only on Map, IndexAccess not on Map
- [x] **Duplicate record fields** — no two fields share the same name
- [x] **Duplicate variant cases** — no two cases share the same name
- [x] **Module coverage** — all checks apply to imported user modules

### Self-Describing IR

- [x] **PowInt** — split from PowFloat. Lowerer dispatches `**` by operand type (like all other arithmetic ops)
- [x] **MapAccess / MapInsert** — split from IndexAccess / IndexAssign. Map key lookup vs list index access are distinct IR nodes
- [x] **MatchSubjectPass** — `.as_str()` / `.as_deref()` insertion moved from walker to nanopass. Walker no longer checks types for match subjects

### Infrastructure

- [x] **IrVisitor trait** (`src/ir/visit.rs`) — shared walker for read-only IR passes. verify.rs and unknown.rs migrated
- [x] **ExprId** — already complete (HashMap<ExprId, Ty>, parser-allocated IDs)

## Phase 2 — Done

| Check | Status |
|-------|--------|
| **CallTarget validity** | ✅ Module function calls validated against known functions (22→25 tests) |
| **Constructor pattern type propagation** | ✅ Checker + Lowerer: payload types extracted from subject type instead of Unknown |

## Phase 2 — Deferred

| Check | Reason |
|-------|--------|
| **Use-count cross-check** | Low priority. Existing use_count works; errors surface as rustc compile failures |
| **Migrate use_count.rs to IrVisitor** | Special loop/lambda bump semantics don't fit IrVisitor cleanly. Risk/reward ratio too high |
| **Migrate ResultErr/OptionNone to nanopass** | Walker type checks are minimal (2 sites), nanopass would add IR complexity without benefit |

## Design Principles

1. **IR self-description**: Every IR node's meaning is determined by its variant, not by runtime type inspection
2. **Walker = pure renderer**: The template walker (Layer 3) reads IR and annotations, never checks types
3. **Nanopass = semantic transform**: Type-dependent decisions happen in nanopass passes (Layer 2)
4. **Verification = feedback loop**: Self-describing IR enables type constraint verification

## Affected files

| File | Role |
|------|------|
| `src/ir/verify.rs` | Verification logic (19 tests) |
| `src/ir/visit.rs` | IrVisitor trait + walk functions |
| `src/ir/mod.rs` | PowInt, MapAccess, MapInsert, module registration |
| `src/codegen/pass_match_subject.rs` | MatchSubject nanopass (Rust-only) |
| `src/codegen/walker.rs` | Type dispatch removal |
| `src/lower/expressions.rs` | PowInt/MapAccess lowering |
| `src/lower/statements.rs` | MapInsert lowering |
| `src/main.rs` | Verification pipeline insertion |
