# Codegen Optimization [IN PROGRESS]

Almide generates Rust code that is near-identical in performance to hand-written Rust for numeric workloads (n-body: 1.74s vs Rust 1.69s). However, heap-allocated types (String, List) incur unnecessary clone overhead. The goal is to close this gap **without exposing ownership to the user**.

### Phase 1: Eliminate unnecessary clones (transparent)

No language changes — the emitter generates smarter Rust code.

#### 1a. Last-use move analysis

If a variable's last usage is a function call or assignment, emit it directly instead of `.clone()`.

```almide
let name = "hello"
println(name)        // name is never used again
```

```rust
// Before: println!("{}", name.clone());
// After:  println!("{}", name);          // move, no clone
```

- [ ] Liveness analysis in emitter: track last usage of each variable
- [ ] Emit `.clone()` only when the variable is used again after the current expression
- [ ] Handle control flow (if/match branches) conservatively

#### 1b. String concatenation optimization

Detect `var = var ++ expr` pattern and emit `push_str` instead of allocating a new String.

```almide
var s = ""
for i in 0..n {
  s = s ++ "x"
}
```

```rust
// Before: s = format!("{}{}", s.clone(), "x".to_string());
// After:  s.push_str("x");
```

- [ ] Detect `Assign { name, value: BinOp(PlusPlus, Ident(same_name), rhs) }` pattern
- [ ] Emit `{name}.push_str(&{rhs})` for String, `.extend()` for List

### Phase 2: In-place mutation syntax

New syntax for mutating elements of `var` collections and record fields.

#### 2a. List element update

```almide
var xs = [1, 2, 3]
xs[1] = 99
```

```rust
xs[1] = 99i64;
```

- [ ] Parser: `Stmt::IndexAssign { target, index, value }`
- [ ] Checker: verify target is `var`, element type matches
- [ ] Emitter: direct index assignment

#### 2b. Record field update

```almide
var user = { name: "alice", age: 30 }
user.age = 31
```

```rust
user.age = 31i64;
```

- [ ] Parser: `Stmt::FieldAssign { target, field, value }`
- [ ] Checker: verify target is `var`, field exists, type matches
- [ ] Emitter: direct field assignment

### Phase 3: Borrow inference (future)

The compiler infers when a function parameter is read-only and emits `&str` / `&[T]` instead of owned types. Callers no longer need to clone.

```almide
fn len(s: String) -> Int = string.len(s)
```

```rust
// Before: fn len(s: String) -> i64 { s.clone().len() as i64 }
// After:  fn len(s: &str) -> i64 { s.len() as i64 }
```

- [ ] Analyze function bodies: does the parameter escape, get stored, or get mutated?
- [ ] If read-only: emit `&str` for String, `&[T]` for List
- [ ] Adjust call sites: pass `&x` instead of `x.clone()`

### Priority order

| Step | Difficulty | Impact | User-visible change |
|---|---|---|---|
| 1a. Last-use move | Medium | High | None (transparent) |
| 2a. List element update | Low | High | New syntax `xs[i] = v` |
| 1b. String concat optimization | Low | Medium | None (transparent) |
| 2b. Record field update | Low | Medium | New syntax `r.f = v` |
| 3. Borrow inference | High | High | None (transparent) |

---
