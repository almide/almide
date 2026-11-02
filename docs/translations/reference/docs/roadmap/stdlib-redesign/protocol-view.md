> Description (fixed history): Stdlib redesign — the protocol pyramid: the standard library's protocols layered after Swift, each type's conformances, the holes they expose and the decisions they force (R-L0-37).
<a id="標準ライブラリの-protocol-の階層"></a>

# Standard-library protocol hierarchy

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](protocol-view.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](protocol-view.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](protocol-view.zh-CN.md)

> Non-normative historical design snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh mechanical regeneration, Dojo execution or independent implementation/performance verification. Counts, zero gaps, conformances and @reuse speed claims belong to the original intended design, not proof that all protocols/defaults/conditional conformances currently exist. Preserve explicitly landed built-ins versus the remaining design list and tuple/UFCS limits. Earlier Ord numeric defaults in the language-feature example coexist with the main numeric-requirement allocation; source counts/type lists are not silently reconciled. Source diagrams and API inventories remain byte-faithful. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)

> Last updated: 2026-10-22
> Original records: [R-L0-37](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md)

Per R-L0-37, express standard concrete types as “combinations of protocols they conform to” plus “type-specific functions”. The hierarchy takes Swift's standard library as its model; layers absent in Swift are added with reasons. Conformances and gaps are mechanically derived from [target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md) (post-redesign functions).

This hierarchy is an **intended design** (R-L0-37's “positioning”). Implementation currently must follow the listed names and shapes; internally, concrete per-type functions are acceptable. As protocol features enter the language (inheritance, associated types, default implementations, conditional conformance; #4000), internals move toward this hierarchy. Receiverless requirement calls (`M.empty()`) have landed. `Numeric`, `Integer` and `Text` are implemented as built-in protocols, used by `from`, `list.sum`, `list.join` and `BigInt` operators.

- Of 1,089 functions across 37 value types, 796 (73%) are protocol requirements. The remaining 293 are type-specific.
- Given the conformances, 0 required functions are still absent (gaps). Requirements supplied by default implementations are not counted.
- The Swift protocols used as models are listed in the “Swift” column.

<a id="階層"></a>

## Hierarchy

Trees by family (foundations, numbers, containers, I/O, domains). A lower protocol includes an upper one (inheritance). Inclusion from other families appears after “＋”.

Foundations:

```
Eq
├─ Hash
└─ Ord
Repr
└─ Parse
Codec
```

Numbers:

```
Additive  ＋ Eq
├─ Numeric
│  ├─ Signed
│  │  └─ FloatingPoint  ＋ Ord・Parse
│  │     └─ BinaryFloatingPoint
│  └─ Integer  ＋ Ord・Hash・Parse
│     ├─ FixedWidthInteger
│     ├─ SignedInteger  ＋ Signed
│     └─ UnsignedInteger
└─ TimeAmount  ＋ Ord
```

Containers:

```
Sized
├─ Container
│  ├─ Membership
│  │  └─ SetAlgebra
│  ├─ Sequence
│  │  └─ Ordered
│  │     ├─ Indexed
│  │     └─ Text
│  ├─ Growable
│  └─ PriorityQueue
└─ KeyValue
```

I/O:

```
Read
Write
Close
```

Domains:

```
HttpMessage
```

<a id="protocol"></a>

## Protocols

| Layer | protocol | Includes | Requirements (functions) | Default implementation | Swift | Conforming types |
|---|---|---|---|---|---|---|
| Foundation | `Eq` | — | — | — | Equatable | Bool・String・Bytes・List[A]・Set[A]・BigInt・Float・Float32・Float64・Duration・Compute・Uuid・Url・Option[A]・Result[A, E]・Value・Matrix・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Foundation | `Hash` | `Eq` | — | — | Hashable | Bool・String・Bytes・List[A]・Set[A]・BigInt・Duration・Compute・Uuid・Url・Option[A]・Result[A, E]・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Foundation | `Repr` | — | `to_string` | `to_string` | CustomStringConvertible | Bool・String・BigInt・Float・Float32・Float64・Uuid・Url・Value・Matrix・Regex・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Foundation | `Parse` | `Repr` | `parse` | — | LosslessStringConvertible | Bool・BigInt・Float・Float32・Float64・Uuid・Url・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Foundation | `Ord` | `Eq` | `compare` | — | Comparable | Bool・String・Bytes・BigInt・Float・Float32・Float64・Duration・Compute・Uuid・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Foundation | `Codec` | — | — | — | Codable | Bool・String・Bytes・List[A]・Deque[A]・Set[A]・Map[K, V]・BigInt・Float・Float32・Float64・Duration・Uuid・Url・Option[A]・Result[A, E]・Value・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Number | `Additive` | `Eq` | — | — | AdditiveArithmetic | BigInt・Float・Float32・Float64・Duration・Compute・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Number | `Numeric` | `Additive` | `from`・`from_checked` | — | Numeric | BigInt・Float・Float32・Float64・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Number | `Signed` | `Numeric` | `abs` | — | SignedNumeric | BigInt・Float・Float32・Float64・Int・Int8・Int16・Int32・Int64 |
| Number | `Integer` | `Numeric`・`Ord`・`Hash`・`Parse` | `min`・`max`・`clamp`・`floor_div`・`modulo`・`pow`・`is_even`・`is_odd`・`parse_radix`・`to_string_radix`・`bit_length`・`band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr` | `is_even`・`is_odd` | BinaryInteger | BigInt・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Number | `FixedWidthInteger` | `Integer` | `min_value`・`max_value`・`count_leading_zeros`・`count_trailing_zeros`・`count_ones`・`byte_swap`・`wrap_add`・`wrap_sub`・`wrap_mul`・`rotate_left`・`rotate_right`・`from_saturating`・`from_wrapping` | — | FixedWidthInteger | Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| Number | `SignedInteger` | `Integer`・`Signed` | `sign` | — | SignedInteger | BigInt・Int・Int8・Int16・Int32・Int64 |
| Number | `UnsignedInteger` | `Integer` | — | — | UnsignedInteger | UInt8・UInt16・UInt32・UInt64 |
| Number | `FloatingPoint` | `Signed`・`Ord`・`Parse` | `min`・`max`・`clamp`・`nan`・`infinity`・`epsilon`・`min_value`・`max_value`・`is_nan`・`is_finite`・`is_infinite`・`floor`・`ceil`・`round`・`trunc`・`signum` | — | FloatingPoint | Float・Float32・Float64 |
| Number | `BinaryFloatingPoint` | `FloatingPoint` | `to_bits`・`from_bits` | — | BinaryFloatingPoint | Float・Float32・Float64 |
| Number | `TimeAmount` | `Additive`・`Ord` | `ns`・`us`・`ms`・`s`・`min`・`h` | — | DurationProtocol | Duration・Compute |
| Container | `Sized` | — | `len`・`is_empty` | `is_empty` | Collection（count・isEmpty） | String・Bytes・List[A]・Deque[A]・Set[A]・Heap[A]・Map[K, V] |
| Container | `Container` | `Sized` | `new`・`from_list`・`to_list`・`any`・`all`・`count`・`filter`・`clear` | `any`・`all`・`count` | — | String・Bytes・List[A]・Deque[A]・Set[A]・Heap[A]・Map[K, V] |
| Container | `Membership` | `Container` | `contains` | — | Sequence.contains（Element: Equatable） | List[A]・Deque[A]・Set[A]・Heap[A] |
| Container | `Sequence` | `Container` | `fold`・`find`・`enumerate`・`zip`・`zip_with`・`filter_map`・`find_map`・`reduce`・`scan`・`partition`・`group_by`・`sort`・`sort_by`・`sort_with`・`min`・`max`・`min_by`・`max_by`・`min_with`・`max_with`・`sum`・`product` | `find`・`enumerate`・`zip`・`zip_with`・`filter_map`・`find_map`・`reduce`・`scan`・`partition`・`group_by`・`sort`・`sort_by`・`sort_with`・`min`・`max`・`min_by`・`max_by`・`min_with`・`max_with`・`sum`・`product` | Sequence | String・Bytes・List[A]・Deque[A]・Set[A]・Map[K, V] |
| Container | `Ordered` | `Sequence` | `first`・`last`・`reverse`・`take`・`drop`・`take_end`・`drop_end`・`slice`・`find_index`・`take_while`・`drop_while`・`chunk`・`window`・`split_at`・`dedup`・`intersperse`・`unique`・`unique_by` | `find_index`・`take_while`・`drop_while`・`chunk`・`window`・`split_at`・`dedup`・`intersperse`・`unique`・`unique_by` | BidirectionalCollection | String・Bytes・List[A]・Deque[A] |
| Container | `Indexed` | `Ordered` | `get`・`set` | — | RandomAccessCollection・MutableCollection | Bytes・List[A]・Deque[A] |
| Container | `Growable` | `Container` | `push`・`pop`・`insert`・`remove_at` | — | RangeReplaceableCollection | String・Bytes・List[A]・Deque[A] |
| Container | `SetAlgebra` | `Membership` | `insert`・`remove`・`union`・`intersection`・`difference`・`symmetric_difference`・`is_subset`・`is_disjoint` | — | SetAlgebra | Set[A] |
| Container | `KeyValue` | `Sized` | `new`・`get`・`set`・`remove`・`contains`・`keys`・`values`・`update`・`upsert`・`merge` | — | Dictionary(concrete type) | Map[K, V] |
| Container | `PriorityQueue` | `Container` | `push`・`pop`・`peek` | — | Heap（swift-collections） | Heap[A] |
| Container | `Text` | `Ordered` | `contains`・`starts_with`・`ends_with`・`index_of`・`last_index_of`・`count_matches`・`split`・`replace`・`replace_first`・`strip_prefix`・`strip_suffix`・`repeat`・`pad_start`・`pad_end`・`is_equal_constant_time` | — | StringProtocol | String・Bytes |
| I/O | `Read` | — | `read`・`read_exact`・`read_to_end` | `read_exact`・`read_to_end` | — | TcpStream・Fd・Stdin |
| I/O | `Write` | — | `write` | — | — | TcpStream・Fd・Stdout・Stderr |
| I/O | `Close` | — | `close` | — | — | TcpStream・TcpListener・UdpSocket・Fd |
| Domain | `HttpMessage` | — | `header`・`header_values`・`headers`・`body`・`body_bytes`・`set_header`・`set_body`・`set_body_bytes` | — | — | HttpRequest・HttpResponse |

Notes:

- `Eq`: `==`/`!=`. Operators, not functions. All value types except `Fn` automatically have it.
- `Hash`: can be Map keys/Set elements. Floating-point types do not have it.
- `Repr`: the contents of `"${x}"`. Function form is `<mod>.to_string`.
- `Parse`: only types satisfying `parse(to_string(x)) == ok(x)` have it.
- `Ord`: `<`, etc. Two-value selection functions `min`/`max`/`clamp` are numeric protocol requirements (`TimeAmount`'s `min` means minutes; sequence `min` means smallest element, so they are not placed in `Ord`).
- `Codec`: `encode`/`decode`. Added by derivation.
- `Additive`: `+`/`-` and 0.
- `Numeric`: `*`. `from`/`from_checked` accept conversions from any numeric type (R-L0-40; Swift `init(_:)`/`init(exactly:)`).
- `Signed`: unary `-`.
- `Integer`: `/`/`%`. Bit operations are functions (`band`, etc., R-int-10); removed symbols are not reused (R-L0-43). `bigint` belongs here too (Swift `BinaryInteger` is designed to include arbitrary-precision integers). Numeric conversions (`to_<T>`, `_checked`, `_saturating`, `_wrapping`) also belong here (Swift `init(_:)`, `init(exactly:)`, `init(clamping:)`, `init(truncatingIfNeeded:)`).
- `FixedWidthInteger`: fixed-width integers. `wrap_*`/`rotate_*` use the type width (Swift `&+`, `&-`, `&*`). `from_saturating`/`from_wrapping` correspond to Swift `init(clamping:)`/`init(truncatingIfNeeded:)`. `count_trailing_zeros` returns the type width for 0, so it is not placed in widthless `bigint`.
- `FloatingPoint`: `/`. Ordering is IEEE totalOrder (R-L0-16).
- `TimeAmount`: `Duration`/`Compute` (R-duration-03).
- `Sized`: default `is_empty` implementation from `len`.
- `Container`: queries that do not rely on order. `filter` returns the same type (Swift `RangeReplaceableCollection.filter`). A layer absent in Swift, provided for containers such as `heap` that do not expose their internal order (R-heap-05).
- `Membership`: whether an element is present, only when elements are `Eq`.
- `Sequence`: traversable one by one in a fixed order. Sequence algorithms are written once as defaults from `fold` and work for any sequence (like Swift `Sequence` extensions). Results changing element types return `List`. `sum`/`product` require `Numeric` elements; `sort`/`min`/`max` require `Ord` elements (R-L0-42).
- `Ordered`: countable from both ends. Slicing functions (`take_while`, `chunk`, `split_at`, etc.) return the same type (Swift `SubSequence`).
- `Indexed`: positional reading/writing.
- `Growable`: can add/remove. `push`/`pop`/`clear` mutate in place (`mut`); `insert`/`remove_at` return copies. The verb determines the style, consistently for every type (R-L0-04; copies become as fast as in-place operations through `@reuse`, so mutation is restricted to buffer-growing/shrinking verbs). Swift makes all these `mutating`, but Almide takes a functional approach with value semantics.
- `KeyValue`: `contains` takes a key.
- `PriorityQueue`: Swift `Heap` is not a `Sequence` either.
- `Text`: search by subsequence. Its `contains` accepts a subsequence, so it cannot coexist with `Membership` on the same type.
- `Read`: R-L0-44. Place generic functions once in `io` (`io.read[H: Read]`). `read` returns at most `max` bytes, empty if the peer closes.
- `Write`: R-L0-44. Writes everything (Rust `write_all`).
- `Close`: R-L0-44. A second call is also ok.
- `HttpMessage`: R-http-12.

<a id="型ごとの準拠と穴"></a>

## Per-type conformances and gaps

“Conformance” means declared protocols (included protocols omitted). “Gaps” are required functions still absent; parentheses identify the protocol requiring them.

| Type | Conformance | Functions | Type-specific | Gaps |
|---|---|---|---|---|
| `Bool` | Parse・Ord・Hash・Codec | 3 | 0 | None |
| `String` | Text・Growable・Ord・Repr・Hash・Codec | 85 | 21 | None |
| `Bytes` | Text・Indexed・Growable・Ord・Hash・Codec | 177 | 106 | None |
| `List[A]` | Indexed・Growable・Membership・Hash・Codec | 73 | 16 | None |
| `Deque[A]` | Indexed・Growable・Membership・Codec | 60 | 3 | None |
| `Set[A]` | SetAlgebra・Sequence・Hash・Codec | 42 | 1 | None |
| `Heap[A]` | PriorityQueue・Membership | 16 | 2 | None |
| `Map[K, V]` | KeyValue・Sequence・Codec | 37 | 1 | None |
| `BigInt` | SignedInteger・Codec | 24 | 0 | None |
| `Float` | BinaryFloatingPoint・Codec | 26 | 2 | None |
| `Float32` | BinaryFloatingPoint・Codec | 24 | 0 | None |
| `Float64` | BinaryFloatingPoint・Codec | 24 | 0 | None |
| `Duration` | TimeAmount・Hash・Codec | 7 | 0 | None |
| `Compute` | TimeAmount・Hash | 7 | 0 | None |
| `Uuid` | Parse・Ord・Hash・Codec | 7 | 4 | None |
| `Url` | Parse・Hash・Codec | 9 | 7 | None |
| `Option[A]` | Hash・Codec | 13 | 13 | None |
| `Result[A, E]` | Hash・Codec | 14 | 14 | None |
| `Value` | Repr・Eq・Codec | 29 | 29 | None |
| `Matrix` | Repr・Eq | 25 | 25 | None |
| `Regex` | Repr | 15 | 15 | None |
| `HttpRequest・HttpResponse` | HttpMessage | 34 | 26 | None |
| `TcpStream` | Read・Write・Close | 0 | 0 | None |
| `TcpListener` | Close | 0 | 0 | None |
| `UdpSocket` | Close | 0 | 0 | None |
| `Fd` | Read・Write・Close | 0 | 0 | None |
| `Stdin` | Read | 0 | 0 | None |
| `Stdout・Stderr` | Write | 0 | 0 | None |
| `Int` | FixedWidthInteger・SignedInteger・Codec | 45 | 8 | None |
| `Int8` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | None |
| `Int16` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | None |
| `Int32` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | None |
| `Int64` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | None |
| `UInt8` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | None |
| `UInt16` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | None |
| `UInt32` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | None |
| `UInt64` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | None |

<a id="この階層で決めたこと"></a>

## Decisions driven by this hierarchy

Questions requiring answers once the hierarchy was set, and their decisions. Judgments use both Swift and Almide's criterion (whether LLM-written code runs unchanged). “Deferred” items will be decided after Dojo measurements.

| Question | Facts | Decision |
|---|---|---|
| Is `Bool` `Ord`? | Swift does not make `Bool` `Comparable` (truth values have no greater/lesser meaning). Rust, Kotlin, OCaml, Python and Elixir order them. Almide currently accepts `false < true`. | Accepted (R-bool-02). Seven of nine languages (Rust, Python, JS, Gleam, Elixir, OCaml, Kotlin) provide ordering; only Go and Swift do not. Almide also currently accepts `false < true`. Swift supplies the hierarchy skeleton, but type-to-protocol membership follows the majority for each type. |
| `Repr` method name | The built-in `Repr` method is `repr`, while module functions use `int.to_string`: two spellings for one role. | Accepted (R-L0-38/R-bool-03). Like R-L0-22 (`Ord.cmp` → `compare`), make the `Repr` method `to_string`. Then `<mod>.to_string` exists on every value type (`bool.to_string` appears without an additional implementation). |
| Is `regex.parse` `Parse`? | Swift `Regex` is not `LosslessStringConvertible`. `regex.parse` assembles patterns rather than reading a value's string representation. | Accepted (R-regex-11). Do not put `Regex` in `Parse`; spell `regex.parse` as `regex.compile` (like Go/Python/Elixir). |
| Sized integer arithmetic | `int` has `floor_div`, `modulo`, `pow`, bit operations, `abs`, `sign`, but `int8`–`uint64` do not. Membership in `Integer`/`FixedWidthInteger` exposes these as gaps directly. | Accepted (R-sized-10, reconsidering R-sized-05). Put them on all integers as `Integer` requirements. Use one default implementation, not one per type. |
| `bigint` bit operations | Swift `BinaryInteger` requires bit operations (even arbitrary precision behaves as two's complement). `bigint` currently lacks them. | Accepted (R-bigint-07). Include as requirements. Reject removing `bigint` from `Integer` instead, since that prevents using `int` and `bigint` in the same generic functions. |
| Meaning of `bytes.contains` | `Text`'s `contains` takes subsequences; `Membership`'s `contains` takes elements. Almide has no overloading, so a type can join only one. | Put `bytes` in `Text`, like `string` (keep `bytes.contains(b, pat)`). Express a single value with `bytes.any(b, (x) => x == v)`. |
| Is `string` `Indexed`? | Swift `String` stops at `BidirectionalCollection` (positional reading is not O(1)). Almide `string.get` is code-point-based. | Stop at `Ordered`; leave `string.get` as a `string`-specific function. |
| `deque` `push`/`pop` | `Growable` `push`/`pop` operate at the end; `deque` spells them `push_back`/`pop_back`. | Accepted (R-deque-03). Provide `deque.push`/`deque.pop` as `Growable` functions; do not provide `push_back`/`pop_back` (retain `push_front`/`pop_front` as `deque`-specific). |
| N-bit operations on `int` | `int.wrap_add(a, b, bits)`, `rotate_left(a, n, bits)`, `bit_width`, `bshr_logical` treat `Int` values as N-bit numbers. Fixed-width types encode width in the type; putting the same names there without `bits` creates incompatible shapes. | Accepted (R-int-12/R-int-11). Remove `bits` from `wrap_*`/`rotate_*`, use the type width, and make them `FixedWidthInteger` requirements. Combine with `int.wrap(n, bits)` for N-bit use. Rename `bit_width` to `bit_length` to avoid confusion with Swift `bitWidth` (type width). |
| `string` `new`/`from_list`/`to_list` | `Text` includes `Container`, so `string` needs these three. But `to_list` duplicates `string.chars`, and `from_list` duplicates `list.join(xs, "")`. | Accepted (R-string-17). Rename `chars` to `to_list`; provide `new`/`from_list`. |
| `map`/`flat_map` | Swift `Sequence.map` always returns `Array`; `Optional`/`Result` `map` are concrete-type functions. It does not use types taking containers themselves as arguments (`F[_]`). | Follow Swift: do not make a protocol; keep per-type functions. Do not add `F[_]` to the language. |
| `list` `new`/`from_list`/`to_list` | `Container` requires these, but `list` lacks them (it has literal `[]` and is itself `List`). To write `T.new()` inside generics, `list` needs them too. | Accepted (R-list-20). Provide them. Bodies are trivial (`[]` and identity); no need to use them when writing `list` code. |
| Conversions among numeric types | Swift consolidates on generic initializers of the destination type (`Int8(clamping: x)`). Almide lists destination-specific names in source modules (`int.to_int8_saturating`). | Accepted (R-L0-40). Like Swift, place conversions on destination types, using four generic functions: `<T>.from`, `from_checked`, `from_saturating`, `from_wrapping`. 347 functions become 44. |
| Is `map` (Map) `Container`? | Swift `Dictionary` is a `Collection` of `(key, value)` pairs. Almide callbacks in `map.filter`, etc. take two arguments. | Accepted (R-map-09). Change callbacks to one tuple, allow a two-argument anonymous function in a tuple position (like Swift), and make `map` a `Sequence` of `(K, V)`. |

<a id="言語に要るもの"></a>

## Required language features

The table below lists feature designs for realizing the hierarchy (#4000), not a current unimplemented-feature list. As stated above, receiverless requirement calls and built-in `Numeric`, `Integer`, `Text` are implemented. Tuple-callback anonymous functions have the applicability and UFCS limits recorded in [R-map-09](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md). This does not mean that every hierarchy layer, default implementation or conditional conformance in the table has been realized.

Swift has all these features. Like Swift, no types taking containers themselves as arguments (`F[_]`) are used. Function counts and the conformance table also aggregate the intended design; they are not proof of current implementation conformance. See the [standard-library reference](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md) for current public functions.

| Feature | Example | Use |
|---|---|---|
| Protocol inheritance | `protocol Integer: Ord + Hash + Numeric + Parse` | The hierarchy itself |
| Associated types | `Container` element type (`A` for `List[A]`, `Int` for `Bytes`) | All container layers |
| Default implementations | `min`/`max`/`clamp` from `compare`, `is_empty` from `len`, `any`/`all`/`count` from `fold` | Many gaps filled without new implementations |
| Receiverless requirement calls | `T.parse(s)`/`T.new()`/`T.max_value()`/`T.nan()` inside generics | `Parse`/`Container`/`FixedWidthInteger`/`FloatingPoint` |
| Operator requirements | `Additive`'s `+`/`-`, `Numeric`'s `*` | Numeric layers (Swift `static func +`) |
| Anonymous functions in tuple positions | Write `(k, v) => …` where `((K, V)) -> Bool` is expected | `map`'s `Sequence` (R-map-09) |
| Conditional conformance | `List[A]` is `Hash` only if `A: Hash`; `contains` only for `Eq` elements | `Hash`/`Membership` |

<a id="その型だけの関数"></a>

## Type-specific functions

- `String`（21）: `byte_len`、`byte_slice`、`capitalize`、`from_codepoint`、`from_utf16`、`get`、`is_alpha`、`is_alphanumeric`、`is_digit`、`is_lower`、`is_upper`、`is_whitespace`、`lines`、`split_once`、`to_codepoint`、`to_lower`、`to_upper`、`to_utf16`、`trim`、`trim_end`、`trim_start`
- `Bytes`（106）: `append_bool`、`append_bytes`、`append_f16_be`、`append_f16_le`、`append_f32_be`、`append_f32_le`、`append_f64_be`、`append_f64_le`、`append_i16_be`、`append_i16_le`、`append_i32_be`、`append_i32_le`、`append_i64_be`、`append_i64_le`、`append_i8`、`append_u16_be`、`append_u16_le`、`append_u32_be`、`append_u32_le`、`append_u8`、`copy_from`、`copy_within`、`fill`、`from_string`、`get_bool`、`get_f16_be`、`get_f16_be_array`、`get_f16_le`、`get_f16_le_array`、`get_f32_be`、`get_f32_be_array`、`get_f32_le`、`get_f32_le_array`、`get_f64_be`、`get_f64_be_array`、`get_f64_le`、`get_f64_le_array`、`get_i16_be`、`get_i16_be_array`、`get_i16_le`、`get_i16_le_array`、`get_i32_be`、`get_i32_be_array`、`get_i32_le`、`get_i32_le_array`、`get_i64_be`、`get_i64_be_array`、`get_i64_le`、`get_i64_le_array`、`get_i8`、`get_u16_be`、`get_u16_be_array`、`get_u16_le`、`get_u16_le_array`、`get_u32_be`、`get_u32_be_array`、`get_u32_le`、`get_u32_le_array`、`get_u8`、`is_eof`、`is_valid_utf8`、`map`、`next_bool`、`next_bytes`、`next_f16_be`、`next_f16_le`、`next_f32_be`、`next_f32_le`、`next_f64_be`、`next_f64_le`、`next_i16_be`、`next_i16_le`、`next_i32_be`、`next_i32_le`、`next_i64_be`、`next_i64_le`、`next_i8`、`next_u16_be`、`next_u16_le`、`next_u32_be`、`next_u32_le`、`next_u8`、`put_bool`、`put_f16_be`、`put_f16_le`、`put_f32_be`、`put_f32_le`、`put_f64_be`、`put_f64_le`、`put_i16_be`、`put_i16_le`、`put_i32_be`、`put_i32_le`、`put_i64_be`、`put_i64_le`、`put_i8`、`put_u16_be`、`put_u16_le`、`put_u32_be`、`put_u32_le`、`put_u8`、`skip`、`to_string`、`to_string_lossy`、`xor`、`zeros`
- `List[A]`（16）: `binary_search`、`binary_search_by`、`binary_search_with`、`flat_map`、`flatten`、`index_of`、`iterate`、`join`、`map`、`range`、`range_step`、`repeat`、`swap`、`unzip`、`update`、`with_capacity`
- `Deque[A]`（3）: `map`、`pop_front`、`push_front`
- `Set[A]`（1）: `map`
- `Heap[A]`（2）: `new_by`、`new_with`
- `Map[K, V]`（1）: `map`
- `Float`（2）: `pow`、`to_fixed`
- `Uuid`（4）: `from_bytes`、`to_bytes`、`v4`、`v7`
- `Url`（7）: `build_query`、`decode_component`、`decode_form`、`encode_component`、`encode_form`、`parse_query`、`resolve`
- `Option[A]`（13）: `collect`、`collect_map`、`filter`、`flat_map`、`flatten`、`is_none`、`is_some`、`map`、`or_else`、`to_list`、`to_result`、`unwrap_or_else`、`zip`
- `Result[A, E]`（14）: `filter`、`flat_map`、`flatten`、`is_err`、`is_ok`、`map`、`map_err`、`or_else`、`partition`、`to_err_option`、`to_list`、`to_option`、`unwrap_or_else`、`zip`
- `Value`（29）: `array`、`as_array`、`as_bool`、`as_float`、`as_int`、`as_object`、`as_string`、`bool`、`float`、`get`、`get_array`、`get_bool`、`get_float`、`get_int`、`get_object`、`get_path`、`get_string`、`index`、`int`、`key`、`merge`、`null`、`object`、`omit`、`pick`、`remove_path`、`root`、`set_path`、`string`
- `Matrix`（25）: `broadcast_add_row`、`cols`、`concat_cols`、`concat_rows`、`dot_row`、`from_bytes_f16_le`、`from_bytes_f32_le`、`from_bytes_f64_le`、`from_lists`、`gather_rows`、`get`、`map`、`ones`、`rows`、`scale`、`shape`、`slice_rows`、`split_cols_even`、`to_bytes_f16_le`、`to_bytes_f32_le`、`to_bytes_f64_le`、`to_lists`、`transpose`、`zeros`、`zip_with`
- `Regex`（15）: `captures`、`captures_all`、`compile`、`escape`、`find`、`find_all`、`index_of`、`is_full_match`、`is_match`、`new`、`replace`、`replace_first`、`replace_first_with`、`replace_with`、`split`
- `HttpRequest・HttpResponse`（26）: `cancel`、`decode_json`、`get`、`get_response`、`json`、`method`、`mount`、`new_request`、`param`、`path`、`poll`、`query_params`、`read_new`、`redirect`、`request`、`response`、`route`、`router`、`send`、`serve`、`set_status`、`start`、`status`、`wait`、`with_headers`、`wrap`
- `Int`（8）: `bit_reverse`、`bshr_logical`、`log2_ceil`、`log2_floor`、`next_power_of_two`、`prev_power_of_two`、`to_hex`、`wrap`
