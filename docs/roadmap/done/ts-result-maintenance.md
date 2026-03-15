# TS Target: Result 維持 (Erasure → Object)

## 現状

```typescript
// 現在 (erasure): effect fn → throw/catch
function safeDiv(a, b) { if (b === 0) throw "div by zero"; return a / b; }
try { safeDiv(10, 0) } catch (e) { ... }
```

## 目標

```typescript
// 目標 (Result object): effect fn → Result object
function safeDiv(a, b) {
  if (b === 0) return { ok: false, error: "div by zero" };
  return { ok: true, value: a / b };
}
const result = safeDiv(10, 0);
if (result.ok) { use(result.value); } else { handle(result.error); }
```

## Rust 側で今日確立した原則がそのまま使える

| Rust codegen の原則 | TS codegen での対応 |
|-------------------|-------------------|
| `ok(v)` → `Ok(v)` | `ok(v)` → `{ ok: true, value: v }` |
| `err(e)` → `Err(e)` | `err(e)` → `{ ok: false, error: e }` |
| auto-try `?` | `const __tmp = expr; if (!__tmp.ok) return __tmp;` |
| match Ok/Err | `if (result.ok) { ... } else { ... }` |

## IR は同じ

`IrExprKind::ResultOk`, `ResultErr`, `Try` — Rust でも TS でも同じ IR ノード。
codegen レイヤーだけ変える。

## 変更ファイル

- `src/emit_ts/lower_ts.rs` — ResultOk/Err/Try の codegen 変更
- `src/emit_ts_runtime/` — `__almd_result` ヘルパー追加
- テスト: `spec/` のテストを `--target ts` でも実行

## 見積り

2-3日。Rust codegen の auto-try ロジックをそのまま TS に移植。
