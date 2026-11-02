<a id="l1-事実集-9-言語の標準ライブラリ--カテゴリモジュール単位の比較"></a>

# L1事实集：9种语言的标准库——按类别（模块）比较

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](categories.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](categories.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](categories.zh-CN.md)

> 非规范性固定原文历史调查读者版。固定正本不变，不自动同步，未测试API或独立重新核验来源。10月21日调查日期、版本、F标记及记忆单元格描述原作者证据，不是新增核验。计划版本与WASI阶段仍是计划/阶段，不是新的当前可用性断言。保留原文分歧：Go UUID统计称1.27已核验，但末尾称该版本为未核验推测；不擅改计数和排序，包括CSPRNG的Swift标准/pkg说法、OCaml模块列举与float模块计数、第3.12节归组主张及相反的括号说明。安全HTML指原文类型/可信标记，不是新增安全保证。函数数、负值sleep行为与API归组属于历史，未独立测试当前行为。转换占位符中的型意为类型。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)

调查日期：2026-10-21。目的是“收集事实”。此处不写决策或提案。

<a id="0-読み方"></a>

## 0. 阅读方式

<a id="01-記号"></a>

### 0.1 符号

| 符号 | 含义 |
|---|---|
| ● | 包含在标准库中（所有平台，或语言的公共部分） |
| ◐ | 有条件地包含：仅JVM的JDK（Kotlin）、仅Apple OS（Swift）、Erlang/OTP侧模块（Elixir）、编译器随附的独立unix/str/threads库（OCaml）、仅Deno std / Bun / Node（JS）等。单元格中说明原因 |
| pkg: | 不是标准库。只有语言团队的官方包 |
| — | 标准库中没有 |
| [F] | 本次会话中通过WebFetch获取官方文档核验的单元格 |

“数量（●/◐）”列=9种语言中●的个数/◐的个数。不计pkg。
Almide列来自2026-10时的`docs/stdlib/README.md`（44个模块、999个函数）。

<a id="02-標準の範囲言語ごと"></a>

### 0.2 “标准”的范围（各语言）

| 语言 | 计为标准的范围 | 不计入的内容（仅pkg或注记） |
|---|---|---|
| Rust | `std` crate（stable）。unstable另作注记 | rust-lang组织的crate（regex、log）算pkg |
| Go | `pkg.go.dev/std`的全部包 | `golang.org/x/...` |
| Python | 标准库（3.15模块索引） | PyPI |
| TS/JS | ECMAScript内置+各运行时共有Web API（fetch、URL、crypto.subtle等）算●。仅Node内置/Deno `@std`/Bun内置提供的算◐ | npm |
| Gleam | 仅`gleam_stdlib` | gleam-lang组织的官方包（gleam_json等）算pkg |
| Elixir | Elixir自身模块（+随附应用Logger / ExUnit / EEx）算●。Erlang/OTP（stdlib、kernel、crypto、inets、ssl…）无需添加依赖即可由Elixir调用，因此算◐(OTP) | Hex包 |
| OCaml | `Stdlib`算●。编译器随附unix / str / threads算◐ | Jane Street的Base/Core仅作注记（不计数） |
| Kotlin | `kotlin-stdlib`算●（Common或全目标）。JVM专用stdlib包和JDK类算◐(JVM) | kotlinx.* / Ktor算pkg |
| Swift | Swift标准库+工具链随附Foundation（swift-foundation，Linux/Windows也随附）算●。Apple OS专用框架算◐(Apple) | swift-collections等算pkg |

<a id="03-参照した公式ドキュメントの根ルートurl"></a>

### 0.3 所参阅官方文档的根URL

| 语言 | 根URL | 核验版本 |
|---|---|---|
| Rust | https://doc.rust-lang.org/std/ [F] | 1.99.1 |
| Go | https://pkg.go.dev/std [F] | go1.27.3 |
| Python | https://docs.python.org/3/library/ ・ https://docs.python.org/3/py-modindex.html [F] | 3.15.0 |
| TS/JS | ECMAScript: https://tc39.es/ecma262/ ・ 已完成提案：https://github.com/tc39/proposals/blob/main/finished-proposals.md [F] ・ MDN: https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects ・ Node: https://nodejs.org/api/ [F] ・ Deno std: https://jsr.io/@std [F] ・ Bun: https://bun.com/docs/runtime/utils [F] | Node 26.12、Deno std 43个包 |
| Gleam | https://gleam-stdlib.hexdocs.pm/ （旧 https://hexdocs.pm/gleam_stdlib/ ）[F] ・ 官方组织：https://github.com/gleam-lang [F] | gleam_stdlib 1.0.5 |
| Elixir | https://hexdocs.pm/elixir/api-reference.html [F] ・ Erlang stdlib: https://www.erlang.org/doc/apps/stdlib/api-reference.html [F] | Elixir 1.20.4 / OTP 29.1.2 |
| OCaml | https://ocaml.org/manual/latest/api/index.html [F] ・ Core: https://ocaml.org/p/core/latest | OCaml 5.5 |
| Kotlin | https://kotlinlang.org/api/core/kotlin-stdlib/ [F] ・ 官方库：https://kotlinlang.org/docs/api-references.html [F] | 2.4 |
| Swift | https://developer.apple.com/documentation/swift/swift-standard-library （未能获取正文）・ https://github.com/swiftlang/swift-foundation [F] ・ swift-collections: https://swiftpackageindex.com/apple/swift-collections/documentation [F] | swift-collections 1.7.3 |
| WASI | https://github.com/WebAssembly/WASI/blob/main/docs/Proposals.md [F] ・ https://github.com/WebAssembly/wasi-clocks [F] | — |

**通过获取内容核验的事项（[F]的含义）**
- Rust std模块列表（stable / unstable。`random`、`f16`、`f128`为unstable）。
- Go std全部包列表。std中有`uuid`（`uuid.NewV4`、`NewV7`、`Parse`）（https://pkg.go.dev/uuid ）。也有`simd`。
- Python 3.15模块索引（包含`compression`、`tomllib`、`zoneinfo`、`uuid`）。
- TC39已完成提案的年份：Temporal=2027版、Uint8Array base64/hex=2026、Iterator helpers=2025、Set methods=2025、Upsert(Map.getOrInsert)=2026、Math.sumPrecise=2026。
- Deno `@std`的43个包列表。`@std/encoding`包含hex / base32 / base58 / base64 / base64url / ascii85 / varint。`@std/data-structures`含BinaryHeap、BinarySearchTree、(Readonly)Deque等。
- Node 26的API列表（SQLite、Zlib、UDP/datagram、TTY、Test runner、Web Crypto、FFI等）。
- Bun内置：gzip/deflate/zstd、escapeHTML、randomUUIDv7、stringWidth/stripANSI、TOML/YAML读取。
- gleam_stdlib 1.0.5只有19个模块：bit_array、bool、bytes_tree、dict、dynamic、dynamic/decode、float、function、int、io、list、option、order、pair、result、set、string、string_tree、uri。regex和iterator已移出stdlib，成为官方包`gleam_regexp`、`gleam_yielder`。gleam-lang组织中有json、crypto、time、http、httpc、erlang、otp。
- Elixir 1.20模块列表（含`JSON`、`Duration`、`OptionParser`、`IO.ANSI`、`Base`、`URI`、`Version`、`Regex`、`Calendar.TimeZoneDatabase`）。
- Erlang/OTP 29 stdlib列表（含`argparse`、`json`、`queue`、`gb_trees`、`gb_sets`、`base64`、`uri_string`、`calendar`、`rand`、`zip`、`erl_tar`、`zstd`、`io_ansi`、`digraph`）。
- OCaml 5.5 Stdlib列表（含`Dynarray`、`Pqueue`（5.4起）、`Iarray`、`Pair`、`Repr`、`Domain`、`Effect`、`Atomic`、`Arg`、`Buffer`、`Digest`、`Seq`）。外部：unix、str、threads。
- Kotlin stdlib 2.4的包和支持平台（`kotlin.uuid`与`kotlin.io.encoding`为Common+JVM，`kotlin.time`为全目标，`Instant`/`Clock`从2.3起提供，`kotlin.system`仅JVM+Native）。官方kotlinx列表：coroutines、serialization、kotlinx-io、kotlinx-datetime、collections.immutable、Ktor。
- swift-collections模块：DequeModule、HeapModule、OrderedCollections、BitCollections、HashTreeCollections等。
- swift-foundation随Linux/Windows工具链附带（FoundationEssentials / FoundationInternationalization）。
- WASI：Phase 3=cli、clocks、filesystem、sockets、random、http。Phase 2=clocks的timezone。Phase 1=crypto、tls、kv-store、native-threads。

未标[F]的单元格依据记忆（众所周知的API）。不确定项在脚注中标为“未核验”。

---

<a id="1-カテゴリ比較表"></a>

## 1. 类别比较表

列顺序：Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift

<a id="1a-数値と基本型"></a>

### 1.A 数值与基本类型

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 ●/◐ |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 整数（默认Int） | `int` | `i64`等primitive的固有方法 | builtin `int` + `strconv`, `math` | `int` | `Number` / `BigInt` | `gleam/int` [F] | `Integer` [F] | `Int` [F] | `Int`/`Long`（kotlin） | `Int` | 9/0 |
| 固定位宽整数8–64 bit | `int8` `int16` `int32` `int64` `uint8` `uint16` `uint32` `uint64`（8个模块） | `i8`–`u64` primitive（各类型的方法；`std::i8`等旧模块已弃用） | builtin类型`int8`–`uint64` + `math/bits`, `strconv`, `encoding/binary` | —（仅通过`struct`, `array`, `ctypes`表示） | ◐ 类型化数组`Int8Array`–`BigUint64Array`, `BigInt.asIntN/asUintN` | —（仅Int，只能指定bit array位宽） | —（任意精度整数；通过`<<x::8>>`指定位宽） | ◐ `Int32` `Int64` `Nativeint` [F]（没有8/16 bit类型） | `Byte` `Short` `Int` `Long` `UByte` `UShort` `UInt` `ULong` | `Int8`–`UInt64`结构体 | 4/2 |
| 128 bit整数 | — | `i128`/`u128` | — | （int为任意精度） | — | — | （任意精度） | — | — | `Int128`/`UInt128`（Swift 6） | 2/0 |
| 浮点数（默认） | `float` | `f64` | `float64` + `math` | `float` | `Number` | `gleam/float` [F] | `Float` [F] | `Float` [F] | `Double` | `Double` | 9/0 |
| 32 bit浮点数 | `float32`, `float64`模块 | `f32`, `f64`（常量模块+primitive）[F] | `float32` | —（仅`struct`/`array`的'f'） | ◐ `Float32Array`, `Math.fround`, `Float16Array`(2025) | — | —（bitstring `::float-32`） | —（仅`Int32.float_of_bits`） | `Float` | `Float`, `Float16`, `Float80` | 4/1 |
| 任意精度整数BigInt | — | — | `math/big` (Int) [F] | `int` | `BigInt` | ◐ Int（Erlang目标为任意精度，JS不同） | `Integer`（任意精度） | —（Zarith为外部库） | ◐ `java.math.BigInteger`(JVM) | — | 4/2 |
| 十进制小数/有理数 | — | — | `math/big` (Rat, Float) | `decimal`, `fractions` [F] | — | — | —（Decimal是Hex包） | — | ◐ `java.math.BigDecimal`(JVM) | `Decimal`(Foundation) | 3/1 |
| 复数 | — | — | builtin `complex128` + `math/cmplx` [F] | `complex` + `cmath` [F] | — | — | — | `Complex` [F] | — | pkg: swift-numerics | 3/0 |
| 数学函数 | `math` | `f64`方法 | `math` | `math` | `Math` | ◐ 仅`gleam/float`的一部分（没有三角函数） | ◐ `:math`(OTP) | `Float` | `kotlin.math` [F] | Foundation（经Glibc/Darwin）+ pkg: swift-numerics | 6/2 |
| 统计 | — | — | — | `statistics` | — | — | — | — | — | — | 1/0 |
| 位运算辅助（popcount等） | `int`的一部分 | `count_ones`等整数方法 | `math/bits` | `int.bit_count` | `Math.clz32` | `gleam/int` bitwise_* | `Bitwise` [F] | `Int`的land/lor | `Int.countOneBits`等 | `nonzeroBitCount`等 | 9/0 |
| SIMD | — | `arch`（stable）, `simd`（unstable）[F] | `simd`, `simd/archsimd` [F] | — | — | — | — | — | — | `SIMD2`–`SIMD64`类型 | 3/0 |
| 矩阵/线性代数 | `matrix`（46个函数） | — | — | —（仅有`@`运算符，没有实现） | —（`DOMMatrix`仅浏览器） | — | —（Nx为外部库） | ◐ `Bigarray`（多维数组容器，无运算） | — | ◐ `simd`框架（Apple） | 0/2 |

<a id="1b-文字列とテキスト"></a>

### 1.B 字符串与文本

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 字符串 | `string` | `str`, `string` | `strings`, `strconv` | `str`, `string` | `String` | `gleam/string` [F] | `String` [F] | `String` [F] | `kotlin.text` | `String` | 9/0 |
| 字符类型/Unicode分类 | `string.is_digit`等、`codepoint`（没有字符类型模块） | `char` [F] | `unicode`, `unicode/utf8` [F] | `unicodedata` | `String.prototype.codePointAt`, `Intl.Segmenter` | `string.to_utf_codepoints`, `to_graphemes` | `String.graphemes`, `:unicode`(OTP) | `Char`, `Uchar` [F] | `Char`（`category`） | `Character`, `Unicode.Scalar` | 9/0 |
| UTF-16 / Latin-1等编码 | — | `String::from_utf16`, `encode_utf16` | `unicode/utf16` [F] | `codecs` | `TextDecoder`（utf-16le等）, Node `Buffer` latin1 | — | ◐ `:unicode.characters_to_binary`(OTP) | `Bytes.get_utf_16le_uchar`等 | ◐ `java.nio.charset`(JVM)（Common仅UTF-8） | `String(decoding:as: UTF16.self)`, `String.Encoding` | 6/2 |
| 字符串构建器 | —（`+`和`list.join`） | `String`（可变） | `strings.Builder`, `bytes.Buffer` | `io.StringIO` | —（惯用数组join） | `gleam/string_tree` [F] | iodata（`IO.iodata_to_binary`） | `Buffer` [F] | `StringBuilder` | `String`（可变） | 8/0 |
| 格式化printf | 字符串插值、`string.pad_*` | `fmt`（`format!`） | `fmt` | `str.format`, f-string, `string.templatelib` | 模板字面量, ◐ Deno `@std/fmt` printf | — | `inspect`, ◐ `:io_lib.format` | `Printf`, `Format` [F] | ◐ `String.format`(JVM) | ◐ `String(format:)`(Foundation) | 5/3 |
| 文本排版（折行/差异/宽度） | — | — | `text/tabwriter` [F] | `textwrap`, `difflib` | ◐ Deno `@std/text`, Bun `stringWidth`/`wrapAnsi` | — | — | — | — | — | 2/1 |
| 正则表达式 | `regex` | pkg: `regex`（rust-lang） | `regexp` [F] | `re` | `RegExp` | pkg: `gleam_regexp` [F] | `Regex` [F] | ◐ `Str`（随附库）[F] | `Regex` | `Regex`（5.7起）, `NSRegularExpression` | 6/1 |
| 模板引擎 | —（`html`仅安全拼接） | — | `text/template`, `html/template` [F] | `string.Template` | — | — | EEx（随附应用） | — | — | — | 3/0 |
| 区域设置/国际化格式 | — | — | — | `locale`, `gettext` | `Intl` | — | — | — | ◐ JVM | `Locale`, FormatStyle（FoundationInternationalization） | 3/1 |

<a id="1c-コレクション"></a>

### 1.C 集合容器

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 列表/动态数组 | `list` | `Vec`, `vec`, `slice` | slice + `slices` | `list` | `Array` | `gleam/list`（链表）[F] | `List`, `Enum` | `List`, `Array`, `Dynarray` [F] | `List`/`ArrayList` | `Array` | 9/0 |
| 哈希映射 | `map`（保留插入顺序） | `HashMap` | builtin map + `maps` | `dict`（插入顺序） | `Map`（插入顺序） | `gleam/dict` [F] | `Map` | `Hashtbl` | `Map`（`mapOf`为插入顺序） | `Dictionary` | 9/0 |
| 插入顺序映射 | `map` | — | — | `dict` | `Map` | — | `Keyword`（列表） | — | `LinkedHashMap`（默认） | pkg: `OrderedDictionary` [F] | 4/0 |
| 排序映射（树） | — | `BTreeMap`, `BTreeSet` | — | — | ◐ Deno `RedBlackTree`/`BinarySearchTree` [F] | — | ◐ `:gb_trees`, `:gb_sets`(OTP) [F] | `Map`, `Set`（平衡树、有序） | ◐ `sortedMapOf`→`TreeMap`(JVM) | — | 2/3 |
| 集合 | `set`（插入顺序） | `HashSet` | —（`map[T]struct{}`） | `set` | `Set` + 2025集合运算 | `gleam/set` [F] | `MapSet` | `Set` | `Set` | `Set` | 8/0 |
| 双端队列/队列 | — | `VecDeque` | ◐ `container/list`（双向链表）[F] | `collections.deque`, `queue` | ◐ Deno `@std/data-structures` Deque [F] | —（旧`gleam/queue`不在stdlib中[F]） | ◐ `:queue`(OTP) [F] | `Queue`, `Stack` | `ArrayDeque` | pkg: `Deque` [F] | 4/3 |
| 堆/优先队列 | — | `BinaryHeap` | `container/heap` [F] | `heapq` | ◐ Deno `BinaryHeap` [F] | — | —（用`gb_sets`代替） | `Pqueue`（5.4起）[F] | ◐ `java.util.PriorityQueue`(JVM) | pkg: `Heap` [F] | 4/2 |
| 链表 | （`list`基于数组） | `LinkedList` | `container/list`, `container/ring` | — | — | `gleam/list` | `List` | `List` | ◐ `LinkedList`(JVM) | — | 5/1 |
| 位集合 | — | — | — | — | — | — | — | — | ◐ `java.util.BitSet`(JVM) | pkg: `BitSet` [F] | 0/1 |
| 多重集合/计数器 | — | — | — | `collections.Counter` | — | — | — | — | — | — | 1/0 |
| 惰性序列/迭代器 | —（list为立即求值，仅`fs.fold_lines`系列逐次处理） | `iter` [F] | `iter`（1.23起）[F] | `itertools`, generator | Iterator helpers（ES2025）[F] | pkg: `gleam_yielder`（原`gleam/iterator`）[F] | `Stream` [F] | `Seq` [F] | `Sequence`（`kotlin.sequences`） | `Sequence`, `LazySequence` | 8/0 |
| 范围Range | 语法`0..n` | `ops::Range` | `for range n` | `range` | — | `list.range` | `Range` [F] | — | `kotlin.ranges` | `Range` | 7/0 |
| 比较/排序 | — | `cmp` | `cmp` | `functools.cmp_to_key` | — | `gleam/order` [F] | — | `compare` | `kotlin.comparisons` | `Comparable` | 7/0 |
| 元组/对 | 语法 | 元组 | — | 元组 | — | `gleam/pair` [F] | `Tuple` | `Pair`（5.5）[F] | `Pair`, `Triple` | 元组 | 7/0 |
| Option / Result | `option`, `result` | `option`, `result` | —（多值返回） | —（异常） | — | `gleam/option`, `gleam/result` [F] | —（`{:ok, v}`惯例） | `Option`, `Result`, `Either` [F] | `T?`, `Result` | `Optional`, `Result` | 6/0 |
| 错误 | `error` | `error` | `errors` [F] | 异常（builtins） | `Error` | —（Result） | `Exception` | `Printexc` | `Throwable` | `Error` | 8/0 |

<a id="1d-バイト列符号化データ形式"></a>

### 1.D 字节序列、编码与数据格式

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 字节序列 | `bytes`（146个函数，含LE/BE读写） | `Vec<u8>`, `slice` | `bytes`, `encoding/binary` | `bytes`, `bytearray`, `struct` | `Uint8Array`, `DataView`, ◐ Node `Buffer` | `gleam/bit_array`, `bytes_tree` [F] | binary, ◐ `:binary` | `Bytes` | `ByteArray` | `[UInt8]`, `Data`(Foundation) | 9/0 |
| Hex | `hex`（3个函数）+ `int.to_hex` | — | `encoding/hex` [F] | `bytes.hex()`, `binascii` | `Uint8Array.toHex/fromHex`（2026）[F], ◐ Deno `@std/encoding` | `bit_array.base16_encode` | `Base.encode16` [F] | —（仅`Digest.to_hex`） | `HexFormat`, `toHexString`（kotlin.text） | — | 7/0 |
| Base64 | `base64`（4个函数） | — | `encoding/base64` [F] | `base64` | `btoa/atob`, `Uint8Array.toBase64`（2026）[F] | `bit_array.base64_encode` | `Base` [F] | — | `kotlin.io.encoding.Base64`（Common+JVM）[F] | `Data.base64EncodedString`(Foundation) | 7/0 |
| Base32 | — | — | `encoding/base32` [F] | `base64.b32encode` | ◐ Deno `@std/encoding` [F] | — | `Base.encode32` | — | — | — | 3/1 |
| Ascii85 / Base85 / Base58 | — | — | `encoding/ascii85` [F] | `base64.b85encode`, `a85encode` | ◐ Deno（ascii85, base58）[F] | — | — | — | — | — | 2/1 |
| JSON | `json`（15个函数） | — | `encoding/json` [F] | `json` | `JSON` | pkg: `gleam_json` [F] | `JSON`（1.18起）[F], ◐ `:json`(OTP 27起)[F] | — | pkg: kotlinx.serialization-json [F] | `JSONEncoder`/`JSONDecoder`(Foundation) | 5/0 |
| 动态值（JSON式数据模型） | `value`（20个函数） | — | `any` + `reflect` | dict/list | object | `gleam/dynamic`, `dynamic/decode` [F]（只读解码器） | 项（term）本身 | — | pkg: `JsonElement` | `Any` + `JSONSerialization` | 6/0 |
| CSV | — | — | `encoding/csv` [F] | `csv` | ◐ Deno `@std/csv` [F] | — | — | — | — | — | 2/1 |
| TOML | — | — | — | `tomllib`（仅读取）[F] | ◐ Deno `@std/toml`, Bun [F] | — | — | — | — | — | 1/1 |
| YAML | — | — | — | — | ◐ Deno `@std/yaml`, Bun [F] | — | — | — | — | — | 0/1 |
| XML | — | — | `encoding/xml` [F] | `xml.*` | ◐ Deno `@std/xml` [F]（`DOMParser`属浏览器） | — | ◐ `:xmerl`(OTP) | — | ◐ `javax.xml`(JVM) | `XMLParser`(Foundation) | 3/3 |
| INI / dotenv | — | — | — | `configparser` | ◐ Deno `@std/ini`, `@std/dotenv` [F] | — | — | — | — | ◐ `PropertyListEncoder`(plist) | 1/2 |
| 二进制序列化（gob/pickle/cbor等） | — | — | `encoding/gob` [F] | `pickle`, `marshal` | `structuredClone`, ◐ Deno `@std/cbor`, `@std/msgpack` [F] | — | ◐ `:erlang.term_to_binary` | `Marshal` [F] | pkg: serialization-cbor/protobuf [F] | `PropertyListEncoder` | 5/2 |
| URL | `url`（6个函数）+ `http.url_decode` | — | `net/url` [F] | `urllib.parse` | `URL`, `URLSearchParams` | `gleam/uri` [F] | `URI` [F] | — | ◐ `java.net.URI`(JVM) | `URL`, `URLComponents`(Foundation) | 6/1 |
| HTML转义 | `html`（`SafeHtml`类型） | — | `html`, `html/template` [F] | `html` | ◐ Deno `@std/html`, `Bun.escapeHTML` [F] | — | — | — | pkg: kotlinx.html（构建器，未核验） | — | 2/1 |
| MIME类型 | — | — | `mime`, `mime/multipart` [F] | `mimetypes`, `email` | ◐ Deno `@std/media-types` [F] | — | — | — | — | — | 2/1 |
| UUID | — | — | `uuid`（v4/v7）[F] | `uuid` [F] | `crypto.randomUUID()`, ◐ Deno `@std/uuid`, `Bun.randomUUIDv7` [F] | — | — | —（Core_unix有Uuid） | `kotlin.uuid.Uuid`（Common+JVM）[F] | `UUID`(Foundation) | 5/0 |
| ULID | — | — | — | — | ◐ Deno `@std/ulid` [F] | — | — | — | — | — | 0/1 |
| 版本（semver） | — | — | — | — | ◐ Deno `@std/semver`, `Bun.semver` [F] | — | `Version` [F] | — | — | — | 1/1 |

<a id="1e-ハッシュ暗号乱数"></a>

### 1.E 哈希、密码学与随机数

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 非密码哈希（FNV, CRC32, Adler） | `hash.fnv1a32`, `zlib.crc32` | `hash`（SipHash的`DefaultHasher`） | `hash/fnv`, `crc32`, `crc64`, `adler32`, `maphash` [F] | `zlib.crc32`, `adler32`, `hash()` | ◐ `Bun.hash`（ECMAScript没有） | — | ◐ `:erlang.phash2`, `:erlang.crc32` | `Hashtbl.hash` | `hashCode`, ◐ `java.util.zip.CRC32`(JVM) | `Hasher` | 6/2 |
| SHA-256 | `hash.sha256` | — | `crypto/sha256` [F] | `hashlib` | `crypto.subtle.digest` | pkg: `gleam_crypto` [F] | ◐ `:crypto`(OTP) | —（`Digest`有MD5和BLAKE2，没有SHA；BLAKE2从5.2起，未核验） | ◐ `MessageDigest`(JVM) | ◐ CryptoKit(Apple) / pkg: swift-crypto | 3/3 |
| MD5 / SHA-1 / SHA-512 / SHA-3 | — | — | `crypto/md5`, `sha1`, `sha512`, `sha3` [F] | `hashlib`（md5, sha1, sha2, sha3, blake2） | `crypto.subtle`（SHA-1/384/512）, ◐ `node:crypto`, Deno `@std/crypto` | pkg: `gleam_crypto` | ◐ `:crypto` | `Digest`（MD5） | ◐ JVM | ◐ CryptoKit / pkg | 4/3 |
| HMAC / KDF | — | — | `crypto/hmac`, `hkdf`, `pbkdf2` [F] | `hmac`, `hashlib.pbkdf2_hmac` | `crypto.subtle`（HMAC, PBKDF2, HKDF） | pkg: `gleam_crypto` | ◐ `:crypto.mac` | — | ◐ `javax.crypto.Mac`(JVM) | ◐ CryptoKit / pkg | 3/2 |
| 密码功能（AES、签名、TLS） | — | — | `crypto/aes`, `ed25519`, `tls`, `mlkem`等[F] | `ssl`（没有密码原语） | `crypto.subtle` | — | ◐ `:crypto`, `:ssl` | — | ◐ JVM | ◐ CryptoKit / pkg | 3/3 |
| 伪随机数 | `random`（4个函数） | —（`random`为unstable [F]） | `math/rand/v2` [F] | `random` | `Math.random` | `int.random`, `float.random` | `Enum.random`, ◐ `:rand` [F] | `Random` [F] | `kotlin.random` [F] | `Int.random(in:)` | 8/0 |
| 带种子（可复现）随机数 | — | — | `rand.New(rand.NewPCG(..))` | `random.Random(seed)` | ◐ Deno `@std/random`（seeded） | — | ◐ `:rand.seed` | `Random.State` | `Random(seed)` | —（stdlib没有带种子生成器） | 4/2 |
| 密码安全随机数（CSPRNG） | — | — | `crypto/rand` [F] | `secrets` | `crypto.getRandomValues` | pkg: `gleam_crypto` | ◐ `:crypto.strong_rand_bytes` | — | ◐ `SecureRandom`(JVM) | `SystemRandomNumberGenerator`（CSPRNG） | 4/2 |

<a id="1f-圧縮アーカイブ"></a>

### 1.F 压缩与归档

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| DEFLATE / zlib / gzip | `zlib` | — | `compress/flate`, `gzip`, `zlib` [F] | `zlib`, `gzip` | `CompressionStream`（gzip/deflate/deflate-raw）, ◐ `node:zlib`, `Bun.gzipSync` [F] | — | ◐ `:zlib`(erts) | — | ◐ `java.util.zip`(JVM) | ◐ Compression(Apple) | 3/3 |
| 其他压缩（bzip2, lzma, zstd, brotli, lzw） | — | — | `compress/bzip2`（仅解压）, `lzw` [F] | `bz2`, `lzma`, `compression.zstd` [F] | ◐ `node:zlib`（brotli, zstd）, `Bun.zstd*` [F] | — | ◐ `:zstd`(OTP 29) [F] | — | — | ◐ Compression(Apple) | 2/3 |
| 归档（tar, zip） | — | — | `archive/tar`, `archive/zip` [F] | `tarfile`, `zipfile` | ◐ Deno `@std/tar` [F] | — | ◐ `:zip`, `:erl_tar`(OTP) [F] | — | ◐ `java.util.zip`(JVM) | — | 2/3 |

<a id="1g-時間"></a>

### 1.G 时间

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 经过时间类型Duration（实际时间） | `duration`（6个函数，仅构造器和运算） | `time::Duration` | `time.Duration` | `datetime.timedelta` | `Temporal.Duration`（计划ES2027 [F]） | pkg: `gleam_time`（`gleam/time/duration`）[F] | `Duration`（1.17起）[F] | —（Core: `Time_ns.Span`） | `kotlin.time.Duration` [F] | `Duration`（5.7起） | 7/0 |
| 确定性计算时间（Almide `compute`） | `compute` | — | — | —（`time.process_time`为CPU时间） | — | — | —（最接近的是BEAM的reductions计数，不是值类型） | —（`Sys.time`为CPU时间） | — | — | 0/0 |
| 单调时钟/秒表 | `datetime.monotonic_ns` | `time::Instant` | `time.Now`（含monotonic）, `time.Since` | `time.monotonic`, `perf_counter` | `performance.now()` | pkg: `gleam_time` | `System.monotonic_time` | ◐ `Unix.gettimeofday`（非单调；Stdlib无单调时钟） | `TimeSource.Monotonic`, `measureTime` [F] | `ContinuousClock`, `SuspendingClock` | 7/1 |
| 当前时间（Unix时间） | `datetime.now`, `env.unix_timestamp`, `env.millis` | `SystemTime` | `time.Now` | `time.time` | `Date.now` | pkg: `gleam_time` | `System.os_time`, `DateTime.utc_now` | ◐ `Unix.time` | `kotlin.time.Clock.System`（2.3起）[F] | `Date()`(Foundation) | 7/1 |
| 日历日期/时间 | `datetime`（Int时间戳+分量提取） | — | `time.Time` | `datetime`, `calendar` | `Date`, `Temporal.PlainDate*`（2027） | pkg: `gleam_time`（calendar） | `Date`, `Time`, `NaiveDateTime`, `DateTime`, `Calendar` [F] | ◐ `Unix.gmtime/localtime` | pkg: kotlinx-datetime [F] | `Calendar`, `DateComponents`(Foundation) | 5/1 |
| 时区（IANA） | —（仅UTC和偏移） | — | `time.LoadLocation`, `time/tzdata` [F] | `zoneinfo` [F] | `Intl.DateTimeFormat`(timeZone), `Temporal.ZonedDateTime` | — | ◐ `DateTime` + `Calendar.TimeZoneDatabase`（默认仅UTC [F]，tz数据来自外部） | — | pkg: kotlinx-datetime `TimeZone`, ◐ `java.time.ZoneId`(JVM) | `TimeZone`(Foundation) | 4/2 |
| 日期时间格式化/解析 | `datetime.format`, `parse_iso`, `to_iso` | — | `time.Format`, `time.Parse` | `strftime`, `fromisoformat` | `toISOString`, `Intl` | pkg | `Calendar.strftime`, `DateTime.from_iso8601` | — | `Instant.parse`（ISO）[F] | `ISO8601DateFormatter`, FormatStyle | 6/0 |
| 休眠 | `env.sleep_ms`, `process.sleep` | `thread::sleep` | `time.Sleep` | `time.sleep` | `setTimeout`, ◐ `Bun.sleep` | —（pkg: gleam_erlang `process.sleep`） | `Process.sleep` | ◐ `Unix.sleepf` | ◐ `Thread.sleep`(JVM), pkg: coroutines `delay` | `Task.sleep` | 6/2 |
| 定时器（稍后执行） | — | — | `time.AfterFunc`, `time.Ticker` | `threading.Timer`, `sched` | `setTimeout`, `setInterval` | — | `:timer`(OTP), `Process.send_after` | — | pkg: coroutines | `Task` + `Clock.sleep` | 4/1 |

<a id="1h-osファイルプロセス"></a>

### 1.H OS、文件与进程

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 文件系统 | `fs`（34个函数） | `fs` [F] | `os`, `io/fs` [F] | `os`, `shutil`, `pathlib` | ◐ `node:fs`, `Deno.*`, `@std/fs`, `Bun.file` | —（simplifile为外部库） | `File` [F] | `Sys`, `In_channel`, `Out_channel` [F], ◐ `Unix` | ◐ `kotlin.io`（JVM的`File`）, pkg: kotlinx-io | `FileManager`(Foundation) | 6/2 |
| 路径操作 | `path` | `path` [F] | `path`, `path/filepath` [F] | `os.path`, `pathlib` | ◐ `node:path`, `@std/path` | — | `Path` [F] | `Filename` [F] | ◐ `kotlin.io.path`(JVM)[F] | `URL`(file), pkg: swift-system `FilePath` | 6/2 |
| 临时文件 | `fs.temp_dir`, `create_temp_file`, `create_temp_dir`, `env.temp_dir` | `env::temp_dir`（仅目录名） | `os.CreateTemp`, `os.MkdirTemp` | `tempfile` | ◐ `Deno.makeTempFile`, `fs.mkdtemp` | — | `System.tmp_dir` | `Filename.temp_file` | ◐ `createTempFile`(JVM) | `FileManager.temporaryDirectory` | 6/2 |
| glob | `fs.glob` | — | `filepath.Glob`, `fs.Glob` | `glob`, `fnmatch` | ◐ `fs.glob`(Node 22起), `Bun.Glob`, Deno `expandGlob` | — | `Path.wildcard` | — | ◐ `PathMatcher`(JVM) | — | 3/2 |
| 文件监视 | — | — | — | — | ◐ `fs.watch`, `Deno.watchFs` | — | — | — | ◐ `WatchService`(JVM) | ◐ FSEvents(Apple) | 0/3 |
| 标准输入输出 | `io`, `println`, `eprintln` | `io` | `os.Stdin`, `bufio`, `fmt` | `sys.stdin`, `print`, `input` | `console`, ◐ `process.stdin`, `Deno.stdin` | `gleam/io`（仅输出，不读取stdin）[F] | `IO` [F] | `In_channel`, `Out_channel`, `print_*` | `println`, `readln` | `print`, `readLine` | 8/1 |
| 终端/ANSI颜色/TTY | — | `io::IsTerminal` | —（x/term为外部库） | `curses`, `tty`, `termios`, `os.isatty` | ◐ `node:tty`, `util.styleText`, `@std/fmt/colors`, `Bun.stripANSI` | — | `IO.ANSI` [F], ◐ `io_ansi`(OTP)[F] | ◐ `Unix.isatty` | — | — | 3/2 |
| 环境变量 | `env.get`, `env.set` | `env::var` | `os.Getenv` | `os.environ` | `process.env`, `Deno.env` | — | `System.get_env` | `Sys.getenv` | ◐ `System.getenv`(JVM)（Common没有） | `ProcessInfo.environment` | 7/1 |
| 命令行参数（原始） | `env.args`, `args.raw` | `env::args` | `os.Args` | `sys.argv` | `process.argv`, `Deno.args`, `Bun.argv` | — | `System.argv` | `Sys.argv` | `main(args)` | `CommandLine.arguments` | 8/0 |
| 标志解析 | `args`（6个函数：flag、option、positional） | — | `flag` [F] | `argparse`, `getopt`, `optparse` [F] | ◐ `util.parseArgs`(Node), `@std/cli` [F] | — | `OptionParser` [F], ◐ `argparse`(OTP)[F] | `Arg` [F] | —（官方API列表无kotlinx-cli [F]） | pkg: swift-argument-parser | 4/1 |
| 工作目录 | `env.cwd` | `env::current_dir` | `os.Getwd` | `os.getcwd` | `process.cwd()` | — | `File.cwd` | `Sys.getcwd` | ◐ JVM | `FileManager.currentDirectoryPath` | 7/1 |
| 子进程执行 | `process`（17个函数） | `process` [F] | `os/exec` [F] | `subprocess` | ◐ `child_process`, `Deno.Command`, `Bun.spawn` | — | `System.cmd`, `Port` [F] | `Sys.command`, ◐ `Unix.create_process` | ◐ `ProcessBuilder`(JVM) | `Process`(Foundation) | 6/3 |
| 退出码exit | `process.exit` | `process::exit` | `os.Exit` | `sys.exit` | `process.exit` | — | `System.halt` | `exit` | `exitProcess`（JVM+Native [F]） | `exit` | 8/0 |
| 信号 | `process.kill`（仅发送） | — | `os/signal` [F] | `signal` | ◐ `process.on('SIGINT')`, `Deno.addSignalListener` | — | `System.trap_signal` | `Sys.signal` | — | ◐ Dispatch `DispatchSource` | 4/2 |
| OS信息/用户 | `env.os` | `env::consts` | `runtime.GOOS`, `os/user` [F] | `platform`, `os`, `getpass`, `pwd` | ◐ `node:os` | — | `:os.type`, `System` | `Sys.os_type` | ◐ JVM | `ProcessInfo` | 5/2 |
| 日志 | — | pkg: `log`（rust-lang） | `log`, `log/slog` [F] | `logging` | `console.*`, ◐ Deno `@std/log` [F] | — | `Logger`（随附应用） | — | — | pkg: swift-log, ◐ `os.Logger`(Apple) | 5/0 |
| 数据库（SQLite等） | — | — | `database/sql`（仅连接接口）[F] | `sqlite3` | ◐ `node:sqlite` [F], `bun:sqlite` | — | ◐ `:ets`, `:dets`, mnesia(OTP) | — | — | — | 2/2 |

<a id="1i-並行非同期"></a>

### 1.I 并发与异步

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 线程/并行执行 | 语言功能`fan`（非stdlib模块） | `thread` [F] | goroutine（语言）+ `runtime` | `threading`, `multiprocessing`, `concurrent.futures` | ◐ `worker_threads`, Web Worker | —（pkg: gleam_otp, gleam_erlang） | `Task`, `Process`, `Agent` [F] | `Domain` [F], ◐ `Thread` | ◐ `kotlin.concurrent`(JVM+Native)[F], pkg: coroutines | `Task`（Concurrency）, `Thread`(Foundation) | 6/2 |
| 通道/消息 | — | `sync::mpsc` | `chan`（语言） | `queue.Queue`, `asyncio.Queue` | `MessageChannel` | —（pkg: gleam_erlang `Subject`） | `send`/`receive`, `GenServer` | ◐ `Event`(threads) | pkg: coroutines `Channel` | `AsyncStream` | 6/1 |
| 锁/原子操作 | — | `sync`（Mutex, Arc, Atomic*） | `sync`, `sync/atomic` | `threading.Lock` | `Atomics`, `SharedArrayBuffer` | — | ◐ `:atomics`(OTP) | `Mutex`, `Atomic`, `Condition`, `Semaphore` [F] | `kotlin.concurrent.atomics` [F] | `Synchronization`（`Atomic`, `Mutex`） | 7/1 |
| async / await与执行器 | — | `future`, `task`（无执行器） | — | `asyncio` | `Promise`、async函数 | — | `Task.async/await` | `Effect`（无执行器） | 语言`suspend` + pkg: coroutines | `async`/`await`, `Task`, actor | 5/0 |
| 取消/截止时间 | `fan.timeout`（语言） | — | `context` [F] | `asyncio.timeout` | `AbortController`, `AbortSignal.timeout` | — | `Task.yield/shutdown` | — | pkg: coroutines `withTimeout` | `Task.cancel` | 5/0 |

<a id="1j-ネットワーク"></a>

### 1.J 网络

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| TCP | `net.tcp_*` | `net::TcpStream`, `TcpListener` [F] | `net` [F] | `socket`, `socketserver` | ◐ `node:net`, `Deno.connect`, `Bun.connect` | — | ◐ `:gen_tcp`(OTP kernel) | ◐ `Unix`套接字 | ◐ `java.net.Socket`(JVM), pkg: ktor-network | —（pkg: swift-nio） | 3/4 |
| UDP | — | `net::UdpSocket` | `net` | `socket` | ◐ `node:dgram` [F], Deno | — | ◐ `:gen_udp` | ◐ `Unix` | ◐ `DatagramSocket`(JVM) | — | 3/4 |
| Unix域套接字 | `net.unix_*`, `net.shm_*` | `os::unix::net` | `net` | `socket`(AF_UNIX) | ◐ `node:net`(path) | — | ◐ `:gen_tcp` `{:local, path}` | ◐ `Unix` | ◐ JVM 16起 | — | 3/4 |
| DNS名称解析 | —（connect中隐式执行） | `ToSocketAddrs` | `net.LookupHost` | `socket.getaddrinfo` | ◐ `node:dns`, `Deno.resolveDns` | — | ◐ `:inet.gethostbyname` | ◐ `Unix.getaddrinfo` | ◐ `InetAddress`(JVM) | — | 3/4 |
| IP地址类型 | — | `net::IpAddr` | `net/netip` [F] | `ipaddress` | ◐ `net.isIP`, `BlockList` | — | ◐ `:inet.parse_address` | ◐ `Unix.inet_addr_of_string` | ◐ JVM | — | 3/4 |
| HTTP客户端 | `http.get`等 | — | `net/http` [F] | `urllib.request`, `http.client` | `fetch` | pkg: `gleam_httpc`, `gleam_fetch`（类型在`gleam_http`）[F] | ◐ `:httpc`(OTP inets) | — | ◐ `java.net.http`(JVM), pkg: Ktor | `URLSession`(Foundation) | 4/2 |
| HTTP服务器 | `http.serve` | — | `net/http` | `http.server`（开发用途） | ◐ `node:http`, `Deno.serve`, `Bun.serve`, `@std/http` | pkg: `gleam_httpd`?（组织中有`httpd`仓库[F]，包名未核验） | ◐ `:httpd`(inets) | — | pkg: Ktor | — | 2/2 |
| TLS | （仅http内部） | — | `crypto/tls` [F] | `ssl` | ◐ `node:tls` | — | ◐ `:ssl` | — | ◐ JVM | ◐ Network.framework(Apple) | 2/4 |
| WebSocket | — | — | — | — | `WebSocket`（Node 22起的客户端、Deno、Bun） | — | — | — | pkg: Ktor | ◐ `URLSessionWebSocketTask` | 1/1 |
| 邮件/SMTP | — | — | `net/mail`, `net/smtp` [F] | `email`, `smtplib` | — | — | — | — | — | — | 2/0 |

<a id="1k-テスト"></a>

### 1.K 测试

| 类别 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift | 数量 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 测试框架/断言 | `testing` + 语言`test`块 | `#[test]`, `assert!`（语言+std） | `testing` [F] | `unittest`, `doctest` | ◐ `node:test`, `Deno.test`, `@std/assert`, `bun:test` | —（gleeunit为外部库） | ExUnit（随附） | — | pkg: `kotlin.test` [F] | XCTest / swift-testing（工具链随附） | 5/1 |
| 基准测试 | CLI `almide bench` | （unstable `#[bench]`） | `testing.B` | `timeit` | ◐ `Deno.bench` | — | — | — | — | — | 2/1 |
| 属性测试 | — | — | `testing/quick` [F] | — | — | — | — | — | — | — | 1/0 |

---

<a id="2-何言語が標準に持つかの集計almide-に無いものだけ多い順"></a>

## 2. “多少种语言在标准库中提供”的统计（仅Almide没有的类别，按数量降序）

●=无条件属于标准，◐=有条件（见0.1）。将“● + ◐ ≥ 3”的项目排在前面。

| Almide没有的类别 | ● | ◐ | 合计 | 备注 |
|---|---:|---:|---:|---|
| 惰性序列/迭代器 | 8 | 0 | 8 | Gleam也有官方pkg（gleam_yielder） |
| UTF-16 / Latin-1等编码 | 6 | 2 | 8 | |
| 字符串构建器 | 8 | 0 | 8 | 仅JS没有 |
| 格式化printf | 5 | 3 | 8 | |
| 锁/原子操作 | 7 | 1 | 8 | |
| 线程（作为模块） | 6 | 2 | 8 | Almide通过语言功能`fan`并行 |
| 双端队列/队列 | 4 | 3 | 7 | Swift有官方pkg |
| UDP | 3 | 4 | 7 | |
| DNS名称解析（显式） | 3 | 4 | 7 | |
| IP地址类型 | 3 | 4 | 7 | |
| 通道/消息 | 6 | 1 | 7 | |
| CSPRNG | 4 | 2 | 6 | Gleam、Swift(pkg)也有官方pkg |
| 带种子随机数 | 4 | 2 | 6 | |
| 堆/优先队列 | 4 | 2 | 6 | Swift有官方pkg |
| 时区（IANA） | 4 | 2 | 6 | Kotlin有官方pkg |
| 任意精度整数 | 4 | 2 | 6 | |
| 二进制序列化 | 5 | 2 | 7 | |
| XML | 3 | 3 | 6 | |
| 更丰富的非密码哈希（CRC64、maphash等） | — | — | — | Almide仅有fnv1a32和crc32 |
| MD5 / SHA-1 / SHA-512 / SHA-3 | 4 | 3 | 7 | |
| 排序映射 | 2 | 3 | 5 | |
| UUID | 5 | 0 | 5 | Go在1.27纳入std [F] |
| 日志 | 5 | 0 | 5 | Rust/Swift有官方pkg |
| HMAC / KDF | 3 | 2 | 5 | |
| 密码功能（AES、签名、TLS） | 3 | 3 | 6 | |
| async / await | 5 | 0 | 5 | Almide的`fan`是另一种设计 |
| 取消/截止时间 | 5 | 0 | 5 | Almide有`fan.timeout` |
| 终端/ANSI/TTY | 3 | 2 | 5 | |
| 其他压缩（bzip2、zstd等） | 2 | 3 | 5 | |
| 归档（tar, zip） | 2 | 3 | 5 | |
| 完整标志解析（生成帮助、带类型） | 4 | 1 | 5 | Almide `args`是最小读取器 |
| 十进制小数/有理数 | 3 | 1 | 4 | |
| Base32 | 3 | 1 | 4 | |
| 定时器 | 4 | 1 | 5 | |
| 数据库（SQLite等） | 2 | 2 | 4 | |
| 插入顺序映射（独立类型） | 4 | 0 | 4 | Almide的`map`本身即插入顺序 |
| SIMD | 3 | 0 | 3 | |
| 复数 | 3 | 0 | 3 | |
| 模板引擎 | 3 | 0 | 3 | |
| 区域设置/国际化格式 | 3 | 1 | 4 | |
| CSV | 2 | 1 | 3 | |
| MIME类型 | 2 | 1 | 3 | |
| Ascii85 / Base58 | 2 | 1 | 3 | |
| 文件监视 | 0 | 3 | 3 | 全部有条件 |
| 128 bit整数 | 2 | 0 | 2 | |
| TOML | 1 | 1 | 2 | |
| INI / dotenv | 1 | 2 | 3 | |
| semver | 1 | 1 | 2 | |
| WebSocket | 1 | 1 | 2 | |
| 邮件/SMTP | 2 | 0 | 2 | |
| 统计 | 1 | 0 | 1 | |
| YAML | 0 | 1 | 1 | 仅Deno/Bun |
| 位集合 | 0 | 1 | 1 | Swift有官方pkg |
| 多重集合Counter | 1 | 0 | 1 | |

Almide拥有、其他语言标准库中罕见的内容：

| Almide模块 | 标准库提供的语言数 ●/◐ | 事实 |
|---|---|---|
| `matrix` | 0/2 | 9种语言标准库均无矩阵运算。OCaml `Bigarray`仅为容器，Swift `simd`仅Apple |
| `compute` | 0/0 | 没有标准库将确定性计算时间作为值类型 |
| `html`（SafeHtml类型） | 2/1 | Go/Python/Deno/Bun提供转义函数。接近“用独立类型表示安全HTML”设计的仅Go `html/template`的`template.HTML`类型 |
| `hex`独立模块 | — | 独立模块仅Go `encoding/hex`。大多与base64放在同一处 |
| `zlib` | 3/3 | Rust、Gleam、OCaml没有 |
| `testing`（断言集） | 5/1 | |

---

<a id="3-almide-の分け方まとめ方が多数派と違って見える点--9-言語の事実"></a>

## 3. Almide的“拆分/归组”看起来与多数不同之处——9种语言的事实

<a id="31-固定幅整数を-8-モジュールに分けているint8uint64"></a>

### 3.1 固定位宽整数拆为8个模块（int8–uint64）

Almide：每种类型一个模块（`int8`20个函数…`uint64`26个函数）。内容是位宽转换（`to_<型>`, `to_<型>_checked`, `to_<型>_saturating`）、`to_string`、`min_value`/`max_value`。此外`int`中也有`to_int8`、`to_uint8_checked`、`from_uint64`等，以及不返回相应类型的`int.to_u8`、`int.to_u32`（返回Int）。

| 语言 | 组织事实 |
|---|---|
| Rust | `i8`–`u128`为primitive类型。方法直接附于类型（`i8::MAX`, `checked_add`, `saturating_sub`, `wrapping_mul`, `to_le_bytes`）。转换分3类：`as`（截断/float→int饱和）、`From`（无损）、`TryFrom`（可失败）。`std::i8`等旧模块已弃用[F：std列表不出现i8模块] |
| Go | 类型为语言内置。函数按用途分包：常量`math.MaxInt8`、位操作`math/bits`、转字符串`strconv.FormatInt(int64(x),10)`、转字节`encoding/binary`。没有按类型分包。转换为`int8(x)`（仅截断） |
| Python | 无固定位宽整数类型（int为任意精度）。仅以`struct`、`array`类型码、`ctypes.c_int8`表示位宽 |
| TS/JS | Number（f64）和BigInt。通过`Int8Array`等类型化数组、`BigInt.asIntN(n, x)`、`x | 0`、`x >>> 0`表示位宽 |
| Gleam | 仅Int。位宽通过bit array的`<<x:size(8)>>` |
| Elixir | 仅任意精度整数。位宽通过`<<x::signed-8>>` |
| OCaml | `Int`（63 bit）、`Int32`、`Int64`、`Nativeint`共4个模块[F]。没有8/16 bit（仅用`Bytes.get_int8`、`get_int16_le`读取） |
| Kotlin | `Byte`–`ULong`类型附转换成员（`toByte()`, `toUInt()`）。仅截断，无标准checked/saturating函数（用`coerceIn`手写） |
| Swift | `Int8`–`UInt64`（+ `Int128`）结构体。3种初始化器转换：`init(truncatingIfNeeded:)`（截断）、`init(clamping:)`（饱和）、`init(exactly:)`（失败为nil）。最接近Almide的“三件套” |

<a id="32-float32--float64-モジュールと-float"></a>

### 3.2 `float32` / `float64`模块与`float`

Almide：默认`float`（46个函数）。`float32`、`float64`各有10个位宽转换函数。

| 语言 | 事实 |
|---|---|
| Rust | `f32`、`f64`为primitive（方法附于类型）。`std::f32`、`std::f64`模块仅含常量（`consts::PI`）[F] |
| Go | `float32`、`float64`类型。`math`仅float64。`math.Float32bits`等 |
| Python / Gleam / Elixir / OCaml | 只有64 bit一种浮点数。32 bit仅作为数组或字节表示 |
| JS | Number（64 bit）。`Math.fround`、`Float32Array`、`Float16Array`（2025）[F] |
| Kotlin | `Float`、`Double`类型附成员 |
| Swift | `Float`、`Double`、`Float16`、`Float80`类型附成员 |

→ 9种语言中按类型设置“模块”的为0种。要么给类型附方法（Rust、Kotlin、Swift），要么按用途分包（Go）。Almide的float32/float64通过UFCS可写`x.to_int64()`，因此用法接近“类型附方法”。

<a id="33-compute-と-durationと-datetime-env-の時間関数"></a>

### 3.3 `compute`与`duration`（以及`datetime`、`env`的时间函数）

Almide：`duration`（实际时间、6个函数）、`compute`（确定性计算时间、6个函数）、`datetime`（Int时间戳、22个函数，`now`和`monotonic_ns`）、`env.unix_timestamp`、`env.millis`、`env.sleep_ms`、`process.sleep`。

| 语言 | 时间的归属位置 |
|---|---|
| Rust | `std::time`一个模块包含`Duration`、`Instant`、`SystemTime`。sleep为`std::thread::sleep` |
| Go | `time`一个包包含Duration、Time、Sleep、Timer、Location（tz） |
| Python | `time`（时钟/sleep）、`datetime`（日历/timedelta）、`zoneinfo`（tz）、`calendar` |
| JS | `Date`、`performance.now`、`setTimeout`，以及新的`Temporal`（Duration、Instant、ZonedDateTime…，计划2027版）[F] |
| Gleam | stdlib没有时间。官方pkg `gleam_time`（duration、timestamp、calendar）[F] |
| Elixir | `Duration`、`Date`、`Time`、`DateTime`、`NaiveDateTime`、`Calendar`（独立模块）[F] + `System.monotonic_time`、`Process.sleep` |
| OCaml | Stdlib时钟仅`Sys.time`（CPU时间）。实际时间在随附`Unix`。Core有`Time_ns` |
| Kotlin | `kotlin.time`含`Duration`、`TimeSource`、`Instant`、`Clock` [F]。日历和tz在pkg kotlinx-datetime |
| Swift | stdlib含`Duration`、`Clock`协议（`ContinuousClock`、`SuspendingClock`）。日历在Foundation的`Date`、`Calendar`、`TimeZone` |

事实：
- 没有语言用类型将“计算量时钟”与实际时间分开。接近的有BEAM reductions计数（Elixir `Process.info(pid, :reductions)`）、CPU时间（Python `time.process_time`、OCaml `Sys.time`）。它们都是不同于Duration的值，而非预算类型。
- Swift的`Clock`协议是“每个时钟有不同Instant类型，共享Duration”。Almide则是每个时钟的量类型本身不同（`Compute`与`Duration`不能混用）。
- 多数（Rust、Go、Kotlin）将“经过时间/时刻/sleep”放在同一模块。Almide当前时刻在`datetime`与`env`重复，sleep在`env`与`process`重复。

<a id="34-引数-args--envargs--process"></a>

### 3.4 参数：`args` / `env.args` / `process`

Almide：`env.args()`（argv[1..]）、`args.raw()`（README的一行说明为“与env.args相同，不含argv[0]”，但args.md正文写“包含argv[0]”，描述矛盾）、`args.flag/option/positional`。

| 语言 | 原始argv | 标志解析 |
|---|---|---|
| Rust | `std::env::args` | 无（clap为外部库） |
| Go | `os.Args` | `flag` |
| Python | `sys.argv` | `argparse`（+旧`getopt`、`optparse`） |
| JS | `process.argv`、`Deno.args`、`Bun.argv` | `util.parseArgs`(Node)、`@std/cli`(Deno) |
| Gleam | 无（外部`argv`） | 无 |
| Elixir | `System.argv` | `OptionParser`（+ OTP `argparse`） |
| OCaml | `Sys.argv` | `Arg`（Core为`Command`） |
| Kotlin | `main(args)`的参数 | 无（官方API列表没有kotlinx-cli [F]） |
| Swift | `CommandLine.arguments` | 官方pkg swift-argument-parser |

→ 原始argv归“环境/OS”侧（Rust `env`、Go `os`、Python `sys`、Elixir `System`、OCaml `Sys`）。标志解析用独立模块（Go `flag`、Python `argparse`、Elixir `OptionParser`、OCaml `Arg`）。这种两层组织与Almide的`env.args` + `args`相同。区别是Almide的`args`也有返回原始argv的函数（`raw`），形成重复。

<a id="35-value-と-json"></a>

### 3.5 `value`与`json`

Almide：`value`（数据模型20个函数：构造/提取/`stringify`/`pick`/`omit`/键名camel-snake转换）、`json`（`parse`、`stringify`、`get_*`、`JsonPath`）。`value.stringify`与`json.stringify`重复。

| 语言 | 事实 |
|---|---|
| Go | `encoding/json`解析为`any`（map[string]any等）。没有专用值类型 |
| Python | `json`解析为dict/list/str。没有专用值类型 |
| JS | `JSON`解析为普通对象 |
| Gleam | stdlib `gleam/dynamic` + `gleam/dynamic/decode`（将无类型值解码为带类型值）[F]。JSON本体是官方pkg `gleam_json`。构造值用gleam_json的`json.object`等 |
| Elixir | `JSON`（1.18起）解析为map/list。值为普通项 |
| Kotlin | pkg kotlinx.serialization：`JsonElement`（`JsonObject`、`JsonArray`、`JsonPrimitive`）是JSON专用值类型 |
| Swift | `JSONDecoder`直接解码到类型（Codable）。无类型形式用`JSONSerialization`（Any） |
| Rust / OCaml | 标准库没有（serde_json、yojson为外部库） |

→ 标准库中放置“与格式无关的动态值类型”的只有Gleam（`dynamic`）。但Gleam dynamic为只读，构造器在JSON包一侧。Kotlin（pkg）拥有JSON专用值类型。

<a id="36-hex-と-base64-を別モジュールにしている"></a>

### 3.6 `hex`与`base64`分成独立模块

| 语言 | 所在位置 |
|---|---|
| Go | 独立包，但并列在`encoding/`下：`encoding/hex`、`base32`、`base64`、`ascii85`、`pem`、`csv`、`json`、`xml`、`binary` [F] |
| Python | 一个`base64`模块（b16、b32、b64、b85、a85）+ `binascii` + `bytes.hex()` |
| JS | `Uint8Array`的方法`toBase64/fromBase64/toHex/fromHex`（2026）[F]。Deno为一个`@std/encoding`包[F] |
| Gleam | 两者均为`gleam/bit_array`函数（`base16_encode`、`base64_encode`、`base64_url_encode`） |
| Elixir | 一个`Base`模块（encode16/32/64） |
| Kotlin | Base64为`kotlin.io.encoding.Base64`，hex为`kotlin.text.HexFormat`（不同位置） |
| Swift | Base64是Foundation `Data`的方法。标准库没有hex |
| Rust / OCaml | 两者均不在标准库中 |

→ 将“hex与base64放在同一处（`Base`、`base64`、`@std/encoding`、`bit_array`）”的有5种语言。Go分开但有共同父级。完全分开的是Kotlin。

<a id="37-hashfnv-と-sha-256-を同居と-zlibcrc32"></a>

### 3.7 `hash`（FNV与SHA-256共存）及`zlib.crc32`

| 语言 | 非密码哈希与密码哈希的位置 |
|---|---|
| Go | 非密码在`hash/*`（fnv、crc32、crc64、adler32、maphash），密码在`crypto/*`（sha256等）。共同接口`hash.Hash` [F] |
| Python | 密码在`hashlib`，CRC/Adler在`zlib`和`binascii`，`hash()`为内置 |
| JS | 密码在`crypto.subtle.digest`，标准库没有非密码哈希（只有Bun.hash） |
| Elixir | `:crypto.hash`(OTP)、`:erlang.phash2`、`:erlang.crc32` |
| OCaml | `Digest`（MD5、BLAKE2）、`Hashtbl.hash` |
| Rust | `std::hash`仅Hasher trait和SipHash，无密码功能 |

→ Almide“FNV与SHA-256同处`hash`，CRC32在`zlib`”接近Python的组织（crc32在zlib），与Go非密码/密码分树的组织不同。

<a id="38-html"></a>

### 3.8 `html`

标准提供转义的是Go `html`（+ `html/template`）、Python `html`、Deno `@std/html`、Bun `escapeHTML`。将“已转义字符串作为独立类型”的是Go `html/template`的`template.HTML`类型（可信标记）。Rust、Gleam、Elixir（Phoenix.HTML为外部库）、OCaml、Kotlin（kotlinx.html为pkg）、Swift均无标准提供。

<a id="39-matrix"></a>

### 3.9 `matrix`

9种语言均无标准矩阵运算（第2节表）。数值计算为外部库（numpy、ndarray、gonum、Nx、Owl、multik、swift-numerics的一部分）。Almide在epoch 14将25个NN函数移至`almide-graphics/nn`包（matrix.md开头）。

<a id="310-env-に入っているもの"></a>

### 3.10 `env`中的内容

Almide `env`：unix_timestamp、args、get、set、cwd、millis、sleep_ms、temp_dir、os（9个函数）。

| 语言 | “env”对应部分的内容 |
|---|---|
| Rust `std::env` | args、vars、var、set_var、current_dir、temp_dir、consts（OS名）。无时刻或sleep |
| Go `os` | Args、Getenv、Setenv、Getwd、TempDir、Exit、文件操作 |
| Python `os` / `sys` | environ、getcwd / argv、exit、platform |
| Elixir `System` | argv、get_env、cwd、tmp_dir、os_time、monotonic_time、cmd、halt（也含时刻） |
| OCaml `Sys` | argv、getenv、getcwd、command、time（CPU）、os_type |

→ Rust `env`与Almide `env`内容几乎相同，但Rust将时刻/sleep放在`time`/`thread`。将时刻放在一起的是Elixir `System`和OCaml `Sys`。

<a id="311-iostdinstdoutと-println"></a>

### 3.11 `io`（stdin/stdout）与`println`

全部9种语言都有标准输入输出。Gleam `gleam/io`仅输出，无读取stdin函数[F]。Almide有`io.write_bytes(List[Int])`与`io.write(Bytes)`两套。

<a id="312-nettcp--unix-ソケット--共有メモリ"></a>

### 3.12 `net`（TCP + Unix套接字+共享内存）

标准库中将“Unix域套接字与共享内存”放在与TCP相同模块的是Rust（`std::os::unix::net`为独立模块）和Go（`net`共置TCP/UDP/Unix，但无共享内存）。标准提供共享内存（shm）的有Python `multiprocessing.shared_memory`与`mmap`。9种语言中没有“无UDP却有Unix套接字和shm”的组合。

<a id="313-almide-内部の重複事実のみ"></a>

### 3.13 Almide内部重复（仅事实）

| 功能 | Almide内的位置 |
|---|---|
| argv | `env.args`、`args.raw` |
| 当前时刻 | `datetime.now`、`env.unix_timestamp`、`env.millis` |
| sleep | `env.sleep_ms`（负值hang）、`process.sleep`（负值为0） |
| 临时目录 | `env.temp_dir`、`fs.temp_dir` |
| JSON字符串化 | `value.stringify`、`json.stringify` |
| URL解码 | `url.decode_component`、`http.url_decode` |
| 截断到32/8 bit | `int.to_u32`/`int.to_u8`（返回Int）、`int.to_uint32`/`int.to_uint8`（返回UInt类型） |
| hex | `hex.encode`（Bytes）、`int.to_hex`（Int） |
| CRC32 | `zlib.crc32`（非`hash`） |

---

<a id="4-almide-に無いカテゴリが-wasmwasi-でホスト機能を要るか"></a>

## 4. Almide缺少的类别在WASM/WASI中是否需要宿主功能

WASI官方接口（Phase 3=进入p2/p3的接口）：`wasi:cli`（参数、环境变量、stdin/stdout/stderr、exit、终端判定）、`wasi:clocks`（monotonic-clock、wall-clock/system-clock）、`wasi:filesystem`、`wasi:sockets`（tcp、udp、ip-name-lookup）、`wasi:random`（random、insecure、insecure-seed）、`wasi:http`（incoming/outgoing handler）。Phase 2：clocks的`timezone`。Phase 1：`wasi-crypto`、`wasi-tls`、`wasi-kv-store`、`wasi-native-threads` [F]。WASI没有进程创建和信号。

| 缺少的类别 | 所需内容 | WASI中的情况 |
|---|---|---|
| 惰性序列/迭代器 | 纯计算 | 不需要 |
| 字符串构建器 | 纯计算 | 不需要 |
| printf/文本排版 | 纯计算 | 不需要 |
| UTF-16 / Latin-1编码 | 纯计算 | 不需要 |
| 双端队列、堆、排序映射、位集合、Counter | 纯计算 | 不需要 |
| 任意精度整数、十进制小数、复数、统计 | 纯计算 | 不需要 |
| Base32 / Ascii85 / Base58 | 纯计算 | 不需要 |
| CSV / TOML / YAML / XML / INI | 纯计算（读写字符串） | 不需要 |
| 二进制序列化（cbor, msgpack） | 纯计算 | 不需要 |
| MIME类型表 | 纯计算（嵌入表） | 不需要 |
| UUID v4 | 随机数 | `wasi:random` |
| UUID v7 / ULID | 随机数+当前时刻 | `wasi:random` + `wasi:clocks` wall-clock |
| MD5 / SHA-1 / SHA-512 / SHA-3 / HMAC / KDF | 纯计算 | 不需要（`wasi-crypto`为Phase 1，不可用） |
| AES/签名 | 纯计算（密钥随机数用`wasi:random`） | 不需要（但定时间实现是难点） |
| CSPRNG | 随机数 | `wasi:random/random`（明确用于密码学） |
| 带种子随机数 | 纯计算 | 不需要（初始种子也可使用`insecure-seed`） |
| 其他压缩（bzip2, zstd, lzma）、tar/zip | 纯计算 | 不需要 |
| 时区（IANA） | tz数据（嵌入时为纯计算）；获取本地tz需宿主 | 嵌入：不需要。本地tz：`wasi:clocks/timezone`（Phase 2，尚未稳定） |
| 定时器（稍后执行） | 时钟+执行器 | `wasi:clocks` monotonic-clock的`subscribe`/p3 async |
| 日志 | 输出到stderr/文件 | `wasi:cli` stderr、`wasi:filesystem`（logging专用新接口wasi-logging属于Phase 1系列，未核验） |
| 终端/ANSI/TTY | 着色为纯计算，TTY判定需宿主 | `wasi:cli/terminal-stdout`等 |
| 完整标志解析 | 参数（已有`env.args`） | `wasi:cli/environment` |
| 信号 | 宿主 | WASI没有 |
| 文件监视 | 宿主 | WASI没有 |
| 数据库（SQLite） | 文件 | `wasi:filesystem`（将SQLite随wasm附带时） |
| UDP | 套接字 | `wasi:sockets/udp` |
| DNS名称解析 | 套接字 | `wasi:sockets/ip-name-lookup` |
| IP地址类型 | 纯计算 | 不需要 |
| TLS | 套接字+密码功能 | `wasi-tls`为Phase 1。经`wasi:http`则由宿主处理https |
| WebSocket | 套接字 | `wasi:sockets/tcp`（+ TLS） |
| 邮件/SMTP | 套接字 | `wasi:sockets/tcp` |
| 线程/锁/原子操作/通道 | 执行器 | `wasi-native-threads`为Phase 1。p3 async（stream/future）为协作并发，无共享内存线程 |
| async/取消 | 执行器 | 用p3 async表达（Almide已通过`fan`提供） |
| 128 bit整数/SIMD | 纯计算 | 不需要（SIMD使用wasm simd128指令） |
| 矩阵（已有） | 纯计算 | 不需要 |

---

<a id="5-注記確かめきれなかったこと"></a>

## 5. 注记与未能完全核验的内容

- Swift标准库正文页因JavaScript渲染无法获取。Swift单元格（`Duration` 5.7、`Regex` 5.7、`Int128` 6.0、`Synchronization`模块、`SystemRandomNumberGenerator`为CSPRNG）依据记忆。
- OCaml `Digest`支持BLAKE2（5.2起）无法从获取的Pqueue页面确认（依据记忆）。
- Kotlin `HexFormat`（kotlin.text）、`Base64`稳定化时间未核验（存在性已在kotlin.io.encoding包列表确认[F]）。
- Gleam的HTTP服务器官方包：已确认gleam-lang组织有`httpd`仓库[F]，包名与用途未核验。
- Elixir `JSON`从1.18起、`Duration`从1.17起的版本号依据记忆。存在性已在1.20模块列表确认[F]。
- Go `uuid`包：已确认std中存在且生成v4/v7 [F]。加入版本（推测1.27）未能核验。
- Python 3.15模块索引也保留已删除模块（cgi、imp等）作为旧版本记录。表中未采用。
- “数量”高度依赖0.2的范围定义。尤其JS（是否包含Deno/Bun/Node）、Elixir（是否包含OTP）、Kotlin（是否包含JVM）会使◐增减。
