<!-- description: Historical v0.23.4 region work and an unimplemented full-region-inference proposal -->
# Region-based Memory Management

> **Status**: Full region inference remains an on-hold proposal. Phases 1+2
> below record the v0.23.4 implementation, not the current Wasm allocator.
> The structural backend now uses reference counting and size-class free lists
> (`crates/almide-wasm/src/alloc_inline.rs`); do not infer a current leak from
> the limitations of the old bump-restore design.

## Historical implementation (v0.23.4)

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

### Cases motivating the original proposal

These describe limits of bump-only region restoration, not a current defect
inventory. Nested-value drops and mutable replacement must be evaluated against
the current ownership/drop implementation before proposing a new allocator.

1. **Nested heap structures**: `List[List[Int]]` — inner lists can't be
   individually freed by bump restore (restoring frees everything above the mark)

2. **Mutable variable replacement**: `var s = "hello"; s = "world"` — the old
   "hello" allocation cannot be reclaimed by that bump restore alone
   (restoring would invalidate "world"); this is not a claim about current RC behavior

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

### Original motivation

The v0.23.4 plan prioritized these patterns for Phases 1+2:
- Short-lived processes (Phase 1)
- Processing loops (Phase 2a)
- Pipeline transformations (Phase 2b)

The original motivation for Phase 3 was long-running servers with complex
state. Reopening this proposal requires a measured need beyond current RC and
free-list reclamation; the historical region plan alone does not establish one.
