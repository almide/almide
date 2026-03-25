# Config Merger

**Level**: 3 (Hard)

## Description

Implement a config file parser and merger. Config files use `key=value` format with `#` comments and blank lines.

### Parsing rules
- Lines starting with `#` are comments (ignored)
- Blank lines are ignored
- Each non-comment line must contain `=`
- The key is everything before the first `=`, the value is everything after
- Keys must be non-empty
- Duplicate keys within a single file are an error

### Error format
- Missing `=`: `"line N: missing '='"`
- Empty key: `"line N: empty key"`
- Duplicate key: `"line N: duplicate key: KEY"`

### Merge rules
- Later files override earlier files for matching keys
- Non-matching keys from both files are preserved
- Order: base keys first, then new overlay keys

## Functions

```
parse_config(content: String) -> Result[List[Pair], String]
merge_configs(base: List[Pair], overlay: List[Pair]) -> List[Pair]
serialize_config(pairs: List[Pair]) -> String
lookup(pairs: List[Pair], key: String) -> Option[String]
filter_by_prefix(pairs: List[Pair], prefix: String) -> List[Pair]
```

Where `Pair` is a key-value pair (language-specific representation).

## Test Cases

### Parsing
| Input | Expected |
|-------|----------|
| `"host=localhost\nport=8080"` | `ok([("host","localhost"), ("port","8080")])` |
| `"# comment\n\nhost=localhost"` | `ok([("host","localhost")])` |
| `""` | `ok([])` |
| `"host=ok\nbadline"` | `err("line 2: missing '='")` |
| `"=value"` | `err("line 1: empty key")` |
| `"host=a\nhost=b"` | `err("line 2: duplicate key: host")` |
| `"formula=a=b+c"` | `ok([("formula","a=b+c")])` |

### Merging
| Base | Overlay | Expected |
|------|---------|----------|
| `[("host","localhost")]` | `[("port","8080")]` | `[("host","localhost"),("port","8080")]` |
| `[("host","localhost"),("port","3000")]` | `[("port","8080")]` | `[("host","localhost"),("port","8080")]` |

### Lookup & Filter
| Pairs | Operation | Expected |
|-------|-----------|----------|
| `[("host","localhost"),("port","8080")]` | `lookup("port")` | `some("8080")` |
| `[("host","localhost")]` | `lookup("port")` | `none` |
| `[("db_host","localhost"),("db_port","5432"),("app_port","8080")]` | `filter_by_prefix("db_")` | `[("db_host","localhost"),("db_port","5432")]` |
