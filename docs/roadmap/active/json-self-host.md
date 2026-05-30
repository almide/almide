# JSON in Almide — self-hosting the codec (道A)

Status: **foundation landed** (Almide-native JSON proven on v2 WASM + Rust);
production cut-over **scoped, pending review**.

## Goal

Reimplement the `json` (and `value`) stdlib in **Almide source** so it compiles
natively to both targets — closing the last v2 WASM coverage gap (`json_value`)
and turning the codec into a self-hosting artifact. Rust-target performance is
reclaimed where it matters via `@inline_rust`.

## Current state (before this work)

- `stdlib/json.almd` = 29 `@intrinsic` declarations (`= _`) forwarding to the
  native Rust runtime `runtime/rs/src/json.rs` (335 lines).
- `stdlib/value.almd` = 20 `@intrinsic` accessors over the runtime `Value`.
- `Value` is a **built-in Rust enum** (`runtime/rs/src/value.rs`):
  `Null | Bool | Int | Float | Str | Array(Vec<Value>) | Object(Vec<(String,Value)>)`.
- On **WASM**: these become `RuntimeCall` symbols. Legacy emits them (json runs
  on legacy WASM). **v2 cannot lower them** → every json program falls back to
  legacy under `ALMIDE_WASM_V2=1`. `json_value` is the last wasm_cross holdout.

Coupling is **contained**: only `json.almd` + `value.almd` reference `Value`.
`http` and the rest do not, so migrating these two modules is self-contained.

## The pivot: `Value` representation

`Value` must become an **Almide ADT** so Almide code can pattern-match it and the
v2 engine can lay it out (the engine already supports recursive variant ADTs,
proven by `variants_adt` and the self-host parser):

```almide
type Value =
  | VNull
  | VBool(Bool)
  | VInt(Int)
  | VFloat(Float)
  | VStr(String)
  | VArr(List[Value])
  | VObj(List[(String, Value)])
```

Both targets compile the same Almide source. On Rust the ADT lowers to a
generated enum; the hand-tuned native `Value`/json runtime is kept only behind
`@inline_rust` for hot paths (parse/stringify), added after profiling — not for
correctness.

## What landed (foundation, proven)

`spec/wasm_cross/json_almide.almd` — a complete JSON subsystem in Almide:
- `parse: String -> Result[Value, String]` — null/bool/int/string (with `\" \\ \n
  \r \t \/` escapes)/array/object, whitespace-skipping recursive descent.
- `stringify: Value -> String` — inverse, with string escaping matching the
  legacy `almide_rt_value_stringify` format (`[a,b]`, `{"k":v}`, no spaces).
- accessors: `vget`, `as_int`, `as_str`, `as_bool`, `as_array`, `keys`.
- `fn main` exercises all of it and prints results, so `scripts/wasm-v2-diff.sh`
  builds it under v2 **and** legacy and compares — proving v2 lowers it
  correctly and byte-matches legacy. Runs on the Rust target too.

This is **self-hosting milestone 2**: a real JSON parser+serializer (objects,
strings, escapes) written in Almide, running on the v2 engine.

## Production cut-over (next, needs review)

Swapping the production `stdlib/json.almd` + `stdlib/value.almd` from `@intrinsic`
forwarding to the Almide `Value` ADT + Almide bodies. Verifiable but higher risk
(must keep the json/value spec tests green on **both** targets — exact float
formatting, escape coverage, error-message strings, key order). Plan:

1. Land the Almide `Value` ADT + parse/stringify/accessors in `stdlib/value.almd`
   + `stdlib/json.almd` behind the same public signatures.
2. Add `@inline_rust(...)` to `parse`/`stringify` so the Rust target keeps the
   native runtime for perf; WASM uses the Almide body.
3. Gate on the full json/value spec suite (Rust) + the differential gate (WASM).
   Any semantic mismatch (float `{}` formatting, `\uXXXX`, error text) is a
   blocker — match legacy exactly or adjust the tests deliberately.
4. Remaining native-only pieces (JsonPath/set_path, encode/decode list/option
   helpers, typed `get_string`/`get_int`) migrate or stay `@inline_rust`.

The cut-over is deferred to a reviewed step because it changes a shipped stdlib
type (`Value`) and its regression surface spans both targets — exactly the kind
of change the Guarantees Charter says to land behind explicit verification.
