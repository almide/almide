# ADR-0038: An output line is streamed to the sink — `println("${n} …")` whose holes all have a direct sink writer holds the stream's lock and writes each part in order, builds no String, and E071 admits it by its types and shape

- **Status**: Accepted (design), 2026-11-20, with the rulings recorded under [Rulings](#rulings-2026-11-20). Not implemented yet.
- **Date**: 2026-11-20
- **Scope**: the output builtins (`println`, `eprintln`) when their argument is an interpolated string; the E071 rule (ALS-B4) under `@bounded` and `--profile critical`; the native (v1) and wasm lowerings of that one shape.
- **Related**:
  - #4855: E071 decides allocation from the call's declared result type, not from a name table. Since then `println(int.to_string(n))` inside a counted loop is refused, because `int.to_string` returns a heap `String`.
  - `tests/static_memory_test.rs::reference_app_is_critical_clean`: the flight reference app (`spec/wasm_cross/flight_pid_control.almd`, C-230, G-F4 #776) prints `int.to_string(cmd)` in its 16-step counted loop and now fails `--profile critical`. The stdlib has no way to print an Int without allocating.
  - C-311 (ALS-B4): no allocation inside a counted loop of a `@bounded` function.
  - #4871: E071 does not follow a callee. That is a separate fix; this ADR does not rely on it, and the flight app must not pass through it.
  - ADR-0036 (one admission stage): the E071 decision runs on every route through `admit`.

## Context

The critical profile promises that a counted loop allocates nothing (ALS-B4, E071). Since #4855, E071 reads that from types: a call whose result is a heap block allocates. This is the right rule, and it has a direct consequence: no program in the critical profile can print a number inside its control loop. The only way to turn an Int into text is a `String`, and every `String` is a heap block.

The flight reference app is the profile's own showcase, and it does exactly this:

```almide
for _t in 0..<16 {
  ...
  println(int.to_string(cmd))
}
```

Code in the critical profile has to report state every cycle. That is the application's purpose, not a convenience. So the profile needs an output path that does not allocate. Making the app pass by weakening E071 (a name exception for `int.to_string`, or a "small bounded allocation" allowance) would break the promise the profile exists to make.

Languages built for this kind of code all solve it the same way. They do not build a string. They format each value straight into an output sink:

- **Rust's `core::fmt`**: `write!(sink, "{} {}", a, b)` drives a `Write` implementation one piece at a time. Integer formatting writes digits into a small stack buffer and passes the slice to the sink. Nothing reaches the heap, and the facility exists in `no_std`.
- **Zig**: `writer.print("{d} {d}\n", .{a, b})` is checked at compile time against the format string and writes each piece into the writer. No allocator is passed, so none can be used.
- **Ada**: `Integer_Text_IO.Put(N)` writes an integer's image to the current output, with no heap. High-integrity Ada profiles admit it.

All three have the same shape: each value is written straight into the sink, in order, and nothing is assembled first. A scalar's text is produced in a small fixed area; an existing string is handed over as it is. In Almide, the place where a program already says "these values and literals form one line of output" is the interpolated string passed to `println`.

## Decision

### 1. Surface: no new syntax and no new stdlib name

The admitted form is the one programs already write:

```almide
println("${cmd}")
println("t=${t} cmd=${cmd} mode=${mode_name} sat=${saturated}")
eprintln("fault ${code} at ${stage}")
```

An **output line** is a call to an output builtin (`println`, `eprintln`, as `OUTPUT_ABORT_BUILTINS` describes them) whose single argument is an interpolated string, or a plain string literal, written directly in the argument position. Every hole's checked type must have a **direct sink writer**, meaning the value can be written to the sink as it is, without first being turned into a new String:

| Hole type | What the sink receives |
|---|---|
| `Int` (i64) and the sized ints `Int8` … `Int64`, `UInt8` … `UInt64` | the digits that type's `to_string` produces, from the shared digit writer (§3) |
| `Float` (f64), `Float32` | the text `float.to_string` / `float32.to_string` produces (shortest round-trip digits, no exponent, `.0` on an integral value; `NaN`, `inf`, `-inf`), from the shared float writer (§3) |
| `Bool` | `true` / `false` |
| `String` | the existing string's bytes, borrowed in place; nothing is copied or allocated |

The line is streamed (§3), so no part needs a maximum width and the line has no length bound. The digit and float writers still use a small fixed scratch area each: at most 20 bytes for an integer, 327 for an f64 and 48 for an f32. Those are measured constants. The longest f64 texts are `-5e-324` and the smallest normal `-2.2250738585072014e-308`, both 327 bytes; the longest f32 text is `-1.4e-45`, at 48 bytes. A unit test pins each constant by walking every binary exponent.

Every other hole type is refused (fail closed): records, variants, lists, maps, sets, `Option`, `Result`, tuples, type variables and unknown types.

Programs outside the critical profile see no change in meaning or output. The lowering in §3 applies to the shape wherever it appears, so normal and critical builds of the same line emit the same code. The checker admits the shape *because* its lowering never allocates; it does not depend on a profile switch.

### 2. How E071 judges it

E071 keeps refusing an interpolated string inside a counted loop, with one exception, decided from **position and types**, never from a name:

1. The interpolated string is the direct, single argument of a call that resolves to an output builtin. "Resolves" means the #4823 rule: the bare callee is the builtin itself, as the builtin table describes it ("writes to stdout" / "writes to stderr"), and a user declaration cannot shadow it.
2. Every hole's type in the checker's `type_map` passes `ty_has_sink_writer(ty)`. This is a closed predicate over the `Ty` constructors in the table above. An unknown type, a type variable, or any constructor not in the table answers **no** (fail closed).
3. Each hole expression is still walked by every other rule:
   - `println("${f(x)}")` where `f` allocates is refused by the call rules for `f`, including through a callee (#4871).
   - `println("${a + b}")` on Strings is refused, because the concatenation allocates.
   - A Float hole *observes* its value, as `float.to_string` does under E077. Computing inside the hole (`${a * b}` on Floats) is still refused by E077.

   The exemption covers only the line's own text.

Everything else stays refused, as today:

- `let s = "${n}"` in a loop is a String bound to a name (E071).
- `println(int.to_string(n))` builds a String and then prints it (E071, from the declared `String` result). This stays refused deliberately: admitting it would need either a name exception or a declaration-level "renders into a sink" attribute on stdlib functions. The second is a public surface change that this ADR does not need.
- `println("${xs}")` with `xs: List[Int]` (or a record, or an `Option`) has no direct sink writer (E071).

The novel-item test for the decision-site ledger (condition 2) has two parts:

- A hole of a fresh user type (a record, or a variant declared in the test) is refused.
- Every scalar and sized-int type is enumerated from the type registry and must be admitted, so a type added later is covered without editing the test.

The control patch (condition 4) widens the predicate to `true` and must turn the record-hole and list-hole fixtures red.

### 3. Lowering: hold the stream's lock for the line, stream each part in order

This is the writer and formatter style of Rust's locked `stdout` with `write!`, and of Zig's `writer.print`. The line takes the output stream's lock, writes each part in source order, writes the newline, and releases the lock. Between the first part and the newline, no other line on the same stream can write, from fan threads or anywhere else, so lines never interleave.

The holes are evaluated to values **before** the lock is taken. The write is therefore a leaf: it runs no user code while it holds the lock, so it cannot deadlock or re-enter.

- **MIR.** A new runtime op `RtFn::WriteLine { sink: Stdout | Stderr }` takes a list of parts:
  - `Lit(data-segment slice)`;
  - `Int(scalar, kind)`;
  - `Float(scalar, kind)`;
  - `Bool(scalar)`;
  - `Str(borrowed handle)`.

  It borrows every `Str` handle, so it produces no ownership event and the certificate sees only borrows. Its capability is the same `Capability::Stdout` / `Stderr` that `RtFn::PrintStr` carries, and the `RtFn → Capability` mapping stays exhaustive, so the capability witness accounts for it automatically.
- **Native (v1 renderer).** The op takes the process-wide lock of the stream (Rust's `Stdout::lock` / `Stderr::lock`, the same handle `PrintStr` uses) and writes each part with `write_all`:
  - literal and `Str` parts write their bytes directly;
  - `Int` and `Float` parts are formatted by the shared writer into a stack scratch area of the fixed size given in §1, and that slice is written;
  - the newline is written last, and the lock is released.

  stdout's line buffer is allocated **once**, on the first write of the process, and never again; stderr is unbuffered. Neither allocates per line. The `alloc_count` row records this: the one buffer allocation at the first output, then zero allocations per output line.
- **Wasm: one gathered host write per line.** Separate host calls per part are not provable here, because the instance-parallel fan (#3003, ADR-0011 §D2a, op 74 in `almide-wasm-run/src/host.rs`) runs chunks on fresh instances that share the host's stdout. Another instance's output could land between two calls of one line. The lowering therefore makes the whole line **one** host call:
  - It builds an iovec list: literal parts point at the data segment, `Str` parts point at the existing string's bytes in place, and `Int` / `Float` / `Bool` parts point at their formatted text in a scratch region.
  - It issues one `fd_write` with that list.
  - Both the iovec list and the scalar scratch region have a size the compiler knows: the part count, and the sum of the fixed scalar widths. Strings are referenced, never copied. One region of the largest such size in the module is reserved at link time.
  - The Almide host's `fd_write` holds the process-wide stdout lock for the whole iovec list. That is the one host-side change, and it makes the gathered write atomic across instances.
  - Under a foreign host, no instance-parallel fan exists (op 74 is offered only by the Almide host), so no other writer is in the instance and the single call cannot be split.

  The component targets (WASI preview 2/3) have no gather write in the output-stream interface. Their lowering is one `blocking-write-and-flush` per part, issued under a host-held line lock: the host takes it at the line's first part and releases it at the newline or when the instance traps. This needs one host change, and the implementation must show it with a two-instance interleaving test. Until that test is green, the component target does not lower output lines with more than one part, and refuses them with an "unavailable on this target" diagnostic rather than writing them unlocked.
- **One digit algorithm, one float algorithm.** `int.to_string` and each sized int's `to_string` become "measure the width, allocate exactly that, call the shared digit writer". `float.to_string` and `float32.to_string` become the same with the shared float writer.
  - On native, each writer is one runtime function per kind.
  - On wasm, each writer is self-hosted. Today `stdlib/float_to_string.almd` builds a String; it is split into a writer into a byte region plus a thin String wrapper.

  The String path and the sink path then share one implementation per target, so the two forms cannot diverge on either target. Native-versus-wasm equality of the writers is a separate claim, and the cross-target fixtures in §4 prove it.

### 4. Contracts and als

New contracts land in almide/als first. The order is: als PR → advance `proofs/als-pin.txt` → the implementation PR.

- **C-311 (ALS-B4), amended statement**: "… constructing, concatenating or pushing to a list/string/map/set … is rejected with E071, except an output line (ADR-0038). An output line is an interpolated string written directly as the argument of an output builtin, whose holes all have a direct sink writer (the integer types, Float, Float32, Bool, String). It builds no string." The ALS spec text in `docs/specs/als/bounded.md` §ALS-B4 changes with it. This is the approved spec change.
- **New contract (id assigned in als)**: "An output line writes the same bytes on both targets as the String path would, allocates nothing per line, and never interleaves with another line on the same stream." Its evidence:
  - a `spec/wasm_cross` fixture printing boundary integers both as an output line and through `to_string`: `i64::MIN`, `-1`, `0`, `i64::MAX`, each sized int's min and max, `true` and `false`. It also prints lines that mix String, Int and literal parts, compared against the same text built with `+`. Native and wasm must match each other, and the two forms must match each other.
  - a `spec/wasm_cross` Float fixture on the new writer path, with the same two-form, two-target comparison. For f64 and f32 it covers:
    - `0.0` and `-0.0`;
    - the smallest and largest subnormals, and the smallest normal;
    - the largest finite value and its negation;
    - NaN, `inf` and `-inf`;
    - the values whose shortest text is the longest.

    The fixture builds NaN and the infinities from bits, so it needs no Float computation.
  - a fan fixture in which parallel arms each print many multi-part lines, on native and on the wasm instance-parallel path. Every output line must appear whole.
  - the `alloc_count` ledger row: one buffer allocation at the first output, then 0 allocations per output line.
  - E071 diagnostic fixtures:
    - admitted: a line with Int, Float, Bool and String holes;
    - refused: a record hole, a list hole, `let`-bound interpolation, and `println(int.to_string(n))`.
- **No dialect epoch.** No program that compiled before stops compiling. The change only admits more under `@bounded` / critical. The interface diff is *identical*: no stdlib function is added or removed.

### 5. C-230: the flight app stays byte-identical

The app changes one line, in als first and then here:

```diff
-    println(int.to_string(cmd))
+    println("${cmd}")
```

`"${cmd}"` for an `Int` produces exactly the digits `int.to_string(cmd)` does (§3, one digit algorithm). The 16-line stdout trace is therefore byte-identical, natively and on wasm, and C-230's statement does not change. `reference_app_is_critical_clean` turns green on its own merits, with no exemption. It remains the known red on `decision-sites` until this lands, and it is not to be made green through the #4871 gap.

## Alternatives considered

1. **A scalar print family** (`io.print_int(n)`, `io.print_bool(b)`, `io.print_str(s)`, `io.newline()`, and one per sized int). This is rejected for several reasons:
   - It adds about 12 public names that must be extended by matrix (every sized int, both sinks).
   - It gives LLM-written code a second way to print, which costs accuracy against the mission metric.
   - It splits one line into several unlocked writes, so lines could interleave.
   - The flight app's single line would become three statements.
2. **Admit `println(int.to_string(x))` as a special case.** This is a name-keyed exception, so it fails condition (1) of the decision-site inspection, and the next formatter function would need another one. Rejected.
3. **Assemble the whole line in a fixed-size buffer and write it once.** This was the first revision of this ADR. It needs a compile-time maximum line length, so it cannot admit String holes, and it copies every byte once more. It is superseded by streaming under the lock (ruling of 2026-11-20). The wasm gathered write keeps the one property of it that is needed there, one host call per line, without copying strings.
4. **An explicit writer value** (`let w = io.stdout()` and `w.write_int(n)`, as in Zig and Rust `core::fmt` with an explicit sink). This is the most general option and could later serve byte buffers and files. It needs a new capability-carrying type and a public stdlib surface. It may come later, built on the same `WriteLine` op; it is not needed to close the profile's gap. Rejected for now.
5. **Allow a bounded allocation that is freed in the same iteration.** This changes what ALS-B4 promises, from "no heap in the loop" to "bounded heap per iteration", and brings an allocator into the critical path. Rejected.
6. **Change the app to print after the loop** (collect into a pre-sized list). This changes the app's trace semantics (reporting at the end instead of every cycle) and only moves the allocation. Rejected.

## Consequences

- The critical profile can report its state every cycle, including String-valued state, which its reference application needs.
- E071 keeps its #4855 type rule unchanged. The exception is one closed predicate over types plus one position test, and it fails closed.
- Normal programs get the same code for the same line. The only difference is fewer allocations (a `println("x=${n}")` in any loop stops allocating), with no change in output.
- `int.to_string` / `float.to_string` and the output line share one writer per kind, so the String form and the streamed form agree on each target by construction. The cross-target fixtures carry the native-versus-wasm claim.
- The Almide wasm host's `fd_write` takes the process stdout lock for a whole iovec list. That is the host change the interleaving claim rests on.
- The decision-site ledger gets one new row (the output-line admission) with a novel test, a probe on every route, and a control.

## Open questions

1. **Interpolation outside output builtins.** Should it ever be admitted (for example, into a caller-provided buffer)? That is the explicit-writer alternative (4).
2. **Component targets.** The host-held line lock (§3) is unproven until the two-instance test exists. Until then, multi-part lines are refused on those targets.

## Rulings (2026-11-20)

1. ○ to the design, including the amended statement of C-311 (ALS-B4). That spec change is approved.
2. Hole types: Int, the sized ints, Bool, **Float** (f64 and f32), and, by the revision below, **String**. Byte equality between native and wasm on the new writer path is proven by the Float boundary fixture under a new contract or contract clause that lands in almide/als first.
3. `float.to_string` is rebuilt on the same writer as the Float hole (§3), as `int.to_string` is, so the two cannot diverge.
4. **Revision:** the lowering holds the output stream's lock for the line and streams each part in order, instead of assembling a fixed buffer. So String holes are allowed and no maximum line width is needed. Lines must still never interleave across fan threads. Native uses a process-wide lock with no allocation per line; a buffer allocated once at startup is fine, and the `alloc_count` row says so. Wasm shows that no other output can land inside a line. This ADR chooses one gathered host write per line (§3), because separate calls per part cannot be shown safe under the instance-parallel fan.
5. Order: the als PR (C-311 amendment, the new contract, its fixtures) → the pin commit here → the implementation → the flight app changes to `println("${cmd}")`, which turns `reference_app_is_critical_clean` green.
