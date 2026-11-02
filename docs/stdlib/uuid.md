# uuid

RFC 9562 identifiers: the random version 4, the time-ordered version 7, the
8-4-4-4-12 text form and the 16 raw bytes. `import uuid`.

A `uuid.Uuid` is its 16 bytes. It is an opaque type: only this module builds
one, so every `Uuid` holds exactly 16 bytes. Making one is an `effect fn`
(`v4` reads randomness, `v7` the clock too); converting and comparing are
plain `fn`s.

`v4()` and `v7()` return a plain `Uuid`, but as `effect fn`s their calls yield
a `Result`: write `uuid.v4()!`.

**Targets.** Every function runs on native and on wasm. On wasm, `v7` keeps
its last stamp in the module instance, so each `Uuid` it makes is greater than
the one before it in the same run, as on native.

## Making one

### `uuid.v4() -> Uuid`

A random (version 4) `Uuid`: 122 random bits from the OS's cryptographic
random source.

```almd check
import uuid

effect fn main() -> Unit = {
  let id = uuid.v4()!
  println(uuid.to_string(id))
}
```

### `uuid.v7() -> Uuid`

A time-ordered (version 7) `Uuid`: the Unix time in milliseconds, a counter,
then random bits. Within one process each `v7` is greater than the one before,
even two made in the same millisecond, so sorting them with `compare` puts them
in the order they were made — a good key for rows inserted over time. Native
only for now.

```almd check
import uuid

effect fn main() -> Unit = {
  let a = uuid.v7()!
  let b = uuid.v7()!
  println(if uuid.compare(a, b) < 0 then "ordered" else "not ordered")
}
```

## Text and bytes

### `uuid.parse(s: String) -> Result[Uuid, String]`

The `Uuid` that `s` spells: 8-4-4-4-12 hex digits in either case, optionally
inside `{...}` or after `urn:uuid:`. Anything else is an `err`
(`uuid.parse: not an 8-4-4-4-12 UUID: "..."`).

### `uuid.to_string(u: Uuid) -> String`

`u` as 8-4-4-4-12 lowercase hex digits.

### `uuid.from_bytes(b: Bytes) -> Result[Uuid, String]`

The `Uuid` of exactly 16 bytes; an `err` for any other length.

### `uuid.to_bytes(u: Uuid) -> Bytes`

The 16 bytes of `u`.

```almd run
import uuid

effect fn main() -> Unit = {
  let u = uuid.parse("{550E8400-E29B-41D4-A716-446655440000}")!
  println(uuid.to_string(u))
  println("${bytes.len(uuid.to_bytes(u))} bytes")
}
```
```output
550e8400-e29b-41d4-a716-446655440000
16 bytes
```

## Ordering

### `uuid.compare(a: Uuid, b: Uuid) -> Int`

Negative, zero or positive as `a` orders before, with or after `b`, comparing
byte by byte from the first. For `v7` values this is the order they were made
in.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (7 functions)

```
// A random (version 4) Uuid: 122 bits from the OS CSPRNG.
// @since unreleased
effect uuid.v4() -> Uuid

// A time-ordered (version 7) Uuid: Unix ms, a counter, random bits; each is greater than the last.
// @since unreleased
effect uuid.v7() -> Uuid

// The Uuid s spells: 8-4-4-4-12 hex digits, either case, optionally in {…} or after urn:uuid:.
// @since unreleased
uuid.parse(s: String) -> Result[Uuid, String]

// u as 8-4-4-4-12 lowercase hex digits.
// @since unreleased
uuid.to_string(u: Uuid) -> String

// The Uuid of exactly 16 bytes b; err for any other length.
// @since unreleased
uuid.from_bytes(b: Bytes) -> Result[Uuid, String]

// The 16 bytes of u.
// @since unreleased
uuid.to_bytes(u: Uuid) -> Bytes

// Negative, zero or positive as a orders before, with or after b, byte by byte.
// @since unreleased
uuid.compare(a: Uuid, b: Uuid) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
