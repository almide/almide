# Perceus Void Block Stack Balance — CI Blocker

> **Status**: Active — blocks Windows CI (Linux/macOS pass)
> **Tests**: `wasm_list_nested_map_filter`, `wasm_cross_target_spec`
> **Error**: `values remaining on stack at end of block` (wasmtime 45+)

## Problem

Perceus inserts `Ret(var_ref)` as tail expression in void blocks. This pushes a value onto the WASM stack in a context that expects no value. Strict validators (wasmtime 45+, wasmparser) reject this.

```
// IR after Perceus:
Block {
    stmts: [
        Bind { xs = list.filter(...) },
        Bind { result = list.map(xs, ...) },
        RcDec(xs),        // Perceus cleanup
    ],
    expr: Some(Var(result)),  // ← Perceus tail Ret: pushes i32
    ty: Applied(List, [Int]), // ← Perceus updated type to match tail
}
// But the enclosing function is void (ret_ty = Unit)
```

The `functions.rs` drop fix handles the function-level case, but the Block's own `.ty` is updated by Perceus to match the tail, so the `block_vt.is_none() && tail_vt.is_some()` check never triggers.

## Why Grain Doesn't Have This Problem

Grain uses **uniform representation** — all values (including void) are `i32`:
- `const_void = MConstI32(0x6FFFFFFE)` — void is a tagged i32
- Every block always returns i32 — no type mismatch possible
- `MDrop(arg)` is an explicit IR node that emits `Expression.Drop.make`

## Fix Options (ranked)

### Option A: Perceus pass — don't Ret in void blocks (recommended)

In `pass_perceus.rs`, when converting Block tail to `FnBody::Ret`, check if the enclosing function returns Unit. If so, emit the tail as a `Stmt(Expr)` + `Nop` instead of `Ret`.

```rust
// pass_perceus.rs, block_to_fnbody()
// Before:
Some(e) => FnBody::Ret { expr: *e }
// After:
Some(e) => {
    if is_void_context {
        FnBody::Expr { expr: *e, body: Box::new(FnBody::Nop) }
    } else {
        FnBody::Ret { expr: *e }
    }
}
```

The `Expr` variant wraps the value as a statement, which `emit_stmt(Expr)` will drop automatically (line 246: `if ty_to_valtype.is_some() { drop; }`).

**Pro**: Fixes at the source. No downstream hacks.
**Con**: Needs `is_void_context` threading through Perceus pass.

### Option B: ANF pass — wrap void-context tail in Drop

After ANF + Perceus, add a cleanup pass that walks all void functions' body blocks and wraps non-Unit tail expressions in a Drop statement:

```rust
if func.ret_ty == Unit && func.body.ty != Unit {
    // Convert tail to statement + drop
    let tail = take(&mut func.body.expr);
    func.body.stmts.push(Stmt::Expr(tail));
    func.body.expr = None;
    func.body.ty = Unit;
}
```

**Pro**: Simple, isolated pass. Doesn't touch Perceus.
**Con**: Post-hoc fix rather than root cause.

### Option C: Emit-level drop (current partial fix)

Already implemented in `functions.rs` (line 166): `if func_expects.is_none() && body_produces.is_some() { drop; }`. Works for the top-level function body but not for inner blocks where Perceus updated `.ty`.

**Pro**: Already partially implemented.
**Con**: Doesn't catch inner blocks. Perceus updates `.ty` so condition doesn't trigger.

## Reproduction

```bash
echo 'fn main() -> Unit = {
  let result = [1, 2, 3, 4] |> list.filter((x) => x % 2 == 0) |> list.map((x) => x * x)
  println(int.to_string(list.len(result)))
}' > /tmp/test.almd
# Fails with wasmparser validation (debug build):
cargo run -- build /tmp/test.almd --target wasm -o /tmp/test.wasm
# Passes with wasm-tools 1.245 but fails with wasmtime 45+

# Split version (no pipe) works:
echo 'fn main() -> Unit = {
  let xs = [1, 2, 3, 4]
  let filtered = list.filter(xs, (x) => x % 2 == 0)
  let result = list.map(filtered, (x) => x * x)
  println(int.to_string(list.len(result)))
}' > /tmp/split.almd
cargo run -- build /tmp/split.almd --target wasm -o /tmp/split.wasm  # OK
```

## References

- Grain `MDrop` IR node: `/tmp/grain-src/compiler/src/codegen/compcore.re:2780`
- Grain `const_void`: `/tmp/grain-src/compiler/src/codegen/mashtree.re:582`
- Perceus `block_to_fnbody`: `crates/almide-codegen/src/pass_perceus.rs:49`
- Current partial fix: `crates/almide-codegen/src/emit_wasm/functions.rs:166`
