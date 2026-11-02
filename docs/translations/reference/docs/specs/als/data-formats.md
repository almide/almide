<a id="als--データ形式data-formats"></a>

# ALS — Data Formats

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](data-formats.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](data-formats.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](data-formats.zh-CN.md)

> Authoritative source: [docs/specs/als/data-formats.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/data-formats.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader snapshot, not a replacement for the original ALS or checker/ledger/generator input; it is not automatically synchronized. Requirements, historical status, evidence and safety claims keep the source scope. No independent execution, measurement, certification or proof was performed for this edition. The source-language edition is a reader copy; other languages are translations.

> Historical spelling note: `bytes.read_f16_le` and `json.stringify_pretty` are preserved from the source requirements, not asserted here as current API names. Check the pinned/current API reference separately; no rule or identifier is silently rewritten.


> Last updated: 2026-10-26

Observable requirements for JSON / regular expressions / binary formats. References follow the same method as the beginning of [strings.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md).

<a id="als-d1-json-パス操作"></a>

## ALS-D1 JSON path operations

The edge cases of `value.get_path`, `set_path`, and `remove_path` (missing keys, array bounds, type-mismatched nodes, empty paths) are observationally equivalent to the infallible native oracle (a reference implementation on serde_json): missing values fall back to none / no-op, without trapping.
Contracts: C-031.

<a id="als-d2-value-の-json-テキスト表現"></a>

## ALS-D2 JSON text representation of Value

Stringifying a dynamic `Value` is byte-identical to its **JSON text**, including key order, number representation, and escaping. It has the same form both on its own and as a field of a Repr record.
Contracts: C-060.

<a id="als-d3-異種ネスト文書の走査"></a>

## ALS-D3 Traversing heterogeneous nested documents

Parsing heterogeneous nested JSON (glTF-class: arrays of arrays, mixed types, deep nesting) and traversing it element by element (`as_array` / indexing / fields) is byte-identical on both targets.
Contracts: C-063.

<a id="als-d4-正規表現エンジン"></a>

## ALS-D4 Regular-expression engine

The regular-expression engine (match / find / replace / captures) is byte-identical to the native engine **throughout the fuzzed grammar**. Dialect differences (such as PCRE versus RE2 disagreements) are nonconforming: the supported syntax has a single normative grammar.
Contracts: C-032.

<a id="als-d5-半精度浮動小数のデコード"></a>

## ALS-D5 Decoding half-precision floating point

`bytes.read_f16_le` exactly widens IEEE-754 binary16 to f64, including subnormals, ±inf, NaN, and ±0.
Contracts: C-037.

<a id="als-d6-codec-と-json-デコード"></a>

## ALS-D6 Codec and JSON decoding

Decoding JSON numbers and `\u` escapes, widening integer-form numbers to f64, indentation output from `json.stringify_pretty`, cross-module dispatch of derived Codec, the dynamic Value model (merge / array roundtrips), and the wording of decode error messages are byte-identical across targets.

`Codec` is the composition of `Encode` and `Decode` (C-435). A type `T` declaring `: Decode` derives only `T.decode` and `T.decode_strict`; a type declaring `: Encode` derives only `T.encode`; `: Codec` derives both. A `: Codec` type conforms to both `Encode` and `Decode` (satisfying a `[T: Decode]` bound). Referencing a method in an undeclared direction is rejected with E002. A `: Decode` type that is not `Encode` is called an input type, and all its fields are input. Adding `@skip` / `@skip_decode` to a field of an input type is rejected with E053.
A type `T` declaring `: Codec` has static methods `T.encode` / `T.decode`.
Record encoding proceeds in declaration order and omits `none` fields (it does not emit explicit null; only `some(null)` in `Value?` becomes explicit null). Fields with defaults emit their actual value unchanged, and `Value` fields pass through unchanged.
Decoding walks fields in declaration order and returns the first error: `expected Object` for a non-object, `missing field '<key>'` for a missing required key, and, for type mismatches, `expected Int` / `expected Float` / `expected Str` / `expected Bool` using Value variant names (C-084's vocabulary). Unknown keys are ignored.
Derived `T.decode_strict` (record and variant) walks the value before decoding and returns `unknown key 'k'` for the first key `k` outside the type at any depth. Nested keys include the record's path, in the form `, at address.geo` / `, at tags[1]`.
For a record, it first examines its own keys (in document order, looking for keys that are not wire keys of decoded fields), then walks fields of `Decode` types in declaration order. For a variant, the first key is the tag and subsequent keys are unknown. It walks the tag's case payload under the case-name path: as a record for record payloads, or with `[i]` on each element for tuple payloads. If nothing is found, it returns the same result as `T.decode` (C-429).
An `@skip` field is neither decoded nor encoded; an `@skip_decode` field is not decoded but is encoded. Both take the default, and their key is treated as unknown. A skipped field without a default is rejected with E104 (C-428, `spec/wasm_cross/codec_field_skip_matrix.almd`).
A `Secret[A]` field is decoded from the wire representation of `A` and wrapped. A type containing Secret does not derive `T.encode`; a reference to `T.encode` is rejected with E023 (C-427, `spec/wasm_cross/secret_eq_and_decode.almd`). An Option field folds both absence and explicit null into `none`, but `Value?` has three states: absence → `none`, explicit null → `some(null)`, value → `some(v)`. A field with a default takes that default when absent. A `Float` field accepts integer-form JSON numbers by widening them to f64 (C-085); `List[U]` decodes arrays element by element and surfaces the first err unchanged, without an index qualifier. Nested Codec records recurse; a non-object in record position yields `expected Object`. Variant Codec is externally tagged (`{"Case": [payload…]}`, or `{"Case": null}` for a unit case); an unknown tag returns `unknown variant for <T>`.
The behavior matrix is pinned by C-298's corpus: `spec/wasm_cross/codec_decode_errors.almd`, `spec/wasm_cross/codec_none_omission.almd`, `spec/wasm_cross/codec_variant_roundtrip.almd`, `spec/wasm_cross/codec_extra_keys.almd`, and `spec/wasm_cross/codec_float_int.almd`.
Contracts: C-084, C-085, C-087, C-095, C-098, C-103, C-209, C-427, C-428, C-429, C-435.

<a id="als-d7-バイト列ブリッジ"></a>

## ALS-D7 Byte-sequence bridge

Byte movement through RawPtr / linear memory copies byte values unchanged. `bytes.from_list(List[Int])` copies values in the byte range (0..=255) unchanged, and **wraps out-of-range values modulo 256** (`[65, 300, -1]` → 65, 44, 255; measured on both targets in 0.59.1. There is no non-truncating requirement over the full Int range).
Contracts: C-062, C-090.
