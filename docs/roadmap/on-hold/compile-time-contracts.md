<!-- description: Compile-time preconditions and type invariants via where clauses -->
# Compile-Time Contracts

**Priority:** 2.x — After type system stabilization
**Principle:** Reduce the probability of LLMs generating "runs but is wrong" code. Raise modification survival rate with type checking + alpha.
**Status:** General function preconditions and type-value invariants remain a
proposal. The `newtype`/function-`where` forms below are original design sketches,
not current syntax. `where` already exists in test contexts; shipped inline
generic protocol bounds are a different feature (see
[type-where-constraints.md](../active/type-where-constraints.md)).

> "Types guarantee *what something is*. Contracts guarantee *what range it falls within*."

---

## Why

Almide's type checker prevents "passing String where Int is expected." But it can't prevent "passing 0 where 0 must not be passed."

```almd
fn divide(a: Int, b: Int) -> Int = a / b

// If the LLM writes this, it type-checks. Zero division at runtime
let x = divide(10, 0)
```

Contracts are a mechanism to make function preconditions, postconditions, and type invariants verifiable at compile time. No SMT solver is used. Limited to predicates the compiler can evaluate statically — when evaluation is impossible, it degrades to a runtime check.

### Effect on Modification Survival Rate

When an LLM modifies code:
1. Type error → Detected immediately (**current state**)
2. Contract violation → Compile error or immediate runtime detection (**this proposal**)
3. Logic bug → Undetectable (remains in any language)

**Contracts fill the gap between 1 and 3.** Boundary conditions (zero division, out-of-bounds access, passing negative numbers) are patterns LLMs frequently get wrong, and they overlap with the domain contracts can catch.

---

## Design

### Function Contracts

```almd
fn divide(a: Int, b: Int) -> Int
  where b != 0
= a / b

fn clamp(value: Int, lo: Int, hi: Int) -> Int
  where lo <= hi
= if value < lo then lo
  else if value > hi then hi
  else value
```

- `where` is written between the function signature and body
- Multiple conditions are comma-separated: `where b != 0, lo <= hi`
- Conditions can only reference parameters (not local variables in the body)
- At the call site of a function with `where`, the compiler statically verifies the conditions hold

### Proposed Type Contracts (Invariants; original `newtype` syntax sketch)

```almd
type Percentage = newtype Int
  where self >= 0, self <= 100

type NonEmpty[T] = newtype List[T]
  where self.len() > 0

type Port = newtype Int
  where self >= 0, self <= 65535
```

- Used in combination with `newtype`
- `self` refers to the value itself
- Contract is verified at construction time

### Static Verification and Dynamic Degradation

The compiler processes contracts in 3 stages:

| Judgment | Action | Example |
|----------|--------|---------|
| **Statically true** | Remove check | `divide(10, 3)` — literal 3 != 0 is obvious |
| **Statically false** | Compile error | `divide(10, 0)` — literal 0 != 0 is false |
| **Unknown** | Insert runtime check | `divide(a, b)` — value of b unknown until runtime |

Scope of static verification:
- Literal value evaluation
- Constant folding (`const` propagation)
- Simple range analysis (conditions that hold inside if branches)
- Condition propagation after guard

```almd
// Statically verifiable: after guard, b != 0 is guaranteed
effect fn safe_divide(a: Int, b: Int) -> Result[Int, String] = {
  guard b != 0 else { return err("zero division") }
  ok(divide(a, b))  // ← compiler knows b != 0
}

// Cannot be statically verified: runtime check is inserted
let result = divide(x, y)
// ↓ Compiler-generated code (conceptual)
// if !(y != 0) { panic("contract violation: b != 0 at divide()") }
// let result = x / y
```

**Why no SMT solver:**
- Unpredictable compile times (Z3 is exponential in the worst case)
- "Unknown" verdicts are frequent in real code, degrading user experience
- LLMs may not write SMT-friendly predicates
- When static verification fails, runtime checks are sufficient — "crash immediately" is far better and more debuggable than "runs but is wrong"

### Cognitive Load for LLMs

`where` clauses are:
- Work without them (opt-in)
- Clear where they should be written (zero division, out-of-bounds, empty list)
- Syntax is a natural extension of function signatures

LLM learning cost: Minimal. Adding a few lines to CHEATSHEET.md is enough to enable generation.
Probability of LLMs getting `where` wrong: Low. Condition expressions use the same syntax as `if`, no new concepts.

---

## Proposed lowering (not existing contract codegen)

### Rust

```rust
fn divide(a: i64, b: i64) -> i64 {
    debug_assert!(b != 0, "contract: b != 0");
    a / b
}

// newtype
struct Percentage(i64);
impl Percentage {
    fn new(value: i64) -> Percentage {
        debug_assert!(value >= 0 && value <= 100, "contract: 0..=100");
        Percentage(value)
    }
}
```

- `debug_assert!` checks only in debug builds (removed in release builds)
- Or enable in release builds with `--contracts=always` flag

### Original TypeScript alternative (historical only; target retired)

```typescript
function divide(a: number, b: number): number {
    if (!(b !== 0)) throw new Error("contract violation: b != 0");
    return Math.trunc(a / b);
}
```

- This was an illustrative runtime-check strategy, not a current compiler target
  or a claim about all JavaScript engine optimizations.

### WASM

```wasm
(func $divide (param $a i64) (param $b i64) (result i64)
  local.get $b
  i64.eqz
  if
    unreachable  ;; contract violation
  end
  local.get $a
  local.get $b
  i64.div_s
)
```

---

## Scope Limitations — What We Won't Do

| Not doing | Reason |
|---|---|
| Postconditions (`ensures`) | Return value verification is better handled by tests in most cases. Not worth the syntax cost |
| Quantifiers (`forall`, `exists`) | Requires SMT. Compile time becomes unpredictable |
| Dependent types | Type-level programming reduces LLM accuracy |
| Conditions with side effects | Only pure fn calls allowed in `where` (len, is_empty, etc.) |
| Loop invariants | Requires a verifier. Outside contract scope |

**`where` is limited to preconditions and type invariants.** This restriction ensures no SMT is needed and LLMs can write them accurately.

---

## Relationship with Existing Features

| Existing feature | Relationship |
|---|---|
| `guard` | Runtime early return. Contract is the "compile-time promoted version" of guard |
| `effect fn` | Contracts can be attached to both pure fn and effect fn |
| Wrapper types | Proposed value invariants need a construction/validation contract; the old `newtype` sketch is not current syntax |
| Type checker | Contract verification runs after type checking, before lowering |
| nanopass | A proposed `ContractCheckPass`; not established as implemented |

### Complementary Relationship with guard

```almd
// guard: checks condition at runtime, early return on failure
effect fn parse_port(s: String) -> Result[Port, String] = {
  let n = int.parse(s)!
  guard n >= 0, n <= 65535 else { return err("invalid port") }
  ok(Port(n))
}

// contract: the Port type itself guarantees 0..65535
type Port = newtype Int
  where self >= 0, self <= 65535

// Construct Port from guard-verified value → contract statically satisfied
```

guard is for "validating user input", contract is for "baking the properties of validated values into types." The two are orthogonal and used in combination.

---

## Implementation Sketch

### Phase 1: Parser + Checker

- Specify function-contract grammar without conflicting with existing test-context `where`
- Parser: implement the adopted function/type contract grammar; original `newtype` spelling is not a current prerequisite
- AST: Add `WhereClause { conditions: Vec<Expr> }` to FnDecl / TypeDecl
- Checker: Verify that expressions in where conditions return Bool
- Checker: Restrict referenceable variables in where conditions (parameters or self only)

### Phase 2: Static Verification

- Insert `ContractCheckPass` nanopass after lowering
- Constant evaluation of literal arguments
- Condition propagation after guard / if branches (simple dataflow)
- Statically false → diagnostic error
- Statically unknown → insert runtime check node in IR

### Phase 3: Codegen

- Rust: Generate `debug_assert!`
- No TS implementation target: the original TS backend sketch is historical
- WASM: Generate `if ... unreachable`
- `--contracts=always` / `--contracts=debug` / `--contracts=off` flags

### Phase 4: Diagnostic

- Contract violation error message: `contract violated: b != 0 at divide()`
- Show caller's code location
- Hint: insert `guard b != 0 else { ... }`, or change the argument

---

## Success Criteria

- `divide(10, 0)` becomes a compile error
- `divide(10, n)` compiles with runtime check when no guard is present
- The adopted constrained-type syntax validates construction of a 0..100 value (original `newtype ... where` spelling remains a sketch)
- All existing tests pass (code without `where` is completely unaffected)
- Addition to CHEATSHEET.md is 10 lines or fewer
- Verify that LLMs can generate code correctly using `where`

## Why ON HOLD

- Requires type system and nanopass pipeline stabilization first
- Avoid conflicts with Phase 3 (Effect System)
- Outside 1.0 scope — to be considered after language spec freeze
- Revisit how value invariants compose with current wrapper types and generic bounds; do not assume the original `newtype` design is shipped
