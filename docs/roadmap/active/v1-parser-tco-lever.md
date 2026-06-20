# v1 — the parser-TCO lever (the real "heap-result-expr" cross-repo lever)

The org-trust dashboard's top wall reason (~40, blocking toml/svg/aes/base64/csv) reads as the
"heap-result-expr family" (`heap-result if`/`match` … "would move out an empty deferred heap
value"). Targeting csv (a working v0 oracle, unlike toml — see below) revealed the TRUE cause.

## Finding: it is NOT the heap-result ARM shapes — those already lower

`lower_heap_result_arm` (control.rs) already handles tuple-construct arms, Named/Module-call arms,
concat arms, nested if/match, blocks, Option/Result ctors. csv's `heap-result if` walls come from
ONE deliberate guard: a **self-recursive call arm is walled** (control.rs ~2162):

```
if name.as_str() == self.fn_name { return None; }  // v1 has NO TCO → deep recursion traps
```

csv's parser is all tail-self-recursion: `parse_unquoted_field(text, pos+1, acc+c)`,
`parse_quoted_field`, `parse_rows_rec`, `parse_after_field` — each recurses, so each heap-result
`if` hits the self-rec guard. So the lever is **TCO of self-recursive heap-result parser
functions**, NOT the arm shapes.

## What TCO already covers (`try_tco_rewrite`, mod.rs:2734) vs the gap

Covers: (1) a list-iterator forward scan (`list.drop(cs,1)` carried), (2) APPEND ACCUMULATORS
(`acc + [x]`, `ConcatList`) → an owned loop-carried slot (option C, cert `check_cert_lc`). yaml
(byte-verified) lowers because its parser fits these.

GAP (csv/toml parser-combinator shape):
- **String accumulator** `acc + c` (`ConcatStr`, not `ConcatList`) — extend the append-accumulator
  to a String slot (the same drop-old/alloc-new-per-iter, cert `i(id)m`).
- **Tuple-result base** `(acc, pos)` — the base returns a `(String, Int)` carrying the accumulator
  + the scalar position, not the carried type directly.
- **Multi-accumulator + tuple-destructure self-calls** (`parse_rows_rec`: carries `rows`,
  `current_row` both `List`, and a self-call's arg is `current_row + [field]` where `let (field, np)
  = parse_quoted_field(...)`).

## Plan (byte-match-first; csv has a WORKING v0 oracle)

Oracle: `parse("a,b,c\n1,2,3\n\"x,y\",4,5\n")` → v0 native = `[["a","b","c"],["1","2","3"],["x,y","4","5"]]`
(confirmed). Driver = csv/src/mod.almd + an `effect fn main` calling `parse` (single file →
render_program). Target: v1 == that.

1. Extend the append-accumulator in `try_tco_rewrite` to a **ConcatStr (String) accumulator** +
   the smallest tuple-result base — unblock `parse_unquoted_field`/`parse_quoted_field`. Gate:
   corpus-wall ACCEPT (the loop-carried cert `check_cert_lc`) + a String-accumulator leak loop +
   byte-match.
2. Multi-accumulator + tuple-destructure self-calls — `parse_rows_rec`/`parse_after_field`.
3. Then `parse` (the `ok(value.array(...))` ResultOk) + `parse_records` (a `list.map` closure)
   lower in cascade. csv → byte-match `[["a"…]]`.

EACH step gated on corpus-wall ACCEPT (TCO is correctness/leak-prone — the loop-carried-slot cert
is the gate) AND the csv v0==v1 byte-match. The lever clears the same class across toml/svg/aes/
base64 (all parser-shaped).

## PROGRESS (commit 63a7a1a6) — step 1 DONE + a pre-existing miscompile fixed

While wiring the ConcatStr accumulator the byte-match surfaced a PRE-EXISTING silent miscompile (the
② cardinal violation): a TCO loop body is `{ if base then … else step }`, so the base-check arrives
as a BLOCK-TAIL `if`, and that tail fell STRAIGHT to `lower_branch` (run BOTH arms with the cond
record-elided) — turning `if done then {rk:=k} else {step}` into an UNCONDITIONAL `rk:=k`, so the
loop ran exactly ONCE. ANY recursive parser with a heap `let c = peek(...)` in its body hit it (v0
`hello`, v1 `h`). **Fix**: route the block-tail if/match through `try_lower_unit_if` FIRST (a real
branch); fall to `lower_branch` only when it cannot execute. This both kills the miscompile AND makes
the scalar-index append-accumulator parser loops EXECUTE.

DONE in this commit:
- ✅ block-tail base-check now branches (the run-once miscompile fixed — list AND string).
- ✅ ConcatStr (String) accumulator + tuple-result base `(String, Int)` — `is_self_append` matches
  ConcatStr, the upfront slot-copy is String-aware (`acc + ""`). Leak-loop verified (2000×).
- ✅ corpus-wall ACCEPT (ownership 16303), diff-fuzz green, the 4 `*_loop_reclaims` tests still pass,
  a new wasmtime cargo test (`string_accumulator_parser_tco_executes_on_wasmtime`).

## PROGRESS (commit 1d8bdd92) — step 2 partial: multi-accumulator reset + cross-read

The multi-accumulator gap decomposed into FOUR sub-gaps (minimal repros each). Two are now DONE:
- ✅ **RESET** a heap accumulator to a fresh empty (`cur = []` / `acc = ""`) — admitted as a
  loop-carried slot update (the parser resets the current-row acc after a delimiter).
- ✅ **heap-acc-reads-heap-acc** (`out = out + cur` while `cur = ""`) — per-iteration heap assigns
  emitted in READ-DEPENDENCY topological order (reader before readee); only a CYCLE walls.
  A two-String-accumulator parser now byte-matches v0 (leak-loop verified, cargo test
  `multi_accumulator_reset_and_cross_read_tco_executes_on_wasmtime`).

STILL WALLS (csv `List[List[String]]` parser — the remaining two sub-gaps + the earlier ones):
- ❌ **nested heap-element list** — `rows: List[List[String]]`, so `rows + [cur]` has a `List[String]`
  ELEMENT. `try_lower_concat_list` admits scalar / String / Value / (String,Value) elements only —
  a List element (a list-of-lists) needs the doubly-recursive drop (`DropListListStr`). This is the
  csv blocker now (csvcore repro walls here, not in the TCO).
- ❌ **scalar-var list literal** `[pos]` — a `List[Int]` literal with a variable element does not
  materialize (`alloc_init` yields `Init::Opaque`; `try_lower_str_list_literal` only does heap
  elements). General gap (mc3 repro), not csv-specific but on the same path.
- ❌ **mutual recursion + tuple-destructure** — `parse_rows_rec` ⇄ `parse_after_field` (mutual tail
  recursion → needs `inline_mutual_tail_recursion` to fuse them) and the `"` branch's
  `let (field, np) = parse_quoted_field(...)` (a tuple-destructure self-call).
- `parse`/`parse_records`: the `ok(value.array(...))` ResultOk wrapper + a `list.map` closure.

## SEPARATE blocker: toml's v0 oracle is BROKEN

toml was the first proposed target but `almide run` (native v0) emits INVALID Rust for it
(`error[E0308]: expected String, found &str`, 2×) — so toml has NO byte-match oracle until that v0
Rust-codegen bug is fixed. csv was chosen instead (v0 test passes). The toml v0 bug is a
v0-backend issue to fix separately before toml can be byte-verified.
