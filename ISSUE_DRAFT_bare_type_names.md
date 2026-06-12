# Bare type names reach codegen for several reference shapes (#433 family) — fixed by entry-point repair + complete link mangle

## Symptom

Since v0.26.16 (PR #476 namespacing), any package whose modules use these
shapes dies with `[COMPILER BUG] unresolvable bare type name(s) reached
codegen` — or, for shapes the verifier did not scan, with raw rustc E0425 on
the generated code:

| Reference shape | Example | ≤0.27.3 |
|---|---|---|
| Aliased qualified annotation | `import m as q` + `fn f(x: q.Cfg)` | NG |
| `Option[OwnType]` fn return | `fn find(xs) -> Option[Thing]` | NG |
| Own variant in any signature | `fn read(..) -> (GGUFValue, Int)` | NG |
| Lambda/closure param & return types | `list.fold(ts, d, (acc, t) => …)` | NG (E0425) |
| `Call.type_args` / `RcWrap.cast_ty` | `Rc<dyn Fn(i64) -> Thing>` cast | NG (E0425) |
| Direct param/return, `List[T]`, tuples | | OK |

Minimal repro (16 lines): module `things` with `type Thing` + a fold whose
lambda params are `Thing`; import it from a test file. Real-world impact:
`nn` package — generate.almd / whisper_loader.almd / gguf.almd stopped
compiling from 0.26.16 onward.

## Root cause

PR #476 pins module type DECLARATIONS to qualified names (`m.Type`) and
mangles them at link, but several reference producers still emit bare names,
and the link mangle missed Ty positions stored outside `expr.ty`:
`IrExprKind::Lambda.params`, `ClosureCreate.captures`, `Call.type_args`,
`RcWrap.cast_ty`.

## Fix (branch `fix-bare-type-refs-in-lambdas`, commit 92dd805b)

1. **`verify_names::repair_bare_type_names`** — runs at codegen entry, before
   the gate: rewrites every unambiguous bare reference (exactly one qualified
   declaration, no bare twin) to its canonical qualified name, across all Ty
   positions including the four missed ones. Ambiguous names are left for the
   gate to reject, so the machine-checked invariant is unchanged.
2. **`pass_ir_link_flatten`** — the qualified→mangled rename now also covers
   Lambda params, ClosureCreate captures, Call type_args, RcWrap cast_ty.
3. **verifier** — scans those positions too.
4. +5 unit tests (repair completes / refuses ambiguous / respects bare-decl
   shadowing / reaches lambda params).

Validation: almide-codegen 107/107, spec suite 265/265, all repro shapes
above compile and run on both targets.

## Related but separate (NOT in this branch's scope)

- `impl AlmideRepr for AlmideMatrix` was missing → any record containing a
  Matrix failed **native** compile across many versions (unnoticed because
  `almide test` prefers WASM). Added in runtime/rs/src/matrix.rs (nested-list
  literal form). Included in the commit.
- `src/cli/mod.rs replace_matrix_runtime`: the burn-splice marker
  (`pub type AlmideMatrix = Vec<Vec<f64>>`) went stale with the flat-struct
  migration — it now matches the embedded almide-kernel bridge alias and
  splices the enum runtime into the wrong scope, breaking ALL native matrix
  builds at HEAD (v0.27.3 included). The commit contains a marked LOCAL
  workaround (skip the splice); the real fix belongs to the flat-matrix
  migration. **Review this part before merging.**
- Codegen demotes Bytes params to by-value when the fn contains a
  data-capturing fold closure (`skip_value(data: Vec<u8>)` vs sibling fns'
  `&Vec<u8>`) — a 2.4 GB GGUF buffer gets cloned per call. Perf, not
  correctness; worth its own issue.
