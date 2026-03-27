<!-- description: Static verification that pure functions cannot perform I/O -->
<!-- done: 2026-03-17 -->
# Effect Isolation (Security Layer 1)

Pure fns cannot perform I/O. Statically verified by the compiler.

## Design

```
fn parse(s: String) -> Value = ...          // pure. Cannot do I/O
effect fn load(path: String) -> String = ... // Can do I/O
```

- `fn` cannot call `effect fn`. Compiler enforces this
- Pure fns cannot access the outside world at all. Data exfiltration and external communication are type errors
- **Security implication**: If a package only exports pure fns, that package is inherently harmless
- Same for stdlib effect fns (`fs.read_text` etc. error when called from pure fn)
- `fan` blocks also error inside pure fns

## Implementation

- Checker: `src/check/calls.rs` — error on `sig.is_effect && !self.env.in_effect`
- Tests: `tests/checker_test.rs` — 7 tests (pure→effect, effect→effect, test→effect, stdlib effect, fan in pure/effect)
