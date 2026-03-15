# Codec Advanced [ACTIVE]

Codec 基盤 (encode/decode/Value/JSON roundtrip) は完成。高度な機能。

## DecodeError 構造化
- `DecodeError { path: List[String], kind: DecodeErrorKind }`
- error path: `"coord.lon"` 形式
- 現在は `err("missing field 'name'")` — 文字列のみ

## json.validate[T]
- decode せずに問題を列挙
- `List[DecodeIssue]` を返す

## json.repair[T]
- 修復しながら decode (Safe/Coercive)
- `RepairResult[T] = Valid(T) | Repaired(T, fixes) | Invalid(issues)`

## json.describe[T] — JSON Schema
- JSON Schema Draft 2020-12 互換
- LLM function calling に使える

## Variant decode (Tagged)
- Variant encode は実装済み
- decode: Object のキーから variant name を判定し、payload を decode

## json.decode[T](text) convenience
- `json.decode[Person](text)` → `json.parse(text)? |> Person.decode`
- checker の型引数解決が必要

## Codec(naming_strategy) 構文 sugar
- `type ApiRes: Codec(snake_case) = { ... }`
- 現在は `value.to_snake_case(v)` で手動対応可能
