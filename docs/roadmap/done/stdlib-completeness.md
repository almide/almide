# Stdlib Completeness [DONE]

Fill gaps that make Almide less capable than Python/Go for everyday tasks.

### int module ✅

- [x] `int.parse(s)` → `Result[Int, String]` (parse decimal string)
- [x] `int.parse_hex(s)` → `Result[Int, String]`
- [x] `int.abs(n)` → `Int`
- [x] `int.min(a, b)` / `int.max(a, b)`

### string module ✅

- [x] `string.pad_right(s, n, ch)` → `String`
- [x] `string.trim_start(s)` / `string.trim_end(s)` → `String`
- [x] `string.count(s, sub)` → `Int`

### list module ✅

- [x] `list.index_of(xs, x)` → `Option[Int]`
- [x] `list.last(xs)` → `Option[T]`
- [x] `list.chunk(xs, n)` → `List[List[T]]`
- [x] `list.sum(xs)` / `list.product(xs)` → `Int`

### Stdlib Phase 5: HIGH priority gaps ✅

Functions that every mainstream language has and AI-generated code will expect.

#### string

- [x] `string.is_empty?(s)` → `Bool`
- [x] `string.reverse(s)` → `String`
- [x] `string.strip_prefix(s, prefix)` → `Option[String]` — remove prefix if present
- [x] `string.strip_suffix(s, suffix)` → `Option[String]` — remove suffix if present

#### list

- [x] `list.first(xs)` → `Option[T]` — alias-like for `list.get(xs, 0)`
- [x] `list.is_empty?(xs)` → `Bool`
- [x] `list.flat_map(xs, f)` → `List[U]` — map then flatten
- [x] `list.min(xs)` → `Option[T]` — minimum element
- [x] `list.max(xs)` → `Option[T]` — maximum element
- [x] `list.join(xs, sep)` → `String` — join `List[String]` with separator (UFCS: `xs.join(",")`)

#### map

- [x] `map.merge(a, b)` → `Map[K, V]` — merge two maps (b wins on conflict)
- [x] `map.is_empty?(m)` → `Bool`

#### fs

- [x] `fs.is_dir?(path)` → `Bool` (effect)
- [x] `fs.is_file?(path)` → `Bool` (effect)
- [x] `fs.copy(src, dst)` → `Result[Unit, IoError]` (effect)
- [x] `fs.rename(src, dst)` → `Result[Unit, IoError]` (effect)

#### process

- [x] `process.exec_status(cmd, args)` → `Result[{code: Int, stdout: String, stderr: String}, String]` (effect) — full exec result with exit code

### Stdlib Phase 6: MEDIUM priority gaps ✅

#### string
- [x] `string.replace_first(s, from, to)` → `String`
- [x] `string.last_index_of(s, needle)` → `Option[Int]`
- [x] `string.to_float(s)` → `Result[Float, String]`

#### list
- [x] `list.filter_map(xs, f)` → `List[U]`
- [x] `list.take_while(xs, f)` → `List[T]`
- [x] `list.drop_while(xs, f)` → `List[T]`
- [x] `list.count(xs, f)` → `Int`
- [x] `list.partition(xs, f)` → `(List[T], List[T])`
- [x] `list.reduce(xs, f)` → `Option[T]`
- [x] `list.group_by(xs, f)` → `Map[K, List[T]]`

#### map
- [x] `map.map_values(m, f)` → `Map[K, V2]`
- [x] `map.filter(m, f)` → `Map[K, V]`
- [x] `map.from_entries(entries)` → `Map[K, V]`

#### int / float
- [x] `int.clamp(n, lo, hi)` → `Int`
- [x] `float.min(a, b)` / `float.max(a, b)` → `Float`
- [x] `float.clamp(n, lo, hi)` → `Float`

#### json
- [x] `json.get_float(j, key)` → `Option[Float]`
- [x] `json.from_float(n)` → `Json`
- [x] `json.stringify_pretty(j)` → `String`

### Stdlib Phase 7: remaining gaps (future)

#### path
- `path.stem`, `path.normalize`, `path.resolve`

#### fs
- `fs.walk`, `fs.stat`

#### New modules (future)
- **encoding**: ✅ `base64_encode`, `base64_decode`, `hex_encode`, `hex_decode`, `url_encode`, `url_decode`
- **set**: `Set[T]` API — `new`, `from_list`, `add`, `remove`, `contains`, `union`, `intersection`, `difference`, `len`, `to_list`, `is_empty?`
- **csv**: planned as external package (`almide/csv`) — `parse`, `parse_with_header`, `stringify`

### CLI improvements

- [x] `almide --help`: detailed help with all options and examples
- [ ] `almide check`: show progress for multi-file projects
- [ ] Exit codes: distinguish parse error (65), type error (66), codegen error (70)

---
