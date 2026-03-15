# Codec Test Specification [ACTIVE]

Swift Codable / Serde / Kotlin serialization / Jackson の知見に基づくテストケース集。
実装前に仕様を明文化するためのもの。

## 0. Almide の設計判断 (既決)

| 判断 | 決定 | 根拠 |
|------|------|------|
| missing vs null | **区別しない** (merge) | Option[T] で missing も null も none |
| default on missing | **Yes** | field default があれば missing 時に適用 |
| default on null | **Yes** | null も missing と同様に default で埋める |
| unknown fields | **Ignore** (default) | strict reject は JsonOptions で opt-in |
| alias | **decode only** | encode は alias (or field name) を使用 |
| coercion | **No** (strict) | repair API で opt-in |
| compile-time type 優先 | **Yes** | Almide に runtime polymorphism なし |
| polymorphism | **Tagged 必須** | Variant の Tagged 形式 |
| canonical encode | **field declaration order** | 安定 |
| encode/decode 非対称 | **許容** | manual codec で片方だけ書ける |

## 1. P0: 最初に通すべきテスト (20件)

### Presence / Null / Default (12件)

```
P0-001: required field missing → err("missing field 'id'")
P0-002: required field null → err("expected Int but got Null")
P0-003: optional field missing → none
P0-004: optional field null → none
P0-005: optional field present → some(value)
P0-006: default field missing → default value applied
P0-007: default field null → default value applied
P0-008: default field present → value overrides default
P0-009: type mismatch: id: "1" → err (no coercion)
P0-010: nested required missing → err with field path
P0-011: array element required missing → err with index
P0-012: all fields present → roundtrip success
```

### Unknown / Alias (4件)

```
P0-013: unknown field ignored (default mode)
P0-014: alias decode: type_ as "type" → "type" key accepted
P0-015: alias encode: type_ as "type" → "type" key output
P0-016: unknown field rejected (strict mode, future)
```

### Roundtrip (4件)

```
P0-017: flat record encode-decode roundtrip
P0-018: nested record encode-decode roundtrip
P0-019: variant encode-decode roundtrip (Tagged)
P0-020: encode then stringify then parse then decode = original
```

## 2. P1: 次に通すべきテスト (20件)

### Enum / Variant (6件)

```
P1-001: unit variant encode → {"Red": null}
P1-002: tuple variant encode → {"Circle": [3.0]}
P1-003: record variant encode → {"Rect": {"w": 1.0, "h": 2.0}}
P1-004: unknown variant name → err
P1-005: variant payload mismatch → err
P1-006: variant discriminator missing → err
```

### List (4件)

```
P1-007: empty list decode → []
P1-008: list of primitives roundtrip
P1-009: list of Codec types roundtrip (List[Weather])
P1-010: list element type mismatch → err with index
```

### Error Quality (6件)

```
P1-011: missing field error includes field name
P1-012: type mismatch error includes expected/got types
P1-013: nested error includes full path ("coord.lon")
P1-014: array element error includes index ("weather[2].id")
P1-015: alias error uses wire key name, not field name
P1-016: multiple errors collected (validate mode, future)
```

### Schema Evolution (4件)

```
P1-017: old payload (missing new optional field) → decode success
P1-018: new payload (extra unknown field) → decode success (ignore mode)
P1-019: field renamed with alias → old key still accepted
P1-020: new required field added → old payload fails clearly
```

## 3. P2: 高度なテスト (15件)

### Coercion (repair mode)

```
P2-001: "42" → 42 in repair(Coercive) mode
P2-002: single value → [value] in repair(Coercive) mode
P2-003: 1.5 → 1 rejected (lossy, even in Coercive)
P2-004: repair reports fixes applied
```

### Number Edge Cases

```
P2-005: i64 overflow → err
P2-006: NaN/Infinity → err (JSON spec violation)
P2-007: 1.0 on Int field → err (strict)
P2-008: 1 on Float field → ok (Int → Float promotion)
```

### Map / Dictionary

```
P2-009: Map[String, Int] roundtrip
P2-010: empty Map roundtrip
P2-011: duplicate key → last wins or err (decide)
```

### Performance Boundaries

```
P2-012: 100-level nesting → no stack overflow
P2-013: 10000-element array → reasonable time
P2-014: many unknown fields in strict mode → all reported
P2-015: large string field → no truncation
```

## 4. 未決の判断ポイント

Almide で明文化が必要な決定:

| 問題 | 選択肢 | 推奨 |
|------|--------|------|
| duplicate key | last wins / err | **err** (strict default) |
| null on non-Optional, no default | err / ignore | **err** |
| encode Option none | omit field / emit null | **omit** (codec-and-json.md 決定済み) |
| encode default value | always / omit if == default | **always** (simplicity) |
| error path format | `coord.lon` / `["coord", "lon"]` | **dot notation** |
| unknown enum case preserve | preserve / err | **err** |
| generic T: Codec constraint | compile-time check | **auto-derive 生成時に検証** (実装済み) |
| flatten / inline | support / don't | **don't** (Canonicity — 1つの構造に1つの表現) |

## 5. Almide で Not Applicable

Swift/Serde の概念で Almide に該当しないもの:

- **class 継承の encode/decode** — Almide に class なし
- **property wrapper** — Almide に wrapper pattern なし
- **existential / any Codable** — Almide に existential なし
- **flatten** — Canonicity 違反、サポートしない
- **CodingKeys enum** — field alias で代替
- **runtime type dispatch** — Almide は compile-time only
- **lazy decode / streaming** — 将来検討、今は eager

## 6. テストのフォーマット

```almide
test "P0-001: required field missing" {
  let input = r'{"name": "Alice"}'  // id missing
  let result = json.parse(input) |> Person.decode
  match result {
    err(e) => assert(string.contains(e, "missing field 'id'"))
    ok(_) => assert(false)
  }
}
```

## 関連

- [Codec Implementation Plan](codec-implementation.md) — 実装設計
- [Codec Protocol & JSON](codec-and-json.md) — 元の設計仕様
