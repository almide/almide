<!-- description: Type system extensions (generics migration, inference improvements) -->
<!-- done: 2026-03-15 -->
# Type System Extensions

The type system is Almide's primary lever for surpassing other AI-targeted languages. The goal: **catch more errors at compile time without making the language harder for LLMs to write.** Every feature below follows one rule — the compiler gets smarter, the syntax stays simple.

## Stdlib Generics Migration (Unknown → TypeVar)

**Status:** DONE (v0.4.10)

Stdlib signatures now use named type variables instead of `Unknown` for generic positions:

```
# Before (v0.4.9 and earlier)
list.map : (List[Unknown], Fn[Unknown] -> Unknown) -> List[Unknown]

# After (v0.4.10)
list.map : [A, B](List[A], Fn[A] -> B) -> List[B]
```

### Implementation

- **TOML `type_params` field**: Each function declares its type variables (e.g., `type_params = ["A", "B"]`)
- **build.rs**: `parse_type()` checks against `type_params` and generates `Ty::TypeVar` instead of `Ty::Unknown`
- **Unification engine**: `unify()` in `types.rs` — binds TypeVars from arguments (e.g., `A = Int` from `List[Int]`)
- **Substitution**: `substitute()` replaces TypeVars in return type with bound concrete types
- **Effect auto-unwrap**: `unify()` handles closures returning `Result[X, _]` when `X` is expected (effect context)

### What this enables
- Checker propagates concrete types through stdlib calls (`List[Int]` in → `List[Int]` out)
- **Actionable error messages**: "expects Int but got String" instead of "expects Unknown but got String"
- Chained operations (`map → filter → fold`) maintain type context throughout

### Type variable conventions
- **list.toml**: `A` = element type, `B` = transformed type (38 functions)
- **map.toml**: `K` = key type, `V` = value type, `B` = transformed value (16 functions)
- Concrete-only functions (`sum`, `join`) have no type params

## Tuple Type Propagation

**Status:** DONE (v0.4.10)

Tuple types inside containers are now fully typed:

```
list.enumerate : [A](List[A]) -> List[(Int, A)]
list.zip : [A, B](List[A], List[B]) -> List[(A, B)]
list.partition : [A](List[A], Fn[A] -> Bool) -> (List[A], List[A])
map.entries : [K, V](Map[K, V]) -> List[(K, V)]
```

### Remaining Unknown (2 positions, requires new language features)

| Function | Current | What's needed |
|----------|---------|---------------|
| `map.new()` → `Map[Unknown, Unknown]` | 引数なし、型情報ゼロ | **型注釈構文** (`let m: Map[String, Int] = map.new()`) でコンテキストから推論 |
| `list.flatten()` input `List[Unknown]` | ネストされたリストの内側の型が不明 | **ネスト型制約** (`List[List[A]] → List[A]`) で入力型を制約 |

These are not solvable by the current TypeVar/unification approach — each requires a separate language feature. They are low priority because the Rust backend catches all type errors in these positions.

---

## Open Records & Row Polymorphism [PLANNED]

### Overview

Almide adopts row polymorphism instead of trait/impl or interface. The core principle:

> **Write only the fields you need. The rest are preserved, never lost.**

Surface syntax looks like structural subtyping (simple, familiar). Internal semantics use row polymorphism (type information is never discarded). No interface declaration. No impl block. No trait bounds.

### Why this over trait/impl

| | trait/impl | Almide open records |
|---|---|---|
| AI error: forgot to declare interface | possible | **impossible** (no declaration) |
| AI error: forgot impl | possible | **impossible** (no impl) |
| AI error: wrong trait bounds | possible | **impossible** (same syntax as record type) |
| AI error: type name mismatch | possible | **impossible** (checked by structure, not name) |
| Type info after passing | preserved | **preserved** (row variable keeps remaining fields) |
| Learning cost | trait, impl, bounds, orphan rule | `{ field: Type, .. }` — same syntax as records |

### Three forms of record types

```almide
{ name: String }           // closed — exactly these fields, nothing more
{ name: String, .. }       // open — these fields + anything else (anonymous row)
fn f[T: { name: String, .. }](x: T) -> T  // generic structural bound — T preserves concrete type
```

### Critical rule: records are closed by default

**`{ name: String }` does NOT accept `{ name: String, age: Int }`.** This is different from TypeScript, where all object types are structurally open. In Almide:

- `{ name: String }` = closed. Exactly `name` and nothing else.
- `{ name: String, .. }` = open. `name` plus whatever else.

Users coming from TypeScript will expect open-by-default. Almide is closed-by-default because:
- Explicit `..` forces the programmer to declare intent ("I accept extra fields")
- Generic structural bounds (`[T: { name, .. }]`) only make sense when openness is opt-in
- AI-generated code benefits from explicitness: if `..` is missing, the compiler catches unintended width
- Closed records enable better monomorphization (Rust codegen knows the exact layout)

### Core semantics

**Open records accept any value with the required fields:**

```almide
type Dog { name: String, age: Int, breed: String }
type Cat { name: String, lives: Int }

fn greet(a: { name: String, .. }) -> String = "Hello, {a.name}"

greet(Dog { name: "Pochi", age: 3, breed: "Shiba" })  // OK
greet(Cat { name: "Tama", lives: 9 })                  // OK
greet({ name: "Anonymous" })                            // OK
```

**Generic structural bounds preserve type information through functions:**

```almide
// Without generic bound — input type info lost in return type
fn rename(x: { name: String, .. }, new_name: String) -> { name: String, .. } =
  { ...x, name: new_name }

// With generic structural bound — full type preserved
fn rename[T: { name: String, .. }](x: T, new_name: String) -> T =
  { ...x, name: new_name }

let dog = Dog { name: "Pochi", age: 3, breed: "Shiba" }
let dog2 = rename(dog, "Jiro")
dog2.breed  // OK — breed is preserved via T = Dog
dog2.age    // OK — age is preserved via T = Dog
```

This is the key difference from TypeScript's structural subtyping: **fields never disappear**.

**Depth matching — nested records work structurally:**

```almide
fn get_port(app: { config: { port: Int, .. }, .. }) -> Int = app.config.port

type App { config: { port: Int, host: String }, db: { url: String } }
get_port(app)  // OK — config has port
```

**Function fields are exact match** — no covariance/contravariance:

> **Records are structural, but function types are exact match. Variance is not introduced.**

```almide
type Repo { get_user: fn(String) -> Result[User, String] }

// fn(String) -> Result[AdminUser, String] does NOT satisfy Repo.get_user
// even if AdminUser has all User fields. Exact match only.
// This keeps the rules clean and avoids variance confusion for AI.
```

### Pipe chains preserve types

This is where row polymorphism shines. Each step preserves all fields:

```almide
fn rename[T: { name: String, .. }](x: T, n: String) -> T =
  { ...x, name: n }

fn set_age[T: { age: Int, .. }](x: T, a: Int) -> T =
  { ...x, age: a }

fn display_name(x: { name: String, .. }) -> String = x.name

// Full chain — type never degrades
user
  |> rename("Jiro")
  |> set_age(30)
  |> display_name()   // OK — name still available after set_age
```

With structural subtyping, `set_age` would lose the `name` field. With row polymorphism, it's preserved.

### Shape aliases with `type`

Named types serve as shape aliases. Type checking is structural — the name is for readability only.

```almide
type Named = { name: String, .. }
type UserRepo = {
  get_user: fn(String) -> Result[User, String],
  save_user: fn(User) -> Result[Unit, String],
}

fn greet(a: Named) -> String = "Hello, {a.name}"
// Any record with name: String passes, regardless of its declared type name
```

### Generic bounds via structural types

```almide
fn sort_by_name[T: { name: String, .. }](list: List[T]) -> List[T] =
  list.sort_by(|x| x.name)

fn summarize[T: { name: String, age: Int, .. }](items: List[T]) -> List[String] =
  items.map(|x| "{x.name}: {x.age}")
```

`T: { name: String, .. }` replaces trait bounds. Same syntax as record types. Nothing new to learn.

### Dependency injection via record destructuring

```almide
type UserRepo = {
  get_user: fn(String) -> Result[User, String],
  save_user: fn(User) -> Result[Unit, String],
}

// Destructure in parameters — no prefix needed
fn checkout({ get_user, save_user }: UserRepo, order: Order) -> Result[Receipt, String] =
  do {
    let user = get_user(order.user_id)
    save_user(user)
    ok(Receipt { id: "r1" })
  }

// Production
checkout({
  get_user: |id| db.query("SELECT * FROM users WHERE id = ?", id),
  save_user: |user| db.insert("users", user),
}, order)

// Test — only provide what the function actually uses
test "checkout success" {
  checkout({
    get_user: |_| ok(User { name: "Taro", age: 25 }),
    save_user: |_| ok(()),
  }, mock_order)
}
```

Open records mean partial records work if the function only destructures some fields:

```almide
// This function only uses get_user
fn find_user({ get_user }: { get_user: fn(String) -> Result[User, String], .. }, id: String) -> Result[User, String] =
  get_user(id)

// Can pass a full UserRepo — extra fields preserved
find_user(full_repo, "123")
// Can also pass a minimal record
find_user({ get_user: |_| ok(mock_user) }, "123")
```

### Open row DI: accept extra dependencies without losing them

Dependency records can also be open. This is important when the repo carries extra fields (logger, metrics, config) that this function doesn't need but shouldn't discard:

```almide
fn checkout(
  { get_user, save_user }: { get_user: fn(String) -> Result[User, String], save_user: fn(User) -> Result[Unit, String], .. },
  order: Order,
) -> Result[Receipt, String] =
  do {
    let user = get_user(order.user_id)
    save_user(user)
    ok(Receipt { id: "r1" })
  }

// Full app context with logger, metrics, etc.
let ctx = {
  get_user: db.get_user,
  save_user: db.save,
  log: logger.info,
  metrics: metrics.record,
}
checkout(ctx, order)  // OK — get_user and save_user are used, log and metrics accepted via ..
```

> **Dependencies are data, passed as records. Required functions are destructured. Extra dependencies are preserved via open row.**

### Deep dependencies: explicit threading

Dependencies are always passed as arguments. No implicit injection.

```almide
fn checkout({ get_user }: UserRepo, order: Order) -> Result[Receipt, String] =
  do {
    let user = get_user(order.user_id)
    validate_order({ get_user }, user, order)  // pass it through
  }

fn validate_order({ get_user }: { get_user: fn(String) -> Result[User, String], .. }, user: User, order: Order) -> Result[Unit, String] =
  do {
    let fresh = get_user(user.id)
    // ...
    ok(())
  }
```

This is verbose, but:
- Every dependency is visible in the function signature
- AI never wonders "where does this function come from?"
- No hidden state, no spooky action at a distance

**Scoped context injection (`uses` / `with`)** is a potential future extension for cross-cutting concerns (logging, config), but is intentionally deferred. The current design prioritizes explicitness: one rule, one mechanism.

### Sort / Eq / Ord / Hash — no traits needed

Primitives (Int, String, Float, Bool) are always Eq, Ord, and Hash. No declaration needed.

Sorting uses key functions:

```almide
users.sort_by(|u| u.age)
users.sort_by(|u| (u.last_name, u.first_name))  // tuple key = multi-field sort
scores.sort_by(|s| -s.value)                      // descending
```

Map keys must be structurally primitive (compiler validates):

```almide
let m: Map[String, User] = [:]                        // OK
let m: Map[{id: Int, name: String}, User] = [:]       // OK — all fields primitive
let m: Map[List[Int], User] = [:]                      // compile error
```

### Immutability

Records are immutable. Updates return new values with row preservation:

```almide
let older = user { age = user.age + 1 }   // all other fields preserved
let renamed = user { name = "Jiro" }       // all other fields preserved
```

**Row preservation rule:** `{ ...x, field: value }` preserves x's type. If `x: T` where `T: { name: String, .. }`, then `{ ...x, name: "Jiro" }` has type `T`. The update only replaces the specified field's value; all other fields remain intact. This is what makes pipe chains type-safe.

Optional fields use `Option[T]`, not special syntax:

```almide
type User { name: String, email: Option[String] }

fn send_email(u: { email: Option[String], .. }) -> Result[Unit, String] =
  match u.email {
    Some(addr) -> mail.send(addr, "Hello")
    None -> err("no email")
  }
```

### UFCS interaction

Open records work with UFCS. If the first parameter matches structurally, method-style call works:

```almide
fn greet(a: { name: String, .. }) -> String = "Hello, {a.name}"

Dog { name: "Pochi", age: 3, breed: "Shiba" }.greet()  // OK via UFCS
```

### Codegen strategy

| Target | Strategy |
|--------|----------|
| **TypeScript** | Direct — TS already has structural typing. Row variables erased (TS doesn't need them). |
| **Rust** | Field projection — callers project required fields into an `AlmdRec` struct. Single function body, no monomorphization needed. Zero-cost at compile time. |

Rust example:

```
fn greet(a: { name: String, .. }) → fn greet(a: AlmdRec0<String>)

// Call sites project fields:
greet(dog)  →  greet(AlmdRec0 { name: dog.name.clone() })
greet(cat)  →  greet(AlmdRec0 { name: cat.name.clone() })

// Open→open chain: chain_a({ name, breed, .. }) calls chain_b({ name, .. })
chain_b(x)  →  chain_b(AlmdRec0 { name: x.name.clone() })
```

### Implementation phases

#### Phase 1: Open records (anonymous row) — DONE

- [x] Parse `{ field: Type, .. }` as open record type
- [x] Field matching in checker: value has required fields → passes
- [x] Allow named types to satisfy open record parameter types
- [x] Anonymous record types in function parameters
- [x] Error messages: "Type Dog is missing field 'email' required by { email: String, .. }"
- [x] Separate `OpenRecord` AST/Ty variant (not a bool flag — compiler enforces exhaustive handling)
- [x] Open→open chain calling with field projection codegen

#### Phase 2: Named rows + shape aliases

- [x] Parse generic structural bounds: `[T: { field: Type, .. }]`
- [x] Checker: structural bound unification — `T` bound to concrete arg type at call site
- [x] Monomorphization: `fn f[T: ..](x: T) -> T` → `fn f__Dog(x: Dog) -> Dog`
- [x] `type Name = { field: Type, .. }` as shape alias (structural, not nominal)
- [ ] Record destructuring in function parameters
- [x] Nested structural checks: `{ config: { port: Int, .. }, .. }` with recursive field projection

#### Phase 3: Generic bounds

- [ ] `T: { field: Type, .. }` in generic constraints
- [ ] Structural bounds in stdlib functions (sort_by_name, etc.)
- [ ] Monomorphization for Rust codegen

#### Phase 4: Function field types + DI

- [ ] Records with function-typed fields
- [ ] Exact match for function fields (no variance)
- [ ] DI pattern: destructured function fields as callable bindings

#### Intentionally deferred

- Field deletion / row subtraction
- Complex row-level constraints (Lacks, Cons)
- Function field covariance / contravariance
- Scoped context injection (`uses` / `with`)

### What this replaces

| Old plan (trait/impl) | New plan (open records) |
|-----------------------|------------------------|
| `trait Show { fn show(self) -> String }` | `fn show(x: { .., .. }) -> String` or stdlib auto-Show |
| `impl Show for Point { ... }` | Not needed — Point has the fields, it works |
| `fn sort[T: Ord](xs: List[T])` | `fn sort_by[T](xs: List[T], key: fn(T) -> K)` with key function |
| `trait Hash` for Map keys | Compiler validates key type is structurally primitive |
| Full trait implementation | **Not planned** — open records cover the use cases |

### Design references

| Language | Approach | Almide learns from |
|----------|----------|-------------------|
| **PureScript** | Row polymorphism (`{ name :: String \| r }`) | Theoretical gold standard — row variables preserve all type info |
| **TypeScript** | Structural subtyping (`{ name: string }`) | Surface UX — familiar, minimal syntax |
| **SML#** | Record polymorphism (`#{ field: type }`) | Academic reference — field-based generic constraints |
| **Elm** | Restricted row poly (`{ a \| name : String }`) | Simplicity — keep it minimal |
| **Go** | Structural interfaces | Method-based only — Almide does field-based, which is more general |

Almide's position: **PureScript's power, TypeScript's ergonomics.**

---

## Structured Error Types [PLANNED]

Currently `Result[T, String]` uses a fixed String error type.

```almide
type AppError = NotFound(String) | Unauthorized | Internal(String)
type AppResult[T] = Result[T, AppError]
```

Enables branching by error type in match arms.

## Type Annotation on Empty Containers [PLANNED]

`map.new()` and `[]` return `Unknown` types because there's no argument to infer from. A type annotation syntax would allow the checker to assign concrete types:

```almide
let m: Map[String, Int] = map.new()
let xs: List[Int] = []
```

This requires bidirectional type checking (expected type flows into the expression).

## Inline Union Types [PLANNED]

### Overview

Lightweight anonymous unions for return types and error types, without declaring a separate `type` definition.

### Why this matters

Currently, returning one of several types requires a full variant declaration:

```almide
// Current — must declare a type even for one-off use
type ParseResult = | IntVal(Int) | FloatVal(Float) | StrVal(String)

fn parse_value(s: String) -> ParseResult = ...
```

With union types:

```almide
// Proposed — inline, no declaration needed
fn parse_value(s: String) -> Int | Float | String = ...
```

### Design

**Union types are closed, untagged, and flattened:**

```almide
// Type syntax
Int | String                    // two members
Int | Float | String            // three members (flat, not nested)
Result[Data, NotFound | Timeout | AuthError]  // union in error position

// Match must cover all members
fn handle(x: Int | String) -> String =
  match x {
    i: Int => int.to_string(i)
    s: String => s
  }
```

**Restriction: union members must be distinguishable at runtime:**

```almide
Int | String          // OK — different runtime types
Int | Float           // OK — different runtime types
Int | Int             // error — duplicate
List[Int] | List[String]  // error — indistinguishable at runtime (both are arrays)
```

This restriction exists because:
- LLMs don't need to think about runtime type tags — the compiler handles it
- Rust codegen uses `enum` with discriminant; TS codegen uses type guards
- Keeps `match` exhaustiveness checking straightforward

### Union vs Variant — when to use which

| | Union (`A | B`) | Variant (`type X = A(T) \| B(U)`) |
|---|---|---|
| Declaration needed | No | Yes |
| Named constructors | No | Yes (`A(...)`, `B(...)`) |
| Same-type members | Not allowed | Allowed (`A(Int) \| B(Int)`) |
| Best for | Return types, error types | Domain modeling |

**Rule of thumb:** If members have different types and no semantic name is needed, use union. Otherwise, use variant.

### Interaction with effect fn

Union types work naturally with `Result` and `effect fn`:

```almide
type NotFound { path: String }
type PermissionDenied { path: String }

// Union in error position
effect fn read_config(path: String) -> Result[Config, NotFound | PermissionDenied] = {
  guard fs.exists?(path) else err(NotFound { path })
  guard fs.readable?(path) else err(PermissionDenied { path })
  let text = fs.read_text(path)
  ok(parse_config(text))
}
```

### Interaction with open records

Union types compose with row polymorphism:

```almide
// Accept anything that has a name field, return String or Int
fn get_id(x: { name: String, id: Int, .. }) -> String | Int =
  if x.name == "admin" { x.name } else { x.id }
```

### Codegen strategy

| Target | Strategy |
|--------|----------|
| **TypeScript** | Direct — TS has native union types (`string \| number`) |
| **Rust** | Generate anonymous enum: `enum Union_Int_String { V0(i64), V1(String) }` with match arms |

### Implementation phases

#### Phase 1: Basic union types

- [ ] Add `Ty::Union(Vec<Ty>)` to type system
- [ ] Parse `A | B | C` in type position (return types, let bindings, parameters)
- [ ] Flatten nested unions: `(A | B) | C` → `A | B | C`
- [ ] Reject duplicate/indistinguishable members
- [ ] Exhaustiveness checking in `match`

#### Phase 2: Union + Result integration

- [ ] `Result[T, E1 | E2]` works with `match` on error variants
- [ ] `?` propagation with union error types
- [ ] Auto-widening: `err(NotFound(...))` accepted where `NotFound | Timeout` expected

#### Phase 3: Type narrowing

- [ ] After `match` arm, type is narrowed to the matched member
- [ ] `if x is Int` narrows type in the branch body (future syntax)

---

## Higher-Kinded Types (Container Protocols) [PLANNED]

### The problem with traditional HKT

HKT in Haskell/Scala requires users to:
1. Understand `F[_]` as a "type-level function"
2. Choose between `Functor`, `Applicative`, `Monad`, `Traversable`
3. Write `instance` / `impl` for each combination
4. Navigate a deep class hierarchy

LLMs fail at all four. The abstraction vocabulary is too large, and choosing the wrong level (e.g., `Functor` when `Monad` is needed) causes cascading errors.

### Almide's approach: Built-in Container Protocols

Instead of user-defined type classes, Almide provides a **fixed set of compiler-known protocols** that describe what operations a type constructor supports. Users never write `Functor` or `Monad`. They write `Mappable` or `Chainable` — concrete, verb-based names with a single meaning.

```almide
// LLM writes this:
fn double_all[F: Mappable](xs: F[Int]) -> F[Int] =
  xs.map(fn(x) => x * 2)

double_all([1, 2, 3])       // List[Int] → [2, 4, 6]
double_all(some(5))          // Option[Int] → some(10)
double_all(ok(7))            // Result[Int, E] → ok(14)
```

```almide
// NOT what Almide does:
fn double_all[F[_]: Functor](xs: F[Int]) -> F[Int]   // ← no
```

### Built-in container protocols

| Protocol | Provides | Types | Haskell equivalent |
|----------|----------|-------|-------------------|
| `Mappable` | `map(f)` | List, Option, Result, Map(values) | Functor |
| `Chainable` | `flat_map(f)` | List, Option, Result | Monad (bind only) |
| `Filterable` | `filter(f)` | List, Option, Map | MonadPlus (partial) |
| `Foldable` | `fold(init, f)`, `reduce(f)` | List, Option | Foldable |
| `Iterable` | `for x in xs` | List, Map, Range | Traversable (partial) |

**That's it. Five protocols. No hierarchy. No user-defined additions.**

### Why this works for LLMs

| Traditional HKT | Almide Container Protocols |
|---|---|
| Choose from Functor / Applicative / Monad / ... | Choose from 5 concrete names |
| Write `instance Functor MyType where ...` | Nothing — compiler auto-derives |
| `F[_]` syntax with kind inference | `F: Mappable` — same as any bound |
| Abstraction hierarchy to memorize | Flat list, no hierarchy |
| User-defined instances can conflict | No user instances → no conflicts |

The key insight: **LLMs don't need to define new abstractions over type constructors.** They need to write functions that work across List/Option/Result. A fixed vocabulary of 5 protocols covers 95% of use cases.

### Syntax design

```almide
// Single protocol bound
fn transform[F: Mappable, A, B](xs: F[A], f: Fn[A] -> B) -> F[B] =
  xs.map(f)

// Multiple protocol bounds
fn collect_positive[F: Mappable + Filterable](xs: F[Int]) -> F[Int] =
  xs.filter(fn(x) => x > 0)

// Combining with structural bounds (row poly)
fn named_items[F: Mappable, T: { name: String, .. }](xs: F[T]) -> F[String] =
  xs.map(fn(x) => x.name)
```

### What `F: Mappable` means internally

`F: Mappable` is syntactic sugar. The compiler expands it:

```
F: Mappable  →  F is one of { List, Option, Result, Map }
                AND F has map : (F[A], Fn[A] -> B) -> F[B]
```

At codegen time, the compiler monomorphizes — one concrete function per actual type used:

```rust
// Almide: fn transform[F: Mappable, A, B](xs: F[A], f: Fn[A] -> B) -> F[B]
// Called with List[Int] and Option[Int]:

// Generated Rust:
fn transform_list<A, B>(xs: Vec<A>, f: impl Fn(A) -> B) -> Vec<B> { xs.into_iter().map(f).collect() }
fn transform_option<A, B>(xs: Option<A>, f: impl Fn(A) -> B) -> Option<B> { xs.map(f) }
```

### User-defined Mappable types

Users can make their own types work with container protocols by defining the required function:

```almide
type Tree[A] =
  | Leaf(A)
  | Node(Tree[A], Tree[A])
  deriving Mappable   // compiler checks that map is defined

fn map[A, B](tree: Tree[A], f: Fn[A] -> B) -> Tree[B] =
  match tree {
    Leaf(x) => Leaf(f(x))
    Node(l, r) => Node(map(l, f), map(r, f))
  }

// Now works with container-generic functions:
fn double_all[F: Mappable](xs: F[Int]) -> F[Int] = xs.map(fn(x) => x * 2)
double_all(Node(Leaf(1), Leaf(2)))  // Node(Leaf(2), Leaf(4))
```

`deriving Mappable` is a **conformance declaration**, not an `impl` block. It tells the compiler "check that `map` exists for this type with the right signature." The user writes a plain function, not a trait implementation.

### Why `deriving` and not auto-detect

Auto-detection ("if a `map` function exists, it's Mappable") creates ambiguity:
- What if `map` exists but has the wrong signature?
- What if `map` is imported from another module?
- LLMs can't predict whether their type is Mappable without explicit declaration

`deriving Mappable` is a single line that makes intent explicit. The compiler validates it.

### Interaction with UFCS

Container protocol methods are resolved via UFCS, same as today:

```almide
[1, 2, 3].map(fn(x) => x * 2)    // already works
some(5).map(fn(x) => x + 1)       // already works
tree.map(fn(x) => x * 2)          // works after deriving Mappable
```

No new dispatch mechanism needed. UFCS already resolves `.map()` by receiver type.

### What this does NOT include

| Feature | Why excluded |
|---------|-------------|
| User-defined protocols | One new protocol = vocabulary expansion = LLM accuracy drop |
| Protocol hierarchy (`Chainable extends Mappable`) | Hierarchy navigation is LLMs' weakness |
| Associated types | `type Item` inside protocols adds indirection |
| Default implementations | "Which implementation runs?" is a common LLM confusion |
| Monad transformers | `OptionT[List, A]` is the #1 abstraction-hell pattern |

### Design references

| Language | Approach | What Almide takes |
|----------|----------|-------------------|
| **Rust** | Trait system + GATs | Monomorphization strategy |
| **Swift** | Protocol with associated types | `deriving` conformance declaration |
| **OCaml (modular implicits)** | First-class modules as type classes | Fixed module set, no user extension |
| **1ML** | Modules as first-class values | Type constructor polymorphism without HKT syntax |
| **Koka** | Effect handlers with type constructors | Verb-based naming (not math-based) |

### Implementation phases

#### Phase 1: Built-in protocols (no user types)

- [ ] Define 5 protocols as compiler-internal concepts (not user-visible declarations)
- [ ] Parse `F: Mappable` as a generic bound (extends existing `GenericParam.bounds`)
- [ ] Type checker: `F: Mappable` constrains `F` to `{List, Option, Result, Map}`
- [ ] Monomorphize at codegen: generate one function per actual container type used
- [ ] Error message: "Type String does not satisfy Mappable — only List, Option, Result, Map are Mappable"

#### Phase 2: User-defined conformance

- [ ] Parse `deriving Mappable` on `type` declarations
- [ ] Checker validates that a matching `map` function exists with correct signature
- [ ] Add user-defined types to the set of types satisfying each protocol
- [ ] UFCS resolution includes user-defined map/flat_map/filter

#### Phase 3: Multi-param type constructors

- [ ] `Map[K, V]` as `Mappable` over values (key fixed): `map.map(m, fn(v) => ...)` maps values
- [ ] `Result[T, E]` as `Mappable` over `T` (error type fixed)
- [ ] Partial application of type constructors at the type level

### Undecided questions

**Q1: Should protocols compose?**

```almide
// Option A: flat, no composition
fn process[F: Mappable, F: Filterable](xs: F[Int]) -> F[Int]

// Option B: combined bound
fn process[F: Mappable + Filterable](xs: F[Int]) -> F[Int]
```

Leaning toward Option B — `+` is familiar from other languages and doesn't introduce hierarchy.

**Q2: Should `Chainable` require `Mappable`?**

In theory, every Monad is a Functor. But making this explicit introduces hierarchy. Current answer: **no** — keep them independent. If a function needs both, write both bounds. Flat is better than deep.

**Q3: `deriving` syntax for multiple protocols?**

```almide
type Tree[A] = ...
  deriving Mappable, Foldable   // comma-separated
```

Or separate lines? Leaning toward comma-separated for conciseness.

---

## Competitive Analysis: Type System Power

How Almide's type system compares after all planned extensions are implemented:

| Feature | Almide (planned) | vibe-lang | TypeScript | Rust |
|---------|------------------|-----------|------------|------|
| Row polymorphism | **Yes** (PureScript-style) | No | Structural subtyping (weaker) | No |
| Union types | **Yes** (inline) | No | Yes (but unsound) | No (tagged enum only) |
| Structural generic bounds | **Yes** (`T: { field, .. }`) | Trait bounds | No | Trait bounds |
| Effect tracking | Binary (pure/effect) | Effect rows (richer) | None | None |
| Variance | Intentionally none | Full | Full | Full |
| HKT | **Yes** (container protocols) | Partial | No | No (GAT workaround) |
| Dependent types | No | No | Conditional types (limited) | No |

### Where Almide will be uniquely strong

1. **Row polymorphism + union types + structural bounds** — this combination doesn't exist in any mainstream language. PureScript has row poly but no inline unions. TypeScript has unions but no row poly. Almide will have both.

2. **No trait/impl/interface ceremony** — structural bounds (`T: { field, .. }`) replace the entire trait system. Zero boilerplate for generic constraints.

3. **LLM-optimized tradeoffs** — no variance (eliminates covariance/contravariance errors), container protocols instead of open HKT (eliminates `Functor`/`Monad` confusion), binary effect tracking (eliminates effect row annotation errors). Every omission is intentional.

### What Almide deliberately does NOT pursue

| Feature | Why not |
|---------|---------|
| Effect row polymorphism | LLMs must choose which effects to annotate → error source |
| Open HKT (user-defined type classes) | `Functor[F[_]]` with user instances is the #1 type error source for LLMs |
| Implicit parameters / type classes | Hidden resolution → LLM can't predict what gets passed |
| Variance annotations | `in`/`out` annotations are a decision point LLMs frequently get wrong |
| Dependent types | Type-level computation is LLMs' weakest area |

---

## Priority

Open records (Phase 1-2) > union types (Phase 1) > container protocols (Phase 1) > structured error types > type annotation > open records (Phase 3-4) > union types (Phase 2-3) > container protocols (Phase 2-3)
