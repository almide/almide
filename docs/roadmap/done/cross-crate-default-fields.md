<!-- description: Fix cross-crate codegen for types with default field values -->
<!-- done: 2026-05-25 -->
# Cross-crate default field codegen

> **Priority: high** — blocks almai Message type from being used across crates.

## Problem

When crate A uses a type from crate B that has default field values (`field: Type = default`), the codegen generates Rust struct construction WITHOUT the default fields, causing `missing fields` errors.

## Reproduction

almai defines:
```almide
type Message = {
  role: String,
  content: String,
  tool_call_id: String = "",
  tool_calls: List[ToolCall] = [],
}
```

In a dependent crate:
```almide
import almai
let msg = almai.Message { role: "user", content: "hello" }
```

Generates:
```rust
Message { role: "user".to_string(), content: "hello".to_string() }
// ERROR: missing fields `tool_call_id` and `tool_calls`
```

Should generate:
```rust
Message { role: "user".to_string(), content: "hello".to_string(), tool_call_id: "".to_string(), tool_calls: vec![] }
```

## Root Cause

The codegen doesn't inject default values for fields with `= default` syntax when constructing types from external crates. Within the same crate, this works because the compiler has visibility into the type definition. Cross-crate, the default field info is not propagated to codegen.

## Fix

When emitting struct construction for cross-crate types with default fields, the codegen must include the default value expressions for any fields not explicitly provided.

## Related

- ProcessStatus struct injection (fixed in current develop)
- Both are cross-crate codegen issues

## Exit criteria

```almide
import almai
let msg = almai.Message { role: "user", content: "hello" }
assert_eq(msg.tool_call_id, "")
```
compiles and passes.
