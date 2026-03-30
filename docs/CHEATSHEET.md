# Almide Quick Reference (for AI code generation)

File extension: `.almd`

## File structure
```
import <module>
// declarations...
```

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

### Built-in types
- Primitives: `Int`, `Float`, `String`, `Bool`, `Unit`, `Path`
- Collections: `List[T]`, `Map[K, V]`, `Set[T]`
- Error: `Result[T, E]` (`ok(v)` / `err(e)`), `Option[T]` (`some(v)` / `none`)

## Functions
```
fn name(x: Type, y: Type) -> RetType = expr
effect fn name(x: Type) -> Result[T, E] = expr       // has side effects
```

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

// Satisfy via convention methods
type GreetAction: Action = { greeting: String }
fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok(a.greeting)

// Use as generic bound
fn run_action[T: Action](action: T, ctx: Context) -> Result[String, String] =
  action.execute(ctx)
```
Built-in conventions (Eq, Repr, Ord, Hash, Codec) are protocols too.

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
0..5            // [0, 1, 2, 3, 4]  (exclusive end)
1..=5           // [1, 2, 3, 4, 5]  (inclusive end)
for i in 0..n { ... }    // optimized: no list allocation
let xs = list.map(0..10, (i) => i * i)   // range as List[Int]
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
```

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
```

### Destructuring
```
let { name, age } = user    // record destructure (1 level only)
```

### Unwrap operators
```
expr!              // unwrap Result/Option, propagate err (effect fn only)
expr ?? fallback   // unwrap or use fallback value
expr?              // Result → Option (err → none)
expr?.field        // optional chaining (Option[Record] → Option[FieldType])
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
## Test
```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
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

## Entry point
```
effect fn main(args: List[String]) -> Result[Unit, String] = {
  // args[0] = program name, args[1] = first argument
  let name = list.get(args, 1) ?? "world"
  let content = fs.read_text("config.txt")!   // propagate error with !
  println("Hello, ${name}: ${content}")
  ok(())
}
```
The runtime calls `main(args)` where `args` includes the program name at index 0.

## Operators (precedence high→low)
`. () ! ?? ?` > `not -` > `^` (power) > `* / %` > `+ -` > `== != < > <= >=` (non-assoc) > `and` > `or` > `|>` `>>`

`^` is exponentiation (right-associative, `**` also accepted). `+` is concatenation for strings and lists (overloaded with addition). XOR is available as `int.bxor(a, b)`.

## UFCS
`f(x, y)` ≡ `x.f(y)` — compiler resolves automatically.

<!-- AUTO-GENERATED from stdlib/defs/*.toml — do not edit manually. Run: make cheatsheet-update -->
## Standard library modules

### string (auto-imported)
`string.trim(s)`, `string.split(s, sep)`, `string.join(list, sep)`, `string.len(s)`, `string.contains(s, sub)`, `string.starts_with(s, prefix)`, `string.ends_with(s, suffix)`, `string.slice(s, start, end)`, `string.pad_start(s, n, ch)`, `string.to_bytes(s)`, `string.capitalize(s)`, `string.to_upper(s)`, `string.to_lower(s)`, `string.replace(s, from, to)`, `string.get(s, i)` → `Option[String]`, `string.lines(s)`, `string.chars(s)`, `string.index_of(s, needle)` → `Option[Int]`, `string.repeat(s, n)`, `string.from_bytes(bytes)`, `string.is_digit(s)`, `string.is_alpha(s)`, `string.is_alphanumeric(s)`, `string.is_whitespace(s)`, `string.is_upper(s)`, `string.is_lower(s)`, `string.codepoint(s)` → `Option[Int]`, `string.from_codepoint(n)`, `string.pad_end(s, n, ch)`, `string.trim_start(s)`, `string.trim_end(s)`, `string.count(s, sub)`, `string.is_empty(s)`, `string.reverse(s)`, `string.strip_prefix(s, prefix)` → `Option[String]`, `string.strip_suffix(s, suffix)` → `Option[String]`, `string.replace_first(s, from, to)`, `string.last_index_of(s, needle)` → `Option[Int]`, `string.first(s)` → `Option[String]`, `string.last(s)` → `Option[String]`, `string.take(s, n)`, `string.take_end(s, n)`, `string.drop(s, n)`, `string.drop_end(s, n)`

### list (auto-imported)
`list.len(xs)`, `list.get(xs, i)` → `Option[A]`, `list.get_or(xs, i, default)` → `A`, `list.set(xs, i, val)` → `List[A]`, `list.swap(xs, i, j)` → `List[A]`, `list.sort(xs)` → `List[A]`, `list.reverse(xs)` → `List[A]`, `list.contains(xs, x)`, `list.enumerate(xs)` → `List[(Int, A)]`, `list.zip(xs, ys)` → `List[(A, B)]`, `list.flatten(xss)` → `List[T]`, `list.take(xs, n)` → `List[A]`, `list.drop(xs, n)` → `List[A]`, `list.unique(xs)` → `List[A]`, `list.index_of(xs, x)` → `Option[Int]`, `list.last(xs)` → `Option[A]`, `list.chunk(xs, n)` → `List[List[A]]`, `list.sum(xs)`, `list.product(xs)`, `list.first(xs)` → `Option[A]`, `list.is_empty(xs)`, `list.min(xs)` → `Option[A]`, `list.max(xs)` → `Option[A]`, `list.join(xs, sep)`, `list.map(xs, f)` → `List[B]`, `list.filter(xs, f)` → `List[A]`, `list.find(xs, f)` → `Option[A]`, `list.any(xs, f)`, `list.all(xs, f)`, `list.each(xs, f)`, `list.sort_by(xs, f)` → `List[A]`, `list.flat_map(xs, f)` → `List[B]`, `list.filter_map(xs, f)` → `List[B]`, `list.take_while(xs, f)` → `List[A]`, `list.drop_while(xs, f)` → `List[A]`, `list.count(xs, f)`, `list.partition(xs, f)` → `(List[A], List[A])`, `list.reduce(xs, f)` → `Option[A]`, `list.group_by(xs, f)` → `Map[B, List[A]]`, `list.range(start, end)`, `list.slice(xs, start, end)` → `List[A]`, `list.insert(xs, i, val)` → `List[A]`, `list.remove_at(xs, i)` → `List[A]`, `list.find_index(xs, f)` → `Option[Int]`, `list.update(xs, i, f)` → `List[A]`, `list.repeat(val, n)` → `List[A]`, `list.scan(xs, init, f)` → `List[B]`, `list.intersperse(xs, sep)` → `List[A]`, `list.windows(xs, n)` → `List[List[A]]`, `list.dedup(xs)` → `List[A]`, `list.zip_with(xs, ys, f)` → `List[C]`, `list.fold(xs, init, f)` → `B`, `list.take_end(xs, n)` → `List[A]`, `list.drop_end(xs, n)` → `List[A]`, `list.unique_by(xs, f)` → `List[A]`, `list.shuffle(xs)` → `List[A]`, `list.window(xs, n)` → `List[List[A]]`, `list.push(xs, x)`, `list.pop(xs)` → `Option[A]`, `list.clear(xs)`

### map (auto-imported)
`map.new()` → `Map[K, V]`, `map.get(m, key)` → `Option[V]`, `map.get_or(m, key, default)` → `V`, `map.set(m, key, value)` → `Map[K, V]`, `map.contains(m, key)`, `map.remove(m, key)` → `Map[K, V]`, `map.keys(m)` → `List[K]`, `map.values(m)` → `List[V]`, `map.len(m)`, `map.entries(m)` → `List[(K, V)]`, `map.merge(a, b)` → `Map[K, V]`, `map.is_empty(m)`, `map.from_list(pairs)` → `Map[K, V]`, `map.map(m, f)` → `Map[K, B]`, `map.filter(m, f)` → `Map[K, V]`, `map.fold(m, init, f)` → `A`, `map.any(m, f)`, `map.all(m, f)`, `map.count(m, f)`, `map.each(m, f)`, `map.find(m, f)` → `Option[(K, V)]`, `map.update(m, key, f)` → `Map[K, V]`, `map.insert(m, key, value)`, `map.delete(m, key)`, `map.clear(m)`

### set (auto-imported)
`set.new()` → `Set[A]`, `set.from_list(xs)` → `Set[A]`, `set.insert(s, value)` → `Set[A]`, `set.remove(s, value)` → `Set[A]`, `set.contains(s, value)`, `set.len(s)`, `set.is_empty(s)`, `set.to_list(s)` → `List[A]`, `set.union(a, b)` → `Set[A]`, `set.intersection(a, b)` → `Set[A]`, `set.difference(a, b)` → `Set[A]`, `set.symmetric_difference(a, b)` → `Set[A]`, `set.is_subset(a, b)`, `set.is_disjoint(a, b)`, `set.filter(s, f)` → `Set[A]`, `set.map(s, f)` → `Set[B]`, `set.fold(s, init, f)` → `B`, `set.each(s, f)`, `set.any(s, f)`, `set.all(s, f)`

### int (auto-imported)
`int.to_string(n)`, `int.to_hex(n)`, `int.parse(s)` → `Result[Int, String]`, `int.from_hex(s)` → `Result[Int, String]`, `int.abs(n)`, `int.min(a, b)`, `int.max(a, b)`, `int.band(a, b)`, `int.bor(a, b)`, `int.bxor(a, b)`, `int.bshl(a, n)`, `int.bshr(a, n)`, `int.bnot(a)`, `int.wrap_add(a, b, bits)`, `int.wrap_mul(a, b, bits)`, `int.rotate_right(a, n, bits)`, `int.rotate_left(a, n, bits)`, `int.to_u32(a)`, `int.to_u8(a)`, `int.clamp(n, lo, hi)`, `int.to_float(n)`

### float (auto-imported)
`float.to_string(n)`, `float.to_int(n)`, `float.round(n)`, `float.floor(n)`, `float.ceil(n)`, `float.abs(n)`, `float.sqrt(n)`, `float.parse(s)` → `Result[Float, String]`, `float.from_int(n)`, `float.min(a, b)`, `float.max(a, b)`, `float.to_fixed(n, decimals)`, `float.clamp(n, lo, hi)`, `float.sign(n)`, `float.is_nan(n)`, `float.is_infinite(n)`

### value (auto-imported)
`value.get(v, key)` → `Result[Value, String]`, `value.as_string(v)` → `Result[String, String]`, `value.as_int(v)` → `Result[Int, String]`, `value.as_float(v)` → `Result[Float, String]`, `value.as_bool(v)` → `Result[Bool, String]`, `value.as_array(v)` → `Result[List[Value], String]`, `value.str(s)` → `Value`, `value.int(n)` → `Value`, `value.float(f)` → `Value`, `value.bool(b)` → `Value`, `value.object(pairs)` → `Value`, `value.array(items)` → `Value`, `value.null()` → `Value`, `value.pick(v, keys)` → `Value`, `value.omit(v, keys)` → `Value`, `value.merge(a, b)` → `Value`, `value.to_camel_case(v)` → `Value`, `value.to_snake_case(v)` → `Value`, `value.stringify(v)`

### result (auto-imported)
`result.map(r, f)` → `Result[B, E]`, `result.map_err(r, f)` → `Result[A, F]`, `result.flat_map(r, f)` → `Result[B, E]`, `result.unwrap_or(r, default)` → `A`, `result.unwrap_or_else(r, f)` → `A`, `result.is_ok(r)`, `result.is_err(r)`, `result.to_option(r)` → `Option[A]`, `result.to_err_option(r)` → `Option[E]`, `result.collect(rs)` → `Result[List[T], List[E]]`, `result.partition(rs)` → `(List[T], List[E])`, `result.collect_map(xs, f)` → `Result[List[U], List[E]]`

### option (auto-imported)
`option.map(o, f)` → `Option[B]`, `option.flat_map(o, f)` → `Option[B]`, `option.flatten(o)` → `Option[A]`, `option.unwrap_or(o, default)` → `A`, `option.unwrap_or_else(o, f)` → `A`, `option.is_some(o)`, `option.is_none(o)`, `option.to_result(o, err)` → `Result[A, String]`, `option.filter(o, f)` → `Option[A]`, `option.zip(a, b)` → `Option[(A, B)]`, `option.or_else(o, f)` → `Option[A]`, `option.to_list(o)` → `List[A]`

### fs (requires `import fs`, effect fns)
`fs.read_text(path)` → `Result[String, String]`, `fs.read_bytes(path)` → `Result[List[Int], String]`, `fs.write(path, content)` → `Result[Unit, String]`, `fs.write_bytes(path, bytes)` → `Result[Unit, String]`, `fs.append(path, content)` → `Result[Unit, String]`, `fs.mkdir_p(path)` → `Result[Unit, String]`, `fs.exists(path)`, `fs.read_lines(path)` → `Result[List[String], String]`, `fs.remove(path)` → `Result[Unit, String]`, `fs.list_dir(path)` → `Result[List[String], String]`, `fs.is_dir(path)`, `fs.is_file(path)`, `fs.copy(src, dst)` → `Result[Unit, String]`, `fs.rename(src, dst)` → `Result[Unit, String]`, `fs.walk(dir)` → `Result[List[String], String]`, `fs.remove_all(path)` → `Result[Unit, String]`, `fs.file_size(path)` → `Result[Int, String]`, `fs.temp_dir()`, `fs.stat(path)` → `Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]`, `fs.glob(pattern)` → `Result[List[String], String]`, `fs.create_temp_file(prefix)` → `Result[String, String]`, `fs.create_temp_dir(prefix)` → `Result[String, String]`, `fs.is_symlink(path)`, `fs.modified_at(path)` → `Result[Int, String]`

### env (requires `import env`, effect fns)
`env.unix_timestamp()`, `env.args()`, `env.get(name)` → `Option[String]`, `env.set(name, value)`, `env.cwd()` → `Result[String, String]`, `env.millis()`, `env.sleep_ms(ms)`, `env.temp_dir()`, `env.os()`

### process (requires `import process`, effect fns)
`process.exec(cmd, args)` → `Result[String, String]`, `process.exit(code)`, `process.stdin_lines()` → `Result[List[String], String]`, `process.exec_in(dir, cmd, args)` → `Result[String, String]`, `process.exec_with_stdin(cmd, args, input)` → `Result[String, String]`, `process.exec_status(cmd, args)` → `Result[{code: Int, stdout: String, stderr: String}, String]`

### io (requires `import io`, effect fns)
`io.read_line()`, `io.print(s)`, `io.read_all()`, `io.write_bytes(data)`, `io.write(data)`

### json (requires `import json`)
`json.parse(text)` → `Result[Value, String]`, `json.stringify(v)`, `json.get(j, key)` → `Option[Value]`, `json.keys(j)`, `json.from_string(s)` → `Value`, `json.from_int(n)` → `Value`, `json.from_bool(b)` → `Value`, `json.null()` → `Value`, `json.array(items)` → `Value`, `json.from_float(n)` → `Value`, `json.stringify_pretty(j)`, `json.object(entries)` → `Value`, `json.get_string(j, key)` → `Option[String]`, `json.get_int(j, key)` → `Option[Int]`, `json.get_float(j, key)` → `Option[Float]`, `json.get_bool(j, key)` → `Option[Bool]`, `json.get_array(j, key)` → `Option[List[Value]]`, `json.as_string(j)` → `Option[String]`, `json.as_int(j)` → `Option[Int]`, `json.as_float(j)` → `Option[Float]`, `json.as_bool(j)` → `Option[Bool]`, `json.as_array(j)` → `Option[List[Value]]`, `json.root()` → `JsonPath`, `json.field(path, name)` → `JsonPath`, `json.index(path, i)` → `JsonPath`, `json.get_path(j, path)` → `Option[Value]`, `json.set_path(j, path, value)` → `Result[Value, String]`, `json.remove_path(j, path)` → `Value`

### http (requires `import http`, effect fns)
`http.serve(port, f)`, `http.response(status, body)` → `Response`, `http.json(status, body)` → `Response`, `http.with_headers(status, body, headers)` → `Response`, `http.redirect(url)` → `Response`, `http.status(resp, code)` → `Response`, `http.body(resp)`, `http.set_header(resp, key, value)` → `Response`, `http.get_header(resp, key)` → `Option[String]`, `http.req_method(req)`, `http.req_path(req)`, `http.req_body(req)`, `http.req_header(req, key)` → `Option[String]`, `http.query_params(req)` → `Map[String, String]`, `http.get(url)` → `Result[String, String]`, `http.post(url, body)` → `Result[String, String]`, `http.put(url, body)` → `Result[String, String]`, `http.patch(url, body)` → `Result[String, String]`, `http.delete(url)` → `Result[String, String]`, `http.request(method, url, body, headers)` → `Result[String, String]`

### regex (requires `import regex`)
`regex.is_match(pat, s)`, `regex.full_match(pat, s)`, `regex.find(pat, s)` → `Option[String]`, `regex.find_all(pat, s)`, `regex.replace(pat, s, rep)`, `regex.replace_first(pat, s, rep)`, `regex.split(pat, s)`, `regex.captures(pat, s)` → `Option[List[String]]`

### bytes (requires `import bytes`)
`bytes.len(b)`, `bytes.get(b, i)` → `Option[Int]`, `bytes.get_or(b, i, default)`, `bytes.set(b, i, val)` → `Bytes`, `bytes.slice(b, start, end)` → `Bytes`, `bytes.from_list(xs)` → `Bytes`, `bytes.to_list(b)`, `bytes.is_empty(b)`, `bytes.concat(a, b)` → `Bytes`, `bytes.repeat(b, n)` → `Bytes`, `bytes.new(len)` → `Bytes`, `bytes.push(b, val)`, `bytes.clear(b)`, `bytes.from_string(s)` → `Bytes`

### datetime (requires `import datetime`, effect fns)
`datetime.now()`, `datetime.from_parts(y, m, d, h, min, s)`, `datetime.parse_iso(s)` → `Result[Int, String]`, `datetime.from_unix(seconds)`, `datetime.format(ts, pattern)`, `datetime.to_iso(ts)`, `datetime.to_unix(ts)`, `datetime.year(ts)`, `datetime.month(ts)`, `datetime.day(ts)`, `datetime.hour(ts)`, `datetime.minute(ts)`, `datetime.second(ts)`, `datetime.weekday(ts)`, `datetime.add_days(ts, n)`, `datetime.add_hours(ts, n)`, `datetime.add_minutes(ts, n)`, `datetime.add_seconds(ts, n)`, `datetime.diff_seconds(a, b)`, `datetime.is_before(a, b)`, `datetime.is_after(a, b)`

### log (requires `import log`, effect fns)
`log.debug(msg)`, `log.info(msg)`, `log.warn(msg)`, `log.error(msg)`, `log.debug_with(msg, fields)`, `log.info_with(msg, fields)`, `log.warn_with(msg, fields)`, `log.error_with(msg, fields)`

### math (requires `import math`)
`math.min(a, b)`, `math.max(a, b)`, `math.abs(n)`, `math.pow(base, exp)`, `math.pi()`, `math.e()`, `math.sin(x)`, `math.cos(x)`, `math.tan(x)`, `math.log(x)`, `math.exp(x)`, `math.sqrt(x)`, `math.log10(x)`, `math.log2(x)`, `math.sign(n)`, `math.fmin(a, b)`, `math.fmax(a, b)`, `math.fpow(base, exp)`, `math.factorial(n)`, `math.choose(n, k)`, `math.log_gamma(x)`

### matrix (requires `import matrix`)
`matrix.zeros(rows, cols)` → `Matrix`, `matrix.ones(rows, cols)` → `Matrix`, `matrix.shape(m)` → `(Int, Int)`, `matrix.transpose(m)` → `Matrix`, `matrix.from_lists(rows)` → `Matrix`, `matrix.to_lists(m)` → `List[List[Float]]`, `matrix.get(m, row, col)`, `matrix.rows(m)`, `matrix.cols(m)`, `matrix.add(a, b)` → `Matrix`, `matrix.mul(a, b)` → `Matrix`, `matrix.scale(m, s)` → `Matrix`, `matrix.map(m, f)` → `Matrix`

### random (requires `import random`, effect fns)
`random.int(min, max)`, `random.float()`, `random.choice(xs)` → `Option[T]`, `random.shuffle(xs)` → `List[T]`

### testing (requires `import testing`)
`testing.assert_throws(f, expected)`, `testing.assert_contains(haystack, needle)`, `testing.assert_approx(a, b, tolerance)`, `testing.assert_gt(a, b)`, `testing.assert_lt(a, b)`, `testing.assert_some(opt)`, `testing.assert_ok(result)`

### error (requires `import error`)
`error.context(r, msg)` → `Result[T, String]`, `error.message(r)`, `error.chain(outer, cause)`

### path (requires `import path`)
`path.join(base, child)`, `path.dirname(p)`, `path.basename(p)`, `path.extension(p)` → `Option[String]`, `path.is_absolute(p)` → `Bool`

### args (requires `import args`)
`args.flag(name)` → `Bool`, `args.option(name)` → `Option[String]`, `args.option_or(name, fallback)` → `String`, `args.positional()` → `List[String]`
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

## Common mistakes (DO NOT)
- `list[1, 2, 3]` → **WRONG**. Write `[1, 2, 3]`. `list` is a module, not a type constructor
- `each(xs, f)` → **WRONG**. Write `list.each(xs, f)`. All stdlib functions need module prefix
- `map[K, V]` as a value → **WRONG**. Write `[:]` with type annotation to create an empty map
- `List.new()` → **WRONG**. Write `[]`. There is no `new()` for List
- `{"a": 1}` as a map → **WRONG**. Write `["a": 1]`. Braces `{}` are for records/blocks, brackets `[]` for lists and maps
- `string.length(s)` → **WRONG**. Write `string.len(s)`. No synonyms
- `println(x)` where x is Int → **WRONG**. Write `println(int.to_string(x))`. No implicit conversion
- `1 :: 2 :: []` → **WRONG**. Write `[1, 2]`. There is no cons operator `::`
- `fn foo<T>(x: T)` → **WRONG**. Write `fn foo[T](x: T)`. Use `[]` for generics, not `<>`

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

effect fn main(args: List[String]) -> Result[Unit, AppError] = {
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
