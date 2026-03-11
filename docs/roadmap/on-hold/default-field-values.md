# Default Field Values [ON HOLD]

Support default values on record / variant record fields, so callers can omit optional fields.

## Motivation

Self-tooling pattern: `BeginEndCap` passes `content_name: ""` then checks `if != ""` at emit time. With defaults, the field becomes truly optional:

```almide
type Pat =
  | BeginEndCap {
      scope: String,
      begin: String,
      end: String,
      begin_cap: String = "",
      end_cap: String = "",
      content_name: String = "",
      patterns: List[Pat] = []
    }
```

## Depends on

- Variant Record Fields
