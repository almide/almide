<!-- description: User-defined generics and protocol system for custom types -->
<!-- done: 2026-03-24 -->
# User Generics & Protocol System

**Priority:** 1.x
**Prerequisite:** Generics Phase 1 completed
**Branch:** `feature/protocol`

## Current State

### User-Defined Generics ✅ Verified Working

```almide
fn identity[A](x: A) -> A = x
fn map_pair[A, B](p: (A, A), f: (A) -> B) -> (B, B) = (f(p.0), f(p.1))
fn first[A, B](p: (A, B)) -> A = p.0

type Stack[T] = { items: List[T], size: Int }
type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])
```

All working. Checker + lower + codegen (Rust/TS) fully supported.

### Known Issues

1. **Test name and function name collision** — test "identity" + fn identity causes name collision. Test function name sanitization is insufficient

## Protocol System — Complete

**Keyword: `protocol`** (Swift/Python vocabulary)

Opens the convention system to user-defined protocols. Built-in conventions (Eq, Repr, Codec, etc.) are unified as special cases of protocols.

### Syntax

```almide
// Protocol definition
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
}

// Declare that a type satisfies a protocol (same syntax as existing conventions)
type GreetAction: Action = { greeting: String }

// Implement with convention methods (existing mechanism, no changes)
fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok(a.greeting)

// Used in generic bounds
fn run_action[T: Action](action: T, ctx: Context) -> Result[String, String] =
  action.execute(ctx)

// Implement with impl block (alternative syntax for convention methods)
impl Action for GreetAction {
  fn name(a: GreetAction) -> String = "greet"
  fn execute(a: GreetAction, ctx: Context) -> Result[String, String] =
    ok(a.greeting)
}

// Coexistence with derive
type User: Codec = { name: String, age: Int } derive(Codec)
```

### Design Principles

- `Self` is a placeholder for the implementing type (a type, not a keyword)
- Satisfaction is **explicit** — requires a `type Foo: Protocol` declaration or `impl Protocol for Foo { ... }`
- **Two implementation styles**: convention methods (`fn Foo.method`) and `impl` blocks. Both are equivalent
- Resolved via monomorphization — no dynamic dispatch
- Built-in conventions are registered as protocols (backward compatible)

### Implementation Progress

| Phase | Content | Status |
|-------|---------|--------|
| Phase 1 | AST + Parser (protocol keyword, strongly typed ProtocolMethod, `impl` blocks) | ✅ Done |
| Phase 2 | Type system infrastructure (ProtocolDef, TypeEnv extension, impl_validated) | ✅ Done |
| Phase 3 | Checker (protocol registration, satisfaction verification, impl block integration, signature verification) | ✅ Done |
| Phase 4 | Generic bounds (`fn f[T: Action](x: T)`, `[T: P1 + P2]`) | ✅ Done |
| Phase 5 | Lowerer (generic protocol method call resolution, monomorph rewriting) | ✅ Done |
| Phase 6 | Backward compatibility (integration with existing derive/convention) | ✅ Done |
| Phase 7 | Tests (92+ tests, 12 test files) | ✅ Done |

### NOT in scope

- default methods
- associated types
- dynamic dispatch / protocol objects
- orphan rules
- `derive(UserProtocol)` — only built-in conventions can be auto-derived
