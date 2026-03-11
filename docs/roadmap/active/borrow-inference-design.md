# Borrow Inference — Detailed Design

## Motivation

Almide currently emits `.clone()` for every variable passed to a function:

```almide
fn process(name: String, items: List[Int]) -> Int = {
  let n = string.len(name)
  let total = list.sum(items)
  n + total
}
```

```rust
// Current output — 2 unnecessary clones
fn process(name: String, items: Vec<i64>) -> i64 {
    let n = almide_rt_string_len(&*name.clone());
    let total = almide_rt_list_sum(&items.clone());
    n + total
}
```

```rust
// After borrow inference — zero clones
fn process(name: &str, items: &[i64]) -> i64 {
    let n = almide_rt_string_len(name);
    let total = almide_rt_list_sum(items);
    n + total
}
```

Callers benefit too:
```rust
// Before: process(name.clone(), items.clone())
// After:  process(&name, &items)
```

## Scope

Only affects **Rust codegen**. TypeScript target is unaffected (JS uses reference semantics for objects).

Only applies to **heap-allocated types**: `String`, `Vec<T>` (List), `HashMap<K,V>` (Map). Primitive types (`i64`, `f64`, `bool`, `()`) are `Copy` and never cloned.

## Architecture

```
┌─────────────┐    ┌──────────────────┐    ┌────────────────┐
│ Parse + Check│ →  │ Escape Analysis   │ →  │ Rust Emitter   │
│ (AST + types)│    │ (per-fn analysis) │    │ (uses borrow   │
│              │    │                   │    │  info for sigs) │
└─────────────┘    └──────────────────┘    └────────────────┘
```

New module: `src/emit_rust/borrow.rs`

## Data Structures

```rust
/// Classification of a function parameter for ownership.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParamOwnership {
    Borrow,  // Can be emitted as &str / &[T]
    Owned,   // Must be emitted as String / Vec<T>
}

/// Borrow analysis results for all functions in the program.
struct BorrowInfo {
    /// fn_name → vec of ParamOwnership (one per param)
    fn_params: HashMap<String, Vec<ParamOwnership>>,
}
```

## Escape Analysis Rules

Walk the function body AST. For each parameter `p` of heap type:

### Escapes (→ Owned)

1. **Returned directly or in expression**
   ```almide
   fn id(s: String) -> String = s           // s escapes via return
   fn wrap(s: String) -> List[String] = [s]  // s escapes into list
   ```

2. **Stored in a data structure**
   ```almide
   fn f(s: String) -> Unit = {
     let xs = [s, "other"]    // s stored in list literal
   }
   ```

3. **Assigned to a mutable variable**
   ```almide
   fn f(s: String) -> Unit = {
     var x = s                // s escapes into mutable binding
   }
   ```

4. **Captured by a lambda**
   ```almide
   fn f(s: String) -> Unit = {
     let g = fn(x) => s ++ x  // s captured by closure
   }
   ```

5. **Passed to another user function that takes owned**
   ```almide
   fn f(s: String) -> Unit = other(s)  // depends on other's analysis
   ```
   In Phase 3a, conservatively treat all user-fn args as owned.
   In Phase 3b, use fixpoint results.

6. **Used in `++` concatenation** (consumes ownership for efficiency)
   ```almide
   fn f(s: String) -> String = s ++ " world"  // s consumed by AlmideConcat
   ```

### Does NOT escape (→ Borrow)

1. **Passed to stdlib functions** — all stdlib runtime fns take `&str`/`&[T]`:
   ```almide
   fn f(s: String) -> Int = string.len(s)      // &str ok
   fn g(xs: List[Int]) -> Int = list.sum(xs)    // &[i64] ok
   ```

2. **Used in comparisons** (`==`, `!=`, `<`, etc.)
   ```almide
   fn f(s: String) -> Bool = s == "hello"  // &str ok
   ```

3. **Used in println/eprintln** — takes `&str` via `format!`
   ```almide
   fn f(s: String) -> Unit = println(s)  // &str ok
   ```

4. **Field access** (for records/maps)
   ```almide
   fn f(m: Map[String, Int]) -> Option[Int] = map.get(m, "key")
   ```

5. **Not used at all** (dead parameter — still borrow, cheaper to pass)

## Type Mapping

| Almide type | Owned Rust type | Borrowed Rust type |
|---|---|---|
| `String` | `String` | `&str` |
| `List[T]` | `Vec<T>` | `&[T]` |
| `Map[K, V]` | `HashMap<K, V>` | `&HashMap<K, V>` |

## Emitter Changes

### Function signature

```rust
// Before (all owned):
fn process(name: String, items: Vec<i64>) -> i64

// After (borrow where possible):
fn process(name: &str, items: &[i64]) -> i64
```

### Inside function body

When a param is borrowed, uses change:

| Context | Owned emit | Borrowed emit |
|---|---|---|
| Stdlib call `string.len(s)` | `almide_rt_string_len(&*s.clone())` | `almide_rt_string_len(s)` |
| Stdlib call `list.sum(xs)` | `almide_rt_list_sum(&xs.clone())` | `almide_rt_list_sum(xs)` |
| Comparison `s == "hi"` | `almide_eq!(s.clone(), "hi")` | `almide_eq!(s, "hi")` |
| Println `println(s)` | `println!("{}", s.clone())` | `println!("{}", s)` |

Key: when a param is `&str`, the `.clone()` and `&*` wrapping both become unnecessary. The param is already a reference.

### Call sites

```rust
// Before:
let result = process(name.clone(), items.clone());

// After (callee params are borrow):
let result = process(&name, &items);

// After (callee params are owned, last use):
let result = consume(name);  // move

// After (callee params are owned, not last use):
let result = consume(name.clone());  // clone
```

### gen_arg changes

```rust
// Current:
fn gen_arg(&self, expr: &Expr) -> String {
    match expr {
        Ident { name } if single_use => gen_expr(expr),
        Ident { .. } => format!("{}.clone()", gen_expr(expr)),
        _ => gen_expr(expr),
    }
}

// After:
fn gen_arg(&self, expr: &Expr, callee: &str, param_idx: usize) -> String {
    let ownership = self.borrow_info.param_ownership(callee, param_idx);
    match (expr, ownership) {
        (Ident { name }, Borrow) => format!("&{}", gen_expr(expr)),
        (Ident { name }, Owned) if single_use => gen_expr(expr),
        (Ident { .. }, Owned) => format!("{}.clone()", gen_expr(expr)),
        (_, Borrow) => format!("&({})", gen_expr(expr)),  // non-ident borrow
        (_, Owned) => gen_expr(expr),
    }
}
```

## Phase 3a Implementation Plan

### Step 1: `src/emit_rust/borrow.rs` — Escape analysis

```rust
pub fn analyze_program(program: &Program) -> BorrowInfo {
    let mut info = BorrowInfo::new();
    for decl in &program.decls {
        if let Decl::Fn { name, params, body, .. } = decl {
            let ownerships = analyze_fn(params, body, &info);
            info.fn_params.insert(name.clone(), ownerships);
        }
    }
    info
}

fn analyze_fn(params: &[Param], body: &Expr, _info: &BorrowInfo) -> Vec<ParamOwnership> {
    let heap_params: Vec<&str> = params.iter()
        .filter(|p| is_heap_type(&p.ty))
        .map(|p| p.name.as_str())
        .collect();

    let mut escaped: HashSet<String> = HashSet::new();
    check_escape(body, &heap_params, &mut escaped);

    params.iter().map(|p| {
        if !is_heap_type(&p.ty) {
            ParamOwnership::Owned  // primitives don't need borrow
        } else if escaped.contains(&p.name) {
            ParamOwnership::Owned
        } else {
            ParamOwnership::Borrow
        }
    }).collect()
}
```

### Step 2: Wire into `Emitter`

- Add `borrow_info: BorrowInfo` field to `Emitter` struct
- Run `analyze_program()` before `emit_program()`
- Use in `emit_fn_decl` (param types) and `gen_arg` (call sites)

### Step 3: Handle stdlib call templates

Current TOML templates use `&*{s}` for stdlib calls:
```toml
rust = "almide_rt_string_len(&*{s})"
```

When `s` is `&str`, `&*s` is redundant but correct (`&*(&str)` = `&str`). So **no TOML changes needed** — the Rust compiler optimizes this away.

For non-reference params (`s: String`), the `&*` coercion still works.

### Step 4: Test strategy

1. Compile all existing tests — must still pass
2. Inspect generated Rust for key patterns:
   - `fn process(name: &str, ...)` where appropriate
   - `process(&name, ...)` at call sites
3. Verify Rust compiles without warnings

## Phase 3b: Inter-function fixpoint

```
worklist = all functions
while worklist not empty:
    fn = worklist.pop()
    old = info.fn_params[fn]
    new = analyze_fn(fn.params, fn.body, info)  // uses callee info
    if new != old:
        info.fn_params[fn] = new
        worklist.extend(callers_of(fn))
```

Convergence is guaranteed: params can only change Borrow → Owned (monotone lattice), and there are finitely many params.

## Edge Cases

### Generic functions
```almide
fn identity[T](x: T) -> T = x  // x escapes (returned)
```
Generic params are always Owned (can't know if T is heap-allocated at compile time).

### Effect functions
```almide
effect fn load(path: String) -> String = fs.read_text(path)
```
`path` is passed to `fs.read_text` (stdlib) which takes `&str` → Borrow.

### Recursive functions
```almide
fn f(s: String) -> Unit = f(s)  // s passed to self → depends on self
```
Fixpoint: initialize as Borrow, analyze: `s` passed to `f`'s first param (Borrow) → stays Borrow. Correct.

### Pattern matching
```almide
fn f(s: String) -> Int = match s {
  "a" => 1
  _ => 2
}
```
`match` on `s` — Rust `match` can take `&str` via `match s { "a" => ..., _ => ... }`. Borrow-safe.

## Compatibility

- **TS emitter**: unaffected (no ownership in JS)
- **WASM target**: uses Rust codegen, benefits from borrow inference
- **Bundled stdlib (.almd)**: treated as user modules, analyzed normally
- **@extern functions**: params always Owned (can't analyze external code)
