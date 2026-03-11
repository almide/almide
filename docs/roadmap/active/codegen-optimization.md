# Codegen Optimization [IN PROGRESS]

Almide generates Rust code that is near-identical in performance to hand-written Rust for numeric workloads (n-body: 1.74s vs Rust 1.69s). However, heap-allocated types (String, List, Map) incur unnecessary clone overhead. The goal is to close this gap **without exposing ownership to the user**.

### Phase 0: Correctness fixes ✅

`vec![f1, f2]` moved variables, causing use-after-move. Fixed by emitting `.clone()` for Ident expressions inside list literals.

### Phase 1: Eliminate unnecessary clones ✅

No language changes — the emitter generates smarter Rust code.

#### 1a. Single-use move analysis ✅

Variables used exactly once in a function body skip `.clone()` (safe to move). Conservative: for-loops and lambdas count as multi-use. Parameters always cloned.

#### 1b. String/List concatenation optimization ✅

`var = var ++ expr` → `push_str` / `.extend()` via `AlmidePushConcat` trait dispatch.

### Phase 2: In-place mutation syntax ✅

#### 2a. List element update ✅ — `xs[i] = v`
#### 2b. Record field update ✅ — `r.f = v`

---

### Phase 3: Borrow Inference (Lobster-style auto escape analysis)

**Design doc: [borrow-inference-design.md](./borrow-inference-design.md)**

The compiler infers when a function parameter is read-only and emits `&str` / `&[T]` / `&HashMap<K,V>` instead of owned types. Callers pass `&x` instead of `x.clone()`. Zero user-facing changes.

#### 3a. Intra-function escape analysis

For each user-defined function, analyze whether each heap-type parameter escapes:

| Escape condition | Example | Result |
|---|---|---|
| Returned | `fn id(s: String) -> String = s` | owned |
| Stored in collection | `[s, "other"]` | owned |
| Stored in record | `{ name: s }` | owned |
| Passed to owned param of another user fn | `other_fn(s)` (if `other_fn.s` is owned) | owned |
| Captured by lambda | `fn(x) => s ++ x` | owned |
| Assigned to var | `var x = s` | owned |
| **None of the above** | `string.len(s)`, `println(s)` | **borrow** |

- [ ] `EscapeAnalysis` pass: walk each fn body, classify params as `Borrow` or `Owned`
- [ ] Emit `&str` / `&[T]` / `&HashMap<String, T>` for borrow params
- [ ] Emit `&x` at call sites for borrow params; `x.clone()` or move for owned
- [ ] Stdlib calls: already take `&str`/`&[T]` — no changes needed
- [ ] Other user fn calls: conservatively treat as owned (Phase 3b resolves this)

#### 3b. Inter-function fixpoint analysis

A calls B with param `x`. Whether `x` escapes in A depends on B's classification of that param. Requires fixpoint iteration:

```
1. Initialize all params as Borrow
2. For each function, analyze escape conditions
3. If any param changes Borrow → Owned, re-analyze callers
4. Repeat until no changes (convergence)
```

- [ ] Build call graph: fn → [callees with param mapping]
- [ ] Fixpoint loop with worklist algorithm
- [ ] Handle recursion: params in recursive positions → Owned

#### 3c. Map type borrow

`HashMap<String, T>` → `&HashMap<String, T>`. Simpler than String/List because maps are rarely constructed from parameters.

---

### Priority / Status

| Phase | Status | Impact |
|---|---|---|
| 0. Correctness | ✅ Done | Prerequisite |
| 1a. Single-use move | ✅ Done | High — most common pattern |
| 1b. Concat optimization | ✅ Done | Medium — loop perf |
| 2a. List index assign | ✅ Done | High — mutable algorithms |
| 2b. Field assign | ✅ Done | Medium — record mutation |
| **3a. Intra-fn borrow** | **Next** | **High — idiomatic Rust** |
| 3b. Inter-fn fixpoint | After 3a | High — optimal |
| 3c. Map borrow | After 3b | Low |
