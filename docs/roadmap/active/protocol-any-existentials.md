<!-- description: Declared conformance + opt-in `any P` existentials — take Go's interface-value ergonomics without its implicit-satisfaction and nil-interface traps; the one Swift idea worth stealing, none of the rest -->
# Protocols: declared conformance + opt-in `any P`

> The 2026 protocol-design landscape has no single winner, but it has settled
> verdicts. This roadmap takes exactly one feature (`any P` existentials) and
> deliberately rejects the rest, with reasons recorded so we don't relitigate.

Status: **Active** — design captured, not yet scheduled. Pre-step (docs drift) is immediate.

## Where Almide stands today

Conformance is **declared and nominal** (`type GreetAction: Action`), satisfied
by convention methods (`fn GreetAction.execute(...)`), checked at the
declaration. Eight built-in protocols are registered in
`crates/almide-frontend/src/canonicalize/protocols.rs`:

| protocol | signature |
|---|---|
| `Eq` | `fn eq(a: Self, b: Self) -> Bool` |
| `Repr` | `fn repr(v: Self) -> String` |
| `Ord` | `fn compare(a: Self, b: Self) -> Int` |
| `Hash` | `fn hash(v: Self) -> Int` |
| `Codec` | `fn encode(v: Self) -> Value`, `fn decode(v: Value) -> Result[Self, String]` |
| `Encode` / `Decode` | the one-directional halves of Codec |
| `Numeric` | numeric-bound for generics |

**Pre-step (immediate, tiny):** `Encode`, `Decode`, and `Numeric` are
UNDOCUMENTED — `docs/specs/type-system.md` and the CHEATSHEET list only five.
Sync the docs and add a drift gate (the llms.txt precedent): a check that the
documented protocol table matches the registration table, so built-ins can
never silently fall out of the docs again.

## The 2026 landscape, in one map

```text
                    conformance declaration
             implicit (structural)   explicit (declared)
           ┌───────────────────────┬───────────────────────┐
 dynamic   │  Go                   │  Almide + `any P`     │
 (existen- │  (interface values    │  ← THIS ROADMAP       │
  tials)   │   are the default)    │                       │
           ├───────────────────────┼───────────────────────┤
 static    │  TypeScript           │  Rust / Swift /       │
 (mono-    │  (structural types)   │  Almide today         │
  morphic) │                       │                       │
           └───────────────────────┴───────────────────────┘
```

Settled verdicts we inherit (not relitigated here):

- **Static dispatch by default, existentials explicit** (Swift `some`/`any`,
  Rust `impl`/`dyn`) — the industry converged; nobody mixes them silently
  anymore.
- **Coherence is non-negotiable** (Scala 2 implicits are the cautionary tale).
  Almide's declared-at-the-type conformance is coherent BY CONSTRUCTION: a
  type's conformances live in one place, so "which instance wins" cannot arise.
- **Deriving is the killer app** (Rust's serde shaped an ecosystem). Almide
  already bets on this (auto-derive for Eq/Repr/Ord/Hash/Codec).

## What we take, and from whom

**From Go — the interface-value ergonomics, as an opt-in.** Go proved that 80%
of practical interface use is the existential: a heterogeneous list of
handlers, a plugin table, "anything I can write to". LLMs carry this prior
from Python/TS/Go and write it constantly:

```text
let handlers: List[any Action] = [GreetAction { .. }, LogAction { .. }]
for h in handlers { h.execute(ctx)! }
```

Today this is inexpressible in Almide (lists are monomorphic) — an LLM writes
it, it fails, MSR takes the hit. `any P` closes the hole with one keyword.

**From Swift — only the `any` spelling and its clarity.** Nothing else.

**From Rust — nothing new** (we already have the deriving bet and coherence).

## What we explicitly reject, and why

| feature | why rejected |
|---|---|
| **Implicit structural satisfaction** (Go) | Accidental conformance — a type satisfying a protocol because method names coincidentally match — is a silent-surprise class aimed straight at LLM-written code. Conformance stays a declared intent. |
| **Go's nil-interface semantics** | The eternal #1 Go FAQ ("nil in an interface is not nil"). By keeping existentials opt-in and Almide having no null, the trap surface doesn't exist. |
| **Retroactive conformance** (Swift) | Lets two modules conform the same foreign type differently — the coherence hole Swift 6 itself now warns about (`@retroactive`). Almide's declared-at-the-type rule makes this structurally impossible; keep it that way. |
| **Associated types** (Swift/Rust) | Type-level programming whose error messages are unreadable to humans and worse for LLMs. Defer until a concrete stdlib need exists; generics + concrete types cover today's surface. |
| **Conditional conformance** | The built-ins already behave structurally where it matters (Eq/Hash over containers). Generalizing buys complexity, not capability. |
| **Named instances / HKT** | Out of scope for Almide's mission; complexity eats MSR. |

## Design sketch: `any P`

- **Type**: `any Action` is a first-class type usable anywhere a type goes
  (notably `List[any Action]`, record fields, params, returns).
- **Introduction**: implicit coercion at the declared-conformance boundary —
  assigning/passing a `GreetAction` where `any Action` is expected wraps it.
  Only DECLARED conformances qualify (no structural sneaking).
- **Elimination**: calling a protocol method on `any Action` dynamically
  dispatches to the conforming type's convention method. **No downcast in v1**
  (no `match v { as GreetAction g => .. }`) — add it only when a real program
  needs it; downcasting is where existential designs grow warts.
- **Witness representation**:
  - *native (Rust emit)*: a generated enum over the program's conforming types
    (closed world per compilation — Almide links whole programs, so enum
    dispatch is available, monomorphic, and `match`-exhaustive) OR a
    `Box<dyn>`-style record of fn pointers. **Enum-first**: it keeps Eq/Repr
    derivable and avoids object-safety rules entirely.
  - *wasm*: `[type_tag: i32, value_ptr: i32]` pair + per-protocol method
    tables via the existing closure function-table machinery (precedent:
    `call_indirect` dispatch, the per-type `__repr_<T>` functions from #385).
  - *interpreter*: a tagged `Value::Any { type_name, inner }` — the third
    judge must implement the same dispatch (3-way fixtures required).
- **Interplay with built-ins** (decide in design review, contract-ledger
  entries required):
  - `${any_value}` interpolation → dispatches `Repr`-style to the inner
    type's literal repr (consistent with C-008..C-010).
  - `==` on `any P` → only if `P` requires `Eq`; otherwise a compile error
    with a hint. Same gate for `Ord`/`Hash` (so `any P` in sets/map-keys is
    rejected unless `P: Hash + Eq`).
- **Error quality**: assigning a non-conforming type to `any P` names the
  missing conformance AND the missing methods (the E017 diagnostic standard).

## Staging

1. **Pre-step (now)**: docs sync for Encode/Decode/Numeric + the protocol-table
   drift gate.
2. **Phase 1**: checker (the `any P` type, coercion rule, method-call typing,
   built-in-protocol gating for ==/hash) + native enum-witness emit. Fixtures
   from day one.
3. **Phase 2**: wasm emit (tag+ptr pair, method tables). Cross-target fixtures
   with `// @contract:` headers; new contract entries for dispatch and repr
   semantics.
4. **Phase 3**: interpreter support (3-way), fuzzer generator taught to emit
   `any P` programs (heterogeneous-list shapes), CHEATSHEET + specs update.

Each phase lands behind the existing gates (cross-target byte-gate, contracts
ledger, oracle registry for any new runtime routines, 3-way harness).

## Open questions (design review before Phase 1)

- Closed-world enum witness vs open fn-pointer witness on native: enum is
  simpler and derivable, but every new conforming type recompiles the witness —
  fine for whole-program compilation; revisit if separate compilation ever
  lands.
- `any P1 + P2` (intersection existentials): defer? (Go lacks it and lives;
  Swift's `any P & Q` exists). Lean toward defer-with-syntax-reserved.
- Protocol methods returning `Self`: not representable behind `any` (the
  classic object-safety issue) — reject at the coercion site with a hint, like
  Rust's object safety but with a friendlier message.

## Related

- [llm-first-language.md](llm-first-language.md) — the MSR lens this design is judged by
- [cross-target-completeness.md](cross-target-completeness.md) — the gates every phase must pass
- `docs/specs/type-system.md` — the protocol system spec this extends
