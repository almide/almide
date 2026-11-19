# ADR-0038: An output line of scalars is written to the sink — `println("${n} …")` whose holes all have a fixed-width text form lowers to one write from a fixed-size line buffer, builds no String, and E071 admits it by its types and shape

- **Status**: Proposed (design; not implemented), 2026-11-20. Implementation waits for review.
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

All three have the same shape. The text of a scalar has a known maximum width, so it can be produced in fixed storage and handed to the sink. In Almide, the place where a program already says "these scalars and literals form one line of output" is the interpolated string passed to `println`.

## Decision

### 1. Surface: no new syntax and no new stdlib name

The admitted form is the one programs already write:

```almide
println("${cmd}")
println("t=${t} cmd=${cmd} sat=${saturated}")
eprintln("fault ${code}")
```

An **output line** is a call to an output builtin (`println`, `eprintln`, as `OUTPUT_ABORT_BUILTINS` describes them) whose single argument is an interpolated string, or a plain string literal, written directly in the argument position. Every hole's checked type must have a **fixed-width text form**:

| Hole type | Maximum width (bytes) | Text |
|---|---|---|
| `Int` (i64) | 20 | the same digits `int.to_string` produces, `-9223372036854775808` at most |
| sized ints `Int8` … `Int64`, `UInt8` … `UInt64` | 4 / 6 / 11 / 20 / 3 / 5 / 10 / 20 | the same digits as that type's `to_string` |
| `Bool` | 5 | `true` / `false` |

Each hole's type alone fixes the line's maximum length: the literal bytes, plus the widths above, plus the newline. That bound is what makes the line allocation-free. The whole line fits in a buffer whose size is a compile-time constant.

Not in the fixed-width set in this ADR:

- **`String` holes** (a borrowed, existing value). Writing them does not allocate, but they remove the static line bound, so the line could no longer be one write from a fixed buffer (see Open questions).
- **`Float` holes.** Shortest round-trip float text is bounded (about 24 bytes), but the formatting code is large, and floats belong to E077's domain. This is left for a separate decision.
- **Records, variants, lists, `Option`, type variables, unknown types.** These are refused (fail closed).

Programs outside the critical profile see no change in meaning or output. The lowering in §3 applies to the shape wherever it appears, so normal and critical builds of the same line emit the same code. The checker admits the shape *because* its lowering never allocates; it does not depend on a profile switch.

### 2. How E071 judges it

E071 keeps refusing an interpolated string inside a counted loop, with one exception, decided from **position and types**, never from a name:

1. The interpolated string is the direct, single argument of a call that resolves to an output builtin. "Resolves" means the #4823 rule: the bare callee is the builtin itself, as the builtin table describes it ("writes to stdout" / "writes to stderr"), and a user declaration cannot shadow it.
2. Every hole's type in the checker's `type_map` passes `ty_has_fixed_text_width(ty)`. This is a closed predicate over the `Ty` constructors in the table above. An unknown type, a type variable, or any constructor not in the table answers **no** (fail closed).
3. Each hole expression is still walked by every other rule. `println("${f(x)}")` where `f` allocates is refused by the call rules for `f`; the line exemption covers only the line's own text.

Everything else stays refused, as today:

- `let s = "${n}"` in a loop is a String bound to a name (E071).
- `println(int.to_string(n))` builds a String and then prints it (E071, from the declared `String` result). This stays refused deliberately: admitting it would need either a name exception or a declaration-level "renders into a sink" attribute on stdlib functions. The second is a public surface change that this ADR does not need.
- `println("${s}")` with `s: String` is refused until the open question below is decided.

The novel-item test for the decision-site ledger (condition 2): a hole of a scalar type that has no table row is refused, and a fresh sized-int type added to the predicate is admitted without naming any function. The control patch (condition 4) widens the predicate to `true` and must turn the String-hole and record-hole fixtures green-to-red.

### 3. Lowering

The lowering is **one write per line**, the same as today, so stdout and stderr interleaving between lines (including fan arms) is unchanged. Only the place the bytes are assembled changes: a fixed-size buffer instead of a heap String.

- **MIR.** A new runtime op `RtFn::WriteLine { sink: Stdout | Stderr }` takes a list of parts:
  - `Lit(data-segment slice)`;
  - `Int(scalar, kind)`;
  - `Bool(scalar)`.

  Its capability is the same `Capability::Stdout` / `Stderr` that `RtFn::PrintStr` carries, and the `RtFn → Capability` mapping stays exhaustive, so the capability witness accounts for it automatically. The holes are evaluated to scalars **before** the op, so the op is a leaf: it runs no user code and cannot re-enter.
- **Native (v1 renderer).** The op renders as a stack array `[u8; N]`, where `N` is the static bound from §1. The array is filled by one runtime digit writer (`almide_rt_write_i64_digits(&mut [u8], i64) -> usize`, and the same for unsigned) and then written with one `write_all` on the same locked handle, at the same flush point as `PrintStr`. `--target rust` emits the equivalent formatting macro, which also writes through `core::fmt` without a heap String.
- **Wasm.** The line is assembled in a scratch region of linear memory, reserved once at link time with size `max N` over the module's output lines, and passed to the same host write (`fd_write` on preview 1, the output-stream write on preview 2/3) that `PrintStr` uses. The op is a leaf and an instance runs one write at a time, so one region is enough. If a target ever runs two writes concurrently in one memory, the region must move to per-thread storage, and the lowering will refuse to emit until it does.
- **One digit algorithm.** `int.to_string` (and each sized int's `to_string`) is re-expressed as "measure the width, allocate exactly that, call the same digit writer". The String path and the sink path then share one implementation, so byte equality is enforced by construction rather than by two implementations agreeing. The `alloc_count` ledger records zero allocations for an output line on wasm, and that serves as evidence.

### 4. Contracts and als

New contracts land in almide/als first. The order is: als PR → advance `proofs/als-pin.txt` → the implementation PR.

- **C-311 (ALS-B4), amended statement**: "… constructing, concatenating or pushing to a list/string/map/set … is rejected with E071 — except an output line (ADR-0038): an interpolated string written directly as the argument of an output builtin, whose holes all have a fixed-width text form. It builds no string." The ALS spec text in `docs/specs/als/bounded.md` §ALS-B4 changes with it. This is a spec change and waits for the owner's ○.
- **New contract (id assigned in als)**: "An output line writes the same bytes on both targets as the String path would, and allocates nothing." Its evidence is:
  - a `spec/wasm_cross` fixture printing boundary values (`i64::MIN`, `-1`, `0`, `i64::MAX`, each sized int's min and max, `true`, `false`) both as an output line and through `int.to_string`. Native and wasm must match each other and the two forms must match each other.
  - the wasm `alloc_count` ledger row (0 allocations for the line).
  - E071 diagnostic fixtures: admitted line; refused String hole; refused record hole; refused `let`-bound interpolation; refused `println(int.to_string(n))`.
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
   - It splits one line into several writes, which changes interleaving between lines.
   - The flight app's single line would become three statements.
2. **Admit `println(int.to_string(x))` as a special case.** This is a name-keyed exception, so it fails condition (1) of the decision-site inspection, and the next formatter function would need another one. Rejected.
3. **An explicit writer value** (`let w = io.stdout()` and `w.write_int(n)`, as in Zig and Rust `core::fmt` with an explicit sink). This is the most general option and could later serve byte buffers and files. It needs a new capability-carrying type and a public stdlib surface. It may come later, built on the same `WriteLine` op; it is not needed to close the profile's gap. Rejected for now.
4. **Allow a bounded allocation that is freed in the same iteration.** This changes what ALS-B4 promises, from "no heap in the loop" to "bounded heap per iteration", and brings an allocator into the critical path. Rejected.
5. **Change the app to print after the loop** (collect into a pre-sized list). This changes the app's trace semantics (reporting at the end instead of every cycle) and only moves the allocation. Rejected.

## Consequences

- The critical profile can report scalar state every cycle, which its reference application needs.
- E071 keeps its #4855 type rule unchanged. The exception is one predicate over types plus one position test, and it fails closed.
- Normal programs get the same code for the same line. The only difference is fewer allocations (a `println("x=${n}")` in any loop stops allocating), with no change in output.
- `int.to_string` and the output line share one digit writer, so the cross-target and cross-form byte equality has one source.
- The decision-site ledger gets one new row (the output-line admission) with a novel test, a probe on every route, and a control.

## Open questions (for review)

1. **`String` holes.** Should a borrowed `String` hole be admitted? Doing so removes the static line bound, so the line would be several writes (literal, hole, literal, …) or a bounded buffer plus an overflow path. The ADR's position is to leave it out until a measured need appears.
2. **`Float` holes.** Should they wait for an E077 decision?
3. **Interpolation outside output builtins.** Should it ever be admitted (for example, into a caller-provided fixed buffer)? That is the explicit-writer alternative (3).
