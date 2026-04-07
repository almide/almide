<!-- description: Allow variant constructors to be passed as first-class functions -->
# Variant Constructor as Function

Variant constructors like `Br(Int)` should be usable as `(Int) -> Instr` — i.e., passable to higher-order functions.

## Current behavior

```almide
type Instr = | Br(Int) | Call(Int)

fn apply(ctor: (Int) -> Instr, val: Int) -> Instr = ctor(val)

apply(Br, 5)  // ERROR: argument 'ctor' expects fn(Int) -> Instr but got Instr
```

## Expected behavior

```almide
apply(Br, 5)  // => Br(5)
```

The compiler should treat single-field variant constructors as functions with the corresponding signature.

## Motivation

Found while implementing porta (WASM binary parser). Many opcode dispatch patterns want to pass a constructor to a helper:

```almide
fn read_instr_u(p: Parser, ctor: (Int) -> Instr) -> Result[...] = {
  let r = read_leb128_u(p)?
  ok({val: ctor(r.val), next: r.next})
}

// Would eliminate ~30 lines of boilerplate in the opcode match
0x0C => read_instr_u(p2, Br),
0x0D => read_instr_u(p2, BrIf),
0x10 => read_instr_u(p2, Call),
```

## Workaround

Wrap in lambda: `read_instr_u(p2, (v) => Br(v))`. Functional but noisy.
