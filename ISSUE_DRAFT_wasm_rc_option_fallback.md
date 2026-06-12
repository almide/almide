# [wasm] heap corruption: `String ?? ""` fallback in a loop + later allocation → rc_dec trap / garbage pointers

**Severity**: cross-target contract violation (native ⇄ wasm divergence, C-class: same program, correct output on native, memory corruption on wasm).

**Found by**: nn qwen_tokenizer (pure-Almide BPE) — `pretokenize` returns 5 parts on native, 12-then-trap on wasm. Bisected to the construct below; the tokenizer itself is unmodified and correct on native (20/20 HF parity).

**Verified on**: fable-llm worktree build (develop + #433 repair), 2026-06-12.

## Minimal repro (15 lines)

```almide
effect fn main() -> () = {
  let cs = ["a", " "]
  var out: List[String] = []
  var i = 0
  while i < 2 {
    let nx = list.get(cs, i + 1) ?? ""
    if nx == "x" then {
      list.push(out, "p")
    } else {
      list.push(out, list.slice(cs, i, i + 1) |> list.join(""))
    }
    i = i + 1
  }
  println(out |> list.join(","))
}
```

- native: prints `a, ` (correct)
- wasm: `wasm trap: unreachable executed` in `__rc_dec`, called from `main`.
  Variants of the surrounding code instead die with
  `memory fault at wasm address 0x205b5d61` (= ASCII `" []a"`) or
  `0x7c7c7c7c` (= `"||||"`) — string BYTES being dereferenced as heap
  pointers, i.e. the allocator free-list or an rc header was overwritten
  with string content.

## Bisection facts (each one-step delta, all verified)

| change | wasm result |
|---|---|
| as above | TRAP |
| `let nx = "z"` instead of `list.get(...) ?? ""` | OK |
| keep `?? ""` nx, replace slice/join push with `list.push(out, "q")` | OK (latent — no trap, output correct) |
| drop the loop (single straight-line iteration incl. OOB `?? ""`) | OK |
| same code single-module vs cross-module | identical (not a linker issue) |

So the corruption needs ALL of:
1. `let nx = <Option[String]> ?? ""` where the **none arm is taken** (OOB
   `list.get`) — suspicious: rc_dec at `nx` scope-end on the static/empty
   fallback value,
2. a **loop** (scope-end bookkeeping runs, next iteration reuses heap),
3. a subsequent **allocation** (`list.slice |> list.join` here) — which
   either trips over the corrupted free list (trap in its rc_dec) or
   hands out garbage pointers.

Hypothesis: the wasm emit treats the `??` fallback literal as an owned
heap value and emits a scope-end `__rc_dec` for it; for the
empty-string/static constant this decrements something that was never
incremented (or a shared singleton), corrupting the heap. Native Rust
codegen takes the `unwrap_or` value path and is unaffected.

## Impact

Blocks L3-3 (browser/wasm port of nn): qwen_tokenizer's pretokenizer uses
exactly this shape (`let nx = char_at(cs, i + 1)` with `?? ""` inside a
scan loop). Any wasm-target Almide code that scans with an OOB-defaulted
lookahead is affected.

## Suggested fix locations

- wasm emit of `??` (Option unwrap-or): check rc/ownership of the
  fallback operand — constant strings must not receive a scope-end dec,
  or must be rc_inc'd when bound.
- Add the repro as `spec/wasm_cross/option_fallback_rc.almd` once fixed
  (contract: native ⇄ wasm byte-identical stdout).
