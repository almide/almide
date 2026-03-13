# Trailing Lambda / Builder DSL [WON'T DO]

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

## Why won't do

- **Increases language surface area** — same thing writable two ways lowers modification survival rate
- **The real problem is stdlib, not syntax** — a good `json` stdlib module solves this without new grammar
- **LLMs must learn when to use trailing lambda** — another decision point = another error source
- Almide's value is a small, predictable grammar that LLMs can master completely
