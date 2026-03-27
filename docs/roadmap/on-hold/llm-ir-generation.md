<!-- description: LLM generates typed IR as JSON directly, bypassing parser errors -->
# LLM → IR Direct Generation

LLMs generate typed IR (JSON) directly instead of text, reducing parser errors to zero.

## Why

Current LLM code generation:
```
LLM → .almd text → Lexer → Parser → AST → Checker → Lowering → IR → Codegen
```

Problem: Since LLMs generate text, syntax errors, indentation mistakes, and token boundary ambiguities occur. A major bottleneck for modification survival rate.

Proposed new path:
```
LLM → IR (JSON) → Codegen
```

IR is:
- JSON serializable (`serde::Serialize/Deserialize` done)
- Fixed structure (30 `IrExprKind` variants, 8 `IrStmtKind` variants)
- All nodes carry type information (Ty enum)
- Pipes, UFCS, and string interpolation are already desugared

LLM accuracy for generating structured output (JSON) is higher than text generation. In particular, OpenAI's structured outputs and Anthropic's tool use can guarantee JSON schema-compliant output.

## Architecture

```
                    ┌─────────────────────┐
                    │  Traditional path   │
                    │  .almd → ... → IR   │
                    └──────────┬──────────┘
                               │
                               ▼
LLM → JSON ──────────────▶ IrProgram ──▶ Codegen ──▶ .rs / .ts
                               ▲
                               │
                    ┌──────────┴──────────┐
                    │  Validation pass    │
                    │  (type consistency, │
                    │   VarId resolution) │
                    └─────────────────────┘
```

## Phases

### Phase 1: IR round-trip validation
- `almide emit --emit-ir app.almd | almide compile --from-ir` pipeline
- IR JSON → deserialize → codegen → verify output matches original
- This is an extension of the `--emit-ir` roadmap

### Phase 2: IR validation pass
- Accept externally generated IR JSON and check consistency:
  - Whether VarId exists in VarTable
  - Whether each node's Ty is consistent
  - Whether CallTarget references exist
- Return diagnostics with fix hints on errors

### Phase 3: LLM prompt engineering
- Include IR JSON Schema in LLM system prompts
- Few-shot examples: `.almd` source + corresponding IR JSON pairs
- Have LLMs generate IR directly in structured output mode

### Phase 4: Hybrid mode
- LLM first generates text `.almd` → falls back to direct IR generation if parser errors occur
- Integration with `almide forge` (existing LLM integration roadmap)

## Key insight

IR redesign Phase 5 completion is a prerequisite. Since it has been proven that codegen takes only `&IrProgram` as input, the "LLM → IR → codegen" path is technically viable. If AST fallbacks had remained, codegen from IR alone wouldn't have been possible.

## Risk

- IR JSON is more verbose than `.almd` text (10-50x). Consumes LLM context window
- Unverified whether LLMs can maintain VarId consistency
- Mitigation: Consider an intermediate format where VarIds are name-based, converted to VarIds in a post-pass

## Related

- [--emit-ir](emit-ir.md) — Foundation for IR JSON output
- [LLM Integration](llm-integration.md) — `almide forge` / `almide fix`
