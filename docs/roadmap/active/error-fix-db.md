# Error-Fix Database [ACTIVE]

Structured mapping from every compiler error to fix suggestions with before/after code examples. Enables LLM auto-repair (target: 70%+ repair rate).

## Why

Current error messages have inline hints, but they're:
- Embedded in Rust source code (diagnostic.rs, parser/hints/)
- Not machine-queryable
- Not testable independently
- Not usable as LLM prompt context

An external, structured database makes errors:
- **Queryable**: LLM receives exact fix suggestions for the error it sees
- **Testable**: Each error→fix pair can be verified (apply fix → compiles)
- **Maintainable**: Non-engineers can review and improve suggestions
- **Measurable**: Track which errors LLMs can/can't self-repair

## Design

### Data Format

```json
{
  "errors": [
    {
      "id": "type-mismatch",
      "pattern": "type mismatch: expected {expected}, got {actual}",
      "category": "type-check",
      "severity": "error",
      "description": "Expression type doesn't match expected type",
      "fixes": [
        {
          "description": "Convert the value to the expected type",
          "conditions": "when {actual} is String and {expected} is Int",
          "before": "let x: Int = \"42\"",
          "after": "let x: Int = int.parse(\"42\")",
          "confidence": "high"
        },
        {
          "description": "Change the binding type to match the value",
          "conditions": "when the value type is correct but binding is wrong",
          "before": "let x: Int = string.trim(s)",
          "after": "let x: String = string.trim(s)",
          "confidence": "medium"
        }
      ],
      "related_docs": "docs/errors/type-mismatch.md",
      "test_file": "spec/errors/type_mismatch_fixes_test.almd"
    }
  ]
}
```

### Key Fields

| Field | Purpose |
|-------|---------|
| `id` | Stable identifier for the error |
| `pattern` | Regex-like pattern matching the error message (with placeholders) |
| `category` | type-check, parse, codegen, resolve, runtime |
| `fixes[]` | Ordered list of fix suggestions (most likely first) |
| `fixes[].conditions` | When this fix applies (natural language for LLM) |
| `fixes[].before/after` | Complete, compilable code examples |
| `fixes[].confidence` | high / medium / low — how likely this fix is correct |
| `test_file` | Almide test that verifies the fix |

### Error Categories

| Category | Source | Example |
|----------|--------|---------|
| `parse` | parser/hints/ | `expected 'then' after 'if'` |
| `type-check` | check/ | `type mismatch: expected Int, got String` |
| `resolve` | resolve.rs | `module 'xyz' not found` |
| `codegen` | emit_rust/ | Internal compiler errors |
| `runtime` | generated code | `index out of bounds` |

## Data Source: Extract from Existing Hints

The compiler already has rich hint data scattered across:

```
src/parser/hints/catalog.rs      — 61 parse error patterns
src/parser/hints/keyword_typo.rs — keyword typo → suggestion
src/parser/hints/operator.rs     — operator misuse hints
src/parser/hints/delimiter.rs    — bracket mismatch
src/check/mod.rs                 — type error hints
src/check/calls.rs               — function call error hints
src/check/expressions.rs         — expression type hints
src/diagnostic.rs                — hint rendering
```

Phase 1 extracts these into the JSON database. Phase 2 adds before/after examples.

## Phases

### Phase 1: Extract and Structure

- [ ] Catalog all error messages in the compiler (grep for diagnostic/hint/error)
- [ ] Create `docs/errors/error-db.json` with id, pattern, category, description
- [ ] Estimate: ~100-150 distinct error patterns

### Phase 2: Add Fix Suggestions

- [ ] For each error, write 1-3 fix suggestions with conditions
- [ ] Add before/after code examples (compilable Almide)
- [ ] Prioritize by frequency (common errors first)

### Phase 3: Add Tests

- [ ] For each fix, create a test: `spec/errors/<id>_fix_test.almd`
- [ ] Test verifies: (1) the "before" code produces the error, (2) the "after" code compiles
- [ ] CI runs these tests

### Phase 4: Integration

- [ ] `almide fix <file>` — reads error, looks up DB, applies most likely fix
- [ ] `almide explain <error>` — shows all fixes for an error
- [ ] LLM prompt injection: include relevant error-db entries in context
- [ ] Measure auto-repair rate against PRODUCTION_READY.md target (70%+)

### Phase 5: Feedback Loop

- [ ] Track which fixes succeed/fail when applied
- [ ] Adjust confidence scores based on data
- [ ] Add new fixes for errors that LLMs frequently fail on

## File Structure

```
docs/errors/
├── error-db.json           — The database
├── categories.md           — Category definitions
└── contributing.md         — How to add new entries

spec/errors/
├── type_mismatch_fix_test.almd
├── unknown_function_fix_test.almd
└── ...
```

## Metrics

| Metric | Target |
|--------|--------|
| Error patterns covered | 100% of compiler errors |
| Fix suggestions per error | ≥ 1 |
| Fixes with before/after examples | 100% |
| Fixes with tests | 80%+ |
| LLM auto-repair rate using DB | 70%+ |

## Relationship to Other Work

- **llms.txt** (infra-llms-txt): error-db feeds into llms-full.txt
- **compiler warnings** (phase-b-warnings): new warning types get DB entries
- **LLM integration** (ongoing-llm-fix): `almide fix` consumes this DB
- **PRODUCTION_READY.md**: 70%+ auto-repair rate metric
