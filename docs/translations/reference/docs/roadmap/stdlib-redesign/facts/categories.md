<a id="l1-事実集-9-言語の標準ライブラリ--カテゴリモジュール単位の比較"></a>

# L1 facts: standard libraries of 9 languages — comparison by category (module)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](categories.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](categories.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](categories.zh-CN.md)

> Non-normative fixed-source historical survey reader. The pinned original remains unchanged; no automatic synchronization, API tests or independent source re-verification. Research date October 21, versions, F marks and memory-based cells describe the original author’s evidence, not new verification. Planned editions and WASI phases remain plans/stages, not fresh availability claims. Source disagreements are preserved: the Go UUID summary calls 1.27 verified but the final note calls that version an unverified estimate; counts and order are not silently corrected, including CSPRNG Swift standard/package statements, the float-module count alongside OCaml’s listed modules, and section 3.12’s grouping claim with its contrary parentheses. References to safe HTML describe source types/trust markers, not a new security guarantee. Function counts, negative-sleep behavior and API organization are historical, not independently tested current behavior. In literal conversion placeholders, 型 means type. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)

Research date: 2026-10-21. The purpose is to “collect facts.” No decisions or proposals are recorded here.

<a id="0-読み方"></a>

## 0. How to read this document

<a id="01-記号"></a>

### 0.1 Symbols

| Symbol | Meaning |
|---|---|
| ● | Included in the standard library (on all platforms, or in the language's common subset) |
| ◐ | Included conditionally: JVM JDK only (Kotlin), Apple OS only (Swift), Erlang/OTP modules (Elixir), separately named unix/str/threads libraries bundled with the compiler (OCaml), Deno std / Bun / Node only (JS), etc. The reason is given in the cell |
| pkg: | Not standard. Only an official package from the language team exists |
| — | Not in the standard library |
| [F] | A cell verified by WebFetch of official documentation during this session |

The “Count (●/◐)” column = number of ● / number of ◐ among the 9 languages. Packages are not counted.
The Almide column comes from `docs/stdlib/README.md` as of 2026-10 (44 modules, 999 functions).

<a id="02-標準の範囲言語ごと"></a>

### 0.2 Scope counted as “standard” (by language)

| Language | Scope counted as standard | Not counted (pkg or notes only) |
|---|---|---|
| Rust | `std` crate (stable). Unstable items are noted | Crates from the rust-lang org (regex, log) are pkg |
| Go | All packages in `pkg.go.dev/std` | `golang.org/x/...` |
| Python | Standard library (3.15 module index) | PyPI |
| TS/JS | ECMAScript built-ins + Web APIs common to runtimes (fetch, URL, crypto.subtle, etc.) are ●. Features only in Node built-ins / Deno `@std` / Bun built-ins are ◐ | npm |
| Gleam | Only `gleam_stdlib` | Official gleam-lang org packages (gleam_json etc.) are pkg |
| Elixir | Elixir's own modules (+ bundled Logger / ExUnit / EEx applications) are ●. Erlang/OTP (stdlib, kernel, crypto, inets, ssl…) is callable from Elixir without adding dependencies, so ◐(OTP) | Hex packages |
| OCaml | `Stdlib` is ●. Compiler-bundled unix / str / threads are ◐ | Jane Street Base/Core are notes only (not counted) |
| Kotlin | `kotlin-stdlib` is ● (Common or all targets). JVM-only stdlib packages and JDK classes are ◐(JVM) | kotlinx.* / Ktor are pkg |
| Swift | Swift standard library + toolchain-bundled Foundation (swift-foundation, also bundled on Linux/Windows) are ●. Apple-OS-only frameworks are ◐(Apple) | swift-collections etc. are pkg |

<a id="03-参照した公式ドキュメントの根ルートurl"></a>

### 0.3 Root URLs of official documentation consulted

| Language | Root URL | Version checked |
|---|---|---|
| Rust | https://doc.rust-lang.org/std/ [F] | 1.99.1 |
| Go | https://pkg.go.dev/std [F] | go1.27.3 |
| Python | https://docs.python.org/3/library/ ・ https://docs.python.org/3/py-modindex.html [F] | 3.15.0 |
| TS/JS | ECMAScript: https://tc39.es/ecma262/ ・ finished proposals: https://github.com/tc39/proposals/blob/main/finished-proposals.md [F] ・ MDN: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects ・ Node: https://nodejs.org/api/ [F] ・ Deno std: https://jsr.io/@std [F] ・ Bun: https://bun.com/docs/runtime/utils [F] | Node 26.12, Deno std 43 packages |
| Gleam | https://gleam-stdlib.hexdocs.pm/ (formerly https://hexdocs.pm/gleam_stdlib/ ) [F] ・ official org: https://github.com/gleam-lang [F] | gleam_stdlib 1.0.5 |
| Elixir | https://hexdocs.pm/elixir/api-reference.html [F] ・ Erlang stdlib: https://www.erlang.org/doc/apps/stdlib/api-reference.html [F] | Elixir 1.20.4 / OTP 29.1.2 |
| OCaml | https://ocaml.org/manual/latest/api/index.html [F] ・ Core: https://ocaml.org/p/core/latest | OCaml 5.5 |
| Kotlin | https://kotlinlang.org/api/core/kotlin-stdlib/ [F] ・ official libraries: https://kotlinlang.org/docs/api-references.html [F] | 2.4 |
| Swift | https://developer.apple.com/documentation/swift/swift-standard-library (body could not be retrieved) ・ https://github.com/swiftlang/swift-foundation [F] ・ swift-collections: https://swiftpackageindex.com/apple/swift-collections/documentation [F] | swift-collections 1.7.3 |
| WASI | https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md [F] ・ https://github.com/WebAssembly/wasi-clocks [F] | — |

**What retrieval verified (the content behind [F])**
- Rust std's module list (stable / unstable; `random`, `f16`, `f128` are unstable).
- All Go std packages. The presence of `uuid` in std (`uuid.NewV4`, `NewV7`, `Parse`) (https://pkg.go.dev/uuid ). `simd` also exists.
- Python 3.15's module index (includes `compression`, `tomllib`, `zoneinfo`, `uuid`).
- Years of finished TC39 proposals: Temporal = 2027 edition, Uint8Array base64/hex = 2026, Iterator helpers = 2025, Set methods = 2025, Upsert(Map.getOrInsert) = 2026, Math.sumPrecise = 2026.
- Deno `@std`'s 43-package list. `@std/encoding` includes hex / base32 / base58 / base64 / base64url / ascii85 / varint. `@std/data-structures` includes BinaryHeap, BinarySearchTree, (Readonly)Deque, etc.
- Node 26's API list (SQLite, Zlib, UDP/datagram, TTY, Test runner, Web Crypto, FFI, etc.).
- Bun built-ins: gzip/deflate/zstd, escapeHTML, randomUUIDv7, stringWidth/stripANSI, TOML/YAML loading.
- gleam_stdlib 1.0.5 has only 19 modules: bit_array, bool, bytes_tree, dict, dynamic, dynamic/decode, float, function, int, io, list, option, order, pair, result, set, string, string_tree, uri. regex and iterator have left stdlib and become official packages `gleam_regexp`, `gleam_yielder`. The gleam-lang org has json, crypto, time, http, httpc, erlang, otp.
- Elixir 1.20's module list (includes `JSON`, `Duration`, `OptionParser`, `IO.ANSI`, `Base`, `URI`, `Version`, `Regex`, `Calendar.TimeZoneDatabase`).
- Erlang/OTP 29 stdlib's list (includes `argparse`, `json`, `queue`, `gb_trees`, `gb_sets`, `base64`, `uri_string`, `calendar`, `rand`, `zip`, `erl_tar`, `zstd`, `io_ansi`, `digraph`).
- OCaml 5.5 Stdlib's list (includes `Dynarray`, `Pqueue` (since 5.4), `Iarray`, `Pair`, `Repr`, `Domain`, `Effect`, `Atomic`, `Arg`, `Buffer`, `Digest`, `Seq`). External: unix, str, threads.
- Kotlin stdlib 2.4 packages and platforms (`kotlin.uuid` and `kotlin.io.encoding` are Common+JVM; `kotlin.time` is all-target and has `Instant`/`Clock` since 2.3; `kotlin.system` is JVM+Native only). Official kotlinx list: coroutines, serialization, kotlinx-io, kotlinx-datetime, collections.immutable, Ktor.
- swift-collections modules: DequeModule, HeapModule, OrderedCollections, BitCollections, HashTreeCollections, and others.
- swift-foundation is bundled in Linux/Windows toolchains (FoundationEssentials / FoundationInternationalization).
- WASI: Phase 3 = cli, clocks, filesystem, sockets, random, http. Phase 2 = clocks timezone. Phase 1 = crypto, tls, kv-store, native-threads.

Cells without [F] are from memory (well-known APIs). Uncertain items are footnoted as “unverified.”

---

<a id="1-カテゴリ比較表"></a>

## 1. Category comparison tables

Column order: Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift

<a id="1a-数値と基本型"></a>

### 1.A Numbers and basic types

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count ●/◐ |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Integers (default Int) | `int` | Inherent methods of primitives such as `i64` | builtin `int` + `strconv`, `math` | `int` | `Number` / `BigInt` | `gleam/int` [F] | `Integer` [F] | `Int` [F] | `Int`/`Long` (kotlin) | `Int` | 9/0 |
| Fixed-width integers, 8–64 bit | `int8` `int16` `int32` `int64` `uint8` `uint16` `uint32` `uint64` (8 modules) | `i8`–`u64` primitives (methods on each type; old modules such as `std::i8` are deprecated) | builtin types `int8`–`uint64` + `math/bits`, `strconv`, `encoding/binary` | — (representation only via `struct`, `array`, `ctypes`) | ◐ typed arrays `Int8Array`–`BigUint64Array`, `BigInt.asIntN/asUintN` | — (Int only; bit-array width specifications only) | — (arbitrary-precision integers; width specified with `<<x::8>>`) | ◐ `Int32` `Int64` `Nativeint` [F] (no 8/16 bit types) | `Byte` `Short` `Int` `Long` `UByte` `UShort` `UInt` `ULong` | `Int8`–`UInt64` structs | 4/2 |
| 128 bit integers | — | `i128`/`u128` | — | (int is arbitrary precision) | — | — | (arbitrary precision) | — | — | `Int128`/`UInt128` (Swift 6) | 2/0 |
| Floating point (default) | `float` | `f64` | `float64` + `math` | `float` | `Number` | `gleam/float` [F] | `Float` [F] | `Float` [F] | `Double` | `Double` | 9/0 |
| 32 bit floating point | `float32`, `float64` modules | `f32`, `f64` (constant modules + primitives) [F] | `float32` | — (`struct`/`array` 'f' only) | ◐ `Float32Array`, `Math.fround`, `Float16Array`(2025) | — | — (bitstring `::float-32`) | — (`Int32.float_of_bits` only) | `Float` | `Float`, `Float16`, `Float80` | 4/1 |
| Arbitrary-precision integers BigInt | — | — | `math/big` (Int) [F] | `int` | `BigInt` | ◐ Int (arbitrary precision on Erlang target; differs on JS) | `Integer` (arbitrary precision) | — (Zarith is external) | ◐ `java.math.BigInteger`(JVM) | — | 4/2 |
| Decimal / rational numbers | — | — | `math/big` (Rat, Float) | `decimal`, `fractions` [F] | — | — | — (Decimal is a Hex package) | — | ◐ `java.math.BigDecimal`(JVM) | `Decimal`(Foundation) | 3/1 |
| Complex numbers | — | — | builtin `complex128` + `math/cmplx` [F] | `complex` + `cmath` [F] | — | — | — | `Complex` [F] | — | pkg: swift-numerics | 3/0 |
| Mathematical functions | `math` | `f64` methods | `math` | `math` | `Math` | ◐ only part of `gleam/float` (no trigonometric functions) | ◐ `:math`(OTP) | `Float` | `kotlin.math` [F] | Foundation (via Glibc/Darwin) + pkg: swift-numerics | 6/2 |
| Statistics | — | — | — | `statistics` | — | — | — | — | — | — | 1/0 |
| Bit-operation helpers (popcount etc.) | Part of `int` | Integer methods such as `count_ones` | `math/bits` | `int.bit_count` | `Math.clz32` | `gleam/int` bitwise_* | `Bitwise` [F] | land/lor in `Int` | `Int.countOneBits` etc. | `nonzeroBitCount` etc. | 9/0 |
| SIMD | — | `arch` (stable), `simd` (unstable) [F] | `simd`, `simd/archsimd` [F] | — | — | — | — | — | — | `SIMD2`–`SIMD64` types | 3/0 |
| Matrices / linear algebra | `matrix` (46 functions) | — | — | — (only the `@` operator exists, no implementation) | — (`DOMMatrix` is browser-only) | — | — (Nx is external) | ◐ `Bigarray` (multidimensional-array container; no operations) | — | ◐ `simd` framework (Apple) | 0/2 |

<a id="1b-文字列とテキスト"></a>

### 1.B Strings and text

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Strings | `string` | `str`, `string` | `strings`, `strconv` | `str`, `string` | `String` | `gleam/string` [F] | `String` [F] | `String` [F] | `kotlin.text` | `String` | 9/0 |
| Character types / Unicode classification | `string.is_digit` etc., `codepoint` (no character-type module) | `char` [F] | `unicode`, `unicode/utf8` [F] | `unicodedata` | `String.prototype.codePointAt`, `Intl.Segmenter` | `string.to_utf_codepoints`, `to_graphemes` | `String.graphemes`, `:unicode`(OTP) | `Char`, `Uchar` [F] | `Char` (`category`) | `Character`, `Unicode.Scalar` | 9/0 |
| UTF-16 / Latin-1 etc. encodings | — | `String::from_utf16`, `encode_utf16` | `unicode/utf16` [F] | `codecs` | `TextDecoder` (utf-16le etc.), Node `Buffer` latin1 | — | ◐ `:unicode.characters_to_binary`(OTP) | `Bytes.get_utf_16le_uchar` etc. | ◐ `java.nio.charset`(JVM) (Common has UTF-8 only) | `String(decoding:as: UTF16.self)`, `String.Encoding` | 6/2 |
| String builders | — (`+` and `list.join`) | `String` (mutable) | `strings.Builder`, `bytes.Buffer` | `io.StringIO` | — (array join is idiomatic) | `gleam/string_tree` [F] | iodata (`IO.iodata_to_binary`) | `Buffer` [F] | `StringBuilder` | `String` (mutable) | 8/0 |
| Formatting printf | String interpolation, `string.pad_*` | `fmt` (`format!`) | `fmt` | `str.format`, f-string, `string.templatelib` | Template literals, ◐ Deno `@std/fmt` printf | — | `inspect`, ◐ `:io_lib.format` | `Printf`, `Format` [F] | ◐ `String.format`(JVM) | ◐ `String(format:)`(Foundation) | 5/3 |
| Text layout (wrapping / diff / width) | — | — | `text/tabwriter` [F] | `textwrap`, `difflib` | ◐ Deno `@std/text`, Bun `stringWidth`/`wrapAnsi` | — | — | — | — | — | 2/1 |
| Regular expressions | `regex` | pkg: `regex` (rust-lang) | `regexp` [F] | `re` | `RegExp` | pkg: `gleam_regexp` [F] | `Regex` [F] | ◐ `Str` (bundled library) [F] | `Regex` | `Regex` (since 5.7), `NSRegularExpression` | 6/1 |
| Template engines | — (`html` only provides safe concatenation) | — | `text/template`, `html/template` [F] | `string.Template` | — | — | EEx (bundled application) | — | — | — | 3/0 |
| Locales / internationalized formatting | — | — | — | `locale`, `gettext` | `Intl` | — | — | — | ◐ JVM | `Locale`, FormatStyle (FoundationInternationalization) | 3/1 |

<a id="1c-コレクション"></a>

### 1.C Collections

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Lists / dynamic arrays | `list` | `Vec`, `vec`, `slice` | slice + `slices` | `list` | `Array` | `gleam/list` (linked list) [F] | `List`, `Enum` | `List`, `Array`, `Dynarray` [F] | `List`/`ArrayList` | `Array` | 9/0 |
| Hash maps | `map` (preserves insertion order) | `HashMap` | builtin map + `maps` | `dict` (insertion order) | `Map` (insertion order) | `gleam/dict` [F] | `Map` | `Hashtbl` | `Map` (`mapOf` uses insertion order) | `Dictionary` | 9/0 |
| Insertion-ordered maps | `map` | — | — | `dict` | `Map` | — | `Keyword` (list) | — | `LinkedHashMap` (default) | pkg: `OrderedDictionary` [F] | 4/0 |
| Sorted maps (trees) | — | `BTreeMap`, `BTreeSet` | — | — | ◐ Deno `RedBlackTree`/`BinarySearchTree` [F] | — | ◐ `:gb_trees`, `:gb_sets`(OTP) [F] | `Map`, `Set` (balanced trees, ordered) | ◐ `sortedMapOf`→`TreeMap`(JVM) | — | 2/3 |
| Sets | `set` (insertion order) | `HashSet` | — (`map[T]struct{}`) | `set` | `Set` + 2025 set operations | `gleam/set` [F] | `MapSet` | `Set` | `Set` | `Set` | 8/0 |
| Deques / queues | — | `VecDeque` | ◐ `container/list` (doubly linked list) [F] | `collections.deque`, `queue` | ◐ Deno `@std/data-structures` Deque [F] | — (old `gleam/queue` is absent from stdlib [F]) | ◐ `:queue`(OTP) [F] | `Queue`, `Stack` | `ArrayDeque` | pkg: `Deque` [F] | 4/3 |
| Heaps / priority queues | — | `BinaryHeap` | `container/heap` [F] | `heapq` | ◐ Deno `BinaryHeap` [F] | — | — (`gb_sets` used as substitute) | `Pqueue` (since 5.4) [F] | ◐ `java.util.PriorityQueue`(JVM) | pkg: `Heap` [F] | 4/2 |
| Linked lists | (`list` is array-based) | `LinkedList` | `container/list`, `container/ring` | — | — | `gleam/list` | `List` | `List` | ◐ `LinkedList`(JVM) | — | 5/1 |
| Bit sets | — | — | — | — | — | — | — | — | ◐ `java.util.BitSet`(JVM) | pkg: `BitSet` [F] | 0/1 |
| Multisets / counters | — | — | — | `collections.Counter` | — | — | — | — | — | — | 1/0 |
| Lazy sequences / iterators | — (list is eager; only the `fs.fold_lines` family is sequential) | `iter` [F] | `iter` (since 1.23) [F] | `itertools`, generator | Iterator helpers (ES2025) [F] | pkg: `gleam_yielder` (formerly `gleam/iterator`) [F] | `Stream` [F] | `Seq` [F] | `Sequence` (`kotlin.sequences`) | `Sequence`, `LazySequence` | 8/0 |
| Ranges | Syntax `0..n` | `ops::Range` | `for range n` | `range` | — | `list.range` | `Range` [F] | — | `kotlin.ranges` | `Range` | 7/0 |
| Comparison / ordering | — | `cmp` | `cmp` | `functools.cmp_to_key` | — | `gleam/order` [F] | — | `compare` | `kotlin.comparisons` | `Comparable` | 7/0 |
| Tuples / pairs | Syntax | Tuples | — | Tuples | — | `gleam/pair` [F] | `Tuple` | `Pair` (5.5) [F] | `Pair`, `Triple` | Tuples | 7/0 |
| Option / Result | `option`, `result` | `option`, `result` | — (multiple return values) | — (exceptions) | — | `gleam/option`, `gleam/result` [F] | — (`{:ok, v}` convention) | `Option`, `Result`, `Either` [F] | `T?`, `Result` | `Optional`, `Result` | 6/0 |
| Errors | `error` | `error` | `errors` [F] | Exceptions (builtins) | `Error` | — (Result) | `Exception` | `Printexc` | `Throwable` | `Error` | 8/0 |

<a id="1d-バイト列符号化データ形式"></a>

### 1.D Byte sequences, encodings and data formats

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Byte sequences | `bytes` (146 functions, including LE/BE reading/writing) | `Vec<u8>`, `slice` | `bytes`, `encoding/binary` | `bytes`, `bytearray`, `struct` | `Uint8Array`, `DataView`, ◐ Node `Buffer` | `gleam/bit_array`, `bytes_tree` [F] | binary, ◐ `:binary` | `Bytes` | `ByteArray` | `[UInt8]`, `Data`(Foundation) | 9/0 |
| Hex | `hex` (3 functions) + `int.to_hex` | — | `encoding/hex` [F] | `bytes.hex()`, `binascii` | `Uint8Array.toHex/fromHex` (2026) [F], ◐ Deno `@std/encoding` | `bit_array.base16_encode` | `Base.encode16` [F] | — (`Digest.to_hex` only) | `HexFormat`, `toHexString` (kotlin.text) | — | 7/0 |
| Base64 | `base64` (4 functions) | — | `encoding/base64` [F] | `base64` | `btoa/atob`, `Uint8Array.toBase64` (2026) [F] | `bit_array.base64_encode` | `Base` [F] | — | `kotlin.io.encoding.Base64` (Common+JVM) [F] | `Data.base64EncodedString`(Foundation) | 7/0 |
| Base32 | — | — | `encoding/base32` [F] | `base64.b32encode` | ◐ Deno `@std/encoding` [F] | — | `Base.encode32` | — | — | — | 3/1 |
| Ascii85 / Base85 / Base58 | — | — | `encoding/ascii85` [F] | `base64.b85encode`, `a85encode` | ◐ Deno (ascii85, base58) [F] | — | — | — | — | — | 2/1 |
| JSON | `json` (15 functions) | — | `encoding/json` [F] | `json` | `JSON` | pkg: `gleam_json` [F] | `JSON` (since 1.18) [F], ◐ `:json`(OTP 27 onward)[F] | — | pkg: kotlinx.serialization-json [F] | `JSONEncoder`/`JSONDecoder`(Foundation) | 5/0 |
| Dynamic values (JSON-like data model) | `value` (20 functions) | — | `any` + `reflect` | dict/list | object | `gleam/dynamic`, `dynamic/decode` [F] (read-only decoders) | Terms themselves | — | pkg: `JsonElement` | `Any` + `JSONSerialization` | 6/0 |
| CSV | — | — | `encoding/csv` [F] | `csv` | ◐ Deno `@std/csv` [F] | — | — | — | — | — | 2/1 |
| TOML | — | — | — | `tomllib` (reading only) [F] | ◐ Deno `@std/toml`, Bun [F] | — | — | — | — | — | 1/1 |
| YAML | — | — | — | — | ◐ Deno `@std/yaml`, Bun [F] | — | — | — | — | — | 0/1 |
| XML | — | — | `encoding/xml` [F] | `xml.*` | ◐ Deno `@std/xml` [F] (`DOMParser` is browser-side) | — | ◐ `:xmerl`(OTP) | — | ◐ `javax.xml`(JVM) | `XMLParser`(Foundation) | 3/3 |
| INI / dotenv | — | — | — | `configparser` | ◐ Deno `@std/ini`, `@std/dotenv` [F] | — | — | — | — | ◐ `PropertyListEncoder`(plist) | 1/2 |
| Binary serialization (gob/pickle/cbor etc.) | — | — | `encoding/gob` [F] | `pickle`, `marshal` | `structuredClone`, ◐ Deno `@std/cbor`, `@std/msgpack` [F] | — | ◐ `:erlang.term_to_binary` | `Marshal` [F] | pkg: serialization-cbor/protobuf [F] | `PropertyListEncoder` | 5/2 |
| URL | `url` (6 functions) + `http.url_decode` | — | `net/url` [F] | `urllib.parse` | `URL`, `URLSearchParams` | `gleam/uri` [F] | `URI` [F] | — | ◐ `java.net.URI`(JVM) | `URL`, `URLComponents`(Foundation) | 6/1 |
| HTML escaping | `html` (`SafeHtml` type) | — | `html`, `html/template` [F] | `html` | ◐ Deno `@std/html`, `Bun.escapeHTML` [F] | — | — | — | pkg: kotlinx.html (builder, unverified) | — | 2/1 |
| MIME types | — | — | `mime`, `mime/multipart` [F] | `mimetypes`, `email` | ◐ Deno `@std/media-types` [F] | — | — | — | — | — | 2/1 |
| UUID | — | — | `uuid` (v4/v7) [F] | `uuid` [F] | `crypto.randomUUID()`, ◐ Deno `@std/uuid`, `Bun.randomUUIDv7` [F] | — | — | — (Uuid in Core_unix) | `kotlin.uuid.Uuid` (Common+JVM) [F] | `UUID`(Foundation) | 5/0 |
| ULID | — | — | — | — | ◐ Deno `@std/ulid` [F] | — | — | — | — | — | 0/1 |
| Versions (semver) | — | — | — | — | ◐ Deno `@std/semver`, `Bun.semver` [F] | — | `Version` [F] | — | — | — | 1/1 |

<a id="1e-ハッシュ暗号乱数"></a>

### 1.E Hashing, cryptography and randomness

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Noncryptographic hashes (FNV, CRC32, Adler) | `hash.fnv1a32`, `zlib.crc32` | `hash` (SipHash `DefaultHasher`) | `hash/fnv`, `crc32`, `crc64`, `adler32`, `maphash` [F] | `zlib.crc32`, `adler32`, `hash()` | ◐ `Bun.hash` (absent from ECMAScript) | — | ◐ `:erlang.phash2`, `:erlang.crc32` | `Hashtbl.hash` | `hashCode`, ◐ `java.util.zip.CRC32`(JVM) | `Hasher` | 6/2 |
| SHA-256 | `hash.sha256` | — | `crypto/sha256` [F] | `hashlib` | `crypto.subtle.digest` | pkg: `gleam_crypto` [F] | ◐ `:crypto`(OTP) | — (`Digest` has MD5 and BLAKE2, not SHA; BLAKE2 since 5.2, unverified) | ◐ `MessageDigest`(JVM) | ◐ CryptoKit(Apple) / pkg: swift-crypto | 3/3 |
| MD5 / SHA-1 / SHA-512 / SHA-3 | — | — | `crypto/md5`, `sha1`, `sha512`, `sha3` [F] | `hashlib` (md5, sha1, sha2, sha3, blake2) | `crypto.subtle` (SHA-1/384/512), ◐ `node:crypto`, Deno `@std/crypto` | pkg: `gleam_crypto` | ◐ `:crypto` | `Digest` (MD5) | ◐ JVM | ◐ CryptoKit / pkg | 4/3 |
| HMAC / KDF | — | — | `crypto/hmac`, `hkdf`, `pbkdf2` [F] | `hmac`, `hashlib.pbkdf2_hmac` | `crypto.subtle` (HMAC, PBKDF2, HKDF) | pkg: `gleam_crypto` | ◐ `:crypto.mac` | — | ◐ `javax.crypto.Mac`(JVM) | ◐ CryptoKit / pkg | 3/2 |
| Cryptography (AES, signatures, TLS) | — | — | `crypto/aes`, `ed25519`, `tls`, `mlkem`, etc. [F] | `ssl` (no cryptographic primitives) | `crypto.subtle` | — | ◐ `:crypto`, `:ssl` | — | ◐ JVM | ◐ CryptoKit / pkg | 3/3 |
| Pseudorandom numbers | `random` (4 functions) | — (`random` is unstable [F]) | `math/rand/v2` [F] | `random` | `Math.random` | `int.random`, `float.random` | `Enum.random`, ◐ `:rand` [F] | `Random` [F] | `kotlin.random` [F] | `Int.random(in:)` | 8/0 |
| Seeded (reproducible) randomness | — | — | `rand.New(rand.NewPCG(..))` | `random.Random(seed)` | ◐ Deno `@std/random` (seeded) | — | ◐ `:rand.seed` | `Random.State` | `Random(seed)` | — (no seeded generator in stdlib) | 4/2 |
| Cryptographically secure randomness (CSPRNG) | — | — | `crypto/rand` [F] | `secrets` | `crypto.getRandomValues` | pkg: `gleam_crypto` | ◐ `:crypto.strong_rand_bytes` | — | ◐ `SecureRandom`(JVM) | `SystemRandomNumberGenerator` (CSPRNG) | 4/2 |

<a id="1f-圧縮アーカイブ"></a>

### 1.F Compression and archives

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| DEFLATE / zlib / gzip | `zlib` | — | `compress/flate`, `gzip`, `zlib` [F] | `zlib`, `gzip` | `CompressionStream` (gzip/deflate/deflate-raw), ◐ `node:zlib`, `Bun.gzipSync` [F] | — | ◐ `:zlib`(erts) | — | ◐ `java.util.zip`(JVM) | ◐ Compression(Apple) | 3/3 |
| Other compression (bzip2, lzma, zstd, brotli, lzw) | — | — | `compress/bzip2` (decompression only), `lzw` [F] | `bz2`, `lzma`, `compression.zstd` [F] | ◐ `node:zlib` (brotli, zstd), `Bun.zstd*` [F] | — | ◐ `:zstd`(OTP 29) [F] | — | — | ◐ Compression(Apple) | 2/3 |
| Archives (tar, zip) | — | — | `archive/tar`, `archive/zip` [F] | `tarfile`, `zipfile` | ◐ Deno `@std/tar` [F] | — | ◐ `:zip`, `:erl_tar`(OTP) [F] | — | ◐ `java.util.zip`(JVM) | — | 2/3 |

<a id="1g-時間"></a>

### 1.G Time

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Elapsed-time type Duration (real time) | `duration` (6 functions, constructors and operations only) | `time::Duration` | `time.Duration` | `datetime.timedelta` | `Temporal.Duration` (planned for ES2027 [F]) | pkg: `gleam_time` (`gleam/time/duration`) [F] | `Duration` (since 1.17) [F] | — (Core: `Time_ns.Span`) | `kotlin.time.Duration` [F] | `Duration` (since 5.7) | 7/0 |
| Deterministic computation time (Almide `compute`) | `compute` | — | — | — (`time.process_time` is CPU time) | — | — | — (closest: BEAM reduction count; not a value type) | — (`Sys.time` is CPU time) | — | — | 0/0 |
| Monotonic clocks / stopwatches | `datetime.monotonic_ns` | `time::Instant` | `time.Now` (with monotonic component), `time.Since` | `time.monotonic`, `perf_counter` | `performance.now()` | pkg: `gleam_time` | `System.monotonic_time` | ◐ `Unix.gettimeofday` (not monotonic; no monotonic clock in Stdlib) | `TimeSource.Monotonic`, `measureTime` [F] | `ContinuousClock`, `SuspendingClock` | 7/1 |
| Current time (Unix time) | `datetime.now`, `env.unix_timestamp`, `env.millis` | `SystemTime` | `time.Now` | `time.time` | `Date.now` | pkg: `gleam_time` | `System.os_time`, `DateTime.utc_now` | ◐ `Unix.time` | `kotlin.time.Clock.System` (since 2.3) [F] | `Date()`(Foundation) | 7/1 |
| Calendar dates and times | `datetime` (Int timestamp + component extraction) | — | `time.Time` | `datetime`, `calendar` | `Date`, `Temporal.PlainDate*` (2027) | pkg: `gleam_time` (calendar) | `Date`, `Time`, `NaiveDateTime`, `DateTime`, `Calendar` [F] | ◐ `Unix.gmtime/localtime` | pkg: kotlinx-datetime [F] | `Calendar`, `DateComponents`(Foundation) | 5/1 |
| Time zones (IANA) | — (UTC and offsets only) | — | `time.LoadLocation`, `time/tzdata` [F] | `zoneinfo` [F] | `Intl.DateTimeFormat`(timeZone), `Temporal.ZonedDateTime` | — | ◐ `DateTime` + `Calendar.TimeZoneDatabase` (default UTC only [F]; external tz data) | — | pkg: kotlinx-datetime `TimeZone`, ◐ `java.time.ZoneId`(JVM) | `TimeZone`(Foundation) | 4/2 |
| Date/time formatting and parsing | `datetime.format`, `parse_iso`, `to_iso` | — | `time.Format`, `time.Parse` | `strftime`, `fromisoformat` | `toISOString`, `Intl` | pkg | `Calendar.strftime`, `DateTime.from_iso8601` | — | `Instant.parse` (ISO) [F] | `ISO8601DateFormatter`, FormatStyle | 6/0 |
| Sleep | `env.sleep_ms`, `process.sleep` | `thread::sleep` | `time.Sleep` | `time.sleep` | `setTimeout`, ◐ `Bun.sleep` | — (pkg: gleam_erlang `process.sleep`) | `Process.sleep` | ◐ `Unix.sleepf` | ◐ `Thread.sleep`(JVM), pkg: coroutines `delay` | `Task.sleep` | 6/2 |
| Timers (execute later) | — | — | `time.AfterFunc`, `time.Ticker` | `threading.Timer`, `sched` | `setTimeout`, `setInterval` | — | `:timer`(OTP), `Process.send_after` | — | pkg: coroutines | `Task` + `Clock.sleep` | 4/1 |

<a id="1h-osファイルプロセス"></a>

### 1.H OS, files and processes

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Filesystems | `fs` (34 functions) | `fs` [F] | `os`, `io/fs` [F] | `os`, `shutil`, `pathlib` | ◐ `node:fs`, `Deno.*`, `@std/fs`, `Bun.file` | — (simplifile is external) | `File` [F] | `Sys`, `In_channel`, `Out_channel` [F], ◐ `Unix` | ◐ `kotlin.io` (JVM `File`), pkg: kotlinx-io | `FileManager`(Foundation) | 6/2 |
| Path operations | `path` | `path` [F] | `path`, `path/filepath` [F] | `os.path`, `pathlib` | ◐ `node:path`, `@std/path` | — | `Path` [F] | `Filename` [F] | ◐ `kotlin.io.path`(JVM)[F] | `URL`(file), pkg: swift-system `FilePath` | 6/2 |
| Temporary files | `fs.temp_dir`, `create_temp_file`, `create_temp_dir`, `env.temp_dir` | `env::temp_dir` (directory name only) | `os.CreateTemp`, `os.MkdirTemp` | `tempfile` | ◐ `Deno.makeTempFile`, `fs.mkdtemp` | — | `System.tmp_dir` | `Filename.temp_file` | ◐ `createTempFile`(JVM) | `FileManager.temporaryDirectory` | 6/2 |
| glob | `fs.glob` | — | `filepath.Glob`, `fs.Glob` | `glob`, `fnmatch` | ◐ `fs.glob`(Node 22 onward), `Bun.Glob`, Deno `expandGlob` | — | `Path.wildcard` | — | ◐ `PathMatcher`(JVM) | — | 3/2 |
| File watching | — | — | — | — | ◐ `fs.watch`, `Deno.watchFs` | — | — | — | ◐ `WatchService`(JVM) | ◐ FSEvents(Apple) | 0/3 |
| Standard I/O | `io`, `println`, `eprintln` | `io` | `os.Stdin`, `bufio`, `fmt` | `sys.stdin`, `print`, `input` | `console`, ◐ `process.stdin`, `Deno.stdin` | `gleam/io` (output only; no stdin reading) [F] | `IO` [F] | `In_channel`, `Out_channel`, `print_*` | `println`, `readln` | `print`, `readLine` | 8/1 |
| Terminals / ANSI colors / TTY | — | `io::IsTerminal` | — (x/term is external) | `curses`, `tty`, `termios`, `os.isatty` | ◐ `node:tty`, `util.styleText`, `@std/fmt/colors`, `Bun.stripANSI` | — | `IO.ANSI` [F], ◐ `io_ansi`(OTP)[F] | ◐ `Unix.isatty` | — | — | 3/2 |
| Environment variables | `env.get`, `env.set` | `env::var` | `os.Getenv` | `os.environ` | `process.env`, `Deno.env` | — | `System.get_env` | `Sys.getenv` | ◐ `System.getenv`(JVM) (absent from Common) | `ProcessInfo.environment` | 7/1 |
| Command-line arguments (raw) | `env.args`, `args.raw` | `env::args` | `os.Args` | `sys.argv` | `process.argv`, `Deno.args`, `Bun.argv` | — | `System.argv` | `Sys.argv` | `main(args)` | `CommandLine.arguments` | 8/0 |
| Flag parsing | `args` (6 functions: flag, option, positional) | — | `flag` [F] | `argparse`, `getopt`, `optparse` [F] | ◐ `util.parseArgs`(Node), `@std/cli` [F] | — | `OptionParser` [F], ◐ `argparse`(OTP)[F] | `Arg` [F] | — (kotlinx-cli absent from official API list [F]) | pkg: swift-argument-parser | 4/1 |
| Working directory | `env.cwd` | `env::current_dir` | `os.Getwd` | `os.getcwd` | `process.cwd()` | — | `File.cwd` | `Sys.getcwd` | ◐ JVM | `FileManager.currentDirectoryPath` | 7/1 |
| Child-process execution | `process` (17 functions) | `process` [F] | `os/exec` [F] | `subprocess` | ◐ `child_process`, `Deno.Command`, `Bun.spawn` | — | `System.cmd`, `Port` [F] | `Sys.command`, ◐ `Unix.create_process` | ◐ `ProcessBuilder`(JVM) | `Process`(Foundation) | 6/3 |
| Exit code / exit | `process.exit` | `process::exit` | `os.Exit` | `sys.exit` | `process.exit` | — | `System.halt` | `exit` | `exitProcess` (JVM+Native [F]) | `exit` | 8/0 |
| Signals | `process.kill` (sending only) | — | `os/signal` [F] | `signal` | ◐ `process.on('SIGINT')`, `Deno.addSignalListener` | — | `System.trap_signal` | `Sys.signal` | — | ◐ Dispatch `DispatchSource` | 4/2 |
| OS information / users | `env.os` | `env::consts` | `runtime.GOOS`, `os/user` [F] | `platform`, `os`, `getpass`, `pwd` | ◐ `node:os` | — | `:os.type`, `System` | `Sys.os_type` | ◐ JVM | `ProcessInfo` | 5/2 |
| Logging | — | pkg: `log` (rust-lang) | `log`, `log/slog` [F] | `logging` | `console.*`, ◐ Deno `@std/log` [F] | — | `Logger` (bundled application) | — | — | pkg: swift-log, ◐ `os.Logger`(Apple) | 5/0 |
| Databases (SQLite etc.) | — | — | `database/sql` (interface only) [F] | `sqlite3` | ◐ `node:sqlite` [F], `bun:sqlite` | — | ◐ `:ets`, `:dets`, mnesia(OTP) | — | — | — | 2/2 |

<a id="1i-並行非同期"></a>

### 1.I Concurrency and asynchronous execution

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Threads / parallel execution | Language feature `fan` (not a stdlib module) | `thread` [F] | goroutine (language) + `runtime` | `threading`, `multiprocessing`, `concurrent.futures` | ◐ `worker_threads`, Web Worker | — (pkg: gleam_otp, gleam_erlang) | `Task`, `Process`, `Agent` [F] | `Domain` [F], ◐ `Thread` | ◐ `kotlin.concurrent`(JVM+Native)[F], pkg: coroutines | `Task`(Concurrency), `Thread`(Foundation) | 6/2 |
| Channels / messages | — | `sync::mpsc` | `chan` (language) | `queue.Queue`, `asyncio.Queue` | `MessageChannel` | — (pkg: gleam_erlang `Subject`) | `send`/`receive`, `GenServer` | ◐ `Event`(threads) | pkg: coroutines `Channel` | `AsyncStream` | 6/1 |
| Locks / atomics | — | `sync` (Mutex, Arc, Atomic*) | `sync`, `sync/atomic` | `threading.Lock` | `Atomics`, `SharedArrayBuffer` | — | ◐ `:atomics`(OTP) | `Mutex`, `Atomic`, `Condition`, `Semaphore` [F] | `kotlin.concurrent.atomics` [F] | `Synchronization` (`Atomic`, `Mutex`) | 7/1 |
| async / await and executors | — | `future`, `task` (no executor) | — | `asyncio` | `Promise`, async functions | — | `Task.async/await` | `Effect` (no executor) | Language `suspend` + pkg: coroutines | `async`/`await`, `Task`, actor | 5/0 |
| Cancellation / deadlines | `fan.timeout` (language) | — | `context` [F] | `asyncio.timeout` | `AbortController`, `AbortSignal.timeout` | — | `Task.yield/shutdown` | — | pkg: coroutines `withTimeout` | `Task.cancel` | 5/0 |

<a id="1j-ネットワーク"></a>

### 1.J Networking

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| TCP | `net.tcp_*` | `net::TcpStream`, `TcpListener` [F] | `net` [F] | `socket`, `socketserver` | ◐ `node:net`, `Deno.connect`, `Bun.connect` | — | ◐ `:gen_tcp`(OTP kernel) | ◐ `Unix` sockets | ◐ `java.net.Socket`(JVM), pkg: ktor-network | — (pkg: swift-nio) | 3/4 |
| UDP | — | `net::UdpSocket` | `net` | `socket` | ◐ `node:dgram` [F], Deno | — | ◐ `:gen_udp` | ◐ `Unix` | ◐ `DatagramSocket`(JVM) | — | 3/4 |
| Unix domain sockets | `net.unix_*`, `net.shm_*` | `os::unix::net` | `net` | `socket`(AF_UNIX) | ◐ `node:net`(path) | — | ◐ `:gen_tcp` `{:local, path}` | ◐ `Unix` | ◐ JVM 16 onward | — | 3/4 |
| DNS resolution | — (implicit inside connect) | `ToSocketAddrs` | `net.LookupHost` | `socket.getaddrinfo` | ◐ `node:dns`, `Deno.resolveDns` | — | ◐ `:inet.gethostbyname` | ◐ `Unix.getaddrinfo` | ◐ `InetAddress`(JVM) | — | 3/4 |
| IP address types | — | `net::IpAddr` | `net/netip` [F] | `ipaddress` | ◐ `net.isIP`, `BlockList` | — | ◐ `:inet.parse_address` | ◐ `Unix.inet_addr_of_string` | ◐ JVM | — | 3/4 |
| HTTP clients | `http.get` and others | — | `net/http` [F] | `urllib.request`, `http.client` | `fetch` | pkg: `gleam_httpc`, `gleam_fetch` (types in `gleam_http`) [F] | ◐ `:httpc`(OTP inets) | — | ◐ `java.net.http`(JVM), pkg: Ktor | `URLSession`(Foundation) | 4/2 |
| HTTP servers | `http.serve` | — | `net/http` | `http.server` (development use) | ◐ `node:http`, `Deno.serve`, `Bun.serve`, `@std/http` | pkg: `gleam_httpd`? (org has an `httpd` repository [F]; package name unverified) | ◐ `:httpd`(inets) | — | pkg: Ktor | — | 2/2 |
| TLS | (inside http only) | — | `crypto/tls` [F] | `ssl` | ◐ `node:tls` | — | ◐ `:ssl` | — | ◐ JVM | ◐ Network.framework(Apple) | 2/4 |
| WebSocket | — | — | — | — | `WebSocket` (client in Node 22 onward, Deno, Bun) | — | — | — | pkg: Ktor | ◐ `URLSessionWebSocketTask` | 1/1 |
| Mail / SMTP | — | — | `net/mail`, `net/smtp` [F] | `email`, `smtplib` | — | — | — | — | — | — | 2/0 |

<a id="1k-テスト"></a>

### 1.K Testing

| Category | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | Count |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Test frameworks / assertions | `testing` + language `test` blocks | `#[test]`, `assert!` (language+std) | `testing` [F] | `unittest`, `doctest` | ◐ `node:test`, `Deno.test`, `@std/assert`, `bun:test` | — (gleeunit is external) | ExUnit (bundled) | — | pkg: `kotlin.test` [F] | XCTest / swift-testing (toolchain-bundled) | 5/1 |
| Benchmarking | CLI `almide bench` | (unstable `#[bench]`) | `testing.B` | `timeit` | ◐ `Deno.bench` | — | — | — | — | — | 2/1 |
| Property testing | — | — | `testing/quick` [F] | — | — | — | — | — | — | — | 1/0 |

---

<a id="2-何言語が標準に持つかの集計almide-に無いものだけ多い順"></a>

## 2. Counts of languages providing each category as standard (only categories absent from Almide, descending)

● = unconditionally standard; ◐ = conditional (see 0.1). Items with “● + ◐ ≥ 3” are placed above.

| Category absent from Almide | ● | ◐ | Total | Notes |
|---|---:|---:|---:|---|
| Lazy sequences / iterators | 8 | 0 | 8 | Gleam also has an official pkg (gleam_yielder) |
| UTF-16 / Latin-1 etc. encodings | 6 | 2 | 8 | |
| String builders | 8 | 0 | 8 | Only JS lacks one |
| Formatting printf | 5 | 3 | 8 | |
| Locks / atomics | 7 | 1 | 8 | |
| Threads (as a module) | 6 | 2 | 8 | Almide parallelizes with language feature `fan` |
| Deques / queues | 4 | 3 | 7 | Swift has an official pkg |
| UDP | 3 | 4 | 7 | |
| DNS resolution (explicit) | 3 | 4 | 7 | |
| IP address types | 3 | 4 | 7 | |
| Channels / messages | 6 | 1 | 7 | |
| CSPRNG | 4 | 2 | 6 | Gleam and Swift(pkg) also have official pkgs |
| Seeded randomness | 4 | 2 | 6 | |
| Heaps / priority queues | 4 | 2 | 6 | Swift has an official pkg |
| Time zones (IANA) | 4 | 2 | 6 | Kotlin has an official pkg |
| Arbitrary-precision integers | 4 | 2 | 6 | |
| Binary serialization | 5 | 2 | 7 | |
| XML | 3 | 3 | 6 | |
| Broader noncryptographic hashes (CRC64, maphash etc.) | — | — | — | Almide has only fnv1a32 and crc32 |
| MD5 / SHA-1 / SHA-512 / SHA-3 | 4 | 3 | 7 | |
| Sorted maps | 2 | 3 | 5 | |
| UUID | 5 | 0 | 5 | Entered Go std in 1.27 [F] |
| Logging | 5 | 0 | 5 | Rust/Swift have official pkgs |
| HMAC / KDF | 3 | 2 | 5 | |
| Cryptography (AES, signatures, TLS) | 3 | 3 | 6 | |
| async / await | 5 | 0 | 5 | Almide's `fan` is a different design |
| Cancellation / deadlines | 5 | 0 | 5 | Almide has `fan.timeout` |
| Terminals / ANSI / TTY | 3 | 2 | 5 | |
| Other compression (bzip2, zstd etc.) | 2 | 3 | 5 | |
| Archives (tar, zip) | 2 | 3 | 5 | |
| Full-featured flag parsing (help generation, typed) | 4 | 1 | 5 | Almide `args` is a minimal reader |
| Decimal / rational numbers | 3 | 1 | 4 | |
| Base32 | 3 | 1 | 4 | |
| Timers | 4 | 1 | 5 | |
| Databases (SQLite etc.) | 2 | 2 | 4 | |
| Insertion-ordered maps (as a distinct type) | 4 | 0 | 4 | Almide's `map` itself is insertion-ordered |
| SIMD | 3 | 0 | 3 | |
| Complex numbers | 3 | 0 | 3 | |
| Template engines | 3 | 0 | 3 | |
| Locales / internationalized formatting | 3 | 1 | 4 | |
| CSV | 2 | 1 | 3 | |
| MIME types | 2 | 1 | 3 | |
| Ascii85 / Base58 | 2 | 1 | 3 | |
| File watching | 0 | 3 | 3 | All conditional |
| 128 bit integers | 2 | 0 | 2 | |
| TOML | 1 | 1 | 2 | |
| INI / dotenv | 1 | 2 | 3 | |
| semver | 1 | 1 | 2 | |
| WebSocket | 1 | 1 | 2 | |
| Mail / SMTP | 2 | 0 | 2 | |
| Statistics | 1 | 0 | 1 | |
| YAML | 0 | 1 | 1 | Deno/Bun only |
| Bit sets | 0 | 1 | 1 | Swift has an official pkg |
| Multisets Counter | 1 | 0 | 1 | |

Things present in Almide but rare in other languages' standard libraries:

| Almide module | Languages with it as standard ●/◐ | Fact |
|---|---|---|
| `matrix` | 0/2 | None of the 9 languages has standard matrix operations. OCaml `Bigarray` is only a container; Swift `simd` is Apple-only |
| `compute` | 0/0 | No standard library has deterministic computation time as a value type |
| `html` (SafeHtml type) | 2/1 | Escaping functions in Go/Python/Deno/Bun. Only Go `html/template`'s `template.HTML` type is close to the design of a distinct type for safe HTML |
| `hex` as a separate module | — | Only Go `encoding/hex` is a separate module. Most place it alongside base64 |
| `zlib` | 3/3 | Absent from Rust, Gleam and OCaml |
| `testing` (assertion collection) | 5/1 | |

---

<a id="3-almide-の分け方まとめ方が多数派と違って見える点--9-言語の事実"></a>

## 3. Where Almide's grouping and separation appear different from the majority — facts from 9 languages

<a id="31-固定幅整数を-8-モジュールに分けているint8uint64"></a>

### 3.1 Fixed-width integers split into 8 modules (int8–uint64)

Almide: one module per type (`int8` 20 functions … `uint64` 26 functions). Contents are width conversions (`to_<型>`, `to_<型>_checked`, `to_<型>_saturating`), `to_string`, `min_value`/`max_value`. Additionally, `int` contains `to_int8`, `to_uint8_checked`, `from_uint64`, etc., and `int.to_u8`, `int.to_u32`, which do not return those types (return type is Int).

| Language | Organizational facts |
|---|---|
| Rust | `i8`–`u128` are primitive types. Methods belong directly to types (`i8::MAX`, `checked_add`, `saturating_sub`, `wrapping_mul`, `to_le_bytes`). Three conversion families: `as` (truncation / float→int saturates), `From` (lossless), `TryFrom` (fallible). Old modules such as `std::i8` are deprecated [F: no i8 module in std list] |
| Go | Types are language built-ins. Functions are in purpose-specific packages: constants `math.MaxInt8`, bit operations `math/bits`, string conversion `strconv.FormatInt(int64(x),10)`, bytes `encoding/binary`. No per-type packages. Conversion is `int8(x)` (truncation only) |
| Python | No fixed-width integer types (int is arbitrary precision). Width is represented only by type codes in `struct`, `array`, and `ctypes.c_int8` |
| TS/JS | Number (f64) and BigInt. Width through typed arrays such as `Int8Array`, `BigInt.asIntN(n, x)`, `x | 0`, `x >>> 0` |
| Gleam | Int only. Width through bit-array `<<x:size(8)>>` |
| Elixir | Arbitrary-precision integers only. Width through `<<x::signed-8>>` |
| OCaml | Four modules: `Int` (63 bit), `Int32`, `Int64`, `Nativeint` [F]. No 8/16 bit types (only reading with `Bytes.get_int8`, `get_int16_le`) |
| Kotlin | Conversion members on `Byte`–`ULong` (`toByte()`, `toUInt()`). Truncation only; no standard checked/saturating functions (handwritten with `coerceIn`) |
| Swift | `Int8`–`UInt64` (+ `Int128`) structs. Three initializer conversions: `init(truncatingIfNeeded:)` (truncation), `init(clamping:)` (saturation), `init(exactly:)` (nil on failure). Closest to Almide's “triple” |

<a id="32-float32--float64-モジュールと-float"></a>

### 3.2 `float32` / `float64` modules and `float`

Almide: `float` (46 functions) is the default. `float32`, `float64` each have 10 width-conversion functions.

| Language | Fact |
|---|---|
| Rust | `f32`, `f64` are primitives (methods on types). `std::f32`, `std::f64` modules contain only constants (`consts::PI`) [F] |
| Go | `float32`, `float64` types. `math` supports only float64. `math.Float32bits` etc. |
| Python / Gleam / Elixir / OCaml | Only one floating-point kind, 64 bit. 32 bit is only an array/byte representation |
| JS | Number (64 bit). `Math.fround`, `Float32Array`, `Float16Array` (2025) [F] |
| Kotlin | Members on `Float`, `Double` types |
| Swift | Members on `Float`, `Double`, `Float16`, `Float80` types |

→ Of the 9 languages, 0 have a “module” per type. They attach methods to types (Rust, Kotlin, Swift), or use purpose-specific packages (Go). Almide float32/float64 support UFCS such as `x.to_int64()`, so usage is closer to the “methods on types” approach.

<a id="33-compute-と-durationと-datetime-env-の時間関数"></a>

### 3.3 `compute` and `duration` (and time functions in `datetime`, `env`)

Almide: `duration` (real time, 6 functions), `compute` (deterministic computation time, 6 functions), `datetime` (Int timestamps, 22 functions, `now` and `monotonic_ns`), `env.unix_timestamp`, `env.millis`, `env.sleep_ms`, `process.sleep`.

| Language | Where time lives |
|---|---|
| Rust | One `std::time` module has `Duration`, `Instant`, `SystemTime`. Sleep is `std::thread::sleep` |
| Go | One `time` package has Duration, Time, Sleep, Timer, Location (tz) |
| Python | `time` (clocks/sleep), `datetime` (calendar/timedelta), `zoneinfo` (tz), `calendar` |
| JS | `Date`, `performance.now`, `setTimeout`, and new `Temporal` (Duration, Instant, ZonedDateTime …, planned 2027 edition) [F] |
| Gleam | No time in stdlib. Official pkg `gleam_time` (duration, timestamp, calendar) [F] |
| Elixir | `Duration`, `Date`, `Time`, `DateTime`, `NaiveDateTime`, `Calendar` (separate modules) [F] + `System.monotonic_time`, `Process.sleep` |
| OCaml | The only Stdlib clock is `Sys.time` (CPU time). Real time is in bundled `Unix`. Core has `Time_ns` |
| Kotlin | `kotlin.time` has `Duration`, `TimeSource`, `Instant`, `Clock` [F]. Calendar and tz are in pkg kotlinx-datetime |
| Swift | Stdlib has `Duration`, the `Clock` protocol (`ContinuousClock`, `SuspendingClock`). Calendar features are Foundation `Date`, `Calendar`, `TimeZone` |

Facts:
- No language separates a “computation-cost clock” from real time by type. Nearby features: BEAM reduction counts (Elixir `Process.info(pid, :reductions)`), CPU time (Python `time.process_time`, OCaml `Sys.time`). All are values distinct from Duration, not budget types.
- Swift's `Clock` protocol has “a distinct Instant type for each clock, shared Duration.” Almide's quantity type itself differs by clock (`Compute` and `Duration` cannot be mixed).
- The majority (Rust, Go, Kotlin) group “elapsed time, time of day, sleep” in one module. Almide duplicates current time in `datetime` and `env`, sleep in `env` and `process`.

<a id="34-引数-args--envargs--process"></a>

### 3.4 Arguments: `args` / `env.args` / `process`

Almide: `env.args()` (argv[1..]), `args.raw()` (the README's one-line description says “same as env.args, excludes argv[0]”; however, args.md's body says “includes argv[0],” so the descriptions disagree), `args.flag/option/positional`.

| Language | Raw argv | Flag parsing |
|---|---|---|
| Rust | `std::env::args` | None (clap is external) |
| Go | `os.Args` | `flag` |
| Python | `sys.argv` | `argparse` (+ old `getopt`, `optparse`) |
| JS | `process.argv`, `Deno.args`, `Bun.argv` | `util.parseArgs`(Node), `@std/cli`(Deno) |
| Gleam | None (external `argv`) | None |
| Elixir | `System.argv` | `OptionParser` (+ OTP `argparse`) |
| OCaml | `Sys.argv` | `Arg` (Core has `Command`) |
| Kotlin | Argument to `main(args)` | None (kotlinx-cli not in the official API list [F]) |
| Swift | `CommandLine.arguments` | Official pkg swift-argument-parser |

→ Raw argv lives on the “environment/OS” side (Rust `env`, Go `os`, Python `sys`, Elixir `System`, OCaml `Sys`). Flag parsing uses another module (Go `flag`, Python `argparse`, Elixir `OptionParser`, OCaml `Arg`). This two-level organization matches Almide `env.args` + `args`. The difference is that Almide `args` also has a raw-argv function (`raw`), duplicating it.

<a id="35-value-と-json"></a>

### 3.5 `value` and `json`

Almide: `value` (20 data-model functions: construction/extraction/`stringify`/`pick`/`omit`/camel-snake key-name conversion), `json` (`parse`, `stringify`, `get_*`, `JsonPath`). `value.stringify` and `json.stringify` overlap.

| Language | Fact |
|---|---|
| Go | `encoding/json` parses into `any` (map[string]any etc.). No dedicated value type |
| Python | `json` parses into dict/list/str. No dedicated value type |
| JS | `JSON` parses into ordinary objects |
| Gleam | Stdlib `gleam/dynamic` + `gleam/dynamic/decode` (decode untyped values into typed ones) [F]. JSON itself is official pkg `gleam_json`. Construction uses gleam_json's `json.object` etc. |
| Elixir | `JSON` (since 1.18) parses into map/list. Values are ordinary terms |
| Kotlin | Pkg kotlinx.serialization: `JsonElement` (`JsonObject`, `JsonArray`, `JsonPrimitive`) is a JSON-specific value type |
| Swift | `JSONDecoder` decodes directly into types (Codable). Untyped form uses `JSONSerialization` (Any) |
| Rust / OCaml | Absent from standard library (serde_json, yojson are external) |

→ Only Gleam (`dynamic`) puts a “format-independent dynamic value type” in its standard library. But Gleam dynamic is read-only, with constructors in the JSON package. Kotlin (pkg) has a JSON-specific value type.

<a id="36-hex-と-base64-を別モジュールにしている"></a>

### 3.6 `hex` and `base64` as separate modules

| Language | Location |
|---|---|
| Go | Separate packages under `encoding/`: `encoding/hex`, `base32`, `base64`, `ascii85`, `pem`, `csv`, `json`, `xml`, `binary` [F] |
| Python | One `base64` module (b16, b32, b64, b85, a85) + `binascii` + `bytes.hex()` |
| JS | Methods of `Uint8Array`: `toBase64/fromBase64/toHex/fromHex` (2026) [F]. Deno uses one `@std/encoding` package [F] |
| Gleam | Both are `gleam/bit_array` functions (`base16_encode`, `base64_encode`, `base64_url_encode`) |
| Elixir | One `Base` module (encode16/32/64) |
| Kotlin | Base64 is `kotlin.io.encoding.Base64`; hex is `kotlin.text.HexFormat` (separate locations) |
| Swift | Base64 is a Foundation `Data` method. Hex is absent from standard library |
| Rust / OCaml | Neither is in the standard library |

→ Five languages put “hex and base64 in one place (`Base`, `base64`, `@std/encoding`, `bit_array`).” Go separates them under a common parent. Kotlin separates them completely.

<a id="37-hashfnv-と-sha-256-を同居と-zlibcrc32"></a>

### 3.7 `hash` (FNV and SHA-256 together) and `zlib.crc32`

| Language | Noncryptographic and cryptographic hash locations |
|---|---|
| Go | Noncryptographic in `hash/*` (fnv, crc32, crc64, adler32, maphash); cryptographic in `crypto/*` (sha256 etc.). Shared interface `hash.Hash` [F] |
| Python | Cryptographic in `hashlib`; CRC/Adler in `zlib` and `binascii`; `hash()` is built-in |
| JS | Cryptographic in `crypto.subtle.digest`; no standard noncryptographic hash (only Bun.hash) |
| Elixir | `:crypto.hash`(OTP), `:erlang.phash2`, `:erlang.crc32` |
| OCaml | `Digest` (MD5, BLAKE2), `Hashtbl.hash` |
| Rust | `std::hash` has only the Hasher trait and SipHash. No cryptography |

→ Almide's “FNV and SHA-256 in the same `hash`, CRC32 in `zlib`” resembles Python's organization (crc32 in zlib), unlike Go's separate noncryptographic/cryptographic trees.

<a id="38-html"></a>

### 3.8 `html`

Standard escaping is provided by Go `html` (+ `html/template`), Python `html`, Deno `@std/html`, Bun `escapeHTML`. A “distinct type for escaped strings” is Go `html/template`'s `template.HTML` type (a mark of trust). Not standard in Rust, Gleam, Elixir (Phoenix.HTML is external), OCaml, Kotlin (kotlinx.html is pkg), Swift.

<a id="39-matrix"></a>

### 3.9 `matrix`

None of the 9 languages has standard matrix operations (table in section 2). Numerical computing is external (numpy, ndarray, gonum, Nx, Owl, multik, parts of swift-numerics). At epoch 14, Almide moved 25 NN functions to the `almide-graphics/nn` package (opening of matrix.md).

<a id="310-env-に入っているもの"></a>

### 3.10 Contents of `env`

Almide `env`: unix_timestamp, args, get, set, cwd, millis, sleep_ms, temp_dir, os (9 functions).

| Language | Contents of the “env” equivalent |
|---|---|
| Rust `std::env` | args, vars, var, set_var, current_dir, temp_dir, consts (OS name). No time or sleep |
| Go `os` | Args, Getenv, Setenv, Getwd, TempDir, Exit, file operations |
| Python `os` / `sys` | environ, getcwd / argv, exit, platform |
| Elixir `System` | argv, get_env, cwd, tmp_dir, os_time, monotonic_time, cmd, halt (also time) |
| OCaml `Sys` | argv, getenv, getcwd, command, time (CPU), os_type |

→ Rust `env` and Almide `env` have almost the same contents, but Rust puts time/sleep in `time`/`thread`. Elixir `System` and OCaml `Sys` include time.

<a id="311-iostdinstdoutと-println"></a>

### 3.11 `io` (stdin/stdout) and `println`

All 9 languages have standard I/O. Gleam `gleam/io` is output-only with no function to read stdin [F]. Almide has two families: `io.write_bytes(List[Int])` and `io.write(Bytes)`.

<a id="312-nettcp--unix-ソケット--共有メモリ"></a>

### 3.12 `net` (TCP + Unix sockets + shared memory)

Rust (`std::os::unix::net` is a separate module) and Go (`net` groups TCP/UDP/Unix, but has no shared memory) are the standard-library cases placing “Unix domain sockets and shared memory” in the same module as TCP. Python `multiprocessing.shared_memory` and `mmap` provide standard shared memory (shm). None of the 9 languages combines Unix sockets and shm without UDP.

<a id="313-almide-内部の重複事実のみ"></a>

### 3.13 Internal Almide duplication (facts only)

| Feature | Locations in Almide |
|---|---|
| argv | `env.args`, `args.raw` |
| Current time | `datetime.now`, `env.unix_timestamp`, `env.millis` |
| sleep | `env.sleep_ms` (hangs on negative), `process.sleep` (negative becomes 0) |
| Temporary directory | `env.temp_dir`, `fs.temp_dir` |
| JSON stringification | `value.stringify`, `json.stringify` |
| URL decoding | `url.decode_component`, `http.url_decode` |
| Truncation to 32/8 bit | `int.to_u32`/`int.to_u8` (return Int), `int.to_uint32`/`int.to_uint8` (return UInt types) |
| hex | `hex.encode` (Bytes), `int.to_hex` (Int) |
| CRC32 | `zlib.crc32` (not `hash`) |

---

<a id="4-almide-に無いカテゴリが-wasmwasi-でホスト機能を要るか"></a>

## 4. Do categories absent from Almide require host features in WASM/WASI?

Official WASI interfaces (Phase 3 = included in p2/p3): `wasi:cli` (arguments, environment variables, stdin/stdout/stderr, exit, terminal detection), `wasi:clocks` (monotonic-clock, wall-clock/system-clock), `wasi:filesystem`, `wasi:sockets` (tcp, udp, ip-name-lookup), `wasi:random` (random, insecure, insecure-seed), `wasi:http` (incoming/outgoing handler). Phase 2: clocks `timezone`. Phase 1: `wasi-crypto`, `wasi-tls`, `wasi-kv-store`, `wasi-native-threads` [F]. Process creation and signals are absent from WASI.

| Absent category | Requirement | WASI situation |
|---|---|---|
| Lazy sequences / iterators | Pure computation | Not needed |
| String builders | Pure computation | Not needed |
| printf / text layout | Pure computation | Not needed |
| UTF-16 / Latin-1 encodings | Pure computation | Not needed |
| Deques, heaps, sorted maps, bit sets, Counter | Pure computation | Not needed |
| Arbitrary-precision integers, decimal, complex numbers, statistics | Pure computation | Not needed |
| Base32 / Ascii85 / Base58 | Pure computation | Not needed |
| CSV / TOML / YAML / XML / INI | Pure computation (read/write strings) | Not needed |
| Binary serialization (cbor, msgpack) | Pure computation | Not needed |
| MIME-type tables | Pure computation (embedded table) | Not needed |
| UUID v4 | Randomness | `wasi:random` |
| UUID v7 / ULID | Randomness + current time | `wasi:random` + `wasi:clocks` wall-clock |
| MD5 / SHA-1 / SHA-512 / SHA-3 / HMAC / KDF | Pure computation | Not needed (`wasi-crypto` is Phase 1 and unavailable) |
| AES / signatures | Pure computation (key randomness via `wasi:random`) | Not needed (constant-time implementation remains a challenge) |
| CSPRNG | Randomness | `wasi:random/random` (explicitly cryptographic) |
| Seeded randomness | Pure computation | Not needed (initial seed could also use `insecure-seed`) |
| Other compression (bzip2, zstd, lzma), tar/zip | Pure computation | Not needed |
| Time zones (IANA) | tz data (pure if embedded). Getting local tz requires host | Embedded: unnecessary. Local tz: `wasi:clocks/timezone` (Phase 2, not yet stable) |
| Timers (execute later) | Clock + executor | `wasi:clocks` monotonic-clock `subscribe` / p3 async |
| Logging | Output to stderr / files | `wasi:cli` stderr, `wasi:filesystem` (a new logging-specific interface, wasi-logging, is in the Phase 1 family, unverified) |
| Terminals / ANSI / TTY | Coloring is pure; TTY detection requires host | `wasi:cli/terminal-stdout` etc. |
| Full-featured flag parsing | Arguments (already `env.args`) | `wasi:cli/environment` |
| Signals | Host | Absent from WASI |
| File watching | Host | Absent from WASI |
| Databases (SQLite) | Files | `wasi:filesystem` (if SQLite is bundled into wasm) |
| UDP | Sockets | `wasi:sockets/udp` |
| DNS resolution | Sockets | `wasi:sockets/ip-name-lookup` |
| IP address types | Pure computation | Not needed |
| TLS | Sockets + cryptography | `wasi-tls` is Phase 1. Through `wasi:http`, the host handles https |
| WebSocket | Sockets | `wasi:sockets/tcp` (+ TLS) |
| Mail / SMTP | Sockets | `wasi:sockets/tcp` |
| Threads / locks / atomics / channels | Executor | `wasi-native-threads` is Phase 1. p3 async (stream/future) is cooperative concurrency. No shared-memory threads |
| async / cancellation | Executor | Represented through p3 async (Almide already has it through `fan`) |
| 128 bit integers / SIMD | Pure computation | Not needed (SIMD uses wasm simd128 instructions) |
| Matrices (already present) | Pure computation | Not needed |

---

<a id="5-注記確かめきれなかったこと"></a>

## 5. Notes and items not fully verified

- Swift standard-library body pages could not be retrieved because they are JavaScript-rendered. Swift cells (`Duration` 5.7, `Regex` 5.7, `Int128` 6.0, `Synchronization` module, `SystemRandomNumberGenerator` being a CSPRNG) are from memory.
- OCaml `Digest` BLAKE2 support (since 5.2) could not be verified from retrieval of the Pqueue page (memory).
- Stabilization dates of Kotlin `HexFormat` (kotlin.text) and `Base64` were not verified (existence checked in the kotlin.io.encoding package list [F]).
- Official Gleam HTTP-server package: existence of the `httpd` repository in the gleam-lang org was checked [F]; package name and purpose are unverified.
- Elixir version numbers “`JSON` since 1.18” and “`Duration` since 1.17” are from memory. Existence checked in the 1.20 module list [F].
- Go `uuid` package: presence in std and v4/v7 generation were checked [F]. The version added (estimated 1.27) was not verified.
- Python 3.15's module index retains removed modules (cgi, imp etc.) as historical records. They are not used in the tables.
- Counts depend strongly on the scope choices in 0.2. In particular, ◐ counts vary with JS (including Deno/Bun/Node?), Elixir (including OTP?), and Kotlin (including JVM?).
