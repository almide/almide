# Trailing Lambda / Builder DSL [ON HOLD]

Explore Kotlin-style trailing lambda or builder patterns for structured data construction.

## Motivation

Self-tooling builds JSON manually with `json_obj`, `json_arr_inline`, and string interpolation. A builder DSL would make structured output more natural:

```almide
// Current
json_obj([("name", q(scope)), ("match", q(regex))])

// Potential builder syntax
json {
  "name": scope
  "match": regex
}
```

## Notes

- Low priority — may not be worth the language complexity
- Could be addressed by a good JSON stdlib module instead of language-level syntax
- Trailing lambda (last arg as block) is a smaller, more general feature that enables builder patterns
