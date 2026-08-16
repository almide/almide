# Almide Quick Reference (for AI code generation)

File extension: `.almd`

## File structure
```
@dialect(3)          // optional: the language dialect this file was verified against
import <module>
// declarations...
```

When a function moves or goes away, mark the old one so callers are told how
to fix themselves — the compiler compares the two signatures and only offers
`almide fix` the edit when swapping the name is the whole edit:

```almide
@deprecated(since = 3, use = "string.trim_start")   // renamed
@deprecated(since = 3, note = "unsound on surrogate pairs")  // removed, no successor
```

`@dialect(N)` is optional and goes above everything. It records the language
dialect the file last checked clean against — not a release number: the epoch
advances only when the language surface changes in a way that can break
already-written code (`proofs/dialect-epochs.toml` lists what each one broke).
A stamp older than the compiler is silent; a stamp NEWER is `E051`, because
nothing here has verified that file. Write or advance it with
`almide check <file> --stamp`, which only ever moves it forward.

## Project layout (multi-file)
For projects > 1 file, create a package:
```
mypkg/
  almide.toml          # [package] name = "mypkg"
  src/
    main.almd          # entry: imports siblings via `import self.<name>`
    classifier.almd    # sibling module: bare fn / let / type
    bindings/
      python.almd      # nested namespace → mypkg.bindings.python
```
Sibling import:
```
import self.classifier                          // → classifier.fn(), classifier.LET
import self.classifier.{classify, NUMBERS}      // selective: bare names
import self.classifier as cls                   // alias
```
**Do NOT** write `import x from "./x.almd"` — file paths aren't supported. Always use `import self.<sibling>`.

## Types
```
type Name = { field: Type, ... }                     // record
type Name = | Case1(Type) | Case2 | Case3{f: Type}  // variant (leading |)
type Name[A, B] = { first: A, second: B }            // generic (use [] not <>)
type Name = Type                                     // type alias (transparent)
type Name = Case1(Type) | Case2(Type)                // inline variant (no leading |)
type Handler = (String) -> String                    // function type alias
```

### Conventions
```
type Color: Eq, Repr = Red | Green | Blue   // convention after type name with :
```

**Variant serialization — recommended pattern.** When a variant type
crosses a serialization boundary (JSON / file IO / dojo fixtures),
declare `: Codec` to auto-generate `Type.encode` / `Type.decode`
instead of hand-writing `match` ladders. The derived Codec is
externally tagged — the ctor name is the single key; a record payload
keeps its field names, a tuple payload is a positional array, a unit
case is `null`:
```
type Event: Codec = | Click(Int, Int) | Scroll{dy: Int} | Quit
// Event.encode(Click(10, 20))    → {"Click": [10, 20]}
// Event.encode(Scroll { dy: 5 }) → {"Scroll": {"dy": 5}}
// Event.encode(Quit)             → {"Quit": null}
```
Skip `: Codec` only for variants that never serialize
(internal enums like `Endian` in `stdlib/bytes.almd`). Every other
variant should opt in — the roundtrip code is LLM-error prone when
hand-written.

### Built-in types
- Primitives: `Int`, `Float`, `String`, `Bool`, `Unit`, `Path`
- Collections: `List[T]`, `Map[K, V]`, `Set[T]`
- Error: `Result[T, E]` (`ok(v)` / `err(e)`), `Option[T]` (`some(v)` / `none`)
- Option shorthand: `T?` ≡ `Option[T]` in every type position (canonical — `almide fmt` normalizes to it)

### Integer literals
```
255  1_000_000       // decimal, `_` separators anywhere between digits
0xFF  0x1_00         // hex     (0x / 0X)
0b1010  0b1111_0000  // binary  (0b / 0B)
0o17                 // octal   (0o / 0O)
```
All radixes are plain `Int` values — same arithmetic, both targets identical.
A bare prefix (`0x` with no digits) is a compile error, never a silent `0`.

## Functions
```
fn name(x: Type, y: Type) -> RetType = expr
fn name(x: Type) -> Int!                             // pure-fallible: sugar for Result[Int, String]
effect fn name(x: Type) -> Result[T, E] = expr       // has side effects
```

### Pure-fallible marker `-> T!` (ADR-0002 Phase 1)

`-> T!` declares a pure fn that can fail: the return IS `Result[T, String]`.
`!` propagates inside (no effect fn needed), and a VALUE tail lifts into
`ok(...)` automatically — write the payload, or write the Result explicitly;
both work:

```almide
fn parse_port(s: String) -> Int! = int.parse(s)      // pass-through (already a Result)
fn double_port(s: String) -> Int! = int.parse(s)! * 2  // value tail — lifts into ok(...)
fn checked(s: String) -> Int! = {
  let n = int.parse(s)!                              // ! propagates in a T! body
  guard n > 0 else err("must be positive")
  n                                                  // value tail lifts (ok(n) also fine)
}
```

A LAMBDA whose body uses `!` becomes a fallible closure `(A) -> Result[T, String]`
(first-class; a fallible callback to a core list HOF takes the first-err form;
fn-type slots spell it `(A) -> B!`). In test blocks a lambda's `!` stays plain
unwrap. `!` the RETURN MARKER is legal ONLY in return position of a fn
declaration and in fn-type slots. E is always String —
a custom error type keeps the explicit `Result[T, MyError]` spelling
(ADR-0003/0004: branch on structure, not message text).

### Option shorthand `T?` (ADR-0010)

`T?` ≡ `Option[T]`, valid in EVERY type position (unlike `!`, which marks the
arrow). `?` binds to the type atom just before it and never crosses `->`:

```almide
fn first_even(xs: List[Int]) -> Int? = xs |> list.find((x) => x % 2 == 0)
fn or_zero(v: Int?) -> Int = v ?? 0            // parameter position
type Config = { port: Int? }                   // field position
fn cells(row: List[Int?]) -> Int = ...         // generic-arg position
f: (Int) -> Int?                               // fn slot: a fn RETURNING Option[Int]
on_tick: ((Int) -> Unit)?                      // optional fn VALUE needs parens
pair: (String, Int)?                           // Option of tuple
nested: (Int?)?                                // nested Option (`Int??` lexes as ??)
fn parse_opt(s: String) -> Int?!               // Result[Option[Int], String]
```

`T?` is the canonical spelling: `almide fmt` rewrites `Option[T]` to it.

### Visibility (optional prefix before fn/type)
- `fn f()` — public (default)
- `mod fn f()` — same project only (`pub(crate)` in Rust)
- `local fn f()` — this file only (private)

### Modifiers (order matters): `[local|mod]? effect? fn`

### Predicate: `fn empty(xs: List[T]) -> Bool` (Bool return only)

### Hole / Todo
```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```
Both take whatever type the context demands, so the rest of the program keeps
type-checking, and both PANIC if execution reaches them (native: exit 101 —
`not yet implemented: hole at line N` / `not yet implemented: <your message>`).
This is a NATIVE-ONLY workflow: the wasm MIR has no arm for either node, so a
program that still contains one walls (`almide run --target wasm` exits 1,
"this program shape is not yet supported by the verified wasm renderer").
Fill the hole before you build for wasm.

`_` in a **call argument** is a different thing and is rejected (E046):
```
let v = add(_, 10)          // ✗ E046 — not partial application, no `_` sections
let add10 = (x) => add(x, 10)   // ✓ name the missing value with a lambda
```

### Mutable parameters
```
fn incr(mut x: Int) -> Unit = { x = x + 1 }
var n = 5
incr(n)          // n is now 6 -- mutated in place, not returned
```
Caller must pass a `var` binding (`let` or a temporary is E007). `mut` can be
on any parameter, any position. This is how in-place stdlib ops work
(`list.push`, `list.pop`, `list.clear`, …).

## Built-in Protocols
Eq and Hash are automatic (compiler-derived from type structure). No annotation needed.
```
// Eq: all value types support == (except Fn)
let same = color_a == color_b  // just works
```
### Protocols (user-defined conventions)
```
// Define a protocol
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
}

// Satisfy via convention methods -- no impl block, this is the only way.
// The checker validates the signature against the protocol (arity, types).
type GreetAction: Action = { greeting: String }
fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok(a.greeting)

// Use as generic bound
fn run_action[T: Action](action: T, ctx: Context) -> Result[String, String] =
  action.execute(ctx)
```
Built-in conventions (Eq, Repr, Ord, Hash, Codec) are protocols too.

The first parameter can be named/typed explicitly (`a: GreetAction`) or written as bare `self` (sugar for `self: Self`) — both resolve to the declaring type on a convention method, same as inside a `protocol { ... }` declaration.

## Expressions

### If (MUST have else — no standalone `if`)
```
if cond then expr else expr
if a then x else if b then y else z
```
**`if` without `else` returns Unit.** Use `guard` for early return instead.

### Match (exhaustive, supports guards)
```
match subject {
  Pattern => expr,
  Pattern if guard_cond => expr,
  _ => expr,
}
```

### Patterns
```
_                          // wildcard (match only — NOT a valid variable name)
name                       // bind
ok(inner) / err(inner)     // Result
some(inner) / none         // Option
TypeName(args...)          // constructor
TypeName{ field1, field2 } // record pattern
literal                    // int, float, string, bool
```
**`_` can appear in match patterns, `let _ = x` (discard), `for _ in xs`, and lambda params `(_ ) => expr`.**

**NOT supported in patterns:** no `...` spread, no range patterns (`1..5`), no nested `|` (or-pattern), no `as` binding.

### Lambda
```
(x) => expr
(x, y) => expr
items.map((x) => x + 1)

// multi-line: use a block as the body
let f = (x) => {
  let y = x * 2
  y + 1
}
```

**Lambdas and effects**: a lambda inherits the enclosing fn's effect
capability — one rule for every higher-order callee (`list.map`, …). Inside
an `effect fn`, a lambda may call effect fns, but their results stay
**explicit `Result` values** (auto-`?` never crosses a closure boundary):
unwrap with `?? fallback` or `match` — `!` cannot propagate out of a lambda.
In a pure fn the same lambda is an error. Exception: metered regions
(`fan.bounded` / `fan.race` bodies) are pure by design, so effect calls are
rejected there even inside an effect fn.

**Effect fn-typed slots** (`effect (A) -> B`): a HOF can declare that its
callback runs effects — `effect fn serve(port: Int, f: effect
(HttpRequest) -> HttpResponse) -> Unit`. A lambda checked against an
`effect (…) -> …` slot gets full effect-fn body ergonomics: effect calls are
permitted and `!` propagates (into the handler's own failure channel — a
failing `http.serve` handler becomes the 500 response). Both spellings are
accepted uniformly:

```
http.serve(8080, (req) => {
  let body = fs.read_text("index.html")!   // ← `!` works: the slot is effect-typed
  http.response(200, body)
})!
```

Calling an effect fn-typed VALUE is itself an effect call (`h(x)!` inside the
HOF); doing it from a pure fn is E006. A plain `(A) -> B` slot still rejects
fallible lambdas (E005) — declare the slot `(A) -> B!` or `effect (A) -> B`.
The bare arrow form is the canonical spelling; `almide fmt` normalizes the
legacy `fn(A) -> B` to it.

NAMED fns work as slot values the same way lambdas do (#1148): an effect fn
referenced as a value is a `(A) -> Result[B, String]` closure — bind it
(`let h: effect (String) -> Int = parse_pos`, annotation optional), pipe into
it (`(s |> h)!`), or pass it to an effect slot directly. A PURE named fn also
fills an effect slot (its result is ok-wrapped). UFCS method syntax does not
apply to fn values (`x.h()` is E002 — write `h(x)` or `x |> h`).

### Block (last expression is the value)
```
{
  let x = 1
  let y = 2
  x + y
}
```

### For...in loop
```
for x in xs {
  println(x)
}

for (k, v) in config {
  println(k + " = " + v)
}

for key in m {
  println(key)           // iterates keys only
}
```
### While loop
```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

### Range
```
0..<5           // [0, 1, 2, 3, 4]  (exclusive end)
1...5           // [1, 2, 3, 4, 5]  (inclusive end)
for i in 0..n { ... }    // optimized: no list allocation
let xs = list.map(0..<10, (i) => i * i)  // range as List[Int]
```

### Pipe
```
text |> string.trim |> string.split(",")
xs |> filter(_, (x) => x > 0)      // _ = placeholder for piped value
```

### Record & Spread
```
{ name: "alice", age: 30 }
{ ...base, name: "bob" }
User { name: "alice" }     // named record construction — canonical form
User(name: "alice")        // alias: named-argument call syntax, normalized to the brace form
User("alice")              // WRONG: records take named fields, not positional (E021)
```
The paren alias also works for matching a plain record type (`User(name) => ...`).
It does **not** work for a variant's record-payload case — `Circle { radius }` (a
case of `type Shape = Circle { radius: Float } | ...`) only matches as
`Circle { radius }`, never `Circle(radius)` (E021).

### List
```
[1, 2, 3]
[]                         // empty list (there is NO list.new())
xs[0]                      // index read
xs[i] = value              // index write (var only)
```

### Map
```
["a": 1, "b": 2]          // map literal
[:]                        // empty map (requires type annotation)
let m: Map[String, Int] = [:]
m["key"]                   // index read (returns Option[V])
m["key"] = value           // index write (var only)
```

### String interpolation
```
"hello ${name}, result=${1 + 1}"
```

### String escapes
```
"\n \t \r \\ \" \$"   // newline, tab, return, backslash, quote, dollar
"\x1b"                // \xNN  — two hex digits, codepoint 0x00..0xFF (ESC here)
"\u{1F600}"           // \u{…} — 1..6 hex digits, any Unicode scalar (😀)
```
A malformed numeric escape (e.g. `\xzz`, `\u{}`) is left literal.

### Heredoc (multi-line strings)
```
let sql = """
  SELECT *
  FROM users
"""
// Leading whitespace stripped based on minimum indent
// Interpolation ${expr} works the same
// Raw heredoc: r"""...""" (no escapes)
```

## Statements

### Top-level let (module-scope constant)
```
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```
Top-level `let` is evaluated at compile time (const) or via `LazyLock` (for non-const expressions like String).

### let / var
```
let x = 1                   // immutable
let x: Int = 1              // with type annotation
var y = 2                   // mutable
y = y + 1                   // reassign (var only)
f([]: List[Int])            // type ascription in call args
f([:]: Map[String, Int])    // typed empty map in call args
```

### Destructuring
```
let { name, age } = user    // record destructure (1 level only)
```

### Unwrap operators
```
expr!              // unwrap Result/Option, propagate the failure (effect fn, or a pure fn returning Result/Option)
expr ?? fallback   // unwrap or use fallback value
expr?              // Result → Option (err → none)
expr?.field        // optional chaining (Option[Record] → Option[FieldType])
```

`!` on an effect CALL always compiles: if the fn never fails (`random.int`,
`fs.exists`, …) the `!` is a silent no-op. You never need to know whether a
stdlib effect fn can fail to append it.

### Reading a file that may not exist (ADR-0004 D4)

Absence is a value, not an error — never branch on the error text:

```almide
let cfg = fs.read_text_if_exists(path)! ?? "default"
//  ok(none) = absent (missing parents too) / err = permission, IO — real failures
// family: read_text / read_bytes / read_lines / read_bytes_raw + _if_exists
```

### Processing a file line-by-line (large files)

`fs.read_lines` materializes the whole file — fine for small files, a memory
wall for big ones. Aggregation over a large file is `fs.fold_lines`
(O(longest line) memory, same line semantics as read_lines):

```almide
// ✓ aggregate: fold_lines carries the accumulator through; split_once +
//   map.upsert is the hot-loop form (one lookup, no List, no re-split)
let stats = fs.fold_lines(path, map.new(), (acc, line) =>
  match string.split_once(line, ";") {
    some((key, _)) => map.upsert(acc, key, 1, (n) => n + 1),
    none => acc,
  })!

// ✓ side-effecting walk: for_each_line (callback may mutate captured vars —
//   but keep MAP accumulation on fold_lines: reading a Map captured in a
//   closure clones it per read)
var count = 0
fs.for_each_line(path, (line) => { count = count + 1 })!

// ✓ FALLIBLE body: the callback's `!` instantiates the fallible form here too
//   (first-err short-circuit, ADR-0006) — the callback is never called again
//   after the failing line. Same name, one extra `!`.
let totals = fs.fold_lines(path, map.new(), (acc, line) => add_row(acc, line)!)!

// ✗ avoid for large files: fs.read_lines(path)! |> list.fold(...)
```

The partitioned walkers (`fs.fold_lines_range` / `fs.fold_lines_chunked`) have
NO fallible form — a partitioned walk has no defined first err — so an erring
chunk body handles its own error.

`result.collect` / `collect_map` are REMOVED (ADR-0007 — the name promised Rust's first-err short-circuit and did the opposite). Collect every error with partition:

```almide
let (oks, errs) = result.partition(results)
if list.is_empty(errs) then ok(oks) else err(errs)   // Result[List[T], List[E]]
```

### Error-handling doctrine (ADR-0004)

**Choosing `E` is a layer assignment, not a taste call.** `E = String` is the
**erasure** layer — the *reporting* channel, read by humans and models; it is
the default. A variant `E` is the **refinement** layer — the *branching*
channel, where the program changes behaviour on the content — and it is worth
its cost only inside a **closed domain**: a module or package that takes care
of that error itself. Crossing out of that domain, the variant is **demoted**
back to String, and the demotion is always visible as a `map_err` (there is no
conversion hook, so it cannot happen silently). Full rule:
[docs/specs/result-option-effect.md §8.0](./specs/result-option-effect.md).

**Never branch on the text of an error message** (`string.contains(e, …)`,
`e == "No such file"`) — the text is a report, not an API, and E035 warns.
When a caller must branch on the failure *kind*, that is the signal to
define a variant error type and match on its structure:

```almide
type LoadError = | NotFound(String) | BadValue(String)

match load(p) {
  ok(v)               => v,
  err(NotFound(_))    => default_value,     // branch on structure
  err(BadValue(msg))  => process.exit(1),
}
```

For a kind-independent fallback, don't read the error at all: `load(p) ?? default`.

**Adding context to an error** — the canonical spelling (do not invent
variants; keep `": "` as the separator and `${e}` at the end):

```almide
let cfg = fs.read_text(path) |> result.map_err((e) => "loading config: ${e}")!
// chained calls read as the failure's story:
//   Error: starting server: loading config: No such file or directory

// deliberate replacement is spelled with the discard parameter:
fs.read_text(path) |> result.map_err((_) => "friendly message")
// forgetting ${e} with a NAMED parameter warns (E036) — the original error
// would be silently destroyed
```

### Guard (early return / loop break)
```
guard x > 0 else err("must be positive")
guard fs.exists(path) else err(NotFound(path))

// with block body:
guard not fs.exists(path) else {
  println("already exists")
  ok(())
}
```

## Concurrency & deterministic time (fan)

All `fan.*` forms require an `effect fn` context. There is NO `async`/`await` in Almide.

```
// Dynamic mappers — a list + one callback returning Result (the mapper matrix
// covers every A→B pairing with A, B in {Int, Float, String}):
let results = fan.map(urls, (u) => http.get(u))!          // Result[List[B], String]: first Err (list order) propagates
let winner  = fan.any(mirrors, (m) => fetch(m)) ?? fb     // Result[B, String]: first Ok in LIST order; an Err skips that element
let report  = fan.settle(jobs, (j) => run(j))             // List[Result[B, String]]: EVERY element's Result, Errs captured
// The callback may be an EFFECT fn, in either spelling — an inline lambda that
// calls one, or a bare effect-fn value. Same rule as the block heads' arms.
let checked = fan.map(paths, read_meta)                   // read_meta: an `effect fn`, passed by name

// Block heads — arms are parallel siblings separated by `,` or newline
// (`;` between arms is an error: it means sequencing, and stays legal only
//  INSIDE a block arm — `{ let x = f(); g(x) }`)
let first = fan.any { fetch_a(), fetch_b() } ?? fallback  // first Ok in SOURCE order
let all   = fan.settle { job_a(), job_b() }               // TUPLE of per-arm Results (heterogeneous arms allowed)
let win   = fan.race { solve_fast(), solve_slow() } ?? d  // deterministic winner (least compute spent; tie → source order)

// Budgets: deterministic compute-time limits, built with compute.* constructors
let r = fan.bounded(compute.ms(100)) { work(input) } ?? -1   // Err if work exceeds 100ms of deterministic compute
let w = fan.race(compute.us(50)) { a(), b() } ?? -1          // arms over budget are excluded

// Mapper form: race ONE pure lambda over a dynamic list (winner = cheapest, tie → list order)
let m = fan.race(xs, (x) => ok(solve(x))) ?? fallback        // mapper returns Result: err(...) disqualifies
let n = fan.race(compute.us(50), xs, (x) => ok(solve(x))) ?? fallback  // per-element budget

// Wall-clock deadline (oracle tier): checked cooperatively at charge sites
let t = fan.timeout(duration.ms(5000)) { work(input) } ?? -1 // Err if the wall deadline fires first
```

### Time constructors (closed set)

Two clock types, six units each — `ns / us / ms / s / min / h`:

```
compute.ms(100)     // Compute — deterministic compute-time (fan.bounded / fan.race budgets)
duration.ms(5000)   // Duration — wall-clock time (fan.timeout deadlines)
```

- A bare `Int` is NEVER a time: `fan.bounded(5000) {...}` is a type error — write `compute.ms(5000)`
- `Compute` and `Duration` do not mix: `fan.bounded(duration.ms(5)) {...}` is a type error
- There is no literal suffix: `100ms` does not parse — write `compute.ms(100)`
- A negative argument aborts at runtime (`Error: negative time: ...`); an overflowing construction saturates to the maximum
- `fan.race` / `fan.bounded` results are deterministic: same program + same inputs = same winner/verdict on every target and every machine
## Test
```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
```

### Testing effect fn error cases

In test blocks, `effect fn` calls return `Result[T, String]` — no auto-unwrap. Use `!` for the value, or assert on `ok`/`err` directly:

```almide
effect fn validate(n: Int) -> Int = {
  guard n > 0 else err("bad")!
  n
}

test "ok value" { assert_eq(validate(5)!, 5) }          // explicit unwrap
test "ok result" { assert_eq(validate(5), ok(5)) }      // Result-aware
test "err" { assert_eq(validate(-1), err("bad")) }      // natural
```

## Built-in functions
```
println(s)                 // print line to stdout
eprintln(s)                // print line to stderr
assert_eq(a, b)            // assert equal
assert_ne(a, b)            // assert not equal
assert(cond)               // assert true
```
**There is no `print` function.** Use `println` for all output (including error messages to user).
`eprintln` is for debug/internal errors only — user-facing messages MUST use `println`.

### Stdin & parsing
```
import io                                     // io is NOT auto-imported
effect fn main() -> Unit = {
  let line = io.read_line()                   // plain String — NOT a Result. Do not add ! or ?
  let n = int.parse(string.trim(line)) ?? 0   // int.parse(s) -> Result[Int, String]
  println(int.to_string(n))
}
```
`io.read_line()` reads one stdin line with the trailing newline stripped, returns `""` on EOF,
and requires an `effect fn` caller. `int.parse` / `float.parse` return `Result` — unwrap with
`??`, `!` (effect fn), or `?`. There is no `int.from_string`.

## Entry point
```
effect fn main() -> Unit = {
  let args = process.args()              // command-line args (Go-style)
  let name = list.get(args, 1) ?? "world"
  let content = fs.read_text("config.txt")!   // propagate error with !
  println("Hello, ${name}: ${content}")
}
```
`effect fn main()` is auto-wrapped to return `Result<(), String>`. No need to write `ok(())` or `-> Result[...]`.

**Temp files**: never hardcode `/tmp` — it does not exist on Windows. Use
`fs.temp_dir()` (platform-correct: `%TMP%` on Windows native; on wasm the
Go/Python WASI rule `$TMPDIR` else `/tmp`, so it converges on the host's
real temp dir) or `fs.create_temp_file(prefix)` / `fs.create_temp_dir(prefix)`:

```almide
let dir = fs.temp_dir()
fs.write("${dir}/scratch.txt", data)!
```
Command-line arguments are accessed via `process.args()` (not main parameters).

## Operators (precedence high→low)
`. () [] ! ? ?. ??` (postfix) > `not -` (prefix) > `>>` > `^` (power) > `* / %` > `+ -` > `..< ...` > `|>` > `== != < > <= >=` (non-assoc) > `and` > `or`

`^` is exponentiation (right-associative, `**` also accepted). `+` is concatenation for strings and lists (overloaded with addition). XOR is available as `int.bxor(a, b)`.

`|>` is asymmetric: its right-hand side is a single call/compose chain, so any operator after it applies to the piped result — `xs |> list.map(f) + ys` is `(xs |> list.map(f)) + ys`, and `xs |> f >> g` pipes into the composition `f >> g`.

## UFCS
`f(x, y)` ≡ `x.f(y)` — compiler resolves automatically.

## Standard library modules

Full function signatures: [docs/stdlib/](stdlib/)

| Module | Description | Import | # |
|---|---|---|---|
| [string](stdlib/string.md) | String manipulation | auto-imported | 44 |
| [list](stdlib/list.md) | List operations | auto-imported | 59 |
| [map](stdlib/map.md) | Map (dictionary) operations | auto-imported | 24 |
| [set](stdlib/set.md) | Set operations | auto-imported | 19 |
| [int](stdlib/int.md) | Integer arithmetic and bitwise | auto-imported | 22 |
| [float](stdlib/float.md) | Floating-point operations | auto-imported | 17 |
| [value](stdlib/value.md) | Dynamic value manipulation | auto-imported | 19 |
| [result](stdlib/result.md) | Result type operations | auto-imported | 12 |
| [option](stdlib/option.md) | Option type operations | auto-imported | 12 |
| [json](stdlib/json.md) | JSON parsing and querying | `import json` | 29 |
| [math](stdlib/math.md) | Mathematical functions | auto-imported | 21 |
| [regex](stdlib/regex.md) | Regular expressions | `import regex` | 8 |
| [datetime](stdlib/datetime.md) | Date and time | auto-imported | 21 |
| [compute](stdlib/compute.md) | Deterministic compute-time constructors (fan budgets) | auto (checker surface) | 6 |
| [duration](stdlib/duration.md) | Wall-clock time constructors | auto (checker surface) | 6 |
| [bytes](stdlib/bytes.md) | Binary data | auto-imported | 67 |
| [matrix](stdlib/matrix.md) | 2D matrix operations | auto-imported | 39 |
| [testing](stdlib/testing.md) | Test assertions | `import testing` | 7 |
| [error](stdlib/error.md) | Error construction | auto-imported | 3 |
| [fs](stdlib/fs.md) | File system | `import fs` | 24 |
| [env](stdlib/env.md) | Environment and system | `import env` | 9 |
| [process](stdlib/process.md) | Process execution, env vars, signals | `import process` | 12 |
| [io](stdlib/io.md) | Standard I/O | `import io` | 7 |
| [http](stdlib/http.md) | HTTP client and server | `import http` | 23 |
| [random](stdlib/random.md) | Random number generation | `import random` | 4 |

## JSON & the wire

Typed Codec is the default path for ALL JSON work. The dynamic `json.*` API
(29 functions) is for exploration and schemaless passthrough only — if you
know the shape, declare a type.

**Rule 1 — wire types use wire names.** A type that describes a foreign JSON
format mirrors the format's field names verbatim, even when they break Almide's
snake_case convention. The type IS the documentation of the wire; no renaming
layer exists to misremember:

```almide
type OtlpSpan: Codec = {
  traceId: String,           // camelCase because the WIRE says traceId
  spanId: String,
  parentSpanId: Option[String],  // none = key omitted (proto3 unset)
  startTimeUnixNano: String,
}
// send:    http.request("POST", url, json.stringify(OtlpSpan.encode(s)), headers)
// receive: OtlpSpan.decode(json.parse(body) ?? value.null())
```
Domain logic that wants Almide-shaped names defines its own type and maps in
plain code (a constructor call — the checker verifies every field). Use the
field alias `name as "wire": T` ONLY for keys that cannot be Almide
identifiers (`@type`, `$ref`, `user-id`).

**Rule 2 — none omits the key.** Encode drops a `none` Option field entirely
(no `null`); decode folds a missing key or explicit `null` back to `none`.
`decode(encode(x)) == x` always. Fields with `= default` values stay emitted.
This matches proto3 JSON, so protobuf-defined APIs (OTLP etc.) work directly —
and a proto3 `oneof` is just an all-Option mirror type:

```almide
type AnyValue: Codec = {
  stringValue: Option[String] = none,
  intValue: Option[String] = none,    // proto3 JSON: int64 rides as string
  boolValue: Option[Bool] = none,
}
AnyValue { stringValue: some("hi") }  // → {"stringValue":"hi"} — exactly one key
```

**Rule 3 — `Value` is the escape hatch.** A `Value` field passes through
verbatim in both directions (nested docs, explicit nulls). `Option[Value]` is
the ONE place absent and null differ: missing → `none`, explicit `null` →
`some(value.null())` — use it for RFC-7386-style patch semantics.

**Rule 4 — foreign variant shapes are hand-written codecs.** The derived
variant form is externally tagged (`{"Click": {...}}`). For an API that tags
internally (`{"type": "click", ...}`), give each case its own Codec record and
hand-write only the dispatch — the convention methods compose with every
derived Codec automatically:

```almide
type Click: Codec = { x: Int, y: Int }
type Scroll: Codec = { dy: Int }
type Event = | C(Click) | S(Scroll)

fn Event.encode(e: Event) -> Value = match e {
  C(c) => value.merge(Click.encode(c), value.object([("type", value.str("click"))])),
  S(s) => value.merge(Scroll.encode(s), value.object([("type", value.str("scroll"))])),
}
fn Event.decode(v: Value) -> Result[Event, String] = {
  let tag = value.as_string(value.field(v, "type")!)!
  match tag {
    "click"  => result.map(Click.decode(v), (c) => C(c)),
    "scroll" => result.map(Scroll.decode(v), (s) => S(s)),
    _ => err("unknown Event tag: ${tag}"),
  }
}
// Event now nests in derived Codec types like any other: { evt: Event, at: Int }
```
Always pair a hand-written codec with a roundtrip test:
`test "Event roundtrips" { assert_eq(Event.decode(Event.encode(C(Click{x:1,y:2}))), ok(C(Click{x:1,y:2}))) }`

Codec field types are `String`/`Int`/`Float`/`Bool`/`Value`, other Codec
types, and any nesting of `Option`/`List` over those (`List[Option[Int]]`
keeps element nulls; `Option[List[T]]` omits when none). Rejected at
declaration: `Option[Option[T]]` (indistinguishable on the wire), `Option[Value]`
outside field position, and non-wire leaves (Map, Bytes, tuples, sized ints,
fn types, Result) — wrap those in a named Codec type or convert at the boundary.

## Key rules
- Newline = statement separator (no semicolons needed)
- `[]` for generics, NOT `<>`
- `<` `>` are always comparison operators
- `effect fn` for side effects, NOT `fn name!()`
- Predicate functions return `Bool` (no special suffix)
- No exceptions — use `Result[T, E]` everywhere
- No null — use `Option[T]`
- No inheritance — use composition
- No macros, no operator overloading, no implicit conversions
- Empty list = `[]`, empty map = `[:]` (with type annotation)
- `_` is ONLY for match wildcard patterns, never as a variable name
- The stdlib functions listed above are exhaustive — no other functions exist
- Use `for x in xs { ... }` for iteration
- **No nested functions.** All `fn` must be at the top level. Use lambdas for local helpers
- **No `let mut`.** Use `var` for mutable bindings. `mut` itself is a real keyword, but only as a parameter modifier (`fn f(mut x: Int)`) — see Functions
- Almide is NOT Rust. No `&`, `trait` (use `protocol`), `impl` (use convention methods: `fn Type.method(...)`), `pub`, `mod` (as declaration)

## Naming conventions across stdlib

- `bytes`: `read_<dtype>_le|be(b, pos)` for one value, `read_<dtype>_le_array(b, pos, count)` for bulk, `set_<dtype>_le(b, pos, val)` to overwrite, `append_<dtype>_le(b, val)` to grow. `<dtype>` ∈ `u8|u16|u32|i32|i64|f16|f32|f64`.
- `matrix`: row-shaped ops use `_rows` suffix (`softmax_rows`, `slice_rows`, `gather_rows`, `layer_norm_rows`); singular `_row` when an op consumes/produces *one* row (`linear_row`, `dot_row`, `broadcast_add_row`); column ops use `_cols` (`split_cols_even`, `concat_cols`).
- `from_X` builds from another representation, `to_X` is its inverse (e.g. `from_bytes_f64_le` ↔ `to_bytes_f64_le`).
- sized integers (`int8`…`int64`, `uint8`…`uint64`, plus `int` = i64): `x.to_<dst>()` truncates (Rust `as` semantics); every **lossy** pair also has `to_<dst>_checked` → `Option` (None on overflow) and `to_<dst>_saturating` (clamp) — lossless pairs have only the plain form. Every carrier has `min_value()`/`max_value()`. Widening is `int.from_<src>(x)`; the one lossy widening `UInt64 → Int` has `int.from_uint64_checked/_saturating`.

## Common mistakes (DO NOT)
- `list[1, 2, 3]` → **WRONG**. Write `[1, 2, 3]`. `list` is a module, not a type constructor
- `each(xs, f)` → **WRONG**. Write `list.each(xs, f)`. All stdlib functions need module prefix
- `map[K, V]` as a value → **WRONG**. Write `[:]` with type annotation to create an empty map
- `List.new()` → **WRONG**. Write `[]`. There is no `new()` for List
- `{"a": 1}` as a map → **WRONG**. Write `["a": 1]`. Braces `{}` are for records/blocks, brackets `[]` for lists and maps
- `string.length(s)` → **WRONG**. Write `string.len(s)`. No synonyms
- `string.to_lowercase(s)` → **WRONG**. Write `string.to_lower(s)`. No synonyms
- `string.to_uppercase(s)` → **WRONG**. Write `string.to_upper(s)`. No synonyms
- `string.substring(s, i, j)` → **WRONG**. Write `string.slice(s, i, j)`. No synonyms
- `println(x)` where x is Int → **WRONG**. Write `println(int.to_string(x))`. No implicit conversion
- `1 :: 2 :: []` → **WRONG**. Write `[1, 2]`. There is no cons operator `::`
- `fn foo<T>(x: T)` → **WRONG**. Write `fn foo[T](x: T)`. Use `[]` for generics, not `<>`
- `let mut x = 1` → **WRONG**. Write `var x = 1`. `mut` is only a parameter modifier (`fn f(mut x: Int)`), not a binding modifier
- Nested `fn` inside a function → **WRONG**. All `fn` must be top-level. Use `let helper = (x) => ...` for local functions
- `match x { ... pattern => expr }` with `...` → **WRONG**. No spread in patterns
- `async fn` / `await` → **WRONG**. Almide has no async/await. Use `fan.any` / `fan.settle` / `fan.race` / `fan.bounded` block forms
- `fan.any([a, b])` / `fan.settle([...])` → **WRONG**. The thunk-list form was removed. Write `fan.any { a(), b() }`
- `fan.bounded(100) {...}` / `fan.race(5000) {...}` → **WRONG**. A bare Int is not a time. Write `compute.ms(100)`
- `compute.msec(5)` / `compute.sec(5)` / `compute.m(5)` → **WRONG**. The unit set is closed: `ns / us / ms / s / min / h`
- `100ms` / `5s` as a literal → **WRONG**. There are no time literals. Write `compute.ms(100)` / `duration.s(5)`

## Complete example
```
import fs

type AppError =
  | NotFound(String)
  | Io(String)

effect fn greet(name: String) -> Result[Unit, AppError] = {
  guard string.len(name) > 0 else err(NotFound("empty name"))
  println("Hello, ${name}!")
  ok(())
}

effect fn main() -> Result[Unit, AppError] = {
  let args = process.args()
  let cmd = list.get(args, 1) ?? "help"
  match cmd {
    "greet" => {
      let name = list.get(args, 2) ?? "world"
      greet(name)
    },
    "read" => {
      let path = list.get(args, 2) ?? "input.txt"
      let content = fs.read_text(path).map_err((e) => Io(e))!
      println(content)
      ok(())
    },
    other => {
      println("Usage: app <greet|read> [arg]")
      ok(())
    },
  }
}

test "greet succeeds" {
  assert_eq(string.len("hello"), 5)
}
```

## Common mistakes from other languages

These are the most frequent errors when generating Almide from LLM training on Rust/Python/JS/Scala.

### ✗ `if cond { }` → ✓ `if cond then ... else ...`
```
✗ if x > 0 { "positive" } else { "negative" }
✓ if x > 0 then "positive" else "negative"
```

### ✗ `x => expr` → ✓ `(x) => expr`
Lambda parameters **must** be wrapped in parentheses.
```
✗ list.map(xs, x => x + 1)
✓ list.map(xs, (x) => x + 1)
```

### ✗ `module::func()` → ✓ `module.func()`
Almide uses `.` for module access, not `::`.
```
✗ list::len(xs)
✓ list.len(xs)
```

### ✗ `list.push(xs, item)` for value → ✓ `xs + [item]`
`list.push` mutates a `var` and returns `Unit`. For immutable list building, use `+`.
```
✗ some(list.push(stack, "("))     // returns Option[Unit]
✓ some(stack + ["("])             // returns Option[List[String]]
```

### ✗ `let x = e in body` → ✓ `{ let x = e; body }`
ML-style let-in is not supported. Use a block.
```
✗ let y = f(x) in y + 1
✓ { let y = f(x); y + 1 }
```

### ✗ `foldLeft` / `foldRight` → ✓ `list.fold`
```
✗ xs.foldLeft(0, (acc, x) => acc + x)
✓ list.fold(xs, 0, (acc, x) => acc + x)
```

### ✗ `Char` type → ✓ `String`
Almide has no `Char` type. Single characters are `String`: `"a"`, `"("`.

### ✗ `break` / `continue` → ✓ recursion
Use a recursive helper function instead of loop control keywords.
```
✗ while cond { if done then break }
✓ fn loop(state) = if done then state else loop(next_state)
```

### ✗ `return expr` → ✓ just `expr`
The last expression in a block is the return value. No `return` keyword.
```
✗ fn f(x: Int) -> Int = { return x + 1 }
✓ fn f(x: Int) -> Int = x + 1
```

### ✗ Reassigning function parameters → ✓ `var` copy
Function parameters are immutable. Copy to `var` first.
```
✗ fn f(n: Int) -> Int = { n = n - 1; n }
✓ fn f(n: Int) -> Int = { var m = n; m = m - 1; m }
```
