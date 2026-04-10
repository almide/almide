<!-- description: Draft user-facing spec for Flow[T] — to be promoted to docs/specs/flow.md after Phase 1 -->
# Flow[T] — User Specification (Draft)

> Status: **draft** — will be promoted to `docs/specs/flow.md` after Phase 1 of [flow-design.md](./flow-design.md) is implemented.
>
> This document is the user-facing reference for Flow. Design rationale is in [flow-design.md](./flow-design.md).

---

## 1. Overview

A `Flow[T]` is a **lazy, single-pass, forward-only** sequence of values. Use it when you want to process data without loading it all into memory — log files, streaming I/O, infinite generators, or any pipeline where element count is unknown.

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

For small, bounded data where you need random access or multiple passes, use `List[T]` instead.

---

## 2. Core Properties

### 2.1 Lazy

A Flow pipeline does nothing until a **terminal operation** (`fold`, `each`, `collect`, `find`) is called. Building a pipeline is `O(1)` regardless of source size.

```almide
let pipeline = file.lines("huge.log")!
  |> flow.filter(is_error)
  |> flow.map(parse_line)
// Nothing has been read yet

let count = pipeline |> flow.fold(0, (acc, _) => acc + 1)
// Now the file is opened, lines are pulled one at a time, counted, and closed
```

### 2.2 Single-pass

A Flow can be consumed exactly once. Using a Flow after consumption is a compile error.

```almide
let flow = file.lines(path)!
let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)  // consumes flow
let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)  // ❌ E012: use after consumption
```

If you need multiple passes, materialize first with `flow.collect()`:

```almide
let xs: List[String] = file.lines(path)! |> flow.collect()  // now a List
let n1 = list.len(xs)      // OK
let n2 = list.len(xs)      // OK
```

### 2.3 Forward-only

A Flow cannot be accessed by index, reversed, sorted, or counted without consuming it. Random-access operations are compile errors:

```almide
let flow = file.lines(path)!
list.len(flow)       // ❌ E011: Flow does not support len
list.get(flow, 5)    // ❌ E011: Flow does not support random access
list.reverse(flow)   // ❌ E011: Flow is forward-only
list.sort(flow)      // ❌ E011: Flow cannot be sorted
```

See section 6 (Forbidden Operations) for the full list and alternatives.

---

## 3. When to Use Flow vs List

| Situation | Use | Reason |
|---|---|---|
| Fewer than ~1,000 elements | `List[T]` | Flow overhead not worth it |
| More than ~100,000 elements | `Flow[T]` | Memory O(1) vs O(n) |
| Size unknown (file, network, stdin) | `Flow[T]` | Safest default for I/O |
| Possibly infinite source | `Flow[T]` | Required |
| Need random access by index | `List[T]` | Flow is forward-only |
| Need to iterate multiple times | `List[T]` | Flow is single-pass |
| Need length without walking | `List[T]` | Flow has no length |
| Bounded parallel processing | `Flow[T]` + `fan.map(limit: n)` | Automatic backpressure |

**Simple rule**: if your data fits in RAM and you know its size, use `List`. If it might not or doesn't, use `Flow`.

---

## 4. API Reference

### 4.1 Sources

#### `flow.from_list[T](xs: List[T]) -> Flow[T]`

Convert a List into a Flow. Useful for adapting List data into Flow pipelines.

```almide
let f: Flow[Int] = flow.from_list([1, 2, 3])
f |> flow.map((n) => n * 2) |> flow.fold(0, (acc, n) => acc + n)  // => 12
```

#### `flow.generate[S, T](seed: S, step: (S) -> Option[(T, S)]) -> Flow[T]`

Generate a Flow from a seed. The `step` function takes the current state and returns either the next value + new state, or `none` to terminate.

```almide
// Finite: 1..10
flow.generate(1, (n) => if n <= 10 then some((n, n + 1)) else none)

// Infinite: all natural numbers
flow.generate(1, (n) => some((n, n + 1)))

// Fibonacci
flow.generate((0, 1), (pair) => {
  let (a, b) = pair
  some((a, (b, a + b)))
})
```

Infinite flows are fine — just make sure to `flow.take(n)` before `flow.collect()` or `flow.fold()` on an unbounded accumulator.

#### `file.lines(path: String) -> Flow[String]` 🔴 effect

Streaming read of a file, one line per element. The file handle is closed automatically when the Flow is consumed or dropped.

```almide
effect fn main() = {
  file.lines("log.txt")!
    |> flow.each((line) => println(line))
}
```

**Errors**: Failing to open the file is a `Result::Err`. Mid-read I/O errors panic (use `file.lines_checked` in a future phase for per-line error handling).

For small files, prefer `fs.read_text(path)!` + `string.lines(...)` to get a `List[String]` directly.

---

### 4.2 Transformations (lazy)

All transformations are lazy — they do not pull values until a terminal operation is called. The verb names match `list.*` exactly.

#### `flow.map[T, U](xs: Flow[T], f: (T) -> U) -> Flow[U]`

Apply `f` to each element.

```almide
file.lines(path)!
  |> flow.map(string.trim)
  |> flow.map(string.to_upper)
```

#### `flow.filter[T](xs: Flow[T], pred: (T) -> Bool) -> Flow[T]`

Keep elements where `pred` returns `true`.

```almide
flow.filter(lines, (l) => string.len(l) > 0)
```

#### `flow.filter_map[T, U](xs: Flow[T], f: (T) -> Option[U]) -> Flow[U]`

Apply `f`; keep only `some(...)` results and unwrap them. Useful for parsing where some inputs are invalid.

```almide
file.lines(path)!
  |> flow.filter_map((line) => parse_int(line))
```

#### `flow.flat_map[T, U](xs: Flow[T], f: (T) -> Flow[U]) -> Flow[U]`

Apply `f` (which returns a Flow) and concatenate the results.

```almide
file.lines(path)!
  |> flow.flat_map((line) => flow.from_list(string.split(line, " ")))
```

#### `flow.take[T](xs: Flow[T], n: Int) -> Flow[T]`

Keep only the first `n` elements. Stops pulling upstream after `n`.

```almide
file.lines("big.log")!
  |> flow.take(100)            // only pull first 100 lines
  |> flow.collect()            // List[String] with up to 100 elements
```

#### `flow.drop[T](xs: Flow[T], n: Int) -> Flow[T]`

Skip the first `n` elements; emit the rest.

```almide
file.lines("csv.txt")!
  |> flow.drop(1)              // skip header
  |> flow.each(process_row)
```

---

### 4.3 Terminal Operations 🔴

Terminal operations consume the Flow and produce a result. **This is where evaluation actually happens.**

#### `flow.fold[T, U](xs: Flow[T], init: U, combine: (U, T) -> U) -> U` 🔴

Reduce a Flow to a single value by threading an accumulator through `combine`.

```almide
// Count elements
flow.fold(0, (acc, _) => acc + 1)

// Sum
flow.fold(0, (acc, n) => acc + n)

// Build a Map
flow.fold(map.empty(), (m, (k, v)) => map.insert(m, k, v))
```

`fold` walks the **entire** Flow. For early termination, use `flow.find` or transform upstream with `flow.take`.

#### `flow.each[T](xs: Flow[T], f: (T) -> Unit) -> Unit` 🔴

Apply `f` to each element for its side effects. Returns nothing.

```almide
file.lines(path)!
  |> flow.each((line) => println(line))
```

#### `flow.collect[T](xs: Flow[T]) -> List[T]` 🔴

Materialize a Flow into a List. **This is the only Flow → List boundary.**

```almide
let first_10: List[String] = file.lines(path)!
  |> flow.take(10)
  |> flow.collect()
```

**Warning (W005)**: `flow.collect()` without an upstream `flow.take` triggers a compiler warning if the source size is unbounded. If you know the source is finite and small, suppress with `let _ = flow.collect(xs)` or add `|> flow.take(N)`.

#### `flow.find[T](xs: Flow[T], pred: (T) -> Bool) -> Option[T]` 🔴 (short-circuit)

Return the first element where `pred` returns `true`, or `none`. **Short-circuits** — stops pulling upstream as soon as a match is found.

```almide
// First ERROR line in the file (stops reading after finding it)
file.lines("big.log")!
  |> flow.find((l) => string.contains(l, "ERROR"))
```

Use `find` instead of `fold` for searches — `fold` would walk the entire source.

---

## 5. Resource Cleanup

Flows that hold external resources (file handles, network sockets) are cleaned up automatically when the Flow is consumed or goes out of scope. You do not need to close anything manually.

```almide
effect fn read_first_5(path: String) -> Result[List[String], String] = {
  let xs = file.lines(path)!
    |> flow.take(5)
    |> flow.collect()
  // File is already closed here — flow was consumed by collect()
  ok(xs)
}
```

Even if the pipeline is abandoned mid-stream (panic, early return, scope exit), the Flow is dropped and resources are released:

```almide
effect fn process(path: String) -> Result[Unit, String] = {
  let _n = file.lines(path)!
    |> flow.take(10)           // only read 10 lines
    |> flow.fold(0, (acc, _) => acc + 1)
  // Remaining lines are never read; file handle closed after take(10)
  ok(())
}
```

**You never write `.close()`** in Almide. Scope manages lifetime.

---

## 6. Forbidden Operations

The following operations on `Flow[T]` are **compile errors** (`E011`). Each has a suggested alternative.

| Operation | Why forbidden | Alternative |
|---|---|---|
| `list.len(flow)` | Flow may be infinite | `flow.fold(0, (acc, _) => acc + 1)` |
| `list.get(flow, i)` | Flow is forward-only | `flow.drop(i) \|> flow.take(1) \|> flow.find((_) => true)` |
| `list.reverse(flow)` | Flow is forward-only | `flow.collect() \|> list.reverse` |
| `list.sort(flow)` | Sorting requires all elements | `flow.collect() \|> list.sort` |
| `list.contains(flow, x)` | May walk infinite source | `flow.find((y) => y == x) != none` |
| `list.last(flow)` | Would walk infinite source | `flow.fold(none, (_, x) => some(x))` |

The compiler emits both a "lazy alternative" hint (preferred, memory safe) and a "materialize first" hint (with memory warning). See section 8 for error message examples.

---

## 7. Common Patterns

### 7.1 Log Analysis

```almide
import fs

effect fn count_errors(path: String) -> Result[Int, String] = {
  let n = file.lines(path)!
    |> flow.filter((line) => string.contains(line, "ERROR"))
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**Memory**: O(1) regardless of file size.

### 7.2 CSV → JSON Lines Conversion

```almide
effect fn csv_to_jsonl(input: String, output: String) -> Result[Unit, String] = {
  file.lines(input)!
    |> flow.drop(1)                                       // skip header
    |> flow.map((line) => row_to_json(string.split(line, ",")))
    |> flow.each((j) => fs.append_text(output, json.stringify(j) + "\n")!)
  ok(())
}
```

**Memory**: O(1). Process one row at a time.

### 7.3 Early Search (find)

```almide
effect fn first_error_line(path: String) -> Result[Option[String], String] = {
  let found = file.lines(path)!
    |> flow.find((line) => string.contains(line, "ERROR"))
  ok(found)
}
```

**Memory**: O(1). Stops reading on the first match.

### 7.4 First N Lines

```almide
effect fn head(path: String, n: Int) -> Result[List[String], String] = {
  let lines = file.lines(path)!
    |> flow.take(n)
    |> flow.collect()
  ok(lines)
}
```

**Memory**: O(n), bounded by the argument.

### 7.5 Infinite Source + Finite Use

```almide
fn first_100_primes() -> List[Int] = 
  flow.generate(2, (n) => some((n, n + 1)))
    |> flow.filter(is_prime)
    |> flow.take(100)
    |> flow.collect()
```

**Memory**: O(100). The infinite generator is safe because `take` bounds consumption.

### 7.6 Mixing List and Flow

```almide
let allowed_users: List[String] = ["alice", "bob"]          // small, List

effect fn filter_user_logs(log_path: String) -> Result[Int, String] = {
  let n = file.lines(log_path)!                              // huge, Flow
    |> flow.filter_map((line) => {
        let j = json.parse(line)?
        let user = json.get_string(j, "user")?
        if list.contains(allowed_users, user) then some(line) else none
      })
    |> flow.fold(0, (acc, _) => acc + 1)
  ok(n)
}
```

**Note**: `list.contains` works on `allowed_users` (a List). `flow.filter_map` works on `lines` (a Flow). Each module operates on its own type.

### 7.7 Parallel Processing (Phase 2+)

```almide
effect fn fetch_all(url_file: String) -> Result[Unit, String] = {
  file.lines(url_file)!
    |> flow.filter((url) => string.starts_with(url, "https://"))
    |> fan.map(limit: 10, (url) => http.get(url)!)           // bounded parallel
    |> flow.each((body) => write_to_disk(body)!)
  ok(())
}
```

**Memory**: bounded by `limit` (10 concurrent workers). Backpressure is automatic — the upstream Flow is not pulled until a worker is free.

---

## 8. Error Messages

### E011 — Forbidden operation on Flow

```
error[E011]: list.len cannot be called on Flow[String]
  --> bad.almd:5:11
   |
 5 |   let n = list.len(lines)
   |           ^^^^^^^^^^^^^^^
   = note: Flow[T] is lazy and possibly infinite
   = hint: to count lazily without materialization:
           lines |> flow.fold(0, (acc, _) => acc + 1)
   = hint: to materialize first (may use unbounded memory):
           lines |> flow.collect() |> list.len
```

Every E011 error includes **two hints**: the recommended lazy alternative, and the materialize-first fallback with a memory warning.

### E012 — Flow consumed twice

```
error[E012]: Flow[String] is move-only and has been consumed
  --> double.almd:4:11
   |
 3 |   let n1 = flow |> flow.fold(0, (acc, _) => acc + 1)
   |            ---- consumed here
 4 |   let n2 = flow |> flow.fold(0, (acc, _) => acc + 1)
   |            ^^^^ used after consumption
   = note: Flow is single-pass and cannot be consumed twice
   = hint: materialize first if you need multiple passes:
           let xs = flow |> flow.collect()
           let n1 = list.len(xs)
           let n2 = list.len(xs)
```

### E013 — `flow.collect` on a List

```
error[E013]: flow.collect expects Flow[T], got List[T]
  --> wrong.almd:2:6
   |
 2 |   [1, 2, 3] |> flow.collect()
   |                ^^^^^^^^^^^^^^
   = note: List is already materialized, no conversion needed
   = hint: just use the list directly: [1, 2, 3]
```

### W005 — Unbounded `flow.collect`

```
warning[W005]: flow.collect() on a flow without upstream flow.take
  --> risky.almd:3:6
   |
 3 |   |> flow.collect()
   |      ^^^^^^^^^^^^^^
   = note: if the source is large or infinite, this will OOM
   = hint: bound the collection:
           flow.take(N) |> flow.collect()
   = hint: or use flow.fold for accumulation:
           flow.fold(init, combine)
   = hint: if you know the source is finite and small, suppress with:
           let _ = flow.collect(flow)
```

---

## 9. Quick Reference

### All 12 functions at a glance

```
Sources
  flow.from_list(xs)                          List[T]     -> Flow[T]
  flow.generate(seed, step)                   S           -> Flow[T]
  file.lines(path)            🔴 effect       String      -> Flow[String]

Transformations (lazy)
  flow.map(xs, f)                             Flow[T]     -> Flow[U]
  flow.filter(xs, pred)                       Flow[T]     -> Flow[T]
  flow.filter_map(xs, f)                      Flow[T]     -> Flow[U]
  flow.flat_map(xs, f)                        Flow[T]     -> Flow[U]
  flow.take(xs, n)                            Flow[T]     -> Flow[T]
  flow.drop(xs, n)                            Flow[T]     -> Flow[T]

Terminal operations
  flow.fold(xs, init, combine)  🔴            Flow[T]     -> U
  flow.each(xs, f)              🔴            Flow[T]     -> Unit
  flow.collect(xs)              🔴            Flow[T]     -> List[T]
  flow.find(xs, pred)           🔴 (short)    Flow[T]     -> Option[T]
```

### Verb alignment with `list.*`

| list.* | flow.* |
|---|---|
| `list.map` | `flow.map` |
| `list.filter` | `flow.filter` |
| `list.filter_map` | `flow.filter_map` |
| `list.flat_map` | `flow.flat_map` |
| `list.take` | `flow.take` |
| `list.drop` | `flow.drop` |
| `list.fold` | `flow.fold` |
| `list.each` | `flow.each` |
| `list.find` | `flow.find` |
| `list.from_list` | (not needed) |
| (not in list) | `flow.collect` (boundary) |
| (not in list) | `flow.generate` (source) |

---

## 10. FAQ

**Q: Can I iterate a Flow twice?**
A: No. Flow is single-pass. If you need to iterate twice, materialize first: `let xs = flow |> flow.collect()`, then iterate `xs` (a List) multiple times.

**Q: My file is only a few KB. Should I use `file.lines` or `fs.read_text`?**
A: Use `fs.read_text(path)!` + `string.lines(...)`. That gives you a `List[String]` which is simpler for small files.

**Q: I want to count lines in a huge file. What's the best way?**
A: `file.lines(path)! |> flow.fold(0, (acc, _) => acc + 1)`. Memory is O(1).

**Q: I need random access to elements. Can I use Flow?**
A: No. Flow is forward-only. Either use a List, or `flow.collect()` first.

**Q: Can I `flow.collect` an infinite Flow?**
A: The compiler will warn (W005), but yes — it will try and eventually OOM. Always `flow.take(n)` before collecting an unbounded source.

**Q: How do I combine two Flows?**
A: Phase 1 doesn't have `flow.chain`. Use `flow.flat_map(flow.from_list([a, b]), (f) => f)` as a workaround, or wait for Phase 3.

**Q: What happens to the file handle if my code panics during `flow.fold`?**
A: The handle is closed automatically. Almide uses Rust's `Drop` trait — any resource held by a Flow is released when the Flow is dropped, including on panic.

**Q: Can I get the length of a Flow without consuming it?**
A: No. Length is not known for a lazy, possibly-infinite sequence. Use `flow.fold(0, (acc, _) => acc + 1)` to count (consumes the Flow), or materialize with `flow.collect() |> list.len`.

**Q: How does Flow compare to Rust's Iterator or Kotlin's Sequence?**
A: Similar semantics (lazy, forward-only). Differences:
- Almide uses `flow.*` as the module name; Rust uses method calls on Iterator, Kotlin uses `Sequence` extension functions.
- Almide rejects `list.len(flow)` at compile time; Rust and Kotlin allow it but hang on infinite sources.
- Almide auto-cleans file handles on Drop, like Rust, unlike Kotlin's `useLines` (which requires a scope block).

**Q: How does Flow interact with `fan.map`?**
A: `fan.map(flow, limit: n, f)` processes a Flow in parallel with automatic backpressure. The result is a `Flow[U]` — still lazy, still streaming. Great for parallel HTTP fetching or parallel file processing.

---

## 11. Comparison with Other Languages

For LLM / developer transfer learning:

| Almide | Rust | Kotlin | MoonBit | Haskell |
|---|---|---|---|---|
| `Flow[T]` | `impl Iterator<Item=T>` | `Sequence<T>` | `Iter[T]` | `[T]` (all lazy) |
| `flow.map` | `.map` | `.map` | `.map` | `fmap` |
| `flow.filter` | `.filter` | `.filter` | `.filter` | `filter` |
| `flow.fold` | `.fold` | `.fold` | `.fold` | `foldl'` |
| `flow.find` | `.find` | `.find` | `.find` | `find` |
| `flow.collect` | `.collect::<Vec<_>>()` | `.toList()` | `.collect()` | (automatic) |
| `flow.take(n)` | `.take(n)` | `.take(n)` | `.take(n)` | `take n` |
| `file.lines(path)` | `BufRead::lines()` | `File.useLines { }` | `@fs.lines` | `lines <$> readFile` |
| move-only | move/borrow | (mutable) | single-pass | (pure lazy) |
| auto cleanup | Drop | `useLines { }` scope | manual | lazy I/O (problematic) |

**Closest prior art**: MoonBit's `Iter[T]`. Almide's Flow shares the single-pass, lazy, chain-recommended design with a different module name convention.

---

## 12. Implementation Status

This document describes the **target design**. The implementation is tracked in [flow-design.md](./flow-design.md).

- [ ] Phase 1: Core `Flow[T]` type + 12 API + compile-time checks
- [ ] Phase 2: `fan.map` × Flow integration (parallel streaming)
- [ ] Phase 3: Additional operations (`zip`, `chain`, `take_while`, etc.)
- [ ] Phase 4: Async / WASM backend

When Phase 1 is done, this document moves to `docs/specs/flow.md` with a `> Last updated: YYYY-MM-DD` header and the draft status removed.
