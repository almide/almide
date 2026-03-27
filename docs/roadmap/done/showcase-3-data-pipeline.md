<!-- description: Showcase: CSV-to-JSON data pipeline with list HOFs and pipes -->
<!-- done: 2026-03-18 -->
# Showcase 3: CSV to JSON Pipeline (Data Processing)

**Domain:** Data processing
**Purpose:** CSV read -> transform -> JSON output. Practical example of list higher-order functions + pipe.

## Specification

```
almide run showcase/csv-to-json.almd -- input.csv > output.json
```

- CSV parsing (header row + data rows)
- Filter/aggregate/transform via pipe chains
- JSON output

## Features Used

- `string.split`, `string.trim`, `string.lines`
- `list.map`, `list.filter`, `list.fold`, `list.group_by`
- `|>` pipe chain
- `int.parse`, `float.parse`
- `json.stringify_pretty`
- `map.from_list`

## Success Criteria

- [ ] Works on Tier 1 (Rust)
- [ ] Works on Tier 2 (TS/Deno)
- [ ] Under 40 lines
- [ ] Usage documented in README
