历史描述：对v1 trust spine的对抗扫描（2026-06-27）及暴露的正确性漏洞。
<a id="v1-trust-spine-correctness-holes-adversarial-sweep-2026-06-27"></a>

# v1 trust-spine正确性漏洞（对抗扫描2026-06-27）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-spine-correctness-holes.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-spine-correctness-holes.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-spine-correctness-holes.zh-CN.md)

> 非规范性固定原文历史对抗扫描读者版。6月27日原文报告在render_program复现，此翻译未重跑测试，也未确定今日缺陷/修复状态。rootHint明确是agent猜测，可引用legacy路径；局部h1/h2引用保留，不暗中改为全局编号。许多原结果/rootHint句子在单词中截断，明确保留缺失而不推断补全。三个行末未闭code片段及一个escaped-backtick诊断逐字节保留，在局部span mask前单独检查。源码片段为历史fixture，不是运行指令。不声称自动同步或普遍安全证明。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-spine-correctness-holes.md)

系统性对抗扫描render_program（v1 spine）与native，探测v0 corpus不覆盖的形状空间后发现。
全部**已验证在render_program复现**（非legacy emit_wasm）。
corpus-wall（查cert、不查wat有效性）+ output-parity（124file）结构性漏掉这些。
下文rootHint是扫描agent猜测（一些引用legacy路径，真实位置是v1 spine的
crates/almide-mir/src/render_wasm\*；修复时须重新推导）。

**30个确认漏洞。** 修复=正确lower，或狭窄地wall（绝不emit非法wasm/绝不miscompile），
且不回归正在工作的corpus variant。

<a id="1--effectfn--trap"></a>

## #1 effectfn / trap
```almide
effect fn ld(k: Int) -> Result[String, String] =
  if k > 0 then ok("ok" + int.to_string(k)) else err("bad")
effect fn main() -> Unit = {
  match ld(3) { ok(s) => println(s), err(e) => println("E:" + e) }
  match ld(0) { ok(s) => println(s), err(e) => println("E:" + e) }
}
```
- native：`ok3 / E:bad   (exit 0, clean — both native runs succeed)`
- v1：render_program exit 0，wat2wasm接受wat（合法wasm）。wasmtime在ld(3)打印 `ok3`，ld(0)时TRAP：`wasm trap: wasm \`unreachable\` instruction executed`。
  backtrace顶frame是wasm函数12=$rc_dec，由fn30=$ld调用。独立隔离：只调用ld(的程序［原文在此截断］
- rootHint：Perceus pass（crates/almide-codegen/src/pass_perceus*.rs）为只在if/match一个arm定义的heap临时值
  emit函数scope IR RcDec，却把RcDec放控制流join后（函数退出），故每路径都执行。
  未定义的路径上［原文在此截断］

<a id="2--hof--invalid-wat"></a>

## #2 hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3]
  let r = xs |> list.filter_map((x) => Some(int.to_string(x)))
  println(list.join(r, ","))
}
```
- native：`1,2,3`
- v1：render_program exit 0但wat2wasm拒绝：h1.wat:553:22 'type mismatch in call, expected [i64] but got [i32]'，
  位于(call $int.to_string (local.get $v26))。输入List[Int]元素虽stride为(i64.const 8)，却以(i32.load ...)载入$v26，
  然后i32值被传…［原文在此截断］
- rootHint：list.filter_map在crates/almide-codegen/src/pass_stdlib_lowering_p2.rs:312 desugar成
  IterChain{steps:[IterStep::FilterMap], collector:Collect}。
  v1 trust-spine（render_program）的IterChain→loop lowering+wasm emission从输出元素类型（hea…［原文截断］）选输入load宽度。

<a id="3--hof--invalid-wat"></a>

## #3 hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3, 4]
  let r = xs |> list.filter_map((x) => if x % 2 == 1 then Some("odd" + int.to_string(x)) else None)
  println(list.join(r, ","))
}
```
- native：`odd1,odd3`
- v1：render_program exit 0但wat2wasm拒绝：h2.wat:558:22 'type mismatch in i64.rem_s, expected [i64, i64] but got [i32, i64]'，
  位于(i64.rem_s (local.get $v29) (local.get $v30))；h2.wat:570 int.to_string也期望i64却得i32。
  输入Int元素以(i32.load ...)载入$v29，再用于i64［原文截断］。
- rootHint：与h1同根因：v1 IterChain/FilterMap loop lowering从heap输出layout（i32）而非输入Int类型（i64）选输入load宽度。
  i32载入元素破坏每个消费它的i64 op（i64.rem_s、int.to_string）。修复：绑定源元素…［原文截断］

<a id="4--hof--invalid-wat"></a>

## #4 hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3, 4]
  let r = xs |> list.filter_map((x) => if x > 2 then Some("y" + int.to_string(x)) else None)
  println(list.join(r, ","))
}
```
- native：`y3,y4`
- v1：render_program exit 0，wat2wasm拒绝：h3.wat:558:40 'type mismatch in i64.gt_s, expected [i64, i64] but got [i32, i64]'，
  位于(i64.gt_s (local.get $v29) (local.get $v30))；int.to_string也是i64对i32。
  输入(i32.load ...)载入$v29再用于i64.gt_s，展示缺陷…［原文截断］
- rootHint：同根因：v1 FilterMap/IterChain loop lowering从heap输出元素（i32）而非输入Int（i64）选择输入load宽度。
  谓词operator无关；消费错误宽度元素的任何i64 op都会失败。
  见pass_stdlib_lowering_p2.rs:312（desugar）及It…［原文截断］

<a id="5--hof--invalid-wat"></a>

## #5 hof / invalid-wat
```almide
fn main() -> Unit = {
  let xs = [1, 2, 3]
  let r = xs |> list.filter_map((x) => if x == 2 then Some("two") else None)
  println(list.join(r, ","))
}
```
- native：`two`
- v1：render_program exit 0，wat2wasm拒绝：h4.wat:554:40 'type mismatch in i64.eq, expected [i64, i64] but got [i32, i64]'，
  位于(i64.eq (local.get $v26) (local.get $v27))。输入Int以(i32.load ...)载入$v26再用于i64.eq。
  此variant无int.to_string（常量Some("two")）［原文截断］。
- rootHint：单一底层缺陷的最清楚隔离：只要list.filter_map产生heap（String/List）结果元素，
  v1 IterChain/FilterMap loop emitter就按输出宽度（i32）而非输入类型宽度（Int→i64）load各输入元素。
  四个漏洞都是一个b…［原文截断］

<a id="6--compound--miscompile"></a>

## #6 compound / miscompile
```almide
effect fn main() -> Unit = {
  let t = (1, 2)
  let r = match t {
    (a, b) => a + b
  }
  println(int.to_string(r))
}
```
- native：`3`
- v1：0
- rootHint：tuple-pattern match arm中a,b从未从tuple heap槽（+0/+8）load。
  tuple构造和存储正确，但arm主体把未初始化local（$v9）当a+b结果读，int.to_string得到0。
  Match/tuple-pattern字段绑定（从base+offse… load i64［原文截断］）

<a id="7--compound--miscompile"></a>

## #7 compound / miscompile
```almide
effect fn main() -> Unit = {
  let t = ("k", 3)
  match t {
    (s, n) => println(s + "=" + int.to_string(n))
  }
}
```
- native：`k=3`
- v1：  =0
- rootHint：与h1同tuple-pattern解构bug，混合类型tuple（String,Int）。pattern变量s/n未从tuple槽load；
  s读未初始化string ref（空），n读0，产生'  =0'而非'k=3'。
  match lowering缺tuple-pattern元素绑定（从offset load）。

<a id="8--compound--miscompile"></a>

## #8 compound / miscompile
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
- native：`42`
- v1：0 inner-none outer-none
- rootHint：嵌套Option match，constructor payload绑定（some(inner)的inner、some(n)的n）未load，tag判别也误读。
  v1打印全部三个arm输出和0，表明Option some/none判别测试及payload提取lower错误
  （可能tag/payload…［原文截断］）。

<a id="9--compound--miscompile"></a>

## #9 compound / miscompile
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
- native：`15`
- v1：0 0
- rootHint：对record字段Option（n.next）match，两个arm都执行，n.val字段load和some(x) payload绑定都读0。
  结合损坏的Option判别/payload绑定与record字段作为match subject；n.val未正确load，some payload x未绑定。

<a id="10--compound--trap"></a>

## #10 compound / trap
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
- native：`7`
- v1：0 none [wasm trap: unreachable]
- rootHint：Option tuple的let-tuple解构：a,b未从tuple槽load，所以a为非法/zero ref。
  随后match a误读Option tag（打印0和'none'），再到wasm `unreachable`
  （match穷尽性/default trap，或未绑定payload上的坏pointer deref）。S…［原文截断］

<a id="11--compound--invalid-wat"></a>

## #11 compound / invalid-wat
```almide
effect fn main() -> Unit = {
  let xs = [(1, "a"), (2, "b"), (3, "c")]
  let total = list.fold(xs, 0, (acc, t) => { let (n, _) = t  acc + n })
  println(int.to_string(total))
}
```
- native：`6`
- v1：wat2wasm error: undefined function variable $__drop_list_int_str
- rootHint：drop函数emission缺口：render_program调用$__drop_list_int_str
  （List[(Int,String)]的drop）却未定义。元素类型tuple（Int,String）；逐类型drop-thunk生成器
  没为list-of-tuple元素类型生成定义，留下悬空ca…［原文截断］

<a id="12--compound--invalid-wat"></a>

## #12 compound / invalid-wat
```almide
type Pair = { first: Int, second: Int }
effect fn main() -> Unit = {
  let p = Pair { first: 10, second: 20 }
  let { first, second } = p
  println(int.to_string(first + second))
}
```
- native：`30`
- v1：wat2wasm error: type mismatch in i64.add, expected [i64,i64] but got [i32,i32]
- rootHint：record解构 `let { first, second } = p` 通过(local.set $vN (local.get $v3)) + rc_inc
  将first/second绑定record pointer（i32 local $v3），而非load base+12/base+20的i64字段值。
  然后arm对两个i32 pointer local做i64.add → wat类型不符。Record-p…［原文截断］

<a id="13--string--miscompile"></a>

## #13 string / miscompile
```almide
fn main() -> Unit = {
  let parts = string.split("a,b,c", ",")
  println("elem0=[${parts[0]}] len0=${string.len(parts[0])}")
}
```
- native：`elem0=[a] len0=1`
- v1：elem0=[L  ] len0=3
- rootHint：subject为持list handle的let-bound local（此处string.split）时，subscript `parts[idx]` 被静默省略。
  emit的$main完全丢index：`(local.set $v6 (local.get $v2))` 用整个list pointer代替 `parts[0]`，
  再把list header送入__str…［原文截断］

<a id="14--string--miscompile"></a>

## #14 string / miscompile
```almide
fn main() -> Unit = {
  let parts = string.split("a,b,c", ",")
  let lens = [string.len(parts[0]), string.len(parts[1]), string.len(parts[2])]
  println("${lens}")
}
```
- native：`[1, 1, 1]`
- v1：[3, 3, 3]
- rootHint：与hole 1同根因：list literal元素各 `parts[k]` 被省为整个split结果list pointer。
  string.len读list header len（3）而非各string byte长度。index被忽略，三项全为3。
  value位置subscript在…［原文截断］

<a id="15--string--miscompile"></a>

## #15 string / miscompile
```almide
fn main() -> Unit = {
  let cs = string.chars("hello")
  println("c0=[${cs[0]}] c1=[${cs[1]}]")
}
```
- native：`c0=[h] c1=[e]`
- v1：c0=[H    ] c1=[H    ]
- rootHint：同根因，`cs[0]`/`cs[1]`（cs=string.chars的let-bound结果）丢subscript。
  两者都插值整个list pointer（header bytes按UTF-8 render），无论index都显示同样垃圾 `H    `。
  let-bound list local的value位置Index lowering。

<a id="16--string--miscompile"></a>

## #16 string / miscompile
```almide
fn main() -> Unit = {
  let ls = string.lines("one\ntwo\nthree")
  println("[${ls[1]}]")
}
```
- native：`[two]`
- v1：[@  ]
- rootHint：同根因，`ls[1]`（ls=string.lines的let-bound结果）省为整个list pointer；
  插值render list header bytes（`@  `）而非元素1。
  let-bound list local的value位置subscript丢[idx]。

<a id="17--string--miscompile"></a>

## #17 string / miscompile
```almide
fn tag(s: String) -> String = "<" + s + ">"
fn main() -> Unit = {
  let parts = string.split("x,y,z", ",")
  println(tag(parts[1]))
}
```
- native：`<y>`
- v1：<L  >
- rootHint：同根因，作为call参数（给tag）的 `parts[1]` 省为整个split结果list pointer；
  tag拼接list header bytes（`L  `）。确认不仅插值，参数位置也省略。
  let-bound list local的value位置Index lowering。

<a id="18--string--miscompile"></a>

## #18 string / miscompile
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
- native：`key->val, x->y`
- v1：� ->� , � ->� 
- rootHint：lambda主体内同根因，`kv[0]`/`kv[1]`（kv=string.split的let-bound结果）省为整个内部list pointer；
  插值render list-header bytes（替代字符），`->` 两侧相同。
  map/lambda其他结构正常（产生两pair）；只有th…［原文截断］

<a id="19--recursion--trap"></a>

## #19 recursion / trap
```almide
fn fill(n: Int, acc: List[Int]) -> List[Int] =
  if n <= 0 then acc else fill(n - 1, acc + [n])
fn main() -> Unit = {
  let r = fill(200, [])
  println(int.to_string(list.len(r)))
  println(int.to_string(list.sum(r)))
}
```
- native：`200 / 20100`
- v1：wasm trap: out of bounds memory access — "memory fault at wasm address 0x10000 in linear memory of size 0x10000"。
  wat声明(memory (export "memory") 1)=1page（64KiB），无memory.grow。
  隔离验证：scalar-acc @depth 2000字节匹配、String-acc @depth 300字节匹配、List-acc @dep…［原文截断］
- rootHint：非tail递归累积，call参数 `acc + [n]` 每frame新建List。
  `+`（list concat）消费的list operand在递归call链上从未RC释放，临时分配累积到耗尽单64KiB页
  （$alloc bumps the…［原文截断］）。

<a id="20--recursion--trap"></a>

## #20 recursion / trap
```almide
fn rng(i: Int, n: Int, acc: List[Int]) -> List[Int] =
  if i >= n then acc else rng(i + 1, n, acc + [i])
fn main() -> Unit = {
  let r = rng(0, 250, [])
  println(int.to_string(list.len(r)))
}
```
- native：`250`
- v1：wasm trap: out of bounds memory access — memory fault at 0x10000 in linear memory of size 0x10000。
  与下降fill形同为单page（1页，无memory.grow）耗尽，此处是上升形。
  250元素List[Int]不该需要64KiB；trap来自 `acc + [i]` 中间值的逐frame泄漏［原文截断］。
- rootHint：同fill形根因：call参数为 `acc + [i]`（list concat）的非tail递归。
  每frame消费的accumulator list从未回收，分配越过无memory.grow的单64KiB页。
  修复同为递归lowering中call参数list-concat回收…［原文截断］

<a id="21--recursion--miscompile"></a>

## #21 recursion / miscompile
```almide
effect fn psum(n: Int, acc: Int) -> Int =
  if n <= 0 then acc
  else psum(n - 1, acc + int.parse("2")!)
effect fn main() -> Unit = {
  let s = psum(3, 0)!
  println("sum=" + int.to_string(s))
}
```
- native：`sum=6`
- v1：sum=0，depth 1也确认（parse_rec_arm：native sum=2、wasm sum=0）。
  render的$psum主体中else arm算$v6=n-1（第一参数），再 `(call $psum (local.get $v6) (local.get $v7))`，
  但$v7只声明从未赋值（默认0）。没有 `call $int.parse`，并且［原文截断］
- rootHint：递归call参数为BinOp（`acc + ...`），operand为stdlib Result-returning call
  （int.parse(...)!）的 `!` unwrap，出问题在其值表达式lowering。
  参数内 `!`/Result-unwrap令整个参数表达式未物化，renderer emit an…［原文截断］

<a id="22--recursion--miscompile"></a>

## #22 recursion / miscompile
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
- native：`sum=16`
- v1：sum=0，render的$psum比最小形更被截短：仅计算 `let n = int.parse(list.get(parts,i) ?? "0")`
  到$v31并发出其rc_inc/rc_dec cleanup，随后返回 `(local.get $v34)`，而$v34只声明未赋值（=0）。The…［原文截断］
- rootHint：与最小int.parse!-in-recursion同族，但失败是整体的：
  递归effect-fn block主体内LET绑定的stdlib Result call（int.parse(...)!）之 `!` unwrap令lowering提前结束block，
  只emit let绑定及其RC teardown…［原文截断］

<a id="23--operators--trap"></a>

## #23 operators / trap
```almide
effect fn main() -> Unit = {
  let a = 0
  let cond = a != 0 and (10 / a) > 0
  println("${cond}")
}
```
- native：`false`
- v1：wasm trap: integer divide by zero（wasmtime exit 134）。生成$main平坦lower两operand：
  $v2=(a != 0)，无条件计算$v5=i64.div_s($v3=10,$v4=0)（h1.wat:519），
  再$v8=i64.and($v2,$v7)（h1.wat:522）。eager的10/0 div_s在到达and前trap。
- rootHint：BinOp::And（逻辑 `and`）MIR lowering变成预物化两operand上的eager bitwise i64.and，
  不是短路控制流（LHS false时跳过RHS的 `if`/branch）。
  RHS `(10/a)>0`（i64.div_s）被hoist并无条件求值，所以divide-…［原文截断］

<a id="24--operators--trap"></a>

## #24 operators / trap
```almide
effect fn main() -> Unit = {
  let a = 0
  let cond = a == 0 or (10 / a) > 0
  println("${cond}")
}
```
- native：`true`
- v1：wasm trap: integer divide by zero（wasmtime exit 134）。与 `and` 同flat lowering但用i64.or：
  $v2=(a == 0)，无条件算$v5=i64.div_s(10,0)（h2.wat:519），$v8=i64.or($v2,$v7)（h2.wat:522）。
  native在 `a == 0` 为true时短路而不除；v1 eager除…［原文截断］
- rootHint：与hole 1同一缺陷，但为BinOp::Or（逻辑 `or`）：
  lower成预物化operand的eager bitwise i64.or，而非短路
  （LHS true时then给true并跳过RHS的 `if (result i64)`）。
  RHS `(10/a)>0` i64.div_s无条件求值［原文截断］。

<a id="25--operators--trap"></a>

## #25 operators / trap
```almide
effect fn main() -> Unit = {
  let xs = [1, 2, 3]
  let safe = list.len(xs) > 5 and xs[5] == 0
  println("${safe}")
}
```
- native：`false`
- v1：wasm trap: unreachable instruction executed（wasmtime exit 134）——bounds-check失败路径。
  $main算$v14=(list.len(xs)>5)，随后对len-3 list无条件call $v17=elem_addr(xs,5)
  （h3.wat:537），bounds check触发 `unreachable`，先于$v21=i64.and($v14,$v20)（h3.wat…［原文截断］）。
- rootHint：同holes 1/2根因，BinOp::And eager非短路lowering。
  RHS `xs[5] == 0` 需elem_addr(xs,5)，即越界会以 `unreachable` trap的索引helper。
  and先物化两operand，所以index访问即使…仍执行［原文截断］。

<a id="26--binding--invalid-wat"></a>

## #26 binding / invalid-wat
```almide
type Pt = { x: Int, y: Int }
effect fn main() -> Unit = {
  let p = Pt { x: 3, y: 4 }
  let { x, y } = p
  println(int.to_string(x + y))
}
```
- native：`7`
- v1：render_program exit 0，wat2wasm拒绝：h1.wat:532:22: error: type mismatch in i64.add, expected [i64, i64] but got [i32, i32] — `(local.set $v11 (i64.add (local.get $v9) (local.get $v10)))`。
  WAT中$v9/$v10都绑定 `(local.get $v3)`（i32 record pointer，有rc_inc），inst…［原文截断］
- rootHint：value/binding lowering pass（emit_wasm statements/expressions）的record-pattern解构
  （`let { x, y } = p`）。RecordPattern绑定为各字段名emit `local.set $vN (local.get <record_ptr>)` 而非projection，
  从不计算 `elem/field_addr(base + layout_offs
［原文未闭code span/句子在此截断］

<a id="27--binding--miscompile"></a>

## #27 binding / miscompile
```almide
type P = { name: String, tag: String }
effect fn main() -> Unit = {
  let p = P { name: "al", tag: "vip" }
  let { name, tag } = p
  println(name + "/" + tag)
}
```
- native：`al/vip`
- v1：\0 / \0\n（两个解构String字段绑定record pointer，print/str_concat操作record struct地址，
  输出为literal '/' 周围NUL bytes，精确byte为 `\0 space / \0 space \n`）。
  直接field access `p.name`/`p.tag` render正确（已验control：`al/vi
［原文未闭code span/句子在此截断］
- rootHint：同h1/h5根因，record-pattern解构将各field名绑定整个record pointer
  （`local.set $vN (local.get $v_rec)` + rc_inc），而非field layout offset上的 `i64.load`。
  String字段pointer类型兼容（i32→ptr），wat2wasm接受，但runtime str_con…［原文截断］

<a id="28--binding--invalid-wat"></a>

## #28 binding / invalid-wat
```almide
effect fn main() -> Unit = {
  let pairs = [(1, "a"), (2, "b")]
  println(int.to_string(list.len(pairs)))
}
```
- native：`2`
- v1：render_program exit 0，wat2wasm拒绝：h3.wat:570:11: error: undefined function variable "$__drop_list_int_str" — `(call $__drop_list_int_str (local.get $v1))`。
  WAT中零个 `(func $__drop_list_int_str ...)` 定义，但call点仍emit（sibling元素形drop helper e…［原文截断］）。
- rootHint：List[(T1,T2)] tuple元素的drop-helper生成matrix。
  emitter（rt_string/emit_wasm drop-helper节）从元素类型造helper名（`__drop_list_<elem>`），
  每list local都emit CALL，但helper定义generator缺少 `(Int, Strin
［原文未闭code span/句子在此截断］

<a id="29--binding--miscompile"></a>

## #29 binding / miscompile
```almide
effect fn main() -> Unit = {
  var xs = [1, 2, 3]
  xs[1] = 99
  println(int.to_string(xs[0]))
  println(int.to_string(xs[1]))
  println(int.to_string(xs[2]))
}
```
- native：`1\n99\n3\n`
- v1：1\n2\n3\n（精确byte）。WAT造list [1,2,3]（offset12/20/28存储），跑copy-on-write list_copy rc检查，
  再经elem_addr+i64.load读0/1/2。无index 1的i64.store，也无 `i64.const 99`；
  `xs[1] = 99` 赋值语句lower成…［原文截断］
- rootHint：emit_wasm statements.rs的索引赋值lowering（`xs[i] = v`）。
  IndexAssign/IndexSet语句没有lower——不生成 `elem_addr` + `i64.store`（也无list_set call）。
  copy-on-write `list_copy` guard确实emit（可写性路径运行），但之后实际store i…［原文截断］

<a id="30--binding--invalid-wat"></a>

## #30 binding / invalid-wat
```almide
type P = { name: String, age: Int }
effect fn main() -> Unit = {
  let p = P { name: "al", age: 30 }
  let { name, age } = p
  println(name + " " + int.to_string(age))
}
```
- native：`al 30`
- v1：render_program exit 0，wat2wasm拒绝：h5.wat:544:22: error: type mismatch in call, expected [i64] but got [i32] — `(local.set $v14 (call $int.to_string (local.get $v11)))`。
  $v11（及 `name` 的$v10）绑定 `(local.get $v3)`（i32 record pointer，有rc_inc），故int.to_string receiv…［原文截断］
- rootHint：同h1/h2根因。record-pattern解构把field名绑定record pointer，而非按layout offset投影field。
  Int字段使其表现为wat2wasm类型不符（int.to_string要i64，得i32 ptr）。
  修复=经LayoutId offset+i64.load投影每field，i…［原文截断］
