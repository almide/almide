# Variant Record Fields [DONE]

Allow enum variants to carry named fields (like Rust's struct variants), instead of only positional arguments.

## Motivation

Self-tooling revealed that variants with 3+ positional args are hard to read and error-prone — both for humans and LLMs:

```almide
// Current: what is the 4th string?
BeginEndCap("meta.interpolation.almide", "\\$\\{", "\\}", "punctuation..begin", "punctuation..end", "source.almide.embedded", [Include("source.almide")])
```

Named fields fix this:

```almide
type Pat =
  | Match { scope: String, regex: String }
  | BeginEnd { scope: String, begin: String, end: String, patterns: List[Pat] }
  | BeginEndCap { scope: String, begin: String, end: String, begin_cap: String, end_cap: String, content_name: String, patterns: List[Pat] }
  | Include(String)
```

Construction becomes self-documenting:

```almide
BeginEndCap {
  scope: "meta.interpolation.almide",
  begin: "\\$\\{", end: "\\}",
  begin_cap: "punctuation.section.interpolation.begin.almide",
  end_cap: "punctuation.section.interpolation.end.almide",
  content_name: "source.almide.embedded",
  patterns: [Include("source.almide")]
}
```

## Design

- Variant with `{ }` → record variant; variant with `( )` → tuple variant (existing)
- Single unnamed payload stays as-is: `| Include(String)`
- Pattern matching uses field names: `match pat { BeginEnd { scope, begin, .. } => ... }`
- Codegen: Rust struct variant, TS object with `_tag` discriminator

## Tasks

- [ ] Parser: variant declaration with `{ field: Type, ... }`
- [ ] Parser: construction expression `Variant { field: value, ... }`
- [ ] Parser: destructuring in match arms `Variant { field, .. }`
- [ ] AST: extend `VariantDef` / `Expr::VariantConstruct`
- [ ] Checker: type-check named fields, report missing/extra/duplicate
- [ ] Emit Rust: struct variant declaration + construction + destructuring
- [ ] Emit TS: object with `_tag` field
- [ ] Tests
