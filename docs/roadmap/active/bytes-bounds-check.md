<!-- description: Unify bounds checking across bytes accessors with Option/Result returns -->
# bytes: unify bounds-check semantics

## Motivation

The `bytes` API today is **inconsistent on out-of-bounds access**:

| Accessor | Out-of-bounds behaviour |
|---|---|
| `bytes.get(b, i)` | Returns `Option[Int]` — explicit `none` on miss |
| `bytes.get_or(b, i, default)` | Returns `default` |
| `bytes.read_u32_le(b, pos)` | Returns **`0`** silently |
| `bytes.read_f64_le(b, pos)` | Returns **`0.0`** silently |
| `bytes.read_*_array` | Pads with `0` / `0.0` and continues |

The single-byte accessors are safe (Option-returning); the typed readers fall back to a sentinel zero that is indistinguishable from a real value at offset `pos`. This breaks parsers that need to know whether they ran off the end (GGUF / WAV / network protocols where 0-valued fields are legitimate).

## Goal

A consistent contract:
- **Default-returning readers** stay as-is for ergonomic forward parsing where the caller checks `bytes.len(b)` once up-front.
- **Add an `_at` family** that returns `Option[T]` for safe access:
  - `bytes.read_u32_le_at(b, pos) -> Option[Int]`
  - `bytes.read_f64_le_at(b, pos) -> Option[Float]`
  - … and so on for every read variant.

Same naming convention as existing `bytes.get` (Option-returning) vs `bytes.get_or`.

## Open design questions

1. **Naming**: `_at` suffix vs separate module (`bytes.safe.read_u32_le`)? `_at` is shorter and parallel to `get`.
2. **Bulk readers**: should `read_*_le_array_at` return `Option[List[T]]` (all-or-nothing) or `List[Option[T]]`? Probably the former — it matches how `read_string_at` already errors silently.
3. **Migration path**: keep the existing readers indefinitely (zero-on-miss) or deprecate? Existing usage in `nn` relies on the lenient behaviour during file-format parsing.

## Related

- [Codegen ideal form](./codegen-ideal-form.md) — `_at` variants would slot into the same stdlib-dispatch table.

## Acceptance

- For every existing `bytes.read_<dtype>_le` and `..._be` there is a `..._at` variant returning `Option[T]`.
- Spec tests cover the in-bounds and out-of-bounds case for each pair.
- `docs/stdlib/bytes.md` documents both families and explains when to use which.
