# Reviewing generated Rust (#572)

In the qualified-code-generator model (the SCADE KCG precedent), the
generated source is the reviewable certified artifact. This is the
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

1. **Functions** are the correspondence unit: every `.almd` fn renders as
   one Rust fn (monomorphized generics render one fn per instantiation,
   suffixed with the concrete types). Statement ORDER inside a body is
   preserved by construction — the walker renders the IR statement list
   in sequence; no reordering pass exists on the Rust target.
2. **Stable patterns**: `effect fn` → `Result<T, String>` with explicit
   `?` only where the source wrote `!` (ADR-0008); `==` → `almide_eq!`;
   `+` on strings/lists → `AlmideConcat`; `mut` params → `&mut`;
   variants → Rust enums with the same case names; records → structs
   with the same field names. What the checker accepted is what renders —
   codegen never re-infers types (the TypeMap is the source of truth).
3. **Warning-free**: generated Rust must compile without warnings (a
   standing testing rule), and the whole run-manifest corpus builds under
   the Ferrocene-pinned toolchain weekly (`ferrocene-lane.yml`, #573).

## The certification frame

The generated source + this correspondence story is the
source-to-object half that the QUALIFIED COMPILER below (Ferrocene, the
#573 split) carries onward. The tool-qualification package indexing all
of it is `proofs/TOOL-QUALIFICATION.md` (#574).
