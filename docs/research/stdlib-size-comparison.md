# Standard Library Size Comparison at 1.0 Release

Research date: 2026-03-18

## Summary Table

| Language | Release | Modules | Functions | Philosophy |
|----------|---------|---------|-----------|------------|
| **Elm** (0.19) | 2018 | 18 | ~200 | Radical minimalism |
| **Gleam** (1.0) | 2024 | 19 | ~180 | Small & cohesive |
| **MoonBit** (pre-1.0) | 2024 | ~49 | ~400 | Growing, experimental |
| **Almide** (current) | -- | 34 | 512 | Targeted batteries |
| **Rust** (1.0) | 2015 | ~50 stable | ~2,000 | Small core + ecosystem |
| **Elixir** (1.0) | 2014 | ~77 | ~800 | Batteries + OTP |
| **Kotlin** (1.0) | 2016 | ~20 core | ~1,500 | JDK extension layer |
| **Swift** (1.0) | 2014 | ~1 (monolith) | ~1,000 | Thin + Foundation |
| **Go** (1.0) | 2012 | ~150 | ~5,000+ | Batteries included |
| **Zig** (pre-1.0) | -- | ~90 | ~3,000+ | Batteries included |
| **Deno** (@std) | 2024 | 43 | ~1,500 | Curated batteries |

## Detailed Breakdown

---

### Elm (0.19, 2018) -- Radical Minimalism

**Modules (18):**
Array, Basics, Bitwise, Char, Debug, Dict, List, Maybe, Platform, Platform.Cmd, Platform.Sub, Process, Result, Set, String, Task, Tuple, Elm.JsArray (internal)

**~200 functions total**

**Included:** Core types (List, Dict, Set, Array), Maybe/Result, String, basic math, platform effects (Cmd/Sub), JSON decoders (separate `elm/json` package), HTTP (separate `elm/http`)

**Excluded:** Everything else. HTTP, JSON, random, time, regex, file I/O, URL parsing -- all live in separate official packages (`elm/json`, `elm/http`, `elm/time`, `elm/regex`, `elm/random`, `elm/url`).

**Philosophy:** The core package contains only what is impossible to implement in userland. Elm treats ~8 official packages as the effective stdlib, but the core itself is deliberately tiny. The Elm Architecture (TEA) pushes side effects out of the stdlib entirely.

**Post-stable regrets:** The 0.18 -> 0.19 transition removed `toString`, `Basics.toString`, and restructured the Json/Http modules. The community debated whether the stdlib was *too* minimal, especially around string interpolation and regex support. The package ecosystem never reached critical mass for some domains, which some attribute to the stdlib being too restrictive.

---

### Gleam (1.0, 2024) -- Small & Cohesive

**Modules (19):**
bit_array, bool, bytes_tree, dict, dynamic, dynamic/decode, float, function, int, io, list, option, order, pair, result, set, string, string_tree, uri

**~180 functions total**

**Included:** Core types (Int, Float, String, List, Dict, Set), Option/Result, BitArray, basic I/O (io.println, io.debug), URI parsing, dynamic type checking (for FFI with Erlang/JS)

**Excluded:** HTTP, JSON, file system, regex, crypto, datetime, testing framework, process management, math beyond basics. All live in separate Hex packages (`gleam_json`, `gleam_http`, `gleam_erlang`, `gleam_javascript`).

**Philosophy:** "Gleam will always be a small and cohesive language with a minimal feature set." The stdlib mirrors this: only include what cannot be provided as a package. Gleam inherits the BEAM ecosystem, so HTTP servers (Mist), JSON (gleam_json), etc. are one `gleam add` away.

**Post-1.0 notes:** No regrets yet (too early). The community generally approves of the minimal approach, citing fast compile times and easy learnability. Some friction around the dynamic module being complex for newcomers.

---

### MoonBit (pre-1.0, 2024) -- Growing, Experimental

**Modules (~49):**
abort, argparse, array, bench, bigint, bool, buffer, builtin, byte, bytes, char, cmp, coverage, debug, deque, double, encoding, env, error, float, hashmap, hashset, immut, int, int16, int64, json, list, math, option, prelude, priority_queue, queue, quickcheck, random, ref, result, set, sorted_map, sorted_set, strconv, string, test, tuple, uint, uint16, uint64, unit, plus internal/regex_engine

**~400 functions (estimated)**

**Included:** Rich collection types (hashmap, hashset, sorted_map, sorted_set, deque, priority_queue, queue), numeric types (int16/64, uint16/64, bigint, float, double), JSON, encoding, basic I/O

**Excluded:** HTTP, file system, crypto, datetime, regex (internal only), logging, UUID. MoonBit targets WASM primarily, so system-level I/O is naturally excluded.

**Philosophy:** Data-structure-heavy core. MoonBit includes many collection types that other languages push to packages, reflecting its functional programming heritage and WASM-first target (where collections are the primary "work" since I/O goes through host bindings).

---

### Rust (1.0, 2015) -- Small Core + Ecosystem

**Stable modules (~50):**
any, ascii, borrow, boxed, cell, char, clone, cmp, collections, convert, default, env, error, f32, f64, ffi, fmt, fs, hash, i8/i16/i32/i64/isize, io, iter, marker, mem, net, num, ops, option, os, path, prelude, process, ptr, rc, result, slice, str, string, sync, thread, u8/u16/u32/u64/usize, vec

**~2,000 functions/methods**

**Included:** Core types + traits, collections (Vec, HashMap, BTreeMap, VecDeque, LinkedList), file I/O, TCP/UDP networking, threads, channels (mpsc), Mutex/Arc, process spawning, path manipulation, formatting, environment variables

**Excluded:** HTTP, JSON, serialization, async runtime, regex, crypto, datetime, random, logging, testing beyond basic `#[test]`, argument parsing, TOML/YAML/CSV parsing

**Philosophy:** Deliberately small. "We can 'include batteries' without literally putting them into the standard library; pulling in other crates is nearly as easy as using the standard library." The 2017 "Libz Blitz" initiative focused on improving ecosystem crate quality rather than expanding std.

**Post-1.0 regrets:**
- `std::sync::mpsc` -- considered inferior to crossbeam channels; can't be replaced due to stability guarantee
- `LinkedList` -- universally considered a mistake to include; almost never the right data structure
- `std::net` -- too basic for production use; most use tokio
- Missing `async` primitives -- `spawn`, `block_on`, `select!` exist only in third-party crates, creating ecosystem fragmentation (tokio vs async-std)
- No built-in `rand` -- controversial; many feel random number generation is fundamental enough for stdlib
- The Graydon retrospective wished containers and smart pointers were compiler builtins rather than library types

---

### Elixir (1.0, 2014) -- Batteries + OTP

**Modules (~77):**
Agent, Application, Atom, Base, Behaviour, Bitwise, Code, Dict, Enum, Exception, File, File.Stat, File.Stream, Float, GenEvent, GenEvent.Stream, GenServer, HashDict, HashSet, IO, IO.ANSI, IO.Stream, Inspect.Algebra, Inspect.Opts, Integer, Kernel, Kernel.ParallelCompiler, Kernel.ParallelRequire, Kernel.SpecialForms, Kernel.Typespec, Keyword, List, Macro, Macro.Env, Map, Module, Node, OptionParser, Path, Port, Process, Protocol, Range, Record, Regex, Set, Stream, String, StringIO, Supervisor, Supervisor.Spec, System, Task, Task.Supervisor, Tuple, URI, Version, plus more

**~800-1,000 functions**

**Included:** Core types, Enum/Stream (lazy iteration), File I/O, Regex, Path, URI, process management (GenServer, Supervisor, Agent, Task), macros, code compilation, string manipulation, option parsing, version comparison

**Excluded:** HTTP client/server, JSON, database, crypto, datetime (added in 1.3), UUID, templating. These live in Hex packages.

**Philosophy:** The stdlib is the language's own runtime; OTP (Erlang's libraries) provides the rest. Elixir's stdlib focuses on providing Elixir-idiomatic wrappers for core concepts plus the concurrency primitives (GenServer, Supervisor) that define the language's identity.

**Post-1.0 regrets:**
- `HashDict`/`HashSet` deprecated in favor of `Map`/`MapSet` (1.2+)
- `Dict` behaviour deprecated -- a premature abstraction
- `GenEvent` deprecated in favor of Registry (1.4+)
- Calendar/DateTime types added only in 1.3, which many felt should have been in 1.0

---

### Kotlin (1.0, 2016) -- JDK Extension Layer

**Core packages (~20 at 1.0, ~39 by 2.3):**
kotlin, kotlin.annotation, kotlin.collections, kotlin.comparisons, kotlin.io, kotlin.math, kotlin.properties, kotlin.random, kotlin.ranges, kotlin.reflect, kotlin.sequences, kotlin.system, kotlin.text, plus platform-specific (kotlin.jvm, kotlin.js)

**~1,500 functions/methods (estimated at 1.0)**

**Included:** Collection extensions (map, filter, fold, groupBy, zip, etc. on JDK collections), String extensions, I/O wrappers, ranges, sequences (lazy), comparators, annotations, basic reflection, Pair/Triple, Lazy, Result

**Excluded:** HTTP, JSON, file watching, crypto, datetime (delegated to java.time), regex (wraps java.util.regex), concurrency (kotlinx.coroutines is separate), serialization (kotlinx.serialization is separate)

**Philosophy:** "The less our users have to relearn, reinvent, redo from scratch, and the more they can reuse, the better." Kotlin's stdlib is an extension layer over JDK, adding functional-style collection operations and Kotlin idioms without duplicating the platform. It stays deliberately small because the JDK is already available.

**Post-1.0 regrets:**
- `kotlin.coroutines` was a late and complex addition to std; the actual runtime (`kotlinx.coroutines`) remains external
- Method count matters on Android (DEX limits); stdlib grew from ~7k methods to ~11k, causing periodic concern
- Some wish `kotlinx.serialization` and `kotlinx.datetime` were stdlib-tier rather than separate artifacts

---

### Swift (1.0, 2014) -- Thin Core + Foundation

**Structure: 1 monolithic module (`Swift`)**

**~1,000 functions/methods (estimated at 1.0)**

**Included:** Core types (Int, Double, String, Bool, Character), collections (Array, Dictionary, Set), Optional, protocols (Equatable, Hashable, Comparable, Codable, Sequence, Collection), basic algorithms (sort, map, filter, reduce), unsafe pointers, string interpolation, print

**Excluded:** File I/O, networking, JSON, regex, datetime, crypto, HTTP, process management, path manipulation. All live in Foundation (Apple's Objective-C bridge framework) or external packages.

**Philosophy:** The Swift standard library provides only what the compiler needs to function. Everything else comes from Foundation (on Apple platforms) or Swift packages. This keeps the core language portable across platforms (Linux, Windows, WASM).

**Post-1.0 regrets / evolution:**
- String API was completely redesigned between Swift 1-4 (indexing, views, Unicode correctness)
- No built-in regex until Swift 5.7 (2022) -- an 8-year gap
- No built-in concurrency until Swift 5.5 (2021) -- async/await
- Foundation was not open-source until 2015, creating a stdlib gap on Linux
- The community repeatedly requested adding common functionality (regex, argument parsing, algorithms) that lived in no-man's land between "too complex for stdlib" and "too fundamental for a package"

---

### Go (1.0, 2012) -- Batteries Included

**~150 packages (including sub-packages)**

Major categories:
- **archive/** (tar, zip)
- **compress/** (bzip2, flate, gzip, lzw, zlib)
- **crypto/** (aes, cipher, des, dsa, ecdsa, elliptic, hmac, md5, rand, rc4, rsa, sha1, sha256, sha512, subtle, tls, x509)
- **database/** (sql)
- **encoding/** (ascii85, asn1, base32, base64, binary, csv, gob, hex, json, pem, xml)
- **go/** (ast, build, doc, parser, printer, scanner, token)
- **hash/** (adler32, crc32, crc64, fnv)
- **image/** (color, draw, gif, jpeg, png)
- **net/** (http, mail, rpc, smtp, textproto, url)
- **os/** (exec, signal, user)
- **text/** (scanner, tabwriter, template)
- plus bufio, bytes, errors, expvar, flag, fmt, io, log, math, mime, path, reflect, regexp, runtime, sort, strconv, strings, sync, syscall, testing, time, unicode, unsafe

**~5,000+ functions/methods**

**Included:** Almost everything a server-side application needs: HTTP client/server, JSON, XML, CSV, TLS, crypto, SQL database driver interface, image processing, compression, templating, testing framework, profiling, regex, build tooling

**Excluded:** External database drivers (only the `database/sql` interface), GUI, advanced async patterns (before generics/iterators), dependency management (added much later)

**Philosophy:** "Batteries included." Go ships everything needed for production services in the standard library, reducing dependency on third parties. This reflects Google's internal mono-repo culture where external dependencies are costly.

**Post-1.0 regrets:**
- `net/http` grew too large and complex; difficult to evolve without breaking changes
- `log` package was too simplistic; `log/slog` (structured logging) added only in Go 1.21 (2023), 11 years later
- `syscall` package frozen at Go 1.4 due to maintenance burden; replaced by `golang.org/x/sys`
- `encoding/json` API has known design issues (no streaming decoder, awkward struct tags); can't fix without breaking changes
- `html/template` security model considered overly complex
- Image processing packages rarely used in production; arguably should not be in stdlib
- Go 2 decision: "There will not be a Go 2 that breaks Go 1 programs" -- they chose to live with stdlib baggage rather than break compatibility

---

### Zig (pre-1.0) -- Practical Batteries

**~90 top-level modules/namespaces**

Major categories:
- **Data structures:** ArrayList, ArrayHashMap, HashMap, PriorityQueue, Deque, LinkedList, BitSet, Treap
- **Algorithms:** sort, mem, math, simd
- **I/O:** fs, io, net, http, os, process
- **Encoding:** base64, json, tar, zip, elf, coff, macho, dwarf, leb, zon
- **Crypto:** crypto (full suite)
- **System:** Thread, DynLib, atomic, once
- **Utilities:** fmt, log, testing, time, unicode, ascii, compress, hash, heap, debug, progress

**~3,000+ functions/methods**

**Included:** Data structures, file I/O, HTTP client/server, TLS, crypto, JSON, compression (gzip/zlib/zstd/lz4), tar/zip, ELF/COFF/Mach-O binary parsing, DWARF debug info, GPU compute, SIMD, memory allocators

**Excluded:** Database interfaces, XML, CSV, templating, regex (was removed from stdlib), image processing, GUI

**Philosophy:** "Any functions that need to allocate memory accept an allocator parameter." Zig's stdlib is large but principled: everything included must work without libc, on freestanding targets. The stdlib doubles as a showcase for the language's compile-time features. Notably, regex was removed from the stdlib because the implementation quality wasn't sufficient.

---

### Deno Standard Library (2024)

**43 packages:**
assert, async, bytes, cache, cbor, cli, collections, crypto, csv, data-structures, datetime, dotenv, encoding, expect, fmt, front-matter, fs, html, http, ini, internal, io, json, jsonc, log, math, media-types, msgpack, net, path, random, regexp, semver, streams, tar, testing, text, toml, ulid, uuid, webgpu, xml, yaml

**~1,500 functions (estimated)**

**Included:** Testing, file system, HTTP utilities, path, crypto, multiple serialization formats (JSON, JSONC, TOML, YAML, XML, CSV, CBOR, MsgPack, INI), compression (in encoding), UUID, ULID, semver, dotenv, front-matter, CLI utilities, streams, WebGPU

**Excluded:** HTTP server (built into Deno runtime), database drivers, templating, image processing, full regex engine (wraps JS native)

**Philosophy:** Curated batteries. Unlike Go's monolithic stdlib, Deno's @std is versioned independently per module and published to JSR. Modules can reach 1.0 at their own pace (some are still 0.x). This gives stdlib-level trust with package-level evolution speed. The approach directly addresses the "stdlib can't evolve" problem that plagues Go and Rust.

---

## Almide (Current State)

**34 modules (23 TOML + 11 .almd)**

TOML modules (380 functions): string(44), list(57), map(22), int(21), float(16), option(12), result(9), math(21), json(30), fs(24), http(26), io(3), env(9), process(6), regex(8), datetime(21), uuid(6), crypto(4), random(4), log(8), testing(7), value(19), error(3)

.almd modules (132 functions): args(6), compress(4), csv(9), encoding(10), hash(3), path(7), term(21), time(20), toml(14), url(21), value(17)

**512 total functions**

**Categories:**
- Core types: string, int, float, list, map (160 functions)
- Wrappers: option, result (21)
- I/O: fs, http, io, env, process (68)
- Data: json, value, csv, toml, encoding (80)
- Utility: math, regex, datetime, time, uuid, crypto, random, hash (87)
- Dev: log, testing, error (18)
- CLI/System: args, term, path, url, compress (63)

---

## Comparison Analysis

### Size Tiers

```
Minimal (< 200 fns):    Elm, Gleam
Small   (200-600 fns):  Almide (512), MoonBit (~400)
Medium  (600-2000 fns): Rust (~2,000), Elixir (~1,000), Kotlin (~1,500), Swift (~1,000), Deno (~1,500)
Large   (2000+ fns):    Go (~5,000), Zig (~3,000)
```

### What Almide Includes That Minimal Languages Don't

| Feature | Elm | Gleam | Almide |
|---------|-----|-------|--------|
| HTTP client | no | no | yes |
| JSON | no* | no | yes |
| File system | no | no | yes |
| Regex | no | no | yes |
| Crypto | no | no | yes |
| DateTime | no | no | yes |
| UUID | no | no | yes |
| CSV/TOML | no | no | yes |
| Testing | no* | no | yes |
| Logging | no | no | yes |

*Elm has these as separate official packages, not in core.

### What Go/Zig Include That Almide Doesn't

| Feature | Go | Zig | Almide |
|---------|----|-----|--------|
| Image processing | yes | no | no |
| TLS/SSL | yes | yes | no |
| SQL interface | yes | no | no |
| Binary parsing (ELF/PE) | yes* | yes | no |
| Compression (gzip/zstd) | yes | yes | partial |
| XML | yes | no | no |
| HTML template | yes | no | no |
| Archive (tar/zip) | yes | yes | no |
| Profiling/debug | yes | yes | no |
| SIMD | no | yes | no |

*Go via debug/elf, debug/pe

### Almide's Position

Almide sits in the **"targeted batteries" zone** between Gleam's minimalism and Go's maximalism. It includes:

1. **Everything an application needs for common tasks** (HTTP, JSON, file I/O, crypto, testing)
2. **Nothing an application rarely needs** (image processing, binary format parsing, compression algorithms, TLS implementation details)

This is closest to the **Deno model**: curated batteries where each module is justified by frequency of real-world use, but the modules themselves live close to the language rather than in an external registry.

---

## The Stdlib Dilemma: Key Takeaways from 2024-2026 Discourse

### 1. The Compatibility Tax

Go's experience is the strongest cautionary tale. Once `encoding/json`, `net/http`, and `log` were in the stdlib, fixing their known design flaws became nearly impossible. Go 1.21 added `log/slog` alongside the old `log` package rather than replacing it. The stdlib only grows; it never shrinks.

**Implication for Almide:** Every module in the stdlib is a permanent commitment. The 34-module scope is manageable *only if* each module's API surface is considered final-quality.

### 2. The Discoverability Tax

Rust's experience shows the opposite problem. A small stdlib forces users to discover, evaluate, and choose between competing crates for basic tasks (HTTP: reqwest vs surf vs ureq; async runtime: tokio vs async-std vs smol). The 2017 Libz Blitz was Rust's admission that discoverability is a real cost of minimal stdlibs.

**Implication for Almide:** Almide has no package ecosystem yet. The stdlib *is* the ecosystem. Excluding something from the stdlib means it doesn't exist.

### 3. The Multi-Target Constraint

Almide compiles to both Rust and TypeScript. Every stdlib function must have implementations for both targets. This creates a natural pressure toward a smaller stdlib -- each function costs 2x implementation effort and 2x maintenance.

Go, Kotlin, and Swift don't face this constraint. Zig does (freestanding + hosted), but solves it by making everything work without libc.

### 4. The LLM Accuracy Constraint

Almide's core mission -- "the language LLMs can write most accurately" -- adds a unique dimension. A larger stdlib means more functions for the LLM to know, more parameter orders to remember, more edge cases to handle correctly. But a smaller stdlib means the LLM has to generate more boilerplate, increasing error surface area in a different way.

**The sweet spot:** A stdlib large enough that common tasks are one function call, small enough that the LLM can memorize the entire API. At 512 functions, Almide's stdlib is within the range an LLM can reliably learn from training data or a system prompt.

### 5. The Emerging Consensus (2024-2026)

New languages in 2024 converge on a pattern:

- **Core stdlib:** Types, collections, Option/Result, string/math basics (~100-200 functions)
- **Extended stdlib:** Separately versioned, individually evolving packages for I/O, networking, encoding, testing (~300-1500 functions)
- **Ecosystem:** Community packages for domain-specific needs

Gleam and Deno exemplify this split explicitly. Elm pioneered it. Even Go is moving toward it with `golang.org/x/` sub-repositories.

### 6. What Every Language Regrets

| Regret pattern | Languages affected |
|---|---|
| Included a data structure nobody uses | Rust (LinkedList), Elixir (HashDict) |
| Froze a flawed API forever | Go (encoding/json, log, net/http) |
| Didn't include datetime/time | Elixir (added 1.3), Swift (Foundation dependency) |
| Didn't include async primitives | Rust (no spawn/block_on), Swift (added 5.5) |
| String API was wrong at 1.0 | Swift (redesigned 3 times) |

---

## Recommendations for Almide

Based on this research:

### Keep (justified by universal need + LLM accuracy benefit)
- string, int, float, list, map -- core types, non-negotiable
- option, result -- algebraic types, central to Almide's design
- json -- the lingua franca of data interchange
- fs, io, env, process -- basic system interaction
- testing -- language-integrated testing is a competitive advantage
- math -- universal utility
- error -- core error infrastructure

### Keep but monitor (included by few minimal languages, but high practical value)
- http -- included because Almide has no package ecosystem; would be first to externalize if one existed
- regex -- surprisingly divisive; Zig removed it from stdlib
- datetime, time -- Elixir regretted not including it at 1.0
- path, url -- fundamental for any real program
- log -- Go regretted its initial log; make sure Almide's is designed well
- csv, toml -- important for the data-processing niche Almide targets

### Watch carefully (potentially over-scoped)
- crypto -- only 4 functions; consider if wrapping is worth the maintenance
- uuid -- only 6 functions; niche
- hash -- only 3 functions; niche
- compress -- only 4 functions; niche
- encoding -- 10 functions; could merge with relevant modules
- term -- 21 functions; terminal-specific; may not make sense for TS target
- value -- overlaps with json module conceptually

### The critical question
At 512 functions across 34 modules, Almide is **2.8x Gleam, 0.25x Go, and 0.34x Deno**. For a language with no package ecosystem, this is a defensible scope. The real risk isn't size -- it's API quality. Each of those 512 functions is a permanent contract.
