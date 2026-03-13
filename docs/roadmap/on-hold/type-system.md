# Type System Extensions

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
{ name: String, ..R }      // open with named row — extra fields preserved in type R
```

### Critical rule: records are closed by default

**`{ name: String }` does NOT accept `{ name: String, age: Int }`.** This is different from TypeScript, where all object types are structurally open. In Almide:

- `{ name: String }` = closed. Exactly `name` and nothing else.
- `{ name: String, .. }` = open. `name` plus whatever else.

Users coming from TypeScript will expect open-by-default. Almide is closed-by-default because:
- Explicit `..` forces the programmer to declare intent ("I accept extra fields")
- Row variables (`..R`) only make sense when openness is opt-in
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

**Named rows preserve type information through functions:**

```almide
// Without named row — input type info lost in return type
fn rename(x: { name: String, .. }, new_name: String) -> { name: String, .. } =
  x { name = new_name }

// With named row — full type preserved
fn rename[R](x: { name: String, ..R }, new_name: String) -> { name: String, ..R } =
  x { name = new_name }

let dog = Dog { name: "Pochi", age: 3, breed: "Shiba" }
let dog2 = rename(dog, "Jiro")
dog2.breed  // OK — breed is preserved via ..R
dog2.age    // OK — age is preserved via ..R
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
fn rename[R](x: { name: String, ..R }, n: String) -> { name: String, ..R } =
  x { name = n }

fn set_age[R](x: { age: Int, ..R }, a: Int) -> { age: Int, ..R } =
  x { age = a }

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
fn checkout[R](
  { get_user, save_user }: { get_user: fn(String) -> Result[User, String], save_user: fn(User) -> Result[Unit, String], ..R },
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
checkout(ctx, order)  // OK — get_user and save_user are used, log and metrics are preserved via ..R
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
| **Rust** | Monomorphization — generate concrete function per actual type. No trait objects, no vtable. Zero runtime cost. |

Rust example:

```
fn greet(a: { name: String, .. }) called with Dog and Cat
↓
fn greet_Dog(a: &Dog) -> String { format!("Hello, {}", a.name) }
fn greet_Cat(a: &Cat) -> String { format!("Hello, {}", a.name) }
```

### Implementation phases

#### Phase 1: Open records (anonymous row)

- [ ] Parse `{ field: Type, .. }` as open record type
- [ ] Field matching in checker: value has required fields → passes
- [ ] Allow named types to satisfy open record parameter types
- [ ] Anonymous record types in function parameters
- [ ] Error messages: "Type Dog is missing field 'email' required by { email: String, .. }"

#### Phase 2: Named rows + shape aliases

- [ ] Parse `{ field: Type, ..R }` with named row variable
- [ ] Row unification: input `..R` and output `..R` share the same row
- [ ] `type Name = { field: Type, .. }` as shape alias (structural, not nominal)
- [ ] Record destructuring in function parameters
- [ ] Nested structural checks: `{ config: { port: Int, .. }, .. }`

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

## Priority

Open records (Phase 1-2) > structured error types > type annotation > open records (Phase 3-4)
