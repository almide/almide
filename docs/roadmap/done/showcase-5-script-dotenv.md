<!-- description: Showcase: dotenv file loader and missing-key checker -->
<!-- done: 2026-03-18 -->
# Showcase 5: dotenv Loader (Script)

**Domain:** Script / configuration management
**Purpose:** .env file loading + environment variable checking. Practical example of option + guard.

## Specification

```
almide run showcase/dotenv-check.almd -- .env .env.example
```

- Parse `.env` file into a key=value Map
- Compare with `.env.example` and report missing keys
- Skip comments (`#`) and blank lines
- Early return with `guard`

## Features Used

- `fs.read_text`, `string.lines`, `string.split`
- `map.set`, `map.contains`, `map.keys`
- `option.unwrap_or`, `option.is_none`
- `guard ... else`
- `list.filter`, `list.each`
- `string.trim`, `string.starts_with`

## Success Criteria

- [ ] Works on Tier 1 (Rust)
- [ ] Works on Tier 2 (TS/Deno)
- [ ] Under 40 lines
- [ ] Usage documented in README
