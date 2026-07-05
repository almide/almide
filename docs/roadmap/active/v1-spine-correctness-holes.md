# v1 trust-spine correctness holes (adversarial sweep 2026-06-27)

Found by a systematic adversarial sweep of render_program (v1 spine) vs native, probing shape-space the v0 corpus does NOT cover. All **verified reproducing in render_program** (not legacy emit_wasm). The corpus-wall (checks certs, not wat-validity) + output-parity (124 files) structurally miss these. Root hints below are the SWEEP agents' guesses (several cite legacy paths — the real site is the v1 spine crates/almide-mir/render_wasm\*; re-derive when fixing).

**30 confirmed holes.** Fix = lower correctly, OR narrowly wall (never emit invalid wasm / never miscompile) WITHOUT regressing the working corpus variant.

## #1  effectfn / trap
```almide
effect fn ld(k: Int) -> Result[String, String] =
  if k > 0 then ok("ok" + int.to_string(k)) else err("bad")
effect fn main() -> Unit = {
  match ld(3) { ok(s) => println(s), err(e) => println("E:" + e) }
  match ld(0) { ok(s) => println(s), err(e) => println("E:" + e) }
}
```
- native: `ok3 / E:bad   (exit 0, clean — both native runs succeed)`
- v1: render_program exits 0 and wat2wasm ACCEPTS the wat (valid wasm). wasmtime prints `ok3` for ld(3) then TRAPS on ld(0): `wasm trap: wasm \`unreachable\` instruction executed`, backtrace top frame = wasm function 12 = $rc_dec, called from fn 30 = $ld. Independently isolated: a program calling only ld(
- rootHint: Perceus pass (crates/almide-codegen/src/pass_perceus*.rs) emits function-scope IR RcDec nodes for heap temporaries that are defined inside only ONE arm of an if/match, but places those RcDecs after the control-flow join (function exit), so they execute on every path. On the path that did not define 

## #2  hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3]
  let r = xs |> list.filter_map((x) => Some(int.to_string(x)))
  println(list.join(r, ","))
}
```
- native: `1,2,3`
- v1: render_program exits 0 but wat2wasm REJECTS: h1.wat:553:22 'type mismatch in call, expected [i64] but got [i32]' at (call $int.to_string (local.get $v26)). The input List[Int] element is loaded with (i32.load ...) into $v26 even though the element stride is (i64.const 8); the i32 value is then passe
- rootHint: list.filter_map desugars in crates/almide-codegen/src/pass_stdlib_lowering_p2.rs:312 to IterChain{steps:[IterStep::FilterMap], collector:Collect}. In the v1 trust-spine (render_program) IterChain->loop lowering+wasm emission, the INPUT element load width is selected from the OUTPUT element type (hea

## #3  hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3, 4]
  let r = xs |> list.filter_map((x) => if x % 2 == 1 then Some("odd" + int.to_string(x)) else None)
  println(list.join(r, ","))
}
```
- native: `odd1,odd3`
- v1: render_program exits 0 but wat2wasm REJECTS: h2.wat:558:22 'type mismatch in i64.rem_s, expected [i64, i64] but got [i32, i64]' at (i64.rem_s (local.get $v29) (local.get $v30)); also h2.wat:570 int.to_string expecting i64 got i32. Input Int element loaded as (i32.load ...) into $v29 then used in i64
- rootHint: Same root cause as h1: v1 IterChain/FilterMap loop lowering selects the input-element load width from the heap OUTPUT element layout (i32) rather than the INPUT Int element type (i64), so the i32-loaded element corrupts every i64 op consuming it (i64.rem_s, int.to_string). Fix: bind the source eleme

## #4  hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3, 4]
  let r = xs |> list.filter_map((x) => if x > 2 then Some("y" + int.to_string(x)) else None)
  println(list.join(r, ","))
}
```
- native: `y3,y4`
- v1: render_program exits 0 but wat2wasm REJECTS: h3.wat:558:40 'type mismatch in i64.gt_s, expected [i64, i64] but got [i32, i64]' at (i64.gt_s (local.get $v29) (local.get $v30)); also int.to_string i64 vs i32. Input element loaded (i32.load ...) into $v29 then used in i64.gt_s. Demonstrates the defect 
- rootHint: Identical root cause: v1 FilterMap/IterChain loop lowering picks the input-element load width from the heap OUTPUT element (i32) instead of INPUT Int (i64). The predicate operator is irrelevant; any i64 op consuming the mis-sized element fails. See pass_stdlib_lowering_p2.rs:312 (desugar) and the It

## #5  hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3]
  let r = xs |> list.filter_map((x) => if x == 2 then Some("two") else None)
  println(list.join(r, ","))
}
```
- native: `two`
- v1: render_program exits 0 but wat2wasm REJECTS: h4.wat:554:40 'type mismatch in i64.eq, expected [i64, i64] but got [i32, i64]' at (i64.eq (local.get $v26) (local.get $v27)). Input Int element loaded (i32.load ...) into $v26 then used in i64.eq. This variant has NO int.to_string (constant Some("two")),
- rootHint: Cleanest isolation of the single underlying defect: whenever list.filter_map produces a heap (String/List) result element, the v1 IterChain/FilterMap loop emitter loads each INPUT element at the OUTPUT element width (i32) instead of the input element type's width (Int->i64). All four holes are one b

## #6  compound / miscompile
```almide
effect fn main() -> Unit = {
  let t = (1, 2)
  let r = match t {
    (a, b) => a + b
  }
  println(int.to_string(r))
}
```
- native: `3`
- v1: 0
- rootHint: Tuple-pattern match arm: the bindings a,b are never loaded from the tuple's heap slots (offsets +0/+8). The tuple is built and stored correctly, but the arm body reads an uninitialized local ($v9) as the a+b result, so int.to_string gets 0. Match/tuple-pattern field-binding (load i64 from base+offse

## #7  compound / miscompile
```almide
effect fn main() -> Unit = {
  let t = ("k", 3)
  match t {
    (s, n) => println(s + "=" + int.to_string(n))
  }
}
```
- native: `k=3`
- v1:   =0
- rootHint: Same tuple-pattern destructure bug as h1, mixed-type tuple (String,Int). Pattern vars s and n are not loaded from the tuple slots; s reads an uninitialized string ref (empty) and n reads 0, producing '  =0' instead of 'k=3'. Tuple-pattern element binding (load-from-offset) missing in match lowering.

## #8  compound / miscompile
```almide
effect fn main() -> Unit = {
  let x: Option[Option[Int]] = some(some(42))
  match x {
    some(inner) => match inner {
      some(n) => println(int.to_string(n)),
      none => println("inner-none")
    },
    none => println("outer-none")
  }
}
```
- native: `42`
- v1: 0 inner-none outer-none
- rootHint: Nested Option match: the constructor-payload binding (inner from some(inner), n from some(n)) is not loaded, and the tag discriminant is mis-read. v1 prints all three arms' outputs and 0, indicating the some/none discriminant test and payload extraction for Option are mislowered (likely tag/payload 

## #9  compound / miscompile
```almide
type Node = { val: Int, next: Option[Int] }
effect fn main() -> Unit = {
  let n = Node { val: 5, next: some(10) }
  match n.next {
    some(x) => println(int.to_string(n.val + x)),
    none => println(int.to_string(n.val))
  }
}
```
- native: `15`
- v1: 0 0
- rootHint: Match on a record-field Option (n.next): both arms execute and both n.val field-load and the some(x) payload binding read 0. Combines broken Option discriminant/payload binding with record-field-as-match-subject; n.val is not correctly loaded and the some payload x is not bound.

## #10  compound / trap
```almide
effect fn main() -> Unit = {
  let pair = (some(7), some(8))
  let (a, b) = pair
  match a {
    some(n) => println(int.to_string(n)),
    none => println("none")
  }
}
```
- native: `7`
- v1: 0 none [wasm trap: unreachable]
- rootHint: let-tuple-destructure of a tuple of Options: a,b not loaded from tuple slots, so a holds an invalid/zero ref. The subsequent match on a mis-reads the Option tag (prints 0 then 'none'), then hits a wasm `unreachable` (match exhaustiveness/default trap or a bad pointer deref on the unbound payload). S

## #11  compound / invalid-wat
```almide
effect fn main() -> Unit = {
  let xs = [(1, "a"), (2, "b"), (3, "c")]
  let total = list.fold(xs, 0, (acc, t) => { let (n, _) = t  acc + n })
  println(int.to_string(total))
}
```
- native: `6`
- v1: wat2wasm error: undefined function variable $__drop_list_int_str
- rootHint: Drop-function emission gap: render_program emits a call to $__drop_list_int_str (drop for List[(Int,String)]) but never defines that function. The element type is a tuple (Int,String); the per-type drop-thunk generator does not emit a definition for list-of-tuple element types, leaving a dangling ca

## #12  compound / invalid-wat
```almide
type Pair = { first: Int, second: Int }
effect fn main() -> Unit = {
  let p = Pair { first: 10, second: 20 }
  let { first, second } = p
  println(int.to_string(first + second))
}
```
- native: `30`
- v1: wat2wasm error: type mismatch in i64.add, expected [i64,i64] but got [i32,i32]
- rootHint: Record destructure `let { first, second } = p` binds first and second to the record POINTER (i32 local $v3) via (local.set $vN (local.get $v3)) + rc_inc, instead of loading the i64 field values from base+12 / base+20. The arm then does i64.add on two i32 pointer locals -> wat type mismatch. Record-p

## #13  string / miscompile
```almide
fn main() -> Unit = {
  let parts = string.split("a,b,c", ",")
  println("elem0=[${parts[0]}] len0=${string.len(parts[0])}")
}
```
- native: `elem0=[a] len0=1`
- v1: elem0=[L  ] len0=3
- rootHint: Subscript `parts[idx]` is silently ELIDED when the subject is a let-bound local holding a list handle (here from string.split). In the emitted $main the index is dropped entirely: `(local.set $v6 (local.get $v2))` substitutes the whole LIST POINTER for `parts[0]`, then feeds the list header to __str

## #14  string / miscompile
```almide
fn main() -> Unit = {
  let parts = string.split("a,b,c", ",")
  let lens = [string.len(parts[0]), string.len(parts[1]), string.len(parts[2])]
  println("${lens}")
}
```
- native: `[1, 1, 1]`
- v1: [3, 3, 3]
- rootHint: Same root as hole 1: each `parts[k]` in the list-literal elements is elided to the whole split-result list pointer; string.len then reads the list header's len field (3) instead of each element string's byte length. All three entries become 3 because the index is ignored. Value-position subscript on

## #15  string / miscompile
```almide
fn main() -> Unit = {
  let cs = string.chars("hello")
  println("c0=[${cs[0]}] c1=[${cs[1]}]")
}
```
- native: `c0=[h] c1=[e]`
- v1: c0=[H    ] c1=[H    ]
- rootHint: Same root: `cs[0]`/`cs[1]` (cs = let-bound result of string.chars) lose the subscript; both interpolate the whole list pointer (header bytes rendered as UTF-8), so both arms show identical garbage `H    ` regardless of index. Value-position Index lowering for a let-bound list local.

## #16  string / miscompile
```almide
fn main() -> Unit = {
  let ls = string.lines("one\ntwo\nthree")
  println("[${ls[1]}]")
}
```
- native: `[two]`
- v1: [@  ]
- rootHint: Same root: `ls[1]` (ls = let-bound result of string.lines) is elided to the whole list pointer; interpolation renders the list header bytes (`@  `) instead of element 1. Value-position subscript on a let-bound list local drops [idx].

## #17  string / miscompile
```almide
fn tag(s: String) -> String = "<" + s + ">"
fn main() -> Unit = {
  let parts = string.split("x,y,z", ",")
  println(tag(parts[1]))
}
```
- native: `<y>`
- v1: <L  >
- rootHint: Same root: `parts[1]` as a call ARGUMENT (to tag) is elided to the whole split-result list pointer; tag concatenates the list header bytes (`L  `). Confirms the elision happens in argument position too, not just interpolation. Value-position Index lowering for a let-bound list local.

## #18  string / miscompile
```almide
fn main() -> Unit = {
  let m = "key=val;x=y"
  let pairs = string.split(m, ";")
  let kvs = pairs |> list.map((p) => {
    let kv = string.split(p, "=")
    "${kv[0]}->${kv[1]}"
  })
  println(list.join(kvs, ", "))
}
```
- native: `key->val, x->y`
- v1: � ->� , � ->� 
- rootHint: Same root inside the lambda body: `kv[0]`/`kv[1]` (kv = let-bound result of string.split) are elided to the whole inner list pointer; interpolation renders list-header bytes (replacement chars), identical on both sides of `->`. The map/lambda structure is otherwise fine (two pairs produced); only th

## #19  recursion / trap
```almide
fn fill(n: Int, acc: List[Int]) -> List[Int] =
  if n <= 0 then acc else fill(n - 1, acc + [n])
fn main() -> Unit = {
  let r = fill(200, [])
  println(int.to_string(list.len(r)))
  println(int.to_string(list.sum(r)))
}
```
- native: `200 / 20100`
- v1: wasm trap: out of bounds memory access — "memory fault at wasm address 0x10000 in linear memory of size 0x10000". wat declares (memory (export "memory") 1) = 1 page (64KiB) with NO memory.grow. Verified isolation: scalar-acc @depth 2000 byte-matches, String-acc @depth 300 byte-matches, List-acc @dep
- rootHint: Recursive (non-tail) accumulation where the call argument `acc + [n]` builds a fresh List each frame: the consumed-list operand of `+` (list concat) is never rc-freed across the recursive call chain, so transient list allocations accumulate until the single 64KiB page is exhausted ($alloc bumps the 

## #20  recursion / trap
```almide
fn rng(i: Int, n: Int, acc: List[Int]) -> List[Int] =
  if i >= n then acc else rng(i + 1, n, acc + [i])
fn main() -> Unit = {
  let r = rng(0, 250, [])
  println(int.to_string(list.len(r)))
}
```
- native: `250`
- v1: wasm trap: out of bounds memory access — memory fault at 0x10000 in linear memory of size 0x10000. Same single-page (1 page, no memory.grow) exhaustion as the descending fill shape, here ascending. A 250-element List[Int] cannot need 64KiB; the trap is per-frame leak of the `acc + [i]` intermediate 
- rootHint: Same root as the fill shape: non-tail recursion whose call argument is `acc + [i]` (list concat). The consumed accumulator list per frame is never reclaimed; allocations bump past the single 64KiB page with no memory.grow. Fix is the same call-argument list-concat reclamation in recursive lowering, 

## #21  recursion / miscompile
```almide
effect fn psum(n: Int, acc: Int) -> Int =
  if n <= 0 then acc
  else psum(n - 1, acc + int.parse("2")!)
effect fn main() -> Unit = {
  let s = psum(3, 0)!
  println("sum=" + int.to_string(s))
}
```
- native: `sum=6`
- v1: sum=0 — confirmed at depth 1 too (parse_rec_arm: native sum=2, wasm sum=0). Reading the rendered $psum body: the else arm computes $v6 = n-1 (first arg) then `(call $psum (local.get $v6) (local.get $v7))` — but $v7 is a declared-but-NEVER-ASSIGNED local (default 0). There is NO `call $int.parse` and
- rootHint: Value-expression lowering for a recursive CALL ARGUMENT that is a BinOp (`acc + ...`) whose operand is a `!`-unwrap of a stdlib Result-returning call (int.parse(...)!). The `!`/Result-unwrap inside the argument causes the surrounding argument expression to fail to materialize — the renderer emits an

## #22  recursion / miscompile
```almide
effect fn psum(parts: List[String], i: Int, acc: Int) -> Int =
  if i >= list.len(parts) then acc
  else {
    let n = int.parse(list.get(parts, i) ?? "0")!
    psum(parts, i + 1, acc + n)
  }
effect fn main() -> Unit = {
  let s = psum(["1", "2", "3", "10"], 0, 0)!
  println("sum=" + int.to_string(s))
}
```
- native: `sum=16`
- v1: sum=0 — and the rendered $psum is MORE truncated than the minimal shape. Reading the body: it emits only the `let n = int.parse(list.get(parts,i) ?? "0")` computation into $v31 plus its rc_inc/rc_dec cleanup, then returns `(local.get $v34)` where $v34 is a declared-but-never-assigned local (=0). The
- rootHint: Same family as the minimal int.parse!-in-recursion shape but the failure is total: the `!`-unwrap of a stdlib Result call (int.parse(...)!) in a LET binding inside a recursive effect-fn block body causes the lowering to terminate the block early — only the let-binding (and its RC teardown) is emitte

## #23  operators / trap
```almide
effect fn main() -> Unit = {
  let a = 0
  let cond = a != 0 and (10 / a) > 0
  println("${cond}")
}
```
- native: `false`
- v1: wasm trap: integer divide by zero (wasmtime exit 134). Generated $main lowers both operands flatly: $v2 = (a != 0), then $v5 = i64.div_s($v3=10, $v4=0) computed UNCONDITIONALLY (h1.wat:519), then $v8 = i64.and($v2,$v7) (h1.wat:522). The eager div_s of 10/0 traps before the and is reached.
- rootHint: MIR lowering of BinOp::And (logical `and`) is lowered to an eager bitwise i64.and over two pre-materialized operands instead of short-circuiting control flow (an `if`/branch that skips the RHS when LHS is false). The RHS `(10/a)>0` (i64.div_s) is hoisted out and evaluated unconditionally, so divide-

## #24  operators / trap
```almide
effect fn main() -> Unit = {
  let a = 0
  let cond = a == 0 or (10 / a) > 0
  println("${cond}")
}
```
- native: `true`
- v1: wasm trap: integer divide by zero (wasmtime exit 134). Same flat lowering as the `and` case but with i64.or: $v2 = (a == 0), $v5 = i64.div_s(10,0) computed UNCONDITIONALLY (h2.wat:519), $v8 = i64.or($v2,$v7) (h2.wat:522). Native short-circuits on `a == 0` being true and never divides; v1 divides eag
- rootHint: Same single defect as hole 1 but for BinOp::Or (logical `or`): lowered to an eager bitwise i64.or over pre-materialized operands rather than short-circuiting (an `if (result i64)` whose then-branch yields true and skips the RHS when LHS is true). RHS `(10/a)>0` i64.div_s is evaluated unconditionally

## #25  operators / trap
```almide
effect fn main() -> Unit = {
  let xs = [1, 2, 3]
  let safe = list.len(xs) > 5 and xs[5] == 0
  println("${safe}")
}
```
- native: `false`
- v1: wasm trap: unreachable instruction executed (wasmtime exit 134) — the bounds-check failure path. $main computes $v14 = (list.len(xs) > 5), then UNCONDITIONALLY calls $v17 = elem_addr(xs, 5) (h3.wat:537) on a len-3 list, whose bounds check fires `unreachable`, before $v21 = i64.and($v14,$v20) (h3.wat
- rootHint: Same root cause as holes 1 and 2: BinOp::And eager (non-short-circuiting) lowering. The RHS `xs[5] == 0` requires elem_addr(xs,5), which is a bounds-checked indexing helper that traps via `unreachable` on out-of-bounds. Because the and materializes both operands first, the index access executes even

## #26  binding / invalid-wat
```almide
type Pt = { x: Int, y: Int }
effect fn main() -> Unit = {
  let p = Pt { x: 3, y: 4 }
  let { x, y } = p
  println(int.to_string(x + y))
}
```
- native: `7`
- v1: render_program exits 0 but wat2wasm REJECTS: h1.wat:532:22: error: type mismatch in i64.add, expected [i64, i64] but got [i32, i32] — `(local.set $v11 (i64.add (local.get $v9) (local.get $v10)))`. In the WAT, $v9 and $v10 are both bound to `(local.get $v3)` (the i32 record pointer, with rc_inc) inst
- rootHint: Record-pattern destructuring lowering (`let { x, y } = p`) in the value/binding lowering pass (emit_wasm statements/expressions). The RecordPattern binding emits `local.set $vN (local.get <record_ptr>)` for each field name rather than projecting: it never computes `elem/field_addr(base + layout_offs

## #27  binding / miscompile
```almide
type P = { name: String, tag: String }
effect fn main() -> Unit = {
  let p = P { name: "al", tag: "vip" }
  let { name, tag } = p
  println(name + "/" + tag)
}
```
- native: `al/vip`
- v1: \0 / \0\n (the two destructured String fields are bound to the record pointer, so print/str_concat operate on the record struct address — output is NUL bytes around the literal '/', byte-exact `\0 space / \0 space \n`). Direct field access `p.name`/`p.tag` renders correctly (verified control: `al/vi
- rootHint: Same root as h1/h5: record-pattern destructure binds each field name to the whole record pointer (`local.set $vN (local.get $v_rec)` + rc_inc) instead of `i64.load` at the field's layout offset. For String fields the pointer is type-compatible (i32→ptr) so wat2wasm accepts it, but at runtime str_con

## #28  binding / invalid-wat
```almide
effect fn main() -> Unit = {
  let pairs = [(1, "a"), (2, "b")]
  println(int.to_string(list.len(pairs)))
}
```
- native: `2`
- v1: render_program exits 0 but wat2wasm REJECTS: h3.wat:570:11: error: undefined function variable "$__drop_list_int_str" — `(call $__drop_list_int_str (local.get $v1))`. Zero `(func $__drop_list_int_str ...)` definitions exist in the WAT, yet the call site emits it (sibling element-shape drop helpers e
- rootHint: Drop-helper generation matrix for List[(T1,T2)] tuple elements. The drop-helper emitter (rt_string/emit_wasm drop-helper section) builds the helper name from the element type (`__drop_list_<elem>`) and emits a CALL for every list local, but the helper-DEFINITION generator is missing the `(Int, Strin

## #29  binding / miscompile
```almide
effect fn main() -> Unit = {
  var xs = [1, 2, 3]
  xs[1] = 99
  println(int.to_string(xs[0]))
  println(int.to_string(xs[1]))
  println(int.to_string(xs[2]))
}
```
- native: `1\n99\n3\n`
- v1: 1\n2\n3\n (byte-exact). The WAT builds the list [1,2,3] (stores at offsets 12/20/28), runs the copy-on-write list_copy rc check, then reads back elements 0/1/2 via elem_addr+i64.load. There is NO i64.store for index 1 and no `i64.const 99` anywhere — the `xs[1] = 99` assignment statement lowered to 
- rootHint: Indexed-assignment statement lowering (`xs[i] = v`) in emit_wasm statements.rs. The IndexAssign/IndexSet statement is not lowered — it produces no `elem_addr` + `i64.store` (nor a list_set call). The copy-on-write `list_copy` guard IS emitted (the writability path runs), but the actual store after i

## #30  binding / invalid-wat
```almide
type P = { name: String, age: Int }
effect fn main() -> Unit = {
  let p = P { name: "al", age: 30 }
  let { name, age } = p
  println(name + " " + int.to_string(age))
}
```
- native: `al 30`
- v1: render_program exits 0 but wat2wasm REJECTS: h5.wat:544:22: error: type mismatch in call, expected [i64] but got [i32] — `(local.set $v14 (call $int.to_string (local.get $v11)))`. $v11 (and $v10 for `name`) are bound to `(local.get $v3)` (the i32 record pointer, with rc_inc), so int.to_string receiv
- rootHint: Same root cause as h1/h2: record-pattern destructure binds field names to the record pointer instead of projecting fields at their layout offsets. The Int field exposes it as a wat2wasm type mismatch (int.to_string expects i64, gets i32 ptr). Fix = project each field via LayoutId offset + i64.load i
