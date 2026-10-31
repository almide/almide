# Reviewing generated Rust (#572)

In the qualified-code-generator model (the SCADE KCG precedent), the
generated source is a reviewable artifact. That qualification model is not
a claim that Almide or its current build is qualified. This is the
review guide for Almide's `--target rust` output.

## The correspondence map

```
almide app.almd --target rust --trace-map > app.rs
```

emits `// almd: fn <name> @ line <N>` above every rendered function and
writes `app.trace.json` — rows of `{fn, almd_line, rust_line}`, derived
by scanning the SHIPPED text (the map cannot disagree with it; the gate
is `tests/trace_map_test.rs`). Anchors point at the fn BODY's source
line. Functions linked from the self-hosted stdlib carry their own
stdlib-source lines; their file identity is the stdlib module named in
the fn's dotted prefix. The default (flag-less) emission is byte-identical
to the committed emit baselines — anchors never appear unflagged.

## What a reviewer aligns on

1. **Functions** are the trace-map correspondence unit: surviving `.almd`
   functions normally render as Rust functions (monomorphized generics render one fn per instantiation,
   suffixed with concrete types). Inlining, dead-code removal and helper
   synthesis can change the one-to-one correspondence. The walker renders the
   final IR in sequence, but optimizations such as LICM and stream fusion act
   before it; source statement order cannot be inferred from that fact alone.
   Review observable ordering against the pass contracts and emitted text.
2. **Stable patterns**: effect results use the inferred/declared Result error
   channel, not always String. Postfix `!` follows its enclosing context
   ([context table](../specs/result-option-effect.md)); `==` first lowers through Secret/convention equality where applicable;
   ordinary `BinOp::Eq` renders `almide_eq!` (frontend `lower/expressions_access.rs`);
   `+` on strings/lists → `AlmideConcat`; `mut` params → `&mut`;
   variants → Rust enums with the same case names; records → structs
   with the same field names. What the checker accepted is what renders —
   codegen never re-infers types (the TypeMap is the source of truth).
3. **Warning-free**: generated Rust must compile without warnings (a
   standing testing rule), and the whole run-manifest corpus builds under
   the recorded Ferrocene upstream compatibility pin weekly
   (`ferrocene-lane.yml`, #573). That lane currently installs stock rustc,
   not a qualified Ferrocene binary.

## The certification frame

The proposed qualification split puts source generation here and a qualified
compiler below it. The current stock-rustc compatibility lane does not establish
that qualification. Evidence and remaining gaps are indexed in
[TOOL-QUALIFICATION](../../proofs/TOOL-QUALIFICATION.md) (#574).
