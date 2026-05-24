<!-- description: SSE runtime functions missing in cross-crate codegen -->
# Cross-crate SSE runtime functions

> **Priority: high** — blocks almai from being used as a dependency.

## Problem

When a crate depends on almai, the SSE (Server-Sent Events) streaming runtime functions are referenced in generated code but their definitions are not emitted:

```
error[E0425]: cannot find function `almide_sse_anthropic_messages` in this scope
error[E0425]: cannot find function `almide_sse_openai_chat` in this scope
```

## Root Cause

Same pattern as the ProcessStatus struct injection bug: the IR module tracking (`used_stdlib_modules` / `ir_link`) doesn't register the SSE module when it's only used transitively through a dependency.

almai's `call_streaming` uses these functions. Even though the downstream crate doesn't call `call_streaming`, the full almai module is compiled and references these symbols.

## Fix

Ensure the SSE runtime module is included in `needed` when any dependent code references `almide_sse_*` symbols. This should already be handled by the new `ir_link` approach — check if SSE functions are registered as a separate module or if they need explicit tracking.

## Exit criteria

```almide
[dependencies]
almai = { git = "https://github.com/almide-ai/almai" }
```

```almide
import almai
test "call almai" {
  let msg = almai.user("hello")
  assert_eq(msg.role, "user")
}
```

compiles without SSE errors.
