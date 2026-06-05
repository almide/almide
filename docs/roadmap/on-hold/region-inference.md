<!-- description: Region-based memory management — Phase 1+2 shipped, Phase 3 (full inference) on hold for server workloads -->
# Region-based Memory Management

> **Status**: On hold — Phase 1+2 shipped (v0.23.4) and cover the practical patterns.
> Phase 3 (full region inference) targets long-running servers, a secondary use case;
> revisit when that workload becomes primary.
> Almide WASM uses a bump allocator with automatic region scoping.

## Implemented

### Phase 1: Function-level region (v0.23.4)

Unit-returning functions and lambdas automatically save/restore the
heap pointer at entry/exit. All temporary allocations inside are
reclaimed when the function returns.

**Covers**: bench callbacks, event handlers, request processors.

### Phase 2a: Loop iteration region (v0.23.4)

While loops whose body doesn't assign heap values to outer-scope
variables automatically scope each iteration. Temporary allocations
are reclaimed per iteration.

**Covers**: `while i < N { let _r = list.map(...); i = i + 1 }`

### Phase 2b: Block liveness region (v0.23.4)

For each heap-typed let binding in a block, the compiler finds the
last reference and inserts heap restore after it. Allocations are
reclaimed as soon as they're no longer used.

**Covers**: `let step1 = map(...); let step2 = filter(step1, ...); // step1 freed here`

## Phase 3: Full region inference (future)

### Remaining cases

1. **Nested heap structures**: `List[List[Int]]` — inner lists can't be
   individually freed by bump restore (restoring frees everything above the mark)

2. **Mutable variable replacement**: `var s = "hello"; s = "world"` — the old
   "hello" allocation is leaked (can't restore without invalidating "world")

3. **Closure captures**: Values captured by closures may outlive the capturing
   scope. Region inference must track capture lifetimes.

4. **Cross-function region polymorphism**: `fn make_list() -> List[Int]` —
   the returned list's region depends on the caller's context

### Design approach: MLKit-style region inference

- **Region type annotations**: Every heap type carries an implicit region variable.
  `List[Int]@r1` means "list allocated in region r1".

- **Region inference**: Hindley-Milner unification extended with region constraints.
  Region variables are inferred, not written by the user.

- **Region polymorphism**: `fn map[A, B, r1, r2](xs: List[A]@r1, f: A -> B) -> List[B]@r2`
  The output region r2 is determined by the call site.

- **Region effects**: Functions declare which regions they allocate in.
  `fn f() -r1-> T` means "f allocates in region r1".

- **Letregion**: `letregion r in { ... }` creates a region that's freed at the
  block exit. The compiler inserts these automatically via inference.

### Implementation scope

- **Frontend**: Region variable inference in almide-frontend type checker
- **IR**: Region annotations on IrExpr types
- **Codegen**: Region-aware allocation (multiple bump pointers, one per active region)
- **Runtime**: Region stack (push/pop regions instead of single heap_ptr)

### References

- Tofte & Talpin (1997): "Region-Based Memory Management"
- MLKit compiler: https://github.com/melsman/mlkit
- Cyclone language: Region-based safe C

### Pragmatic observation

Phase 1+2 already cover the vast majority of practical WASM programs:
- Short-lived processes (Phase 1)
- Processing loops (Phase 2a)
- Pipeline transformations (Phase 2b)

Phase 3 is most valuable for long-running servers with complex state
management — a use case that's currently secondary to Almide's primary
targets (WASM modules, Edge functions, CLI tools).
