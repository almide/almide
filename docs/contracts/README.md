# Almide Behavior Contracts

> Auto-generated from [contracts.toml](contracts.toml).
> Run `bash docs/contracts/generate-readme.sh > docs/contracts/README.md` to update.
>
> Each contract is a NORMATIVE, observable promise the compiler keeps on BOTH
> targets (native Rust + wasm32: stdout, stderr, exit code). Native is the oracle;
> native == wasm is a hard invariant. Every contract is traceable to executable
> evidence (a `spec/wasm_cross/*.almd` fixture, a differential fuzz, an emit-time
> Σ-probe, or a Lean theorem) — no claimed behaviour rests on prose alone.

## Change discipline

- **Changing any observable behaviour REQUIRES updating the contract statement
  AND its evidence in the SAME PR.**
- A **new** behaviour = a new `C-NNN` + ≥1 fixture.
- **Removing a divergence** = flip `status` to `active` and drop the flag in the
  same PR. The `flagged-for-revision` count is a ratchet — it may only go **down**.
- The gate (`scripts/check-contracts.sh`, CI + lefthook) enforces that every
  contract has real evidence, every fixture names its contract(s), and the link is
  bidirectional.
- **Aggregate counts are stamped, not regenerated per PR.** The "N contracts"
  total below (and every other total the generated docs quote) renders from
  `proofs/ledger-counts.toml`, dated in its `counts:generated` block. A fixture
  or contract PR regenerates the rows and leaves the block alone; refresh with
  `bash scripts/gen-ledger-counts.sh` before a release seal, or when the nightly
  `scripts/check-ledger-counts.sh` reports drift.

Evidence classes (weakest → strongest): `doc-only` < `by-construction` <
`fixture` < `fuzz` < `exhaustive` < `lean`. An **active** contract must carry
≥1 evidence of class ≥ `fixture`.

<!-- counts:generated:start (as of 2026-10-21) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
388 contracts
<!-- counts:generated:end -->

| ID | Contract | Since | Status | Strongest Evidence | # Fixtures |
|----|----------|-------|--------|--------------------|-----------:|
| C-001 | Integer division/modulo by zero is total — it aborts, never traps | 0.24.0 | active | fixture | 5 |
| C-002 | Signed MIN / -1 overflow aborts, at the TRUE per-width MIN | 0.24.0 | active | fixture | 3 |
| C-003 | Non-aborting integer div/mod stay byte-identical | 0.24.0 | active | fixture | 1 |
| C-004 | fan.map / fan.settle are deterministic by list order | 0.24.0 | active | fixture | 11 |
| C-005 | fan error propagation surfaces as the unified main-error abort | 0.24.0 | active | fixture | 5 |
| C-006 | [fan.timeout does not exist — wall-clock deadlines live at the host boundary](C-006-fan-timeout-removed.md) | 0.29.0 | active | fixture | 0 |
| C-007 | Abortable top-level lets evaluate eagerly at startup | 0.24.0 | active | fixture | 4 |
| C-008 | [Compound interpolation renders the Almide-literal repr (containers)](C-008-009-010-repr.md) | 0.24.0 | active | fixture | 3 |
| C-009 | [Record / variant / anonymous-record interpolation repr (field sorting)](C-008-009-010-repr.md) | 0.24.0 | active | fixture | 4 |
| C-010 | [Recursive / generic ADT interpolation repr keyed by instantiation](C-008-009-010-repr.md) | 0.24.0 | active | fixture | 2 |
| C-011 | Bare-float interpolation Display drops .0; float.to_string keeps it | 0.24.0 | active | fuzz(100000) | 5 |
| C-012 | Const-folded non-finite floats emit named constants | 0.24.0 | active | fixture | 1 |
| C-013 | Map is a compact-ordered-dict: iteration is insertion order | 0.24.0 | active | fixture | 11 |
| C-014 | Set is insertion-ordered and deterministic | 0.24.0 | active | fixture | 3 |
| C-015 | Structural deep equality for compound elements and heap values | 0.24.0 | active | fixture | 10 |
| C-016 | UTF-8 codepoint-aware string ops are byte-identical | 0.24.0 | active | fixture | 3 |
| C-017 | Empty-pattern count / last_index_of follow native codepoint/byte semantics | 0.24.0 | active | fixture | 2 |
| C-018 | Unicode string predicates match Rust char methods over the full domain | 0.24.0 | active | fixture | 1 |
| C-019 | rt_string_extra ops (replace_first, strip_*, predicates, cmp) match native | 0.24.0 | active | fixture | 2 |
| C-020 | Unicode case transforms (to_upper/to_lower/capitalize) are full-Unicode | 0.24.0 | active | fixture | 1 |
| C-021 | Whitespace trim / is_whitespace use the full Unicode White_Space property | 0.24.0 | active | fixture | 1 |
| C-022 | bytes.to_string_lossy is UTF-8-lossy decode (inverse of bytes.from_string) | 0.24.0 | active | fixture | 1 |
| C-023 | float.to_string is shortest round-tripping decimal, byte-equal to Rust Display | 0.24.0 | active | fuzz(100000) | 2 |
| C-024 | float.parse is correctly-rounded round-to-nearest-even (Clinger AlgorithmM) | 0.24.0 | active | fixture | 1 |
| C-025 | float.to_fixed is round-half-to-even on the exact binary value | 0.24.0 | active | fuzz(1000) | 1 |
| C-026 | Vendored-libm trig / exp / log / pow are byte-identical cross-target | 0.24.0 | active | fuzz(4000) | 3 |
| C-027 | base64 encode/decode (standard + URL-safe) is byte-identical incl. errors | 0.24.0 | active | fixture | 1 |
| C-028 | int.parse_radix(s, 16): trim, one optional sign, one optional 0x / 0X, then hex digits | 0.24.0 | active | fixture | 1 |
| C-029 | int.parse error modes byte-match native ParseIntError | 0.24.0 | active | fixture | 1 |
| C-030 | hex.encode / hex.decode are byte-identical incl. positional error detail | 0.24.0 | active | fixture | 2 |
| C-031 | value get/set/remove_path edge cases match the native oracle, which never fails on a path edge case | 0.24.0 | active | fixture | 3 |
| C-032 | Regex engine is byte-identical to the native engine over a fuzzed grammar | 0.24.0 | active | fuzz(220) | 5 |
| C-033 | [Value semantics for aliased mutables (copy-on-write)](C-033-cow-truth-table.md) | 0.24.0 | active | fixture | 21 |
| C-034 | Out-of-range list ops clamp / no-op gracefully (no OOB heap access) | 0.24.0 | active | fixture | 8 |
| C-035 | Effect-main errors terminate uniformly: Error: <msg> + exit 1 | 0.24.0 | active | fixture | 6 |
| C-036 | Records, variants, and pattern matching are byte-identical | 0.24.0 | active | fixture | 10 |
| C-037 | bytes.read_f16_le decodes IEEE-754 half floats identically | 0.24.0 | active | fixture | 1 |
| C-038 | Sized-integer literals narrow to the declared field width | 0.24.0 | active | fixture | 4 |
| C-039 | Type-changing map.map / set.map yield a collection of the new type | 0.24.0 | active | fixture | 2 |
| C-040 | Codegen emit is host-architecture deterministic | 0.24.0 | active | fixture | 3 |
| C-041 | Heap / RC primitives honour the Lean-certified Perceus discipline | 0.24.0 | active | lean | 5 |
| C-042 | fs preopen-dir scan + path resolution is observable-equivalent | 0.24.0 | active | fixture | 1 |
| C-043 | A user type named Box coexists with recursive-enum heap indirection | 0.24.0 | active | fixture | 1 |
| C-044 | Result/Option construction and matching are byte-identical | 0.24.0 | active | fixture | 6 |
| C-045 | A List[String] param works across join / len / index / iteration | 0.24.0 | active | fixture | 3 |
| C-046 | Record spread-update and cross-module monomorphization are byte-identical | 0.24.0 | active | fixture | 1 |
| C-047 | int.pow with a negative exponent is total — it aborts, never traps or wraps | 0.24.0 | active | fixture | 2 |
| C-048 | int.wrap_* / int.rotate_* run on the 64-bit register and int.wrap saturates its mask for bits >= 64 | 0.24.0 | active | fixture | 1 |
| C-049 | float.sign is f64::signum; float/math min/max ignore NaN | 0.24.0 | active | fixture | 1 |
| C-050 | string.split(\"\") is codepoint-granular | 0.24.0 | active | fixture | 1 |
| C-051 | math.log_gamma is bit-identical (both targets use the vendored musl-libm log) | 0.24.0 | active | fixture | 2 |
| C-052 | A fold over an empty collection requires the collection to carry an element type (no codegen defaulting) | 0.24.0 | active | fixture | 1 |
| C-053 | list.min/max/sort/sort_by/unique_by are type-directed and total, native == wasm | 0.24.0 | active | fixture | 5 |
| C-054 | List/string Int counts and indices are i64-clamped before narrowing — no truncation, no OOB | 0.24.0 | active | fixture | 11 |
| C-055 | list.min/max/sort/sort_by over Float use IEEE-754 totalOrder, valid + identical on both targets | 0.24.0 | active | fixture | 3 |
| C-056 | list.product wraps on i64 overflow, consistent with list.sum and plain `*` | 0.24.0 | active | fixture | 1 |
| C-057 | Assigning a Unit-returning in-place mutator's result is a checker error on both targets | 0.24.0 | active | fixture | 1 |
| C-058 | An empty collection with an uninferable element type is a compile error on both targets, never silently defaulted | 0.24.0 | active | fixture | 2 |
| C-059 | Compilation does not overflow the native stack on wide or deep input, identically on every host and build profile | 0.25.0 | active | fixture | 1 |
| C-060 | A Value reprs as its JSON text byte-identically on native and WASM, bare and as a Repr-record field | 0.26.7 | active | fixture | 2 |
| C-061 | A mut Map parameter mutated in place builds on both targets and the mutation persists, byte-identical | 0.26.9 | active | fixture | 1 |
| C-062 | A Region moves bytes byte-identically on both targets, and an access outside it is an err | 0.26.15 | active | fixture | 1 |
| C-063 | Parsing a heterogeneous-nested glTF/JSON document and walking its arrays by element is byte-identical on both targets | 0.26.19 | active | fixture | 1 |
| C-064 | The effect-fn Result auto-unwrap rule is identical across binding positions and type-directed, byte-identical on both targets | 0.26.20 | active | fixture | 3 |
| C-065 | The string position API is codepoint-indexed end-to-end on both targets | 0.26.20 | active | fixture | 3 |
| C-066 | WASM heap is reclaimed by default (true Perceus) | 0.27.0 | active | fixture | 23 |
| C-067 | The xs[i] index syntax aborts on out-of-bounds (read and write) | 0.27.4 | active | fixture | 9 |
| C-068 | Auto-? is target-directed in construction positions | 0.27.4 | active | fixture | 3 |
| C-069 | Effect-fn tail self-recursion loop-converts to O(1) stack on both targets | 0.27.4 | active | fixture | 3 |
| C-070 | Nested constructor patterns match and bind identically on both targets | 0.27.6 | active | fixture | 15 |
| C-071 | Single-part interpolation RC balance | 0.27.6 | active | fixture | 1 |
| C-072 | Inferred named-record repr parity | 0.27.6 | active | fixture | 2 |
| C-073 | Tuple pattern testing a variant constructor | 0.27.6 | active | fixture | 1 |
| C-074 | Iterative split/replace on large inputs | 0.27.6 | active | fixture | 1 |
| C-075 | lowmisc round-5 cluster: borrowed-param owning binding, effect-Option auto-try strip, matching-error ! passthrough | 0.27.6 | active | fixture | 3 |
| C-076 | Producer-side in-module variant construction is target-stable | 0.27.6 | active | fixture | 3 |
| C-077 | Cross-module heap-global init order is dependency-respecting | 0.27.6 | active | fixture | 1 |
| C-078 | Phantom record generic param is stripped on the Rust target | 0.27.6 | active | fixture | 1 |
| C-079 | Variant cases with distinct anonymous-record payloads are target-stable | 0.27.6 | active | fixture | 1 |
| C-080 | Empty map.from_list / set.from_list resolves its element from the result type | 0.27.6 | active | fixture | 1 |
| C-081 | Generic fn in an inferred-param lambda resolves its type parameter | 0.27.6 | active | fixture | 1 |
| C-082 | Calling a closure-typed lambda parameter yields the call result, not the closure | 0.27.6 | active | fixture | 1 |
| C-083 | A negated i64::MIN literal is representable, not folded to zero | 0.27.6 | active | fixture | 1 |
| C-084 | Codec/value decode error messages are byte-identical across targets | 0.27.6 | active | fixture | 3 |
| C-085 | Float decode widens an integral JSON number to f64 | 0.27.6 | active | fixture | 1 |
| C-086 | Pass-through stdlib combinators give their result its own reference | 0.27.6 | active | fixture | 1 |
| C-087 | JSON number and \\u string decoding are byte-identical across targets | 0.27.6 | active | fixture | 4 |
| C-088 | A Rust-keyword function name compiles on both targets | 0.27.6 | active | fixture | 2 |
| C-089 | A default parameter referencing an earlier parameter is filled with its argument | 0.27.6 | active | fixture | 1 |
| C-090 | bytes.from_list on a List[Int] parameter compiles on both targets | 0.27.6 | active | fixture | 1 |
| C-091 | A nested sub-pattern in let-destructuring binds every leaf | 0.27.6 | active | fixture | 1 |
| C-092 | A generic record field is sized by its instantiated type at construction | 0.27.6 | active | fixture | 3 |
| C-093 | Mutually-recursive variant types compile on both targets | 0.27.6 | active | fixture | 1 |
| C-094 | A protocol-method UFCS call on an inferred lambda param resolves the element type | 0.27.6 | active | fixture | 1 |
| C-095 | json.stringify_pretty is byte-identical indented output across targets | 0.27.6 | active | fixture | 2 |
| C-096 | env.args works on WASM and matches native | 0.27.6 | active | fixture | 1 |
| C-097 | generic + on a type parameter concatenates strings/lists identically across targets | 0.27.6 | active | fixture | 1 |
| C-098 | cross-module derived Codec methods dispatch on WASM and match native | 0.27.6 | active | fixture | 0 |
| C-099 | comparison/equality operators byte-match native across all operand types on the v1 wasm path | 0.27.6 | active | fixture | 10 |
| C-100 | Self-hosted String classification/transform ops byte-match native on wasm | 0.27.6 | active | fixture | 4 |
| C-101 | List ops over heap elements (String/Value) byte-match native and are leak/double-free free | 0.27.6 | active | fixture | 18 |
| C-102 | List iteration, call-result element materialization, and tail-recursive list traversal byte-match native | 0.27.6 | active | fixture | 5 |
| C-103 | Self-hosted dynamic Value model (merge, array/as_array roundtrip, tuple TCO) byte-matches native and is leak-free in a loop | 0.27.6 | active | fixture | 8 |
| C-104 | Tail-recursive accumulator shapes lower to bounded-stack loops byte-matching native | 0.27.6 | active | fixture | 6 |
| C-105 | var/append accumulator loops (scalar, owned-handle, cross-dep, mutual-recursion) byte-match native on wasm | 0.27.6 | active | fixture | 7 |
| C-106 | Heap value bound from an if/match arm byte-matches native on the v1 wasm path | 0.27.6 | active | fixture | 17 |
| C-107 | heap Result-of-tuple / Result-of-list Ok payloads round-trip and byte-match native | 0.27.6 | active | fixture | 4 |
| C-108 | Unwrap `!` and let-unwrap desugaring byte-match native in every position | 0.27.6 | active | fixture | 10 |
| C-109 | Self-hosted base64 encode byte-matches canonical / native on the v1 wasm path | 0.27.6 | active | fixture | 1 |
| C-110 | In-place bytes.push mutation accumulator byte-matches native on v1 wasm | 0.27.6 | active | fixture | 1 |
| C-111 | Module-level const heap globals initialize and read identically on v1 wasm and native | 0.27.6 | active | fixture | 2 |
| C-112 | random.int draws stay in-range identically under the WASI entropy floor on v1 wasm | 0.27.6 | active | fixture | 1 |
| C-113 | Let-bound ADT/Result variant matched by tag byte-matches native on v1 wasm | 0.27.6 | active | fixture | 2 |
| C-114 | Matching an Option with a heap payload byte-matches native on v1 wasm | 0.27.6 | active | fixture | 2 |
| C-115 | Pipe into a block-bodied lambda producing a value byte-matches native on v1 wasm | 0.27.6 | active | fixture | 2 |
| C-116 | v1 scalar-value lowering edges byte-match native (tail Bool literal, float.parse inf/nan) | 0.27.7 | active | fixture | 2 |
| C-117 | In-loop let-bound heap if/match is lifted to a tail helper and renders on v1 | 0.27.7 | active | fixture | 3 |
| C-118 | env.args works on WASM and matches native (argv[0] skipped) | 0.27.8 | active | fixture | 1 |
| C-119 | effect-`!` inside a `for` loop body propagates Err and byte-matches native | 0.27.6 | active | fixture | 1 |
| C-120 | capturing filter_map with a conditional keep/skip arm body byte-matches native | 0.27.6 | active | fixture | 1 |
| C-121 | String pass-through fast paths hand back an owned (+1) reference | 0.27.6 | active | fixture | 2 |
| C-122 | Value object ops allocate full list layout and share pairs with +1 | 0.27.6 | active | fixture | 2 |
| C-123 | Record spread shares copied heap fields and alias overrides with +1 | 0.27.6 | active | fixture | 1 |
| C-124 | Value equality is deep structural, mirroring the native PartialEq | 0.27.6 | active | fixture | 1 |
| C-125 | bytes.set has value semantics — never observable through the input | 0.27.6 | active | fixture | 1 |
| C-126 | Nested-lambda HOF params keep their inference link (no literal sig-generic pin) | 0.27.6 | active | fixture | 2 |
| C-127 | unwrap_or sizes its payload from the default when the chain type is unresolved | 0.27.6 | active | fixture | 2 |
| C-128 | datetime.format substitutes strftime specifiers identically on every backend | 0.28.1 | active | fixture | 1 |
| C-129 | list.chunk / list.window non-positive sizes: negative keeps the promoted norm, zero aborts in the T6 form | 0.28.4 | active | fixture | 5 |
| C-130 | option/map combinators hand back OWNED heap results (no bare pass-through handles) | 0.28.5 | active | fixture | 2 |
| C-131 | Loop-rebuilt buffers are O(n): COW guards only LIVE aliases, and LICM never hoists heap allocations | 0.28.6 | active | fixture | 3 |
| C-132 | mut parameters of reallocating containers persist to the caller at every call position | 0.28.6 | active | fixture | 31 |
| C-133 | env.get observes the host environment identically on native and wasm | 0.29.0 | active | fixture | 1 |
| C-134 | Vendored-libm atan / tanh are byte-identical cross-target | 0.30.0 | active | fuzz(3000) | 1 |
| C-135 | Declared-Unit effect fn ABI agrees between def and every call site | 0.30.0 | active | fixture | 2 |
| C-136 | In-place place mutations persist to the subsequent read on both targets | 0.30.0 | active | fixture | 1 |
| C-137 | Relative fs paths resolve against the host CWD on wasm | 0.31.0 | active | fixture | 1 |
| C-138 | ok/err ctor with a stdlib-call payload materializes the real value | 0.31.0 | active | fixture | 1 |
| C-139 | Heap-Ok Result value combinators keep tag and payload | 0.31.0 | active | fixture | 2 |
| C-140 | float.round preserves the sign of a zero result | 0.31.0 | active | fixture | 2 |
| C-141 | list.zip_with routes by element repr — String zips work, no wrong-typed link | 0.31.0 | active | fixture | 1 |
| C-142 | result.unwrap_or_else is valid wasm at the Float instantiation on both legs | 0.31.0 | active | fixture | 1 |
| C-143 | Ctor if-payloads materialize the taken arm | 0.31.0 | active | fixture | 1 |
| C-144 | A scalar-list literal never observes as a silent empty list | 0.31.0 | active | fixture | 1 |
| C-145 | Mono-suffixed stdlib combinator names route by their base name | 0.31.0 | active | fixture | 1 |
| C-146 | A lifted closure returning a captured alias hands out a co-owned reference | 0.31.0 | active | fixture | 1 |
| C-147 | list.unique_by routes by key repr — String keys dedupe by content | 0.31.0 | active | fixture | 1 |
| C-148 | list.scan stores each intermediate at the accumulator's own width | 0.31.0 | active | fixture | 1 |
| C-149 | unwrap_or_else hands back a co-owned heap Ok payload | 0.31.0 | active | fixture | 6 |
| C-150 | Ctors over a heap var are value copies — the var stays live | 0.31.0 | active | fixture | 1 |
| C-151 | Result combinators with a heap-Ok RESULT never link the scalar impl | 0.31.0 | active | fixture | 1 |
| C-152 | An un-admitted heap call payload in a ctor walls, never zeroes | 0.31.0 | active | fixture | 1 |
| C-153 | Non-test assert failures abort in the T6 form on both targets | 0.31.0 | active | fixture | 6 |
| C-154 | clamp with an invalid range aborts in the T6 form | 0.31.0 | active | fixture | 2 |
| C-155 | to_fixed with out-of-domain decimals aborts in the T6 form | 0.31.0 | active | fixture | 3 |
| C-156 | An if-merged some((String, String)) ctor is a real tracked Option | 0.32.0 | active | fixture | 1 |
| C-157 | An unannotated generic-ctor top-let carries its solved payload type to every reader | 0.32.0 | active | fixture | 1 |
| C-158 | A some/ok ctor over a scalar call or tuple payload materializes the real value, never a zeroed ctor | 0.32.0 | active | fixture | 5 |
| C-159 | list.binary_search returns the same index on both targets for duplicate keys | 0.32.0 | active | fixture | 1 |
| C-160 | Pure-Almide bundled stdlib modules link and run byte-identically on wasm | 0.34.4 | active | fixture | 1 |
| C-161 | Matrix constructor dimensions clamp negatives and abort over a shared ceiling | 0.35.0 | active | fixture | 14 |
| C-162 | io.write / io.write_bytes emit in program order, interleaved with println | 0.35.0 | active | fixture | 1 |
| C-163 | A heap-result if/match bound to a let/var executes the taken arm on both targets | 0.35.0 | active | fixture | 1 |
| C-164 | List modifiers and suffix copies co-own tuple / record / nested-list elements | 0.35.0 | active | fixture | 1 |
| C-165 | fold over a String-keyed map, a String list or a String set threads a heap accumulator on both targets | 0.36.0 | active | fixture | 1 |
| C-166 | Map interpolation renders every self-hosted key/value pairing on both targets | 0.36.0 | active | fixture | 1 |
| C-167 | float.clamp returns its input unchanged when in range, sign bit included | 0.36.0 | active | fixture | 1 |
| C-168 | list.flatten borrows its argument, so it composes with another borrow of the same binding | 0.36.0 | active | fixture | 2 |
| C-169 | list.repeat over the size ceiling aborts in the T6 form on both targets | 0.36.0 | active | fixture | 1 |
| C-170 | Integer arithmetic wraps in every position, including a module-level let | 0.36.0 | active | fixture | 3 |
| C-171 | Byte-offset bound checks do not overflow at the i64 boundary | 0.36.0 | active | fixture | 1 |
| C-172 | unwrap_or over any heap payload yields the same value on both targets | 0.36.0 | active | fixture | 2 |
| C-173 | An integer literal outside what its context can represent is a checker error on both targets, never a silent value | 0.36.0 | active | fixture | 2 |
| C-174 | A tail-recursive Map/Set accumulator keeps its seed on both targets, non-empty seeds included | 0.36.0 | active | fixture | 1 |
| C-175 | A List literal of a variant type builds and drops identically on both targets, in bind and heap-result-if-arm position | 0.37.0 | active | fixture | 1 |
| C-176 | some/ok around an inline tuple-returning call materializes the real payload on both targets | 0.37.0 | active | fixture | 1 |
| C-177 | A mutable-global projection read in a loop-body call argument reads the CURRENT slot every iteration | 0.37.0 | active | fixture | 1 |
| C-178 | A mutual tail-recursion chain runs at unbounded depth on the wasm target | 0.37.0 | active | fixture | 2 |
| C-179 | UInt64 reaches its full declared domain, with every observer reading the slot unsigned | 0.37.0 | active | fixture | 5 |
| C-180 | Sized-integer +, -, * and ^ wrap at the declared width on both targets | 0.37.0 | active | fixture | 1 |
| C-181 | args.positional returns every non-flag argument, and the args surface agrees across targets | 0.37.0 | active | fixture | 1 |
| C-182 | A negated float literal takes its context's float type on both targets | 0.37.0 | active | fixture | 2 |
| C-183 | A loop-carried append of a list-of-scalar-records executes on both targets | 0.37.0 | active | fixture | 1 |
| C-184 | The integer `^` operator is the same total exponentiation as int.pow on both targets | 0.37.0 | active | fixture | 1 |
| C-185 | A `??` chain returns the first Ok in source order on both targets, whatever position it is in | 0.37.0 | active | fixture | 1 |
| C-186 | Appending an inline index read to a list accumulator lowers and runs on both targets | 0.37.0 | active | fixture | 2 |
| C-187 | An in-place mutator writing through a module-level `var` buffer behaves identically on both targets | 0.37.0 | active | fixture | 6 |
| C-188 | A scalar `if` arm executes its statement effects — global writes land and outer-var reassignments hit the stable local | 0.37.0 | active | fixture | 1 |
| C-189 | env.os and the temp-dir surface report the HOST — the equivalence law's only exemption, and it is closed | 0.37.0 | active | fixture | 1 |
| C-190 | A closure captures any scalar, not just Int and Bool — Float and every sized int width included | 0.37.0 | active | fixture | 1 |
| C-191 | A module-level global reads its true value on every branch path — the materialization memo is scoped to straight-line context | 0.37.1 | active | fixture | 2 |
| C-192 | An auto-unwrapped effect call inside a string interpolation lifts and propagates like a call argument | 0.37.1 | active | fixture | 3 |
| C-193 | Repeated in-place writes through a mutable global stay in place — the receiver borrows the post-COW handle | 0.37.1 | active | fixture | 1 |
| C-194 | bytes.copy_from with a mutable-global destination writes through the storage slot | 0.38.1 | active | fixture | 1 |
| C-195 | Value-position variant match over the checked-conversion family executes | 0.38.1 | active | fixture | 1 |
| C-196 | Call-stack exhaustion is a defined abort at a per-target depth | 0.41.0 | active | fixture | 1 |
| C-197 | Linear-memory exhaustion is a resource limit with a defined abort | 0.41.0 | active | fixture | 9 |
| C-198 | The head-count-taking matrix entry points left the stdlib at dialect epoch 14 | 0.42.0 | active | fixture | 0 |
| C-199 | A fan block joins every sibling and reports the first Err in list order | 0.42.0 | active | fixture | 2 |
| C-200 | A trap in a fan sibling exits through the unified main-error abort, convergently | 0.42.0 | active | fixture | 3 |
| C-201 | An Option combinator's tuple result is materializable as an owned element for every element-type combination | 0.44.0 | active | fixture | 1 |
| C-202 | Time constructors guard their domain: negative aborts, overflow saturates | 0.47.0 | active | fixture | 3 |
| C-203 | The time-type operator algebra is unit-exact and saturating on both targets | 0.47.0 | active | fixture | 1 |
| C-204 | A fan.bounded verdict is a function of the program and its inputs alone | 0.47.0 | active | fixture | 9 |
| C-205 | The fan.min winner is the (spend, index) lexicographic minimum among admitted arms | 0.47.0 | active | fixture | 8 |
| C-206 | A settle block settles every arm into its own tuple slot, in arm order | 0.47.0 | active | fixture | 1 |
| C-207 | CM-1 is a versioned constant with a ratio-only wall-clock claim | 0.47.0 | active | fixture | 1 |
| C-208 | fan.timeout is omega-relative: fixed ends everywhere, fixed omega replays byte-identically | 0.47.0 | active | fixture | 1 |
| C-209 | Codec encode omits none; decode folds missing/null to none; Value passes through verbatim | 0.52.0 | active | fixture | 2 |
| C-210 | NaN observation is canonical — the deterministic-profile conformance law | 0.53.0 | active | fixture | 3 |
| C-211 | ! propagates in pure Result/Option-returning fns, byte-identical across targets | 0.53.5 | active | fixture | 1 |
| C-212 | Two self-host modules with same-named private helpers link cleanly or wall — never an invalid module | 0.52.0 | active | fixture | 1 |
| C-213 | The bytes byte-level writers encode the same buffer on both targets, out-of-range writes included | 0.53.5 | active | fixture | 6 |
| C-214 | process.exec_status_timeout: the fire-path error is pinned; whether it fires is the host's | 0.53.6 | active | fixture | 0 |
| C-215 | fs content readers: absence is ok(none) via the _if_exists family, classified by the runtime | 0.53.7 | active | fixture | 4 |
| C-216 | explicit ! on a declared-Option effect call is the implicit strip's identical twin | 0.54.1 | active | fixture | 1 |
| C-217 | let _ = f() discards the Result — the err does not propagate | 0.55.0 | active | fixture | 1 |
| C-218 | a heap-payload ?? returned as the fn tail yields the same value on both targets | 0.56.0 | active | fixture | 3 |
| C-219 | a Never-typed call in a branch arm runs on both targets and sets the exit code | 0.56.0 | active | fixture | 5 |
| C-220 | fs streaming line walkers fold/each with read_lines line semantics | 0.56.1 | active | fixture | 2 |
| C-221 | An effect fn-typed slot admits pure and fallible lambdas with one carrier semantics | 0.56.1 | active | fixture | 4 |
| C-222 | An expression-nested scalar unwrap propagates the err identically on both legs | 0.56.1 | active | fixture | 1 |
| C-223 | Matrix arithmetic computes through the vendored musl-libm and native's product order, not the platform's | 0.56.1 | active | fixture | 2 |
| C-224 | if let / guard let bind and release heap payloads identically on both targets | 0.56.1 | active | fixture | 1 |
| C-225 | fs.read_lines materializes a file's lines identically on both targets | 0.56.2 | active | fixture | 2 |
| C-226 | A mut parameter crossing a call boundary mutates the caller's data on both targets | 0.56.2 | active | fixture | 14 |
| C-227 | The fs metadata and composition family answers identically on both targets | 0.56.2 | active | fixture | 1 |
| C-228 | The fs composition family and the matrix row selectors answer identically on both targets | 0.56.2 | active | fixture | 4 |
| C-229 | A selected row past the byte buffer is the all-zero row on both targets | 0.57.1 | active | fixture | 5 |
| C-230 | The flight reference PID kernel runs identically on both targets | 0.57.1 | active | fixture | 1 |
| C-231 | Integer and boolean literal forms evaluate identically on both targets | 0.57.1 | active | fixture | 1 |
| C-232 | Boolean literals and their display form are identical on both targets | 0.57.1 | active | fixture | 1 |
| C-233 | The unit literal, its positions, and its reflexive equality are identical on both targets | 0.57.1 | active | fixture | 1 |
| C-234 | Parenthesized grouping and the 1-tuple disambiguation behave identically on both targets | 0.57.1 | active | fixture | 2 |
| C-235 | Unary operators: `not` negation and prefix minus evaluate identically on both targets | 0.57.1 | active | fixture | 1 |
| C-236 | Tuple construction, destructuring, indexing, and structural equality are identical on both targets | 0.57.1 | active | fixture | 1 |
| C-237 | Option/Result constructor literals build and eliminate identically on both targets | 0.57.1 | active | fixture | 1 |
| C-238 | First-class range values iterate their true bounds on both targets | 0.57.1 | active | fixture | 9 |
| C-239 | Collection literals, indexing, and the empty spellings behave identically on both targets | 0.57.1 | active | fixture | 1 |
| C-240 | Map literals and empty-map reads behave identically on both targets | 0.57.1 | active | fixture | 2 |
| C-241 | Binding statements: let shadowing, var reassignment, and their check-time guards are identical on both targets | 0.57.1 | active | fixture | 1 |
| C-242 | Conditional expressions evaluate the taken arm only, identically on both targets | 0.57.1 | active | fixture | 1 |
| C-243 | Block expressions yield their tail value identically on both targets | 0.57.1 | active | fixture | 1 |
| C-244 | While loops iterate their condition-tested body identically on both targets | 0.57.1 | active | fixture | 1 |
| C-245 | String interpolation embeds each segment's canonical display identically on both targets | 0.57.1 | active | fixture | 2 |
| C-246 | Identifiers resolve to their nearest binding; unresolved names are check-time E003 naming the identifier | 0.57.1 | active | fixture | 2 |
| C-247 | Match expressions select the first matching arm identically on both targets; non-exhaustive is check-time E010 | 0.57.1 | active | fixture | 1 |
| C-248 | for-in iterates list-family heads with optional tuple destructuring identically on both targets | 0.57.1 | active | fixture | 1 |
| C-249 | Tuple let-destructuring binds each component positionally, identically on both targets | 0.57.1 | active | fixture | 2 |
| C-250 | Pipe and composition evaluate as plain application in order, identically on both targets | 0.57.1 | active | fixture | 2 |
| C-251 | if let implicitly unwraps an Option scrutinee with a bare binder, identically on both targets | 0.57.1 | active | fixture | 1 |
| C-252 | Expression statements and comments: Unit calls execute, discards are explicit, comments are invisible | 0.57.1 | active | fixture | 1 |
| C-253 | Place assignments and type ascription behave identically on both targets | 0.57.1 | active | fixture | 1 |
| C-254 | Type ascription supplies an expected type without changing the value on either target | 0.57.1 | active | fixture | 1 |
| C-255 | Record literals, member reads, and spread updates preserve value semantics identically on both targets | 0.57.1 | active | fixture | 2 |
| C-256 | break and continue take effect at their statement position, identically on both targets and the interp | 0.57.1 | active | fixture | 2 |
| C-257 | The scalar error operators evaluate identically on both targets | 0.57.1 | active | fixture | 1 |
| C-258 | Named calls and lambdas evaluate identically on both targets | 0.57.1 | active | fixture | 6 |
| C-259 | Variant constructor references build values eliminated identically by match on both targets | 0.57.1 | active | fixture | 1 |
| C-260 | The declaration family compiles and runs identically on both targets | 0.57.1 | active | fixture | 5 |
| C-261 | Canonical float values including the negative-zero sign display identically on both targets | 0.57.1 | active | fixture | 1 |
| C-262 | The fmt-stable string escapes and the empty string evaluate identically on both targets | 0.57.1 | active | fixture | 1 |
| C-263 | Recovery nodes never appear in accepted programs; a broken file still reports past its first error | 0.57.1 | active | fixture | 1 |
| C-264 | Scalar-field optional chains yield some/none identically on both targets | 0.57.1 | active | fixture | 1 |
| C-265 | The guard statement's pass and raise paths behave identically on both targets | 0.57.1 | active | fixture | 3 |
| C-266 | guard let binds the success payload and raises on the failing polarity, identically on both targets | 0.57.1 | active | fixture | 1 |
| C-267 | The binary operator surface evaluates identically on both targets, including the signed division rulings | 0.57.1 | active | fixture | 1 |
| C-268 | A `_` in call-argument position is a check-time E046, identically on both targets | 0.57.1 | active | fixture | 0 |
| C-269 | A nested variant match bound to a let executes identically on both targets; a constructor pattern the subject lacks is a check-time E048 on both | 0.57.1 | active | fixture | 1 |
| C-270 | A stdlib spelling removed at dialect epoch 14 is a check-time error that names where it went | 0.57.1 | active | fixture | 0 |
| C-271 | `carrier ?? call!` yields the fallback's PAYLOAD, never its Result block, for a handle-carrying payload on both targets | 0.57.1 | active | fixture | 1 |
| C-272 | `fs.list_dir` returns EVERY entry on both targets, whatever the directory's size, sorted by bytes | 0.57.1 | active | fixture | 1 |
| C-273 | The fs write-side floors answer with native's std::io Display text, and fd_write's errno is not dropped | 0.57.1 | active | fixture | 1 |
| C-274 | A fallible callback makes an fs streaming walk fallible, stopping at the first err | 0.57.1 | active | fixture | 1 |
| C-275 | The http response-header surface answers identically on both targets, with case-insensitive field names | 0.57.1 | active | fixture | 1 |
| C-276 | A String-key `list.sort_by` over HEAP elements orders and co-owns identically on both targets | 0.57.1 | active | fixture | 1 |
| C-277 | A return-only-generic constructor pinned by unification builds and runs identically on both targets even when const-folding erases the pinning context | 0.57.1 | active | fixture | 1 |
| C-278 | The rope family left the stdlib at dialect epoch 14 | 0.57.1 | active | fixture | 0 |
| C-279 | A zero-trip loop performs none of its body's effects or traps on either target | 0.57.1 | active | fixture | 1 |
| C-280 | The kernel-conformance family behaves per the machine-checked λ_almd semantics on both targets | 0.57.1 | active | fixture | 1 |
| C-281 | Bare Result constructors and parameter-passthrough effect tails run identically per the kernel semantics | 0.57.1 | active | fixture | 3 |
| C-282 | matrix.get answers none out of range on both targets; the row reductions answer their identity | 0.57.1 | active | fixture | 1 |
| C-283 | A call through a Fn-typed local resolves that local, never a same-named top-level fn | 0.57.1 | active | fixture | 1 |
| C-284 | A fallible list HOF driven by a user effect fn callback runs identically on both targets | 0.57.1 | active | fixture | 8 |
| C-285 | regex.captures answers the whole match at index 0, and `none` means only that nothing matched | 0.57.1 | active | fixture | 2 |
| C-286 | `??` reads its source; the Option survives the elimination and stays readable | 0.57.1 | active | fixture | 1 |
| C-287 | A tail-position heap-result if/match with a none arm executes the taken arm for record and list payloads | 0.57.1 | active | fixture | 1 |
| C-288 | list.enumerate over a rich-variant list executes on both targets | 0.57.1 | active | fixture | 1 |
| C-289 | The Float equality family compares with f64 equality on both targets | 0.57.1 | active | fixture | 1 |
| C-290 | fs text readers refuse invalid UTF-8 identically on both targets; the bytes readers keep the raw bytes | 0.57.2 | active | fixture | 1 |
| C-291 | Each error-channel operator agrees with its defining function on both targets | 0.57.1 | active | fixture | 2 |
| C-292 | The reference-suite conformance corpus runs byte-identical across targets | 0.57.2 | active | fixture | 48 |
| C-293 | Two-level container nestings run byte-identical; the third level and map-valued shapes are pinned walls | 0.57.2 | active | fixture | 6 |
| C-294 | A zero compute budget admits inlined loop-free callees on every leg | 0.57.2 | active | fixture | 2 |
| C-295 | A checker-exhaustive match of guard-lowered box patterns still builds on native | 0.57.2 | active | fixture | 1 |
| C-296 | The generated type x operation matrix runs byte-identical across targets | 0.57.2 | active | fixture | 70 |
| C-297 | flat_map whose closure returns an aliased list field is drop-balanced on both targets | 0.57.2 | active | fixture | 1 |
| C-298 | The Codec conformance corpus round-trips byte-identical across targets | 0.57.2 | active | fixture | 17 |
| C-299 | hash digests are byte-identical across targets | 0.58.0 | active | fixture | 1 |
| C-300 | map literals with Option-scalar values cross targets, leak-free | 0.59.0 | active | fixture | 2 |
| C-301 | map.upsert on the all-String map crosses targets | 0.59.0 | active | fixture | 1 |
| C-302 | Numeric determinism family head: a Float result is a function of the program alone | 0.58.0 | active | fixture | 6 |
| C-303 | Float operations round to nearest-even once each — no contraction, no FMA | 0.58.0 | active | fixture | 1 |
| C-304 | Subnormal floats are preserved — no flush-to-zero on any target | 0.58.0 | active | fixture | 1 |
| C-305 | Transcendental accuracy bounds: sqrt correctly rounded, exp/log/log2/log10/sin/cos/tan/pow within 1 ulp | 0.58.0 | active | fixture | 1 |
| C-306 | Signed zero: IEEE propagation, equality ignores the sign, min/max order -0 below +0 (IEEE 754-2019) | 0.60.0 | active | fixture | 2 |
| C-307 | int.from_saturating of a Float truncates toward zero and saturates, NaN to 0; the from_checked family is exact-or-none | 0.58.0 | active | fixture | 2 |
| C-308 | @bounded is a function attribute that changes nothing about types or values — a bounded function is an ordinary function | 0.60.0 | active | fixture | 1 |
| C-309 | Subset, not dialect: a program's observable behaviour is identical with and without @bounded | 0.60.0 | active | fixture | 2 |
| C-310 | Only counted loops with compile-time-constant range bounds are admissible in a @bounded function (E070) | 0.60.0 | active | fixture | 0 |
| C-311 | No allocation inside a counted loop in a @bounded function (E071) | 0.60.0 | active | fixture | 0 |
| C-312 | break and continue are rejected inside a @bounded function's loops (E072) | 0.60.0 | active | fixture | 0 |
| C-313 | Recursion is rejected in the bounded profile — the reachable call graph is a DAG (E073) | 0.60.0 | active | fixture | 0 |
| C-314 | A @bounded function calls only @bounded functions and first-order pure stdlib members (E074) | 0.60.0 | active | fixture | 0 |
| C-315 | Run-time-length heap construction is rejected in a @bounded function (E075) | 0.60.0 | active | fixture | 0 |
| C-316 | A @bounded effect fn may only use standard output; other effect modules, host-reaching modules and fan are rejected (E076) | 0.60.0 | active | fixture | 0 |
| C-317 | Float operations are provisionally rejected in a @bounded function (E077) | 0.60.0 | active | fixture | 0 |
| C-318 | Early exit inside a counted loop body is rejected in a @bounded function (E078) | 0.60.0 | active | fixture | 0 |
| C-319 | A captured-and-mutated var List is shared storage through a closure | 0.60.0 | active | fixture | 12 |
| C-320 | A budget cut performs exit bookkeeping: exhausted is always Err, regions are independent, cut placement is unobservable | 0.60.0 | active | fixture | 2 |
| C-321 | The fan prefetch split (start-all, await-in-arm-order, abandon-losers) is unobservable | 0.60.0 | active | fixture | 1 |
| C-322 | Constructors are function values, the builtin trio included | 0.60.0 | active | fixture | 1 |
| C-323 | Or-patterns: an arm's alternatives match as a union, binder-free | 0.60.0 | active | fixture | 7 |
| C-324 | Mut-param forwarding from a branch arm: folded to value flow or refused, never silently wrong | 0.60.0 | active | fixture | 2 |
| C-325 | bytes.push growth is amortized: linear retention, byte-identical at scale | 0.60.0 | active | fixture | 1 |
| C-326 | Branch-selected mut-fn calls hoist their write-back out of the arm: the gzip-inflate shape lowers structurally | 0.60.0 | active | fixture | 1 |
| C-327 | env.sleep_ms pauses and returns on every leg | 0.60.1 | active | fixture | 3 |
| C-328 | The http string family answers identically on the native and embedded-wasm lanes | 0.61.1 | active | fixture | 0 |
| C-329 | env.set is observed by env.get identically on every leg, and the p1 shim serves the environ/args ops | 0.62.0 | active | fixture | 1 |
| C-330 | The http string family answers on the stock runtime through the p3 component's async-lowered exchange | 0.62.0 | active | fixture | 0 |
| C-331 | zlib is self-hosted on wasm: decoding is byte-identical, encoding is round-trip-exact with leg-dependent bytes | 0.62.0 | active | fixture | 1 |
| C-332 | List-rest patterns: the tail past the prefix binds as a list, identically on every leg | 0.62.0 | active | fixture | 2 |
| C-333 | As-patterns: the whole value binds beside its own destructuring, identically on every leg | 0.62.0 | active | fixture | 1 |
| C-334 | A closure call takes +1 on a borrowed heap argument: the RC-3 callee-owned convention holds through call_indirect | 0.62.0 | active | fixture | 4 |
| C-335 | path.extension treats a leading dot as a hidden-file marker, not an extension, on both targets | 0.62.0 | active | fixture | 2 |
| C-336 | Each test starts from re-initialized mutable module globals, on both targets | 0.57.1 | active | fixture | 0 |
| C-337 | The bytes search, edit and predicate family answers identically on every leg | 0.62.0 | active | fixture | 2 |
| C-338 | The int bit family, float32.from_bits and the Int -> Float32 convert agree on every leg | 0.62.0 | active | fixture | 2 |
| C-339 | testing.assert_snapshot mismatches abort in the T18 form on both targets, inside test blocks too | 0.62.0 | active | fixture | 1 |
| C-340 | The eager *_or family — option/result.unwrap_or — evaluates its default before the selection on both targets | 0.62.0 | active | fixture | 2 |
| C-341 | matrix.from_bytes_f16_le / _f32_le / _f64_le answer none when the data does not fit, judged on the buffer side so an offset near i64::MAX is none | 0.62.0 | active | fixture | 2 |
| C-342 | bytes.copy_from with a negative length or offset copies nothing on both targets | 0.62.0 | active | fixture | 1 |
| C-343 | A tail `!` over a declared-Result plain callee yields the payload in a lifted effect fn | 0.63.0 | active | fixture | 1 |
| C-344 | guard-else-err inside a call-argument block after a Result bind runs on every leg | 0.63.0 | active | fixture | 1 |
| C-345 | A Result passed as a call argument keeps its Result at every parameter index, generic parameters included | 0.63.0 | active | fixture | 3 |
| C-346 | A heap-payload ?? lowers on the incumbent leg as a call argument and as a match subject | 0.63.0 | active | fixture | 1 |
| C-347 | Operator round trips over a known literal print that literal on both targets | 0.63.0 | active | fixture | 1 |
| C-348 | Byte-indexed string slices validate UTF-8 boundaries | 0.63.0 | active | fixture | 1 |
| C-349 | A `??` inside a fan arm keeps its default on both targets | 0.63.0 | active | fixture | 1 |
| C-350 | process.exit passes the exit status 0..=255 through, rejects any other code, and a WASI preview-1 build walls 126..=255 | 0.63.0 | active | fixture | 3 |
| C-351 | literal process exit codes are checked against the exit-status domain | 0.63.0 | active | fixture | 0 |
| C-352 | Or-pattern guards are evaluated once per matching alternative, in source order | 0.63.0 | active | fixture | 3 |
| C-353 | A matrix kernel's shape precondition is a defined abort on both targets | 0.63.0 | active | fixture | 1 |
| C-354 | A matrix row range clamps like list.slice and a part count of 0 or less is the empty result | 0.63.0 | active | fixture | 1 |
| C-355 | Matrix negation `-m` is IEEE negation, so the sign of a zero flips on both targets | 0.63.0 | active | fixture | 1 |
| C-356 | A non-finite Float in a Value stringifies as the JSON null on every leg | 0.63.0 | active | fixture | 1 |
| C-357 | value.get_int is Int-only: a Float field is none, never truncated | 0.63.0 | active | fixture | 1 |
| C-358 | float.round, math.choose, int.rotate_* and math.log_gamma are exact over their whole domain on every leg | 0.63.0 | active | fixture | 4 |
| C-359 | Pre-epoch timestamps floor to the day they belong to, and %Y renders any year | 0.63.0 | active | fixture | 1 |
| C-360 | datetime.parse_iso applies the UTC offset and refuses an out-of-range field by name | 0.63.0 | active | fixture | 2 |
| C-361 | url.parse validates the host after the host:port split, and refuses the out-of-scope authorities by name | 0.63.0 | active | fixture | 1 |
| C-362 | A `scoped` block's observables are its body's — the region is not observable | 0.64.0 | active | fixture | 1 |
| C-363 | A `scoped fn` computes the same answer inside and outside a region | 0.64.0 | active | fixture | 1 |
| C-364 | A shape outside the scoped fragment is refused at check time, identically on both targets | 0.64.0 | active | fixture | 0 |
| C-365 | A generic protocol with explicit conformance dispatches to the one declared implementation, identically on both targets | 0.64.0 | active | fixture | 1 |
| C-366 | http call handle: per-call limits name themselves when they fire, cancel closes the connection, poll never blocks | 0.64.0 | active | fixture | 0 |
| C-367 | http.serve runs on the embedded wasm lane with native's one-instance, sequential semantics and the same status, header set and body | 0.64.0 | active | fixture | 0 |
| C-368 | http router: a handler is a function, the most specific route answers, a broken table is refused, 404/405/400 come from the router, identically on both targets | 0.64.0 | active | fixture | 0 |
| C-369 | a lambda's failure channel carries the error type its ! operands agree on; a String channel carries a typed error as its interpolation text, identically on both targets | 0.65.0 | active | fixture | 1 |
| C-370 | An http header that would split the request, or that the client manages, is refused with the same err on every lane | 0.66.0 | active | fixture | 0 |
| C-371 | Float32 arithmetic rounds every result to binary32 on every leg | 0.66.0 | active | fixture | 1 |
| C-372 | A Float32 displays as the shortest decimal that round-trips to the same binary32 | 0.66.0 | active | fixture | 2 |
| C-373 | var destructures with the let patterns and every bound name is mutable, identically on both targets | 0.67.0 | active | fixture | 1 |
| C-374 | process on the embedded wasm host answers what native answers; a stand-alone artifact that starts a child carries the private almide:process/spawn import and is refused at load where it is not defined | 0.67.0 | active | fixture | 0 |
| C-375 | a serve-shaped program built for stock wasm is a wasi:http/handler@0.3.0 export that wasmtime serve runs with no flags and that answers what native answers | 0.67.0 | active | fixture | 0 |
| C-376 | A Region holds the Bytes it views, so the block outlives every use of the view | 0.67.0 | active | fixture | 1 |
| C-377 | net tcp: loopback, handle numbering, EOF, close, is_open and tcp_local_port agree on native, the embedded wasm lane and the stock WASI 0.3 component | 0.68.0 | active | fixture | 0 |
| C-378 | net tcp: every failure reads as errno-free classified text from one table, identical on native, the embedded wasm lane and the stock WASI 0.3 component | 0.68.0 | active | fixture | 0 |
| C-379 | net tcp: timeouts and tcp_available are guest-enforced on the stock WASI 0.3 component and answer what native and the embedded wasm lane answer | 0.68.0 | active | fixture | 0 |
| C-380 | An object's pairs built into a Map hold one entry per key: a repeated object key keeps its first position and takes its last value, on every leg | 0.68.0 | active | fixture | 1 |
| C-381 | The testing asserts' failures outside test blocks abort in the T18 form on both targets | 0.68.0 | active | fixture | 8 |
| C-382 | net unix sockets and shm files: bytes, descriptors received, readiness and shm contents agree on native and the embedded wasm lane; the stock artifact refuses them | 0.68.0 | active | fixture | 0 |
| C-383 | net unix sockets and shm files: every failure is an err with the same text on native and the embedded wasm lane, and a descriptor the program does not hold is refused before any system call | 0.68.0 | active | fixture | 0 |
| C-384 | testing.assert_aborts runs only its capture-free closure in a fresh child and passes iff the child ends in a defined abort whose text contains the expected substring, identically on native and wasm | 0.68.0 | active | fixture | 0 |
| C-385 | The http client frames the request body itself: a caller's Content-Length header is dropped on every lane | 0.68.0 | active | fixture | 0 |
| C-386 | A record-form fan block is the record literal of its fields, run as parallel siblings | 0.68.0 | active | fixture | 2 |
| C-387 | Identifiers are UAX #31 names held in NFC; compatibility characters are refused; a caseless name is type-side only by declaration; non-ASCII names run byte-identically on every leg | 0.68.0 | active | fixture | 1 |
| C-388 | A bare call names the definition of its own scope, never a same-spelled fn of the entry program or another module | 0.68.0 | active | fixture | 2 |
| C-389 | http.serve reads up to 64 connections at once and answers a request head not whole within 10 s with 408, so a slow client holds only its own connection | 0.69.0 | active | fixture | 0 |
| C-390 | The zlib decoders bound their output: past the bound they stop with one err text on every leg | 0.69.0 | active | fixture | 1 |
| C-391 | json.parse bounds nesting at 1000 levels with one err text on every leg | 0.69.0 | active | fixture | 1 |
| C-392 | fs.open_root's family keeps every path inside its root: a path that leaves it by .., an absolute path or a symlink is refused with one message on every target | 0.69.0 | active | fixture | 1 |
| C-393 | [permissions] fs bounds every fs call to the granted directories, symlinks resolved, with one refusal on native and the embedded wasm host | 0.69.0 | active | fixture | 0 |
| C-394 | random.bytes draws n bytes from the platform CSPRNG: n = 0 is empty, a negative n aborts with one text on every leg | 0.69.0 | active | fixture | 2 |
| C-395 | regex.new aborts and regex.compile answers err, with one reason on every leg, for a pattern outside the engine's grammar or past 20,000 states | 0.69.0 | active | fuzz(300) | 3 |
| C-396 | fs.walk and fs.glob list a symlink as an entry and never descend into it, on every leg | 0.69.0 | active | fixture | 0 |
| C-397 | fs.create_temp_file and fs.create_temp_dir create a new entry exclusively under an entropy name, private to its owner | 0.69.0 | active | fixture | 0 |
| C-398 | path.join_within admits only a child that stays inside the base it is joined to, and refuses any other child | 0.69.0 | active | fixture | 1 |
| C-399 | io.write and io.write_bytes emit exactly their bytes on every target and host | 0.68.0 | active | fixture | 1 |
| C-400 | json.parse refuses a lone surrogate, an unknown or short escape, a raw control character, a noncharacter and an unterminated string, never deleting input | 0.69.0 | active | fixture | 2 |
| C-401 | json.parse accepts exactly one RFC 8259 value: a duplicate key, trailing data, an unquoted key, a trailing comma, a missing separator, non-JSON whitespace and a malformed number are errs | 0.69.0 | active | fixture | 3 |
| C-402 | url.parse cleans the input as WHATWG does, validates the scheme at the start, refuses a backslash, a control character, a % in the host and a non-canonical numeric host, and removes dot segments, %2e included | 0.69.0 | active | fixture | 2 |
| C-403 | url.resolve cleans the reference by url.parse's rules and runs every path through the one dot-segment rule, %2e included | 0.69.0 | active | fixture | 1 |
| C-404 | http.query_params is url.parse_query of the target after its first ?, one decoder on every leg, every pair kept in order and a bad escape an err | 0.69.0 | active | fixture | 1 |
| C-405 | the socket host answers 400 to a request head RFC 9112 refuses: a bare LF or CR, a malformed request line, a non-origin target, whitespace before a header colon, a colon-less line, obs-fold, a missing or repeated Host | 0.69.0 | active | fixture | 0 |
| C-406 | the router and the middleware before it read one canonical path: a non-canonical path and a parameter that decodes to a traversal are 400 | 0.69.0 | active | fixture | 0 |
| C-407 | every http.serve response carries X-Content-Type-Options: nosniff unless the handler set the field | 0.69.0 | active | fixture | 0 |
| C-408 | the buffered http client refuses a response that ended before its framing did, and caps the response head, as the call-handle client does | 0.69.0 | active | fixture | 0 |
| C-409 | a handler's err never reaches the client: a fixed 500 body names the request's number, and stderr gets the err quoted under it | 0.69.0 | active | fixture | 0 |
| C-410 | datetime.parse_iso refuses a year outside 0..=9999 by name instead of answering a wrapped instant | 0.69.0 | active | fixture | 1 |
| C-411 | base64.decode and decode_url accept only the canonical padded form of their own alphabet | 0.69.0 | active | fixture | 1 |
| C-412 | Case mapping never folds a non-ASCII scalar into ASCII: string.to_upper, to_lower and capitalize leave such a scalar unchanged | 0.69.0 | active | fixture | 2 |
| C-413 | int.parse and float.parse accept only finite decimal text trimmed of ASCII whitespace | 0.69.0 | active | fixture | 6 |
| C-414 | int.from_checked and int.from_saturating complete the Float to Int conversion trio | 0.69.0 | active | fixture | 1 |
| C-415 | random.int, float, choice and shuffle draw from OS entropy on native and the embedded wasm lane: draws in one clock tick differ, and threads started together draw different streams | 0.69.0 | active | fixture | 0 |
| C-416 | net reads and received descriptors are bounded: a read never pre-allocates its length, and at most 28 untaken descriptors wait per Unix socket | 0.69.0 | active | fixture | 0 |
| C-417 | process.kill asks the kernel with kill(2), refuses a pid outside 1..=2147483647, answers fixed err texts, and is bounded by [permissions] proc as the kill command | 0.69.0 | active | fixture | 0 |
| C-418 | a runtime error text escapes the control and bidi characters of the user data it embeds, with one rule and the same bytes on every leg | 0.69.0 | active | fixture | 4 |
| C-419 | bigint text in a radix that is not a power of two is limited to 4300 digits with one fixed text, a power-of-two radix has no limit, and bigint.pow refuses a result sure to pass 30 * 2^28 bits before computing | 0.69.0 | active | fixture | 1 |
| C-420 | list.range_step lists every Int start + i * step strictly before end, with a count that does not wrap at the ends of Int | 0.69.0 | active | fixture | 1 |
| C-421 | env.set refuses an empty name, `=` or NUL in the name and NUL in the value with the same err on every leg, and stores nothing | 0.69.0 | active | fixture | 1 |
| C-422 | hash.hmac_sha256/512 take a Secret[Bytes] key, answer a Secret[Bytes] tag equal to the RFC 4231 vectors on every leg, key first, and is_valid_ accepts exactly the right tag | 0.69.0 | active | fixture | 1 |
| C-423 | bytes/string.is_equal_constant_time answer equality with no data-dependent branch | 0.69.0 | active | fixture | 1 |
| C-424 | md5 and sha1 are not in the stdlib: a call to hash.md5, md5_hex, sha1 or sha1_hex is a check-time error whose hint names SHA-256 | 0.69.0 | active | fixture | 0 |
| C-425 | A value prints through its type's declared repr wherever it is rendered; a Secret prints Secret(***) on every path | 0.69.0 | active | fixture | 1 |
| C-426 | == on Secret[String] and Secret[Bytes] compares the revealed values; any other Secret, and a variant or map holding one, is E103 | 0.69.0 | active | fixture | 1 |
| C-427 | A Codec record decodes a Secret field from its value's wire form and derives no encode (E023) | 0.69.0 | active | fixture | 1 |
| C-428 | A `@skip` record field is neither decoded nor encoded and a `@skip_decode` field is never decoded but still encoded; both need a default | 0.69.0 | active | fixture | 1 |
| C-429 | A derived `T.decode_strict` refuses the first key outside the record's decoded fields with `unknown key '<k>'`; `T.decode` stays lenient | 0.69.0 | active | fixture | 2 |
| C-430 | An attribute on a record field that the compiler does not read is a check error (E053); the field vocabulary is @builtin, @location, @skip, @skip_decode | 0.69.0 | active | fixture | 0 |
| C-431 | the fs whole-file readers read a regular file of at most 1 GiB: a device, FIFO or socket errs Not a regular file without blocking, a larger file errs File too large, on every leg | 0.69.0 | active | fixture | 1 |
| C-432 | A runtime-backed value (HttpRequest, HttpResponse, HttpCall) interpolates as its declared repr, bare and nested, on every leg; the http reprs count the headers and never show a header value | 0.69.0 | active | fixture | 2 |
| C-433 | value.set_path refuses to build a value nested deeper than json.parse accepts, with parse's err text, on every leg | 0.69.0 | active | fixture | 1 |
| C-434 | a runtime abort inside an http.serve handler fails only that request: the fixed C-409 500, one stderr line, and the server keeps serving | 0.69.0 | active | fixture | 0 |
| C-435 | `Codec` is `Encode + Decode`: a `: Decode` type derives only `decode` (+ a record's `decode_strict`), a `: Encode` type only `encode`, and an input type skips no field | 0.69.0 | active | fixture | 1 |
| C-436 | `http.decode_json[T](req)` decodes a request body only into an input type: a `Codec` T is E110, and a key outside T is `400 Bad Request: unknown key '<k>'` | 0.69.0 | active | fixture | 2 |
| C-437 | A process.start child neither waited on nor stopped is stopped when the program exits by main's return, process.exit or an effect error | 0.69.0 | active | fixture | 0 |
| C-438 | env.secret(name) reads an environment variable as Option[Secret[String]]: env.get's value, rendered Secret(***), compared in constant time | 0.69.0 | active | fixture | 1 |
| C-439 | env.vars lists the environment keyed and ordered by name with the program's own set and remove applied, and env.remove unsets a name or refuses it as env.set does, with the same bytes on every leg | 0.69.0 | active | fixture | 1 |
| C-441 | uuid.v7 carries version 7 and the RFC 9562 variant, spells as 8-4-4-4-12 lowercase hex, and each one is strictly greater than the last on every leg, in one millisecond too | 0.69.0 | active | fixture | 1 |

