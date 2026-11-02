# scoped — declarable reclamation boundaries

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](scoped.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](scoped.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](scoped.zh-CN.md)



> Authoritative source: [docs/specs/scoped.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/scoped.md), fixed at `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader snapshot translation, not automatically synchronized. The original specifications and their stated authority remain unchanged. Model proofs, implementation claims, historical status and evidence bounds retain their source scope; this translation supplies no independent execution, measurement, conformance or safety proof. Code and comments inside fences remain verbatim.

> Last updated: 2026-09-22

`scoped` is a modifier that can appear in **two places**. In block position, `scoped { … }` declares when memory ends; in declaration position, `scoped fn` makes “can run inside a region” **part of the signature** (the qualification is not silently lost when the body is edited or a module boundary is crossed). The normative rule is ALS-E31 in [docs/specs/als/expressions.md](../../../../specs/als/expressions.md); the contracts are C-362 / C-363 / C-364.

```almide
type Chain = Nil | Cons(Int, Chain)

scoped fn build(n: Int, acc: Chain) -> Chain =
  if n == 0 then acc else build(n - 1, Cons(n, acc))

scoped fn total(c: Chain, acc: Int) -> Int =
  match c {
    Nil => acc,
    Cons(h, t) => total(t, acc + h),
  }

fn sum_to(n: Int) -> Int = scoped { total(build(n, Nil), 0) }
```

Tests: `spec/wasm_cross/scoped_region_value.almd` (C-362), `spec/wasm_cross/scoped_worker_in_and_out.almd` (C-363), `spec/lang/scoped_test.almd`, `tests/scoped_region_test.rs`, `tests/diagnostics/e08{6,7,8}-scope-*` (C-364).

## Syntax — `scoped` is a contextual keyword

```ebnf
scoped_block := "scoped" block
fn_decl      := [visibility] ["scoped"] ["effect"] "fn" name …
```

`scoped` is read as a modifier in only two forms:

- **Expression position**: when `scoped` is immediately followed by `{` **on the same line**.
- **Declaration position**: when immediately followed by `fn` or `effect`.

Otherwise, `scoped` remains an identifier. In the **head** of `match` / `while` / `for … in`, the following `{` opens that construct's own body, so `scoped` is always read as an identifier — `match scoped { … }` means the same thing before and after #1997. Thus no existing program with a variable, function or module named `scoped` breaks, and this introduction itself did not increase the dialect epoch (epoch 4 at introduction; see `proofs/dialect-epochs.toml` for the current value and subsequent changes). To actually write a scoped block in a head, parenthesize it: `while (scoped { step(i) }) > 0 { … }`.

`almide fmt` round-trips in the order `[visibility] scoped [effect] fn`.

## What a scoped block promises

The **value of `scoped { body }` is the value of `body`**; removing `scoped` does not change stdout, stderr or exit code (C-362). The modifier declares a **memory reclamation boundary**: memory allocated for the block is reclaimed by that boundary, and references into the region do not cross it.

Realization differs by execution leg.

| Leg | Realization |
|---|---|
| Structural wasm | On block entry, `RegionSave` saves the bump pointer and 16 size-class heads in one saved block and zeros the heads; `RegionRestore` rewinds on exit. All blocks allocated inside disappear at once (`crates/almide-wasm/src/region.rs`) |
| native | The call site becomes `almide_region_window(\|\| …)`; `__rgn_` twins (twin enums with the `Copy` handle `AlmideRgn<T>`) run in the prelude arena. Closures that cannot be twinned release ownership at **the same closing point** (`crates/almide-codegen/src/pass_region_window.rs`) |

**Not claimed**: an upper bound on peak memory within the window, termination, or an upper bound on allocation before the boundary. These require separate contracts and benchmark oracles.

## What a scoped fn promises

A `scoped fn` is **an ordinary function outside a region**. Whether called directly or inside `scoped { … }`, its answer is the same and identical on both targets (C-363). The modifier imposes only **checks on the body**: it makes the promise “this function can run in a region” an **obligation**, not a discovery.

The stage 1 admitted fragment:

- **Values**: scalars (`Int` / `Float` / `Bool` / `Unit`), non-generic variant types declared in the same file (each case is unit or has positional fields, and fields are scalars or variants satisfying the same conditions), records with scalar fields only, and tuples and `Option` of those.
- **Workers**: ordinary workers (including tree-walking shapes) and **direct tail-recursive** workers.
- **Allowed calls and forms**: other `scoped fn`s, constructors, scalar stdlib calls in `int` / `float` / `math` / `bool`, scalar operators, `if` / `match` / blocks / local bindings / range `for` / `while`.

## What is refused, and why

Shapes outside the fragment are refused **at check time**. Checking precedes target selection, so `almide check` and `almide check --target wasm` give the same verdict (C-364). Backends do not decide acceptance: if the structural wasm leg disagrees with checking, that is E083 (a compiler defect), not a wall. A program containing `scoped` that takes a route outside that leg is refused rather than shipped without the boundary.

| Code | What is refused | Document |
|---|---|---|
| E086 | Shapes where a region value crosses the boundary (block value, writeback outside) | [E086](../../../../diagnostics/E086.md) |
| E087 | Operations not admitted inside a region (non-`scoped` calls, globals, host resources, unknown indirect calls, outer heap captures, out-of-fragment types, `!` / `guard` / `?`) | [E087](../../../../diagnostics/E087.md) |
| E088 | Recursive shapes retaining continuations (single non-tail self-recursion, mutual recursion) | [E088](../../../../diagnostics/E088.md) |

E086 has two lines, `= allocated at <file:line:col>` and `= scope ends at <file:line:col>`; E087 names “what may be retained” and “which declaration required this”; E088 names “what remains after the call”.

Tree-walking shapes (two or more self-calls in one arm) cannot be rewritten into an accumulator, so they are **accepted as ordinary workers**. E088 catches single non-tail recursion that can be fixed by accumulator conversion, and mutual recursion that retains activations while going back and forth.

## What is unfinished in stage 1

- `scoped` is limited to the entry program. Imported modules' `scoped fn` / `scoped` blocks are refused with E087 (the native region pass twins the root's function and type declarations).
- `String` and `T!` are outside the fragment. The design (#1997) freezes “a new String made from detached bytes may leave” and “`T!` stays `Result[T, String]`”, but detaching at the structural wasm boundary (copying outside the region before rewind) is not in stage 1. Both are refused by E087 / E086, so neither silently passes.
- `scoped effect fn` is accepted grammatically and becomes E087 during checking.
