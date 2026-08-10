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

Evidence classes (weakest → strongest): `doc-only` < `by-construction` <
`fixture` < `fuzz` < `exhaustive` < `lean`. An **active** contract must carry
≥1 evidence of class ≥ `fixture`.

221 contracts

| ID | Contract | Since | Status | Strongest Evidence | # Fixtures |
|----|----------|-------|--------|--------------------|-----------:|
| C-001 | Integer division/modulo by zero is total — it aborts, never traps | 0.24.0 | active | fixture | 3 |
| C-002 | Signed MIN / -1 overflow aborts, at the TRUE per-width MIN | 0.24.0 | active | fixture | 3 |
| C-003 | Non-aborting integer div/mod stay byte-identical | 0.24.0 | active | fixture | 1 |
| C-004 | fan.any / fan.map / fan.settle are deterministic by list order | 0.24.0 | active | fixture | 4 |
| C-005 | fan error propagation surfaces as the unified main-error abort | 0.24.0 | active | fixture | 4 |
| C-006 | [fan.timeout does not exist — wall-clock deadlines live at the host boundary](C-006-fan-timeout-removed.md) | 0.29.0 | active | fixture | 0 |
| C-007 | Abortable top-level lets evaluate eagerly at startup | 0.24.0 | active | fixture | 2 |
| C-008 | [Compound interpolation renders the Almide-literal repr (containers)](C-008-009-010-repr.md) | 0.24.0 | active | fixture | 2 |
| C-009 | [Record / variant / anonymous-record interpolation repr (field sorting)](C-008-009-010-repr.md) | 0.24.0 | active | fixture | 2 |
| C-010 | [Recursive / generic ADT interpolation repr keyed by instantiation](C-008-009-010-repr.md) | 0.24.0 | active | fixture | 2 |
| C-011 | Bare-float interpolation Display drops .0; float.to_string keeps it | 0.24.0 | active | fixture | 5 |
| C-012 | Const-folded non-finite floats emit named constants | 0.24.0 | active | fixture | 1 |
| C-013 | Map is a compact-ordered-dict: iteration is insertion order | 0.24.0 | active | fixture | 3 |
| C-014 | Set is insertion-ordered and deterministic | 0.24.0 | active | fixture | 1 |
| C-015 | Structural deep equality for compound elements and heap values | 0.24.0 | active | fixture | 3 |
| C-016 | UTF-8 codepoint-aware string ops are byte-identical | 0.24.0 | active | fixture | 2 |
| C-017 | Empty-pattern count / last_index_of follow native codepoint/byte semantics | 0.24.0 | active | fixture | 1 |
| C-018 | Unicode string predicates match Rust char methods over the full domain | 0.24.0 | active | fixture | 1 |
| C-019 | rt_string_extra ops (replace_first, strip_*, predicates, cmp) match native | 0.24.0 | active | fixture | 2 |
| C-020 | Unicode case transforms (to_upper/to_lower/capitalize) are full-Unicode | 0.24.0 | active | fixture | 1 |
| C-021 | Whitespace trim / is_whitespace use the full Unicode White_Space property | 0.24.0 | active | fixture | 1 |
| C-022 | string.from_bytes is UTF-8-lossy decode (inverse of to_bytes) | 0.24.0 | active | fixture | 1 |
| C-023 | float.to_string is shortest round-tripping decimal (Dragon4) | 0.24.0 | active | fixture | 2 |
| C-024 | float.parse is correctly-rounded round-to-nearest-even (Clinger AlgorithmM) | 0.24.0 | active | fixture | 1 |
| C-025 | float.to_fixed is round-half-to-even on the exact binary value | 0.24.0 | active | fuzz(1000) | 1 |
| C-026 | Vendored-libm trig / exp / log / pow are byte-identical cross-target | 0.24.0 | active | fuzz(4000) | 3 |
| C-027 | base64 encode/decode (standard + URL-safe) is byte-identical incl. errors | 0.24.0 | active | fixture | 1 |
| C-028 | int.from_hex mirrors i64::from_str_radix incl. native quirks | 0.24.0 | active | fixture | 1 |
| C-029 | int.parse error modes byte-match native ParseIntError | 0.24.0 | active | fixture | 1 |
| C-030 | hex.encode / hex.decode are byte-identical incl. positional error detail | 0.24.0 | active | fixture | 2 |
| C-031 | json get/set/remove_path edge cases match the infallible native oracle | 0.24.0 | active | fixture | 2 |
| C-032 | Regex engine is byte-identical to the native engine over a fuzzed grammar | 0.24.0 | active | fuzz(220) | 2 |
| C-033 | [Value semantics for aliased mutables (copy-on-write)](C-033-cow-truth-table.md) | 0.24.0 | active | fixture | 2 |
| C-034 | Out-of-range list ops clamp / no-op gracefully (no OOB heap access) | 0.24.0 | active | fixture | 8 |
| C-035 | Effect-main errors terminate uniformly: Error: <msg> + exit 1 | 0.24.0 | active | fixture | 4 |
| C-036 | Records, variants, and pattern matching are byte-identical | 0.24.0 | active | fixture | 6 |
| C-037 | bytes.read_f16_le decodes IEEE-754 half floats identically | 0.24.0 | active | fixture | 1 |
| C-038 | Sized-integer literals narrow to the declared field width | 0.24.0 | active | fixture | 2 |
| C-039 | Type-changing map.map / set.map yield a collection of the new type | 0.24.0 | active | fixture | 2 |
| C-040 | Codegen emit is host-architecture deterministic | 0.24.0 | active | fixture | 3 |
| C-041 | Heap / RC primitives honour the Lean-certified Perceus discipline | 0.24.0 | active | lean | 5 |
| C-042 | fs preopen-dir scan + path resolution is observable-equivalent | 0.24.0 | active | fixture | 1 |
| C-043 | A user type named Box coexists with recursive-enum heap indirection | 0.24.0 | active | fixture | 1 |
| C-044 | Result/Option construction and matching are byte-identical | 0.24.0 | active | fixture | 3 |
| C-045 | A List[String] param works across join / len / index / iteration | 0.24.0 | active | fixture | 3 |
| C-046 | Record spread-update and cross-module monomorphization are byte-identical | 0.24.0 | active | fixture | 1 |
| C-047 | math.pow negative exponent and rotate non-positive width are total — they abort, never trap/wrap | 0.24.0 | active | fixture | 3 |
| C-048 | int.wrap_* / int.rotate_* saturate the mask to u64::MAX for bits >= 64 | 0.24.0 | active | fixture | 1 |
| C-049 | float.sign is f64::signum; float/math min/max ignore NaN | 0.24.0 | active | fixture | 1 |
| C-050 | string.split(\"\") and string.run_length_encode are codepoint-granular | 0.24.0 | active | fixture | 1 |
| C-051 | math.log_gamma is bit-identical (both targets use the vendored musl-libm log) | 0.24.0 | active | fixture | 1 |
| C-052 | A fold over an empty collection requires the collection to carry an element type (no codegen defaulting) | 0.24.0 | active | fixture | 1 |
| C-053 | list.min/max/sort/sort_by/unique_by are type-directed and total, native == wasm | 0.24.0 | active | fixture | 1 |
| C-054 | List/string Int counts and indices are i64-clamped before narrowing — no truncation, no OOB | 0.24.0 | active | fixture | 4 |
| C-055 | list.min/max/sort/sort_by over Float use IEEE-754 totalOrder, valid + identical on both targets | 0.24.0 | active | fixture | 2 |
| C-056 | list.product wraps on i64 overflow, consistent with list.sum and plain `*` | 0.24.0 | active | fixture | 1 |
| C-057 | Assigning a Unit-returning in-place mutator's result is a checker error on both targets | 0.24.0 | active | fixture | 1 |
| C-058 | An empty collection with an uninferable element type is a compile error on both targets, never silently defaulted | 0.24.0 | active | fixture | 1 |
| C-059 | Compilation does not overflow the native stack on wide or deep input, identically on every host and build profile | 0.25.0 | active | fixture | 1 |
| C-060 | A Value reprs as its JSON text byte-identically on native and WASM, bare and as a Repr-record field | 0.26.7 | active | fixture | 1 |
| C-061 | A mut Map parameter mutated in place builds on both targets and the mutation persists, byte-identical | 0.26.9 | active | fixture | 1 |
| C-062 | The RawPtr / linear-memory bridge moves bytes byte-identically on both targets | 0.26.15 | active | fixture | 1 |
| C-063 | Parsing a heterogeneous-nested glTF/JSON document and walking its arrays by element is byte-identical on both targets | 0.26.19 | active | fixture | 1 |
| C-064 | The effect-fn Result auto-unwrap rule is identical across binding positions and type-directed, byte-identical on both targets | 0.26.20 | active | fixture | 1 |
| C-065 | The string position API is codepoint-indexed end-to-end on both targets | 0.26.20 | active | fixture | 2 |
| C-066 | WASM heap is reclaimed by default (true Perceus) | 0.27.0 | active | fixture | 4 |
| C-067 | The xs[i] index syntax aborts on out-of-bounds (read and write) | 0.27.4 | active | fixture | 4 |
| C-068 | Auto-? is target-directed in construction positions | 0.27.4 | active | fixture | 2 |
| C-069 | Effect-fn tail self-recursion loop-converts to O(1) stack on both targets | 0.27.4 | active | fixture | 1 |
| C-070 | Nested constructor patterns match and bind identically on both targets | 0.27.6 | active | fixture | 2 |
| C-071 | Single-part interpolation RC balance | 0.27.6 | active | fixture | 1 |
| C-072 | Inferred named-record repr parity | 0.27.6 | active | fixture | 1 |
| C-073 | Tuple pattern testing a variant constructor | 0.27.6 | active | fixture | 1 |
| C-074 | Iterative split/replace on large inputs | 0.27.6 | active | fixture | 1 |
| C-075 | lowmisc round-5 cluster: borrowed-param owning binding, effect-Option auto-try strip, matching-error ! passthrough | 0.27.6 | active | fixture | 1 |
| C-076 | Producer-side in-module variant construction is target-stable | 0.27.6 | active | fixture | 1 |
| C-077 | Cross-module heap-global init order is dependency-respecting | 0.27.6 | active | fixture | 1 |
| C-078 | Phantom record generic param is stripped on the Rust target | 0.27.6 | active | fixture | 1 |
| C-079 | Variant cases with distinct anonymous-record payloads are target-stable | 0.27.6 | active | fixture | 1 |
| C-080 | Empty map.from_list / set.from_list resolves its element from the result type | 0.27.6 | active | fixture | 1 |
| C-081 | Generic fn in an inferred-param lambda resolves its type parameter | 0.27.6 | active | fixture | 1 |
| C-082 | Calling a closure-typed lambda parameter yields the call result, not the closure | 0.27.6 | active | fixture | 1 |
| C-083 | A negated i64::MIN literal is representable, not folded to zero | 0.27.6 | active | fixture | 1 |
| C-084 | Codec/value decode error messages are byte-identical across targets | 0.27.6 | active | fixture | 1 |
| C-085 | Float decode widens an integral JSON number to f64 | 0.27.6 | active | fixture | 1 |
| C-086 | Pass-through stdlib combinators give their result its own reference | 0.27.6 | active | fixture | 1 |
| C-087 | JSON number and \\u string decoding are byte-identical across targets | 0.27.6 | active | fixture | 2 |
| C-088 | A Rust-keyword function name compiles on both targets | 0.27.6 | active | fixture | 1 |
| C-089 | A default parameter referencing an earlier parameter is filled with its argument | 0.27.6 | active | fixture | 1 |
| C-090 | bytes.from_list on a List[Int] parameter compiles on both targets | 0.27.6 | active | fixture | 1 |
| C-091 | A nested sub-pattern in let-destructuring binds every leaf | 0.27.6 | active | fixture | 1 |
| C-092 | A generic record field is sized by its instantiated type at construction | 0.27.6 | active | fixture | 1 |
| C-093 | Mutually-recursive variant types compile on both targets | 0.27.6 | active | fixture | 1 |
| C-094 | A protocol-method UFCS call on an inferred lambda param resolves the element type | 0.27.6 | active | fixture | 1 |
| C-095 | json.stringify_pretty is byte-identical indented output across targets | 0.27.6 | active | fixture | 1 |
| C-096 | process.args works on WASM and matches native | 0.27.6 | active | fixture | 1 |
| C-097 | generic + on a type parameter concatenates strings/lists identically across targets | 0.27.6 | active | fixture | 1 |
| C-098 | cross-module derived Codec methods dispatch on WASM and match native | 0.27.6 | active | fixture | 0 |
| C-099 | comparison/equality operators byte-match native across all operand types on the v1 wasm path | 0.27.6 | active | fixture | 9 |
| C-100 | Self-hosted String classification/transform ops byte-match native on wasm | 0.27.6 | active | fixture | 4 |
| C-101 | List ops over heap elements (String/Value) byte-match native and are leak/double-free free | 0.27.6 | active | fixture | 11 |
| C-102 | List iteration, call-result element materialization, and tail-recursive list traversal byte-match native | 0.27.6 | active | fixture | 3 |
| C-103 | Self-hosted dynamic Value model (merge, array/as_array roundtrip, tuple TCO) byte-matches native and is leak-free in a loop | 0.27.6 | active | fixture | 5 |
| C-104 | Tail-recursive accumulator shapes lower to bounded-stack loops byte-matching native | 0.27.6 | active | fixture | 6 |
| C-105 | var/append accumulator loops (scalar, owned-handle, cross-dep, mutual-recursion) byte-match native on wasm | 0.27.6 | active | fixture | 5 |
| C-106 | Heap value bound from an if/match arm byte-matches native on the v1 wasm path | 0.27.6 | active | fixture | 12 |
| C-107 | heap Result-of-tuple / Result-of-list Ok payloads round-trip and byte-match native | 0.27.6 | active | fixture | 4 |
| C-108 | Unwrap `!` and let-unwrap desugaring byte-match native in every position | 0.27.6 | active | fixture | 6 |
| C-109 | Self-hosted base64 encode byte-matches canonical / native on the v1 wasm path | 0.27.6 | active | fixture | 1 |
| C-110 | In-place bytes.push mutation accumulator byte-matches native on v1 wasm | 0.27.6 | active | fixture | 1 |
| C-111 | Module-level const heap globals initialize and read identically on v1 wasm and native | 0.27.6 | active | fixture | 1 |
| C-112 | random.int draws stay in-range identically under the WASI entropy floor on v1 wasm | 0.27.6 | active | fixture | 1 |
| C-113 | Let-bound ADT/Result variant matched by tag byte-matches native on v1 wasm | 0.27.6 | active | fixture | 1 |
| C-114 | Matching an Option with a heap payload byte-matches native on v1 wasm | 0.27.6 | active | fixture | 1 |
| C-115 | Pipe into a block-bodied lambda producing a value byte-matches native on v1 wasm | 0.27.6 | active | fixture | 1 |
| C-116 | v1 scalar-value lowering edges byte-match native (tail Bool literal, float.parse inf/nan) | 0.27.7 | active | fixture | 2 |
| C-117 | In-loop let-bound heap if/match is lifted to a tail helper and renders on v1 | 0.27.7 | active | fixture | 1 |
| C-118 | env.args works on WASM and matches native (argv[0] skipped) | 0.27.8 | active | fixture | 1 |
| C-119 | effect-`!` inside a `for` loop body propagates Err and byte-matches native | 0.27.6 | active | fixture | 1 |
| C-120 | capturing filter_map with a conditional keep/skip arm body byte-matches native | 0.27.6 | active | fixture | 1 |
| C-121 | String pass-through fast paths hand back an owned (+1) reference | 0.27.6 | active | fixture | 2 |
| C-122 | Value object ops allocate full list layout and share pairs with +1 | 0.27.6 | active | fixture | 1 |
| C-123 | Record spread shares copied heap fields and alias overrides with +1 | 0.27.6 | active | fixture | 1 |
| C-124 | Value equality is deep structural, mirroring the native PartialEq | 0.27.6 | active | fixture | 1 |
| C-125 | bytes.set has value semantics — never observable through the input | 0.27.6 | active | fixture | 1 |
| C-126 | Nested-lambda HOF params keep their inference link (no literal sig-generic pin) | 0.27.6 | active | fixture | 2 |
| C-127 | unwrap_or sizes its payload from the default when the chain type is unresolved | 0.27.6 | active | fixture | 1 |
| C-128 | datetime.format substitutes strftime specifiers identically on every backend | 0.28.1 | active | fixture | 1 |
| C-129 | list.chunk / list.windows non-positive sizes: negative keeps the promoted norm, zero aborts in the T6 form | 0.28.4 | active | fixture | 4 |
| C-130 | option/map combinators hand back OWNED heap results (no bare pass-through handles) | 0.28.5 | active | fixture | 2 |
| C-131 | Loop-rebuilt buffers are O(n): COW guards only LIVE aliases, and LICM never hoists heap allocations | 0.28.6 | active | fixture | 2 |
| C-132 | mut parameters of reallocating containers persist to the caller at every call position | 0.28.6 | active | fixture | 1 |
| C-133 | env.get observes the host environment identically on native and wasm | 0.29.0 | active | fixture | 1 |
| C-134 | Vendored-libm atan / tanh are byte-identical cross-target | 0.30.0 | active | fuzz(3000) | 1 |
| C-135 | Declared-Unit effect fn ABI agrees between def and every call site | 0.30.0 | active | fixture | 1 |
| C-136 | In-place place mutations persist to the subsequent read on both targets | 0.30.0 | active | fixture | 1 |
| C-137 | Relative fs paths resolve against the host CWD on wasm | 0.31.0 | active | fixture | 1 |
| C-138 | ok/err ctor with a stdlib-call payload materializes the real value | 0.31.0 | active | fixture | 1 |
| C-139 | Heap-Ok Result value combinators keep tag and payload | 0.31.0 | active | fixture | 1 |
| C-140 | float.round preserves the sign of a zero result | 0.31.0 | active | fixture | 1 |
| C-141 | list.zip_with routes by element repr — String zips work, no wrong-typed link | 0.31.0 | active | fixture | 1 |
| C-142 | result.unwrap_or_else is valid wasm at the Float instantiation on both legs | 0.31.0 | active | fixture | 1 |
| C-143 | Ctor if-payloads materialize the taken arm | 0.31.0 | active | fixture | 1 |
| C-144 | A scalar-list literal never observes as a silent empty list | 0.31.0 | active | fixture | 1 |
| C-145 | Mono-suffixed stdlib combinator names route by their base name | 0.31.0 | active | fixture | 1 |
| C-146 | A lifted closure returning a captured alias hands out a co-owned reference | 0.31.0 | active | fixture | 1 |
| C-147 | list.unique_by routes by key repr — String keys dedupe by content | 0.31.0 | active | fixture | 1 |
| C-148 | list.scan stores each intermediate at the accumulator's own width | 0.31.0 | active | fixture | 1 |
| C-149 | unwrap_or_else hands back a co-owned heap Ok payload | 0.31.0 | active | fixture | 3 |
| C-150 | Ctors over a heap var are value copies — the var stays live | 0.31.0 | active | fixture | 1 |
| C-151 | Result combinators with a heap-Ok RESULT never link the scalar impl | 0.31.0 | active | fixture | 1 |
| C-152 | An un-admitted heap call payload in a ctor walls, never zeroes | 0.31.0 | active | fixture | 1 |
| C-153 | Non-test assert failures abort in the T6 form on both targets | 0.31.0 | active | fixture | 3 |
| C-154 | clamp with an invalid range aborts in the T6 form | 0.31.0 | active | fixture | 2 |
| C-155 | to_fixed with out-of-domain decimals aborts in the T6 form | 0.31.0 | active | fixture | 3 |
| C-156 | An if-merged some((String, String)) ctor is a real tracked Option | 0.32.0 | active | fixture | 1 |
| C-157 | An unannotated generic-ctor top-let carries its solved payload type to every reader | 0.32.0 | active | fixture | 1 |
| C-158 | A some/ok ctor over a scalar call or tuple payload materializes the real value, never a zeroed ctor | 0.32.0 | active | fixture | 1 |
| C-159 | list.binary_search returns the same index on both targets for duplicate keys | 0.32.0 | active | fixture | 1 |
| C-160 | Pure-Almide bundled stdlib modules link and run byte-identically on wasm | 0.34.4 | active | fixture | 1 |
| C-161 | Matrix constructor dimensions clamp negatives and abort over a shared ceiling | 0.35.0 | active | fixture | 3 |
| C-162 | io.write / io.write_bytes emit in program order, interleaved with println | 0.35.0 | active | fixture | 1 |
| C-163 | A heap-result if/match bound to a let/var executes the taken arm on both targets | 0.35.0 | active | fixture | 1 |
| C-164 | List modifiers and suffix copies co-own tuple / record / nested-list elements | 0.35.0 | active | fixture | 1 |
| C-165 | fold over a String-keyed map, a String list or a String set threads a heap accumulator on both targets | 0.36.0 | active | fixture | 1 |
| C-166 | Map interpolation renders every self-hosted key/value pairing on both targets | 0.36.0 | active | fixture | 1 |
| C-167 | float.clamp returns its input unchanged when in range, sign bit included | 0.36.0 | active | fixture | 1 |
| C-168 | list.flatten borrows its argument, so it composes with another borrow of the same binding | 0.36.0 | active | fixture | 1 |
| C-169 | list.repeat over the size ceiling aborts in the T6 form on both targets | 0.36.0 | active | fixture | 1 |
| C-170 | Integer arithmetic wraps in every position, including a module-level let | 0.36.0 | active | fixture | 1 |
| C-171 | Byte-offset bound checks do not overflow at the i64 boundary | 0.36.0 | active | fixture | 1 |
| C-172 | unwrap_or over any heap payload yields the same value on both targets | 0.36.0 | active | fixture | 1 |
| C-173 | An integer literal outside what its context can represent is a checker error on both targets, never a silent value | 0.36.0 | active | fixture | 1 |
| C-174 | A tail-recursive Map/Set accumulator keeps its seed on both targets, non-empty seeds included | 0.36.0 | active | fixture | 1 |
| C-175 | A List literal of a variant type builds and drops identically on both targets, in bind and heap-result-if-arm position | 0.37.0 | active | fixture | 1 |
| C-176 | some/ok around an inline tuple-returning call materializes the real payload on both targets | 0.37.0 | active | fixture | 1 |
| C-177 | A mutable-global projection read in a loop-body call argument reads the CURRENT slot every iteration | 0.37.0 | active | fixture | 1 |
| C-178 | A mutual tail-recursion chain runs at unbounded depth on the wasm target | 0.37.0 | active | fixture | 2 |
| C-179 | UInt64 reaches its full declared domain, with every observer reading the slot unsigned | 0.37.0 | active | fixture | 2 |
| C-180 | Sized-integer +, -, * and ^ wrap at the declared width on both targets | 0.37.0 | active | fixture | 1 |
| C-181 | args.positional returns every non-flag argument, and the args surface agrees across targets | 0.37.0 | active | fixture | 1 |
| C-182 | A negated float literal takes its context's float type on both targets | 0.37.0 | active | fixture | 2 |
| C-183 | A loop-carried append of a list-of-scalar-records executes on both targets | 0.37.0 | active | fixture | 1 |
| C-184 | The integer `^` operator is the same total exponentiation as math.pow on both targets | 0.37.0 | active | fixture | 1 |
| C-185 | fan.any returns the first Ok in list order on both targets, whatever position it is in | 0.37.0 | active | fixture | 1 |
| C-186 | Appending an inline index read to a list accumulator lowers and runs on both targets | 0.37.0 | active | fixture | 1 |
| C-187 | An in-place mutator writing through a module-level `var` buffer behaves identically on both targets | 0.37.0 | active | fixture | 3 |
| C-188 | A scalar `if` arm executes its statement effects — global writes land and outer-var reassignments hit the stable local | 0.37.0 | active | fixture | 1 |
| C-189 | env.os and the temp-dir surface report the HOST — the equivalence law's only exemption, and it is closed | 0.37.0 | active | fixture | 1 |
| C-190 | A closure captures any scalar, not just Int and Bool — Float and every sized int width included | 0.37.0 | active | fixture | 1 |
| C-191 | A module-level global reads its true value on every branch path — the materialization memo is scoped to straight-line context | 0.37.1 | active | fixture | 1 |
| C-192 | An auto-unwrapped effect call inside a string interpolation lifts and propagates like a call argument | 0.37.1 | active | fixture | 1 |
| C-193 | Repeated in-place writes through a mutable global stay in place — the receiver borrows the post-COW handle | 0.37.1 | active | fixture | 1 |
| C-194 | bytes.copy_from with a mutable-global destination writes through the storage slot | 0.38.1 | active | fixture | 1 |
| C-195 | Value-position variant match over the checked-conversion family executes | 0.38.1 | active | fixture | 1 |
| C-196 | Call-stack exhaustion is a resource limit, not an observable-behavior promise | 0.41.0 | active | fixture | 1 |
| C-197 | Linear-memory exhaustion is a resource limit with a defined abort | 0.41.0 | active | fixture | 2 |
| C-198 | A head count below 1 is a defined abort, identically on both targets | 0.42.0 | active | fixture | 1 |
| C-199 | A fan block joins every sibling and reports the first Err in list order | 0.42.0 | active | fixture | 1 |
| C-200 | A trap in a fan sibling exits through the unified main-error abort, convergently | 0.42.0 | active | fixture | 1 |
| C-201 | An Option combinator's tuple result is materializable as an owned element for every element-type combination | 0.44.0 | active | fixture | 1 |
| C-202 | Time constructors guard their domain: negative aborts, overflow saturates | 0.47.0 | active | fixture | 3 |
| C-203 | The time-type operator algebra is unit-exact and saturating on both targets | 0.47.0 | active | fixture | 1 |
| C-204 | A fan.bounded verdict is a function of the program and its inputs alone | 0.47.0 | active | fixture | 6 |
| C-205 | The fan.race winner is the (spend, index) lexicographic minimum among admitted arms | 0.47.0 | active | fixture | 6 |
| C-206 | A settle block settles every arm into its own tuple slot, in arm order | 0.47.0 | active | fixture | 1 |
| C-207 | CM-1 is a versioned constant with a ratio-only wall-clock claim | 0.47.0 | active | fixture | 1 |
| C-208 | fan.timeout is omega-relative: fixed ends everywhere, fixed omega replays byte-identically | 0.47.0 | active | fixture | 1 |
| C-209 | Codec encode omits none; decode folds missing/null to none; Value passes through verbatim | 0.52.0 | active | fixture | 1 |
| C-210 | NaN observation is canonical — the deterministic-profile conformance law | 0.53.0 | active | fixture | 2 |
| C-211 | ! propagates in pure Result/Option-returning fns, byte-identical across targets | 0.53.5 | active | fixture | 1 |
| C-212 | Two self-host modules with same-named private helpers link cleanly or wall — never an invalid module | 0.52.0 | active | fixture | 1 |
| C-213 | The bytes byte-level writers encode the same buffer on both targets, out-of-range writes included | 0.53.5 | active | fixture | 1 |
| C-214 | process.exec_status_timeout: the fire-path error is pinned; whether it fires is the host's | 0.53.6 | active | fixture | 0 |
| C-215 | fs content readers: absence is ok(none) via the _if_exists family, classified by the runtime | 0.53.7 | active | fixture | 0 |
| C-216 | explicit ! on a declared-Option effect call is the implicit strip's identical twin | 0.54.1 | active | fixture | 1 |
| C-217 | let _ = f() discards the Result — the err does not propagate | 0.55.0 | active | fixture | 1 |
| C-218 | a heap-payload ?? returned as the fn tail yields the same value on both targets | 0.56.0 | active | fixture | 1 |
| C-219 | a Never-typed call in a branch arm runs on both targets and sets the exit code | 0.56.0 | active | fixture | 1 |
| C-220 | fs streaming line walkers fold/each with read_lines line semantics | 0.56.1 | active | fixture | 0 |
| C-221 | An effect fn-typed slot admits pure and fallible lambdas with one carrier semantics | 0.56.1 | active | fixture | 2 |

