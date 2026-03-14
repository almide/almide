# --emit-ir: IR JSON Export [ACTIVE]

## Summary

Add `--emit-ir` flag to output the typed IR as JSON, complementing the existing `--emit-ast`.

## Motivation

Inspired by [lean4-rust-backend](https://github.com/O6lvl4/lean4-rust-backend)'s JSON IR interchange format. Almide's IR already derives `Serialize`/`Deserialize` via serde — this is nearly zero-cost to expose.

## Use Cases

1. **IR visualization & debugging**: Inspect the typed IR after lowering to verify type annotations, VarId assignments, use counts
2. **Optimization pass testing**: Feed IR JSON into tests to verify borrow analysis, single-use optimization independently
3. **Tooling**: External tools can analyze IR structure (linters, documentation generators)
4. **Future: third-party backends**: Enable community-built codegen targets without modifying the compiler

## Design

### CLI

```bash
almide app.almd --emit-ir          # Print IR JSON to stdout
almide app.almd --emit-ir -o ir.json  # Write to file
```

### Implementation

Minimal change in `src/cli.rs` and `src/main.rs`:
1. Add `--emit-ir` flag to CLI parser
2. After lowering (AST → IR), serialize `IrProgram` with `serde_json::to_string_pretty`
3. Output and exit (skip codegen)

### Output Format

Standard JSON serialization of `IrProgram`. The existing `#[serde(tag = "kind", rename_all = "snake_case")]` annotations on IR types produce clean, readable output.

## Testing

- Verify round-trip: `--emit-ir` output can be deserialized back to `IrProgram`
- Verify all IR node types appear correctly in JSON output
