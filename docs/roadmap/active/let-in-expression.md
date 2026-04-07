<!-- description: Support let...in as an inline expression form -->
# Let-In Expression

Support `let x = expr in body` as an expression, allowing inline bindings without opening a block.

## Current behavior

```almide
// Must use a block:
0x0C => {
  let r = read_leb128_u(p2)?
  ok({val: Br(r.val), next: r.next})
},
```

## Proposed

```almide
// Inline let-in:
0x0C => let r = read_leb128_u(p2)? in ok({val: Br(r.val), next: r.next}),
```

## Motivation

Found during porta implementation. Opcode dispatch tables have ~50 match arms that each need one intermediate binding. `let...in` eliminates the block + newline overhead and keeps the table scannable.

Also standard in ML-family languages (OCaml, Haskell, F#, Elm).
