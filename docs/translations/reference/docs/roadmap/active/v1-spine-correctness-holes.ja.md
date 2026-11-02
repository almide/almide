歴史的説明：v1 trust spineへの敵対的掃引（2026-06-27）と、そこで露呈した正しさの穴。
<a id="v1-trust-spine-correctness-holes-adversarial-sweep-2026-06-27"></a>

# v1 trust-spineの正しさの穴（敵対的掃引2026-06-27）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-spine-correctness-holes.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-spine-correctness-holes.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-spine-correctness-holes.zh-CN.md)

> 非規範的な固定原文の歴史的敵対掃引読者版。6月27日原文はrender_programで再現を報告するが、今回テストを再実行せず現在の欠陥/修正状態も確定していない。rootHintはagentの推測と明記されlegacy pathも含み、局所h1/h2参照を全体番号へ黙って振り直さない。多数の結果/rootHint文が単語途中で切れており、推測補完せず途切れを明示。行末未閉code断片3個とescaped-backtick診断1個をbyte単位で保持し、対象限定span mask前に個別検査する。source snippetは歴史fixtureで実行指示ではない。自動同期や全称安全証明を主張しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-spine-correctness-holes.md)

render_program（v1 spine）とnativeの系統的敵対的掃引で、v0 corpusが覆わない形状空間を調べて発見。
すべて **render_programで再現確認済み**（legacy emit_wasmではない）。
corpus-wall（certを検査しwat妥当性は見ない）+ output-parity（124file）は構造的にこれらを見逃す。
下記rootHintは掃引agentの推測（legacy pathを引用するものもある。実箇所はv1 spineの
crates/almide-mir/src/render_wasm\*。修正時に再導出）。

**確認済み30穴。** 修正は正しくlowerするか、動作中corpus variantを回帰させず狭くwallする
（無効wasmのemitやmiscompileは不可）。

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
- v1：render_programはexit 0、wat2wasmはwatをACCEPT（有効wasm）。wasmtimeはld(3)で `ok3`、ld(0)でTRAP：`wasm trap: wasm \`unreachable\` instruction executed`。backtrace先頭はwasm関数12=$rc_dec、呼出元fn30=$ld。独立切分け：ld(だけを呼ぶprogramで［原文ここで途切れ］
- rootHint：Perceus pass（crates/almide-codegen/src/pass_perceus*.rs）がif/matchの片armだけで定義したheap temporaryに関数scopeのIR RcDecを出すが、それを制御flow join後（関数exit）に置くため全pathで実行。定義しなかったpathでは［原文ここで途切れ］

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
- v1：render_programはexit 0だがwat2wasmは拒否：h1.wat:553:22 'type mismatch in call, expected [i64] but got [i32]'、(call $int.to_string (local.get $v26))。入力List[Int]要素をstride (i64.const 8)なのに(i32.load ...)で$v26へloadし、i32値を渡…［原文ここで途切れ］
- rootHint：list.filter_mapはcrates/almide-codegen/src/pass_stdlib_lowering_p2.rs:312でIterChain{steps:[IterStep::FilterMap], collector:Collect}へdesugar。v1 trust-spine（render_program）のIterChain→loop lowering+wasm emissionは入力要素load幅を出力要素型（hea…［原文ここで途切れ］）から選択。

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
- v1：render_programはexit 0、wat2wasmは拒否：h2.wat:558:22 'type mismatch in i64.rem_s, expected [i64, i64] but got [i32, i64]'、(i64.rem_s (local.get $v29) (local.get $v30))。h2.wat:570 int.to_stringもi64期待にi32。入力Intを(i32.load ...)で$v29へloadしi64で使用［原文ここで途切れ］。
- rootHint：h1と同根因。v1 IterChain/FilterMap loop loweringが入力Int型（i64）でなくheap出力要素layout（i32）から入力load幅を選び、i32 load値を使う全i64 op（i64.rem_s、int.to_string）を壊す。修正：source要素をbind…［原文ここで途切れ］

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
- v1：render_programはexit 0、wat2wasmは拒否：h3.wat:558:40 'type mismatch in i64.gt_s, expected [i64, i64] but got [i32, i64]'、(i64.gt_s (local.get $v29) (local.get $v30))。int.to_stringもi64対i32。入力を(i32.load ...)で$v29へloadしi64.gt_sで使用。この欠陥を示…［原文ここで途切れ］
- rootHint：同根因。v1 FilterMap/IterChain loop loweringは入力Int（i64）でなくheap出力要素（i32）から入力load幅を選ぶ。predicate演算子に無関係で、誤幅要素を使う全i64 opが失敗。pass_stdlib_lowering_p2.rs:312（desugar）とIt…［原文ここで途切れ］参照。

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
- v1：render_programはexit 0、wat2wasmは拒否：h4.wat:554:40 'type mismatch in i64.eq, expected [i64, i64] but got [i32, i64]'、(i64.eq (local.get $v26) (local.get $v27))。入力Intを(i32.load ...)で$v26へloadしi64.eqで使用。このvariantにint.to_stringは無い（定数Some("two")）［原文ここで途切れ］。
- rootHint：単一根因の最も明快な切分け。list.filter_mapがheap（String/List）結果要素を生成すると、v1 IterChain/FilterMap loop emitterが各入力要素を入力型幅（Int→i64）でなく出力幅（i32）でload。4穴すべて一つのb…［原文ここで途切れ］

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
- rootHint：tuple-pattern match armでa,bをtuple heap slot（+0/+8）から一度もloadしない。
  tuple構築/格納は正しいがarm本体は未初期化local（$v9）をa+b結果として読みint.to_stringに0。
  match/tuple-pattern field binding（base+offse…からi64 load［原文ここで途切れ］）

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
- rootHint：h1と同じtuple-pattern分解バグ、混合型tuple（String,Int）。pattern変数s/nがslotからloadされず、
  sは未初期化string ref（空）、nは0を読み、'k=3'でなく'  =0'。match loweringにtuple要素bind（offsetからload）が欠ける。

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
- rootHint：入れ子Option match。constructor payload bind（some(inner)のinner、some(n)のn）がloadされずtag識別も誤読。
  v1は3arm全部の出力と0を出し、Optionのsome/none識別検査とpayload抽出の誤loweringを示す
  （おそらくtag/payload …［原文ここで途切れ］）。

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
- rootHint：record field Option（n.next）のmatchで両arm実行、n.val field-loadもsome(x) payload bindも0を読む。
  壊れたOption識別/payload bindとrecord-field match subjectの組合せ。n.valを正しくloadせずsome payload xも未bind。

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
- rootHint：Optionのtupleをlet-tuple分解。a,bをslotからloadせず、aは不正/zero ref。
  続くaのmatchがOption tagを誤読（0と'none'出力）、wasm `unreachable` に達する
  （match網羅性/default trapか未bind payloadの不正pointer deref）。S…［原文ここで途切れ］

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
- rootHint：drop関数emission不足。render_programが$__drop_list_int_str（List[(Int,String)]のdrop）を呼ぶが未定義。
  要素型tuple（Int,String）向けper-type drop-thunk generatorがlist-of-tuple用定義を出さず、dangling ca…［原文ここで途切れ］

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
- rootHint：record分解 `let { first, second } = p` がbase+12/base+20のi64 fieldをloadせず、
  (local.set $vN (local.get $v3)) + rc_incでfirst/secondをrecord pointer（i32 local $v3）へbind。
  armはi32 pointer二つへi64.addしてwat型不一致。Record-p…［原文ここで途切れ］

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
- rootHint：subjectがlist handleを持つlet-bound local（ここではstring.split）だと `parts[idx]` subscriptが黙って省略。
  出力$mainでindex自体を落とし、`(local.set $v6 (local.get $v2))` が `parts[0]` をlist pointer全体に置換、
  list headerを__str…へ渡す［原文ここで途切れ］。

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
- rootHint：hole 1と同根因。list literal内の各 `parts[k]` をsplit結果list pointer全体へ省略し、
  string.lenが各string byte長でなくlist header len（3）を読む。index無視で全3件が3。
  値位置のsubscriptの…［原文ここで途切れ］

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
- rootHint：同根因。`cs[0]`/`cs[1]`（cs=string.charsのlet-bound結果）のsubscript消失。
  両者list pointer全体を補間（header bytesをUTF-8としてrender）し、indexに関係なく同じゴミ `H    `。
  let-bound list localのvalue位置Index lowering。

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
- rootHint：同根因。`ls[1]`（ls=string.linesのlet-bound結果）をlist pointer全体へ省略。
  補間は要素1でなくlist header bytes（`@  `）をrender。
  let-bound list localの値位置subscriptが[idx]を捨てる。

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
- rootHint：同根因。call引数（tagへ）の `parts[1]` をsplit結果list pointer全体へ省略。
  tagはlist header bytes（`L  `）をconcat。補間だけでなく引数位置でも省略すると確認。
  let-bound list localの値位置Index lowering。

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
- rootHint：lambda本体内の同根因。`kv[0]`/`kv[1]`（kv=string.splitのlet-bound結果）を内側list pointer全体へ省略。
  補間がlist-header bytes（replacement chars）をrenderし `->` 両側同一。
  map/lambda構造は他には問題なし（2pair生成）。only th…［原文ここで途切れ］

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
  wat宣言(memory (export "memory") 1)=1page（64KiB）、memory.growなし。
  切分け確認：scalar-acc @depth 2000 byte一致、String-acc @depth 300 byte一致、List-acc @dep…［原文ここで途切れ］
- rootHint：call引数 `acc + [n]` がframeごとに新Listを作る非tail再帰累積。
  `+`（list concat）の消費list operandを再帰call chainでRC解放せず、一時確保が1page64KiBを使い切るまで蓄積
  （$alloc bumps the …［原文ここで途切れ］）。

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
  下降fill形同様1page（memory.growなし）枯渇、今回は上昇形。250要素List[Int]に64KiBは不要なはず。
  trapは `acc + [i]` 中間値のframeごとのリーク…［原文ここで途切れ］
- rootHint：fill形と同根因。call引数が `acc + [i]`（list concat）の非tail再帰。
  frameごと消費accumulator listが回収されず、memory.growなしの1page64KiBを確保が越える。
  修正は再帰loweringのcall引数list-concat回収と同じ…［原文ここで途切れ］

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
- v1：sum=0、depth 1でも確認（parse_rec_arm：native sum=2、wasm sum=0）。
  出力$psum本体：else armは$v6=n-1（第1引数）を計算し `(call $psum (local.get $v6) (local.get $v7))`、
  だが$v7は宣言だけで代入なしlocal（既定0）。`call $int.parse` も無く、そして［原文ここで途切れ］
- rootHint：再帰call引数の値式lowering。そのBinOp（`acc + ...`）のoperandがstdlib Result-returning call
  （int.parse(...)!）の `!` unwrap。引数内 `!`/Result-unwrapで周囲引数式が実体化せず、rendererはan…［原文ここで途切れ］

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
- v1：sum=0、出力$psumは最小形よりさらに切り詰められる。
  `let n = int.parse(list.get(parts,i) ?? "0")` 計算を$v31へ出し、そのrc_inc/rc_dec cleanupのみ。
  その後宣言だけで未代入local（=0）の$v34を `(local.get $v34)` で返す。The…［原文ここで途切れ］
- rootHint：最小int.parse!-in-recursion形と同族だが失敗は全体。
  再帰effect-fn block本体内LET束縛にあるstdlib Result call（int.parse(...)!）の `!` unwrapで
  loweringがblockを早期終了。let-bindとそのRC teardownだけemit…［原文ここで途切れ］

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
- v1：wasm trap: integer divide by zero（wasmtime exit 134）。生成$mainは両operandを平坦にlower：
  $v2=(a != 0)、$v5=i64.div_s($v3=10,$v4=0)を無条件計算（h1.wat:519）、
  その後$v8=i64.and($v2,$v7)（h1.wat:522）。10/0のeager div_sはand到達前にtrap。
- rootHint：BinOp::And（論理 `and`）のMIR loweringが、短絡制御
  （LHS falseならRHSを飛ばす `if`/branch）でなく実体化済み両operandのeager bitwise i64.and。
  RHS `(10/a)>0`（i64.div_s）を外へhoistし無条件評価するためdivide-…［原文ここで途切れ］

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
- v1：wasm trap: integer divide by zero（wasmtime exit 134）。`and` 同様flat loweringだがi64.or：
  $v2=(a == 0)、$v5=i64.div_s(10,0)を無条件計算（h2.wat:519）、$v8=i64.or($v2,$v7)（h2.wat:522）。
  nativeは `a == 0` がtrueで短絡し除算しない。v1はeag…除算［原文ここで途切れ］
- rootHint：hole 1と同じ単一欠陥、今回はBinOp::Or（論理 `or`）。
  短絡（LHS trueでthenがtrueを返しRHSを飛ばす `if (result i64)`）でなく、
  実体化済みoperandのeager bitwise i64.orへlower。RHS `(10/a)>0` i64.div_sは無条件評価［原文ここで途切れ］。

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
- v1：wasm trap: unreachable instruction executed（wasmtime exit 134）、bounds-check失敗経路。
  $mainが$v14=(list.len(xs)>5)を計算後、長さ3listで無条件に$v17=elem_addr(xs,5)
  （h3.wat:537）をcall、bounds checkが `unreachable` を発火。
  $v21=i64.and($v14,$v20)（h3.wat…）の前［原文ここで途切れ］。
- rootHint：holes 1/2と同根因、BinOp::Andのeager非短絡lowering。
  RHS `xs[5] == 0` はelem_addr(xs,5)を要し、これは範囲外なら `unreachable` でtrapする境界検査helper。
  andが先に両operandを実体化するのでindex accessはeven…でも実行［原文ここで途切れ］

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
- v1：render_program exit 0、wat2wasm拒否：h1.wat:532:22: error: type mismatch in i64.add, expected [i64, i64] but got [i32, i32] — `(local.set $v11 (i64.add (local.get $v9) (local.get $v10)))`。
  WATで$v9/$v10はともに `(local.get $v3)`（i32 record pointer、rc_inc付き）へbind、inst…［原文ここで途切れ］
- rootHint：value/binding lowering pass（emit_wasm statements/expressions）のrecord-pattern分解
  （`let { x, y } = p`）。RecordPattern bindはfield名ごと `local.set $vN (local.get <record_ptr>)` を出し、
  projectionしない。`elem/field_addr(base + layout_offs
［原文の未閉code span/文はここで途切れ］

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
- v1：\0 / \0\n（分解したString field二つともrecord pointerへbindされ、print/str_concatがrecord struct addressを操作。
  literal '/' 周囲にNUL bytes、byte正確には `\0 space / \0 space \n`）。
  直接field access `p.name`/`p.tag` は正しくrender（確認control：`al/vi
［原文の未閉code span/文はここで途切れ］
- rootHint：h1/h5と同根因。record-pattern分解はfield名をlayout offsetの `i64.load` で射影せず、
  record pointer全体（`local.set $vN (local.get $v_rec)` + rc_inc）へbind。
  String fieldはpointer型互換（i32→ptr）なのでwat2wasm受理、runtimeではstr_con…［原文ここで途切れ］

<a id="28--binding--invalid-wat"></a>

## #28 binding / invalid-wat
```almide
effect fn main() -> Unit = {
  let pairs = [(1, "a"), (2, "b")]
  println(int.to_string(list.len(pairs)))
}
```
- native：`2`
- v1：render_program exit 0、wat2wasm拒否：h3.wat:570:11: error: undefined function variable "$__drop_list_int_str" — `(call $__drop_list_int_str (local.get $v1))`。
  WAT内 `(func $__drop_list_int_str ...)` 定義はゼロだがcall箇所は出す（sibling要素形drop helper e…［原文ここで途切れ］）。
- rootHint：List[(T1,T2)] tuple要素向けdrop-helper生成matrix。
  emitter（rt_string/emit_wasm drop-helper節）は要素型からhelper名（`__drop_list_<elem>`）を作り各list localにCALLを出すが、
  helper定義generatorは `(Int, Strin
［原文の未閉code span/文はここで途切れ］ が欠ける。

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
- v1：1\n2\n3\n（byte正確）。WATはlist [1,2,3]を構築（offset12/20/28格納）、copy-on-write list_copy rc検査後、
  elem_addr+i64.loadで要素0/1/2を読む。index 1へのi64.storeも `i64.const 99` も無い。
  `xs[1] = 99` 代入文は…へlower［原文ここで途切れ］
- rootHint：emit_wasm statements.rsのindex代入文lowering（`xs[i] = v`）。
  IndexAssign/IndexSetはlowerされず、`elem_addr` + `i64.store` もlist_set callも無い。
  copy-on-write `list_copy` guardはemitされ（書込可能性経路は動く）、しかしその後の実storeはi…［原文ここで途切れ］

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
- v1：render_program exit 0、wat2wasm拒否：h5.wat:544:22: error: type mismatch in call, expected [i64] but got [i32] — `(local.set $v14 (call $int.to_string (local.get $v11)))`。
  $v11（および `name` の$v10）は `(local.get $v3)`（i32 record pointer、rc_inc付き）へbindされ、int.to_stringがreceiv…［原文ここで途切れ］
- rootHint：h1/h2と同根因。record-pattern分解はlayout offsetのfield projectionでなくrecord pointerへfield名をbind。
  Int fieldはwat2wasm型不一致として露呈（int.to_stringはi64期待、i32 ptrを得る）。
  修正=LayoutId offset+i64.loadで各fieldを射影、i…［原文ここで途切れ］
