<!-- description: Replace TS Result erasure (throw/catch) with Result objects -->
<!-- done: 2026-03-16 -->
# TS Target: Result Maintenance (Erasure to Object)

## Current State

```typescript
// Current (erasure): effect fn → throw/catch
function safeDiv(a, b) { if (b === 0) throw "div by zero"; return a / b; }
try { safeDiv(10, 0) } catch (e) { ... }
```

## Goal

```typescript
// Goal (Result object): effect fn → Result object
function safeDiv(a, b) {
  if (b === 0) return { ok: false, error: "div by zero" };
  return { ok: true, value: a / b };
}
const result = safeDiv(10, 0);
if (result.ok) { use(result.value); } else { handle(result.error); }
```

## Principles Established on the Rust Side Apply Directly

| Rust codegen principle | TS codegen equivalent |
|------------------------|----------------------|
| `ok(v)` → `Ok(v)` | `ok(v)` → `{ ok: true, value: v }` |
| `err(e)` → `Err(e)` | `err(e)` → `{ ok: false, error: e }` |
| auto-try `?` | `const __tmp = expr; if (!__tmp.ok) return __tmp;` |
| match Ok/Err | `if (result.ok) { ... } else { ... }` |

## Same IR

`IrExprKind::ResultOk`, `ResultErr`, `Try` — same IR nodes for both Rust and TS.
Only the codegen layer changes.

## Changed Files

- `src/emit_ts/lower_ts.rs` — codegen changes for ResultOk/Err/Try
- `src/emit_ts_runtime/` — add `__almd_result` helper
- Tests: run `spec/` tests with `--target ts` as well

## Estimate

2-3 days. Port the Rust codegen auto-try logic directly to TS.
