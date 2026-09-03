# The DDD gauntlet

Every shape that broke the incumbent between 2026-08-22 and 2026-08-26, as one
runnable matrix. It came from a single exercise: **write an ordinary
ports-and-adapters program** — value objects, an aggregate root, a repository
protocol, two adapters, an application service, wired at a composition root —
and see what the compiler does with it.

```sh
./run.sh /path/to/almide      # check / native / wasm for every cell
```

24 defects were tracked out of this exercise (#1539, #1547, #1549–#1583);
22 are closed on the incumbent, 2 remain open. The value here is not the
fixes — it is that **none of these were reachable by replaying the conformance
corpus**, and several of them are the same mistake wearing different clothes.

## 1. The closure target moved

`NON-CARRYOVER.md` declares closure as "the burn-up replays the whole 599-fixture
corpus". That corpus was 599 when this exercise started. It is **610
`spec/wasm_cross` fixtures plus 4 new `tests/*.rs` native pins** now, and every
one of the new rows is a defect this exercise found:

```
spec/integration/modules/{convmut,genlib,layports}/…      spec/lang/protocol_fallible_return_test.almd
spec/integration/modules/cross_module_generic_chain_test  spec/lang/mut_param_whole_assign_test.almd
spec/integration/modules/cross_module_layered_ports_test  spec/lang/mut_param_protocol_bound_test.almd
spec/integration/modules/cross_module_mut_receiver_test   spec/lang/effect_option_qq_test.almd
spec/integration/modules/cross_module_receiver_mode_test  spec/wasm_cross/bang_in_record_literal_field.almd
spec/wasm_cross/fallible_tuple_value_tail.almd            spec/wasm_cross/record_pair_result_return.almd
spec/wasm_cross/result_option_call_payload_return.almd    spec/wasm_cross/result_tuple_variant_payload.almd
spec/wasm_cross/result_tuple_list_slots.almd              spec/wasm_cross/unwrap_or_option_payload.almd
spec/wasm_cross/variant_pair_result_return.almd           spec/wasm_cross/qq_record_fallback.almd
spec/wasm_cross/mut_param_effect_never_err.almd           tests/native_mut_param_pins_test.rs
spec/wasm_cross/variant_record_literal_equality.almd      tests/generic_letter_type_name_test.rs
spec/wasm_cross/fuel_cut_in_arm_loop.almd                 tests/nested_package_import_test.rs
```

**The lesson is not "raise the number."** A corpus is a closed enumeration of
what has already been ruled; it cannot contain the defect nobody has written a
program to hit yet. A closure claim resting on it should say so. Greenfield's
version of this gate should carry a second axis: *a program-shaped acceptance
suite that is written against the language, not harvested from the tracker.*
This directory is a starting one.

## 2. The defect classes, and the invariant that kills each

### A. `check` accepts, the backend cannot emit it — the dominant class

The dominant class by far, and the one the mission cares about most: the
compiler tells the writer the program is correct, then hands them rustc errors
about generated code they never wrote.

| cell | what it was |
|---|---|
| `c1_cross_module_method` | a convention method's receiver borrow mode did not cross a module boundary — the caller passed by value against a by-ref definition |
| `c2_mut_param_whole_assign` | `p = Record { … }` on a `mut` param emitted `p = …` against a `&mut` binding |
| `c3_mut_param_protocol_bound` | monomorphizing a `mut` param under a protocol bound dropped its by-ref convention |
| `c4_derived_variant_repr` | `.repr()` on a derived-`Repr` variant called a function the derive never generated |
| `c5_generic_to_generic` | a generic fn calling another module's generic fn never monomorphized the callee |
| `c6_borrowed_param_field_store` | a borrowed `String` param stored into a field/slot emitted `&str` where owned was needed |
| `c7_generic_letter_type_name` | a user type named `T` (or `A B C E F K U V`) was taken as a type variable across a module boundary |
| `c8_generic_applied_at_letter` | applying `go[Q]` at a type named `Q` failed to monomorphize |

**Invariant**: a well-typed program is emittable. If the backend has a shape it
cannot lower, that shape is *rejected at check time with a source-level
diagnostic* — never accepted and then failed downstream. Every row above is the
frontend and the backend disagreeing about what the language is. Greenfield
should be able to state where that agreement is enforced, and have a gate that
fuzzes for disagreement rather than waiting for a user.

### B. Names resolved from the wrong namespace — a recurring family

`c7`/`c8` above, plus `#1558` (a user fn sharing a bare name with any linked
stdlib fn was silently excluded from a rewrite: `fn replace` walled, `fn bump`
ran) and the incumbent's older `#1087`. Same shape every time: **a bare name is
looked up in a table that some other module also writes into.** The affected set
is invisible (it depends on which modules got linked) and unstable (adding an
`import` flips an unrelated function).

**Invariant**: resolution is keyed by a resolved identity, never by a bare
spelling. If a pass must key by name, the key carries its scope. A one-letter
type name is an ordinary name.

### C. Silent wrong value — exactly one

`w1_qq_record_fallback`: `??` over a `Result[record, String]` with a record
literal fallback read the wrong value. This is the only row in the set that
produced a wrong answer rather than a refusal; it was found by *widening a
fixture*, not by the fuzzer.

**Invariant**: a route that cannot handle a payload class declines. The failure
mode of a router should be a wall, never a misread.

### D. Surface holes the writer trips over — 2 fixed, 5 still unfiled

| cell | status |
|---|---|
| `s1_protocol_fallible_return` | fixed (`-> T!` / `-> T!E` were parse errors in a protocol method) |
| `s2_multiline_tuple` | fixed (a tuple literal could not span lines; list/record/map could) |
| `s3_module_qualified_protocol` | **open, unfiled** — `ports.Store` is a parse error in a bound and in a conformance list; the bare name resolves globally, while *types* must be qualified. The rule is exactly inverted between the two |
| `s4_bare_mut_self` | **open, unfiled** — `mut self` is a parse error; `mut self: Self` is required |
| `s5_match_self` | **closed (#1590)** — `self` was on the parser's rejected-ident table and the used-as-identifier lookahead had no `{` case; `self` now parses as the ordinary identifier it is, and the cell checks green |
| `s6_generic_protocol` | **diagnosed honestly (#1590), expressiveness open (#1589)** — the adoption now refuses at the declaration with the root cause (generic-protocol adoption cannot bind `T`) instead of demanding an unimplementable `-> Option[T]`; the Repository abstraction itself still needs #1589 |
| `s7_protocol_as_type` | **diagnosed honestly (#1590), expressiveness open (#1589)** — E029 now says "'Policy' is a protocol, not a type" and the derived `E002`s are suppressed; the `List[Policy]` shape still emits one element-mismatch `E001` first (noted on #1590), and existential dispatch remains #1589 |

**Invariant for greenfield**: these are design decisions, not bugs — decide them
deliberately. Whatever `self` means, it means it in every position. Whatever a
protocol is, it is that uniformly (a bound only, or a type too). Whichever
spelling is canonical, the canonical one parses everywhere the other does.

### E. Cross-target parity is a matrix, and it was closed cell by cell

`x01`–`x16` are the shapes where native ran and the wasm leg had to catch up.
The instructive part is the *shape of the burn-down*: five separate issues
(`#1547 → #1564 → #1578 → #1579 → #1580 → #1581 → #1583`) each closed one cell
of what turned out to be one matrix — payload kind × producer × spelling ×
position. Every time, the closing commit's language generalized ("completing the
matrix cells") past what it had actually covered, and the next probe found the
neighbouring cell.

**Invariant**: a family is closed by an *executable matrix gate that enumerates
its cells*, not by fixing the reported instance. The incumbent already states
this rule for stdlib APIs in `CLAUDE.md` ("API families are extended by matrix,
never point-wise… a gated matrix cannot drift") — it simply was not applied to
codegen families. Greenfield should make the rule structural: a lowering route
declares the cells it covers, and a gate asserts the declaration.

## 3. What the verification process taught — the most transferable part

These cost more to discover than any single bug.

1. **A regression test that runs on the wrong leg pins nothing.** Two of four
   fixtures written for this batch passed on the *pre-fix* compiler, because
   `almide test` runs a file on the wasm leg when the shape fits, and the bugs
   were in native codegen. The repo already had the rule written down — in
   `tests/effect_tail_generic_bound_test.rs`: *"a corpus file would run on the
   wasm leg and assert nothing"* — and it was violated three days later.
   **Every pin must be A/B'd against the broken compiler before it is trusted.**

2. **A fix with no pin stays green when re-broken.** One of four items in a
   batch landed with no fixture and no `proofs/` entry at all; it took an issue
   to get the specimen committed.

3. **A specimen that lives only in a commit message is not a specimen.** For
   that item, five reconstructions from the prose all failed to reproduce the
   wall — the real trigger was narrower than the description. If the repro
   cannot be rebuilt from the record, the record is not evidence.

4. **Point-revert A/B beats full-binary A/B.** Reverting one hunk and watching
   the fixture wall proves single causation; rebuilding the whole compiler at
   the parent commit only proves *something* in that commit mattered. Adopt the
   former as the standard of proof for "this fixture pins this fix".

5. **A wall message must name the cause, not the mechanism.** `#1558`'s wall
   said "mut params are not in this brick" when the actual fix was *rename your
   function*. The writer cannot get from that message to that action.

## 4. Current snapshot (incumbent @ `4e5a15696`)

```
clean rows: 25    rows with a check/native/wasm gap: 9
```

The 9: five unfiled surface holes (`s3`–`s7`), one wasm decline that is
deliberate (`x09`, `List[record]` elements), `x13` (a `!` nested under a
call/operator/interpolation inside a record-literal field — **#1583, open**), and
both layered packages, which are blocked on `x13` (functional port) and on
`#1576` (mut port — the `mut`-param err-path semantics need a ratified answer:
what does the caller's argument hold when the callee returns `err` after
mutating?). `#1576` was ratified and closed on the structural leg (0.61.2):
the err propagates before any write-back, so the caller's slot keeps its
pre-call binding, and `pkg/mut_port` runs byte-identically to native in the
greenfield gate below.

## 5. What to do with this here

- Run `./run.sh` against greenfield as soon as it can host the surface. Every
  row is a shape a real program produced, not a synthesized edge case.
- Treat §2's invariants as design review questions, not as a bug list. Most of
  these classes are unrepresentable in a design that keeps the frontend and the
  backend agreeing on one language, keys resolution by identity, and closes
  families by matrix.
- The five unfiled surface holes (`s3`–`s7`) are open language questions on the
  incumbent. Greenfield gets to answer them once, deliberately, instead of
  inheriting the current accidents.

## 6. The greenfield gate (2026-08-26)

This directory is GATED on the greenfield legs:
`crates/almide-wasm/tests/gauntlet.rs` replays every cell through
spine-front → emit → host and pins each verdict (`run` with a byte-exact
stdout hash, `wall` with the refusal reason, `reject` with the front's
first line) in `crates/almide-wasm/tests/golden/gauntlet-manifest.txt`.
Any drift — regression or improvement — is red until ratified:

```sh
ALMIDE_UPDATE_GAUNTLET=1 cargo test --release -p almide-wasm --test gauntlet
```

`RUN_FLOOR` (grow-only, burn-up style) makes demoting a running cell a
two-place edit. `run.sh` remains the incumbent-side comparison runner.
Day one the gate caught two real greenfield defects: the mono clone
dropping `mutated_params` (c3, a silent wrong value) and `io.print`
having no lowering at all — both fixed and pinned the same day.
