# Default Field Values [ON HOLD]

## The Problem

Self-tooling exposed three design smells that share a single root cause:

### 1. Sentinel values as "absent"

```almide
BeginEndCap {
  scope: "string.quoted.double.almide",
  begin: "\"", end: "\"",
  begin_cap: "punctuation.definition.string.begin.almide",
  end_cap: "punctuation.definition.string.end.almide",
  content_name: "",    // empty string means "absent" — dishonest
  patterns: interp
}
```

The emit side checks `if content_name != ""` — the type says `String` but the semantics are `Option[String]`. LLMs can't distinguish the two, and will forget to pass `""` or pass it when they shouldn't.

### 2. Redundant variants for the same concept

`Pat` has 5 variants, but only 3 concepts:

| Concept | Without captures | With captures |
|---------|-----------------|---------------|
| Single match | `Match` | `MatchCap` |
| Begin/end range | `BeginEnd` | `BeginEndCap` |
| Include ref | `Include` | — |

`MatchCap` is `Match` with captures. `BeginEndCap` is `BeginEnd` with captures + content_name. The only reason they're separate variants is that there's no way to omit a field.

### 3. Verbose empty collections

```almide
BeginEnd { scope: "comment.block.almide", begin: "\\(\\*", end: "\\*\\)", patterns: [] }
//                                                                         ^^^^^^^^^^^^
// "no sub-patterns" should be the obvious default, not something you type every time
```

## Design

### Default values on variant record fields

```almide
type Pat =
  | Match {
      scope: String,
      regex: String,
      captures: List[(String, String)] = []
    }
  | BeginEnd {
      scope: String,
      begin: String,
      end: String,
      patterns: List[Pat] = [],
      begin_cap: String = "",
      end_cap: String = "",
      content_name: String = ""
    }
  | Include(String)
```

**5 variants → 3.** No information lost, no sentinel ambiguity for collections.

### What the code becomes

Construction — optional fields omitted:
```almide
// Simple match (was Match)
Match { scope: "keyword.control.almide", regex: "\\b(if|then|else)\\b" }

// Match with captures (was MatchCap)
Match {
  scope: "meta.function.almide",
  regex: "\\b(fn)\\s+([a-z_][a-zA-Z0-9_]*)",
  captures: [("1", "keyword.declaration"), ("2", "entity.name.function")]
}

// Begin/end without captures (was BeginEnd)
BeginEnd { scope: "comment.block.almide", begin: "\\(\\*", end: "\\*\\)" }

// Begin/end with captures (was BeginEndCap)
BeginEnd {
  scope: "string.quoted.double.almide",
  begin: "\"", end: "\"",
  begin_cap: "punctuation.definition.string.begin.almide",
  end_cap: "punctuation.definition.string.end.almide",
  patterns: interp
}
```

Pattern matching — `..` skips defaulted fields:
```almide
fn emit_pat(pat: Pat) -> String = match pat {
  Match { scope, regex, captures } =>
    if list.is_empty(captures) then
      json_obj([("name", q(scope)), ("match", q(regex))])
    else {
      let caps = emit_captures(captures)
      json_obj([("name", q(scope)), ("match", q(regex)), ("captures", caps)])
    }

  BeginEnd { scope, begin, end, patterns, begin_cap, end_cap, content_name } => {
    let pairs = [("name", q(scope)), ("begin", q(begin)), ("end", q(end))]
    let pairs = if begin_cap != "" then pairs ++ [("beginCaptures", emit_cap_obj(begin_cap))] else pairs
    let pairs = if end_cap != "" then pairs ++ [("endCaptures", emit_cap_obj(end_cap))] else pairs
    let pairs = if content_name != "" then pairs ++ [("contentName", q(content_name))] else pairs
    let pairs = pairs ++ [("patterns", json_arr_inline(list.map(patterns, fn(p) => emit_pat(p))))]
    json_obj(pairs)
  }

  Include(r) => json_obj([("include", q(r))])
}
```

### Why NOT `Option[String]` for absent fields?

```almide
// Option approach — verbose, forces match/unwrap at every use site
| BeginEnd {
    scope: String, begin: String, end: String,
    patterns: List[Pat] = [],
    begin_cap: Option[String] = none,
    end_cap: Option[String] = none,
    content_name: Option[String] = none
  }

// Emit side becomes:
let pairs = match begin_cap {
  some(cap) => pairs ++ [("beginCaptures", emit_cap_obj(cap))]
  none => pairs
}
```

This is more "correct" in type theory, but **worse for LLM accuracy**:
- LLMs must remember to wrap in `some()` and match on `some()`/`none`
- The extra verbosity increases token count with no functional benefit
- `""` as "absent" is idiomatic for string-typed config fields across all ecosystems (JSON, TOML, YAML)
- The check `if x != "" then` is simpler than `match x { some(v) => ... none => ... }`

**Rule: Use `Option[T]` when the absence is semantically meaningful (e.g., a missing user, a null result). Use `T` with default when the field is a configuration knob with a natural zero value (`""`, `[]`, `0`, `false`).**

## Semantics

- Default values are evaluated at construction time, not declaration time
- Omitted fields get the default; explicitly passed fields override it
- Pattern matching always sees all fields (defaults are filled in before matching)
- Codegen: Rust — struct literal with defaults filled in; TS — object literal with defaults filled in

## Restrictions

- Default expressions must be compile-time constants: literals, `[]`, `""`, `0`, `true`, `false`, `none`
- No function calls or variable references in defaults (keeps codegen simple and predictable)
- Fields with defaults must come after fields without defaults (positional clarity)

## Impact on LLM accuracy

| Metric | Before | After |
|--------|--------|-------|
| Variant count for TextMate Pat | 5 | 3 |
| Fields to remember for simple Match | 2 | 2 (same) |
| Fields to remember for Match with captures | 3 (+ remember MatchCap name) | 3 (same variant) |
| Fields for simple BeginEnd | 4 (+ `patterns: []`) | 3 (patterns defaulted) |
| Fields for full BeginEnd with captures | 7 (+ remember BeginEndCap name) | 6 (same variant, content_name optional) |
| Sentinel `""` as "none" | Required pattern | Still works, but opt-in |

## Depends on

- Variant Record Fields ✅ (done in v0.5.3)

## Tasks

- [ ] Parser: `field: Type = expr` in variant record and record type declarations
- [ ] Parser: allow omitting fields with defaults at construction sites
- [ ] AST: add `default: Option<Expr>` to `FieldType`
- [ ] Checker: validate default expression type matches field type
- [ ] Checker: at construction, fill in missing fields with defaults
- [ ] Checker: enforce "defaults come last" ordering
- [ ] Emit Rust: fill defaults into struct literal
- [ ] Emit TS: fill defaults into object literal
- [ ] Formatter: preserve `= default` in type declarations
- [ ] Tests: construction with/without defaults, type errors, ordering
