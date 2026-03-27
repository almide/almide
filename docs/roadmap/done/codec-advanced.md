<!-- description: Advanced codec features: structured errors, validation, schema -->
<!-- done: 2026-03-15 -->
# Codec Advanced

Codec foundation (encode/decode/Value/JSON roundtrip) is complete. Advanced features follow.

## Structured DecodeError
- `DecodeError { path: List[String], kind: DecodeErrorKind }`
- error path: `"coord.lon"` format
- Currently `err("missing field 'name'")` — string only

## json.validate[T]
- Enumerate issues without decoding
- Returns `List[DecodeIssue]`

## json.repair[T]
- Decode while repairing (Safe/Coercive)
- `RepairResult[T] = Valid(T) | Repaired(T, fixes) | Invalid(issues)`

## json.describe[T] — JSON Schema
- JSON Schema Draft 2020-12 compatible
- Usable for LLM function calling

## Variant decode (Tagged)
- Variant encode is already implemented
- decode: Determine variant name from Object key and decode payload

## json.decode[T](text) convenience
- `json.decode[Person](text)` → `json.parse(text)? |> Person.decode`
- Requires checker type argument resolution

## Codec(naming_strategy) syntax sugar
- `type ApiRes: Codec(snake_case) = { ... }`
- Currently achievable manually via `value.to_snake_case(v)`
