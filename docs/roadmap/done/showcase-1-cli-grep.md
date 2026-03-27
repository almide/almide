<!-- description: Showcase: CLI grep tool using fan concurrency and regex -->
<!-- done: 2026-03-18 -->
# Showcase 1: almide-grep (CLI Tool)

**Domain:** CLI tool
**Purpose:** File search tool. Practical example of fan concurrency + effect fn + regex.

## Specification

```
almide run showcase/almide-grep.almd -- "pattern" path/
```

- Arguments: search pattern (regex) + target directory
- Recursively traverse files
- Output matching lines as `filename:line_number: content`
- Parallel file reading with `fan.map`

## Features Used

- `effect fn` (fs, io)
- `fan.map` (parallel file processing)
- `regex.find_all`
- `guard` (filtering)
- `list.flat_map`, `string.lines`, `string.contains`
- `env.args` (CLI arguments)

## Success Criteria

- [x] Works on Tier 1 (Rust)
- [x] Works on Tier 2 (TS/Deno)
- [ ] Under 50 lines
- [ ] Usage documented in README
