原文の説明：セルフホスト print 基盤 — セルフホストの残り全体を支える観測可能性の要石
<a id="v1-self-host-print-floor--the-③-observability-keystone"></a>

# v1 セルフホスト print 基盤 — ③ 観測可能性の要石

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-selfhost-print-floor.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-selfhost-print-floor.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-selfhost-print-floor.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-print-floor.md)

> 原文注記：6月14日の設計・進捗履歴であり、現行 runtime の保証や今回実行したテストではない。初期の2要素 gather 設計は後段 DONE で2回の単一 iovec write に変更される。wasmtime の記述は当時の観測として保存し、全版共通の性質としない。初期の即値 offset load/store・PrimHostCall 案と後段 handle/address・Op::Prim 設計が併存する。print 不在、linkage 未解決、基礎計画の後に日付付き print 完了があり、制御フローは NEXT のまま。preamble 関数数の不変だけで安全性や信頼コード不在は証明できず、閉じた基盤の別計上、raw memory prim の制限、checker の前提、capability 計上、arm ごとの均衡が必要。コード概略は compile しておらず、数値整形例には負数等の省略がある。実装・test・proof・benchmark は再実行していない。


**状態**：設計（2026-06-14）。5 agent の設計ワークフロー（先行事例＋コードベース棚卸し＋候補基盤2案。審判ステップが中断したため、これは手作業で統合した決定）から構成した。何も置き換えず、最初のセルフホスト slice に向けて `v1-mir-architecture.md` §4 を*具体化*する。

<a id="why-print-is-the-keystone"></a>

## なぜ print が要石なのか

v1 の実行経路（lower→MIR→render_wasm→wasm→wasmtime）は、現時点で heap 値プログラムと scalar call プログラムについて v0 と byte 一致する（commits 915fe2ee, 8cb5a093）。しかし v0 oracle はプログラムの **stdout/exit** を比較し、v1 には **print がない**。そのため計算は一切 end-to-end に観測できず、scalar 値、制御フロー、データはすべて「目隠し」で動く（unit test のみ可能）。**Print は観測チャネル**であり、他の全機能を v0 と比較して end-to-end に検証可能にする。実プログラムには出力も必要だ。したがって `println` は「v0 の全機能を v1 経由で動かす」ための、最も効果が大きい単一の障害解消である。

配線は既にある。`println(s)` は `Op::Call{RtFn::PrintStr}` に lowering され（lower/calls.rs）、render は `(call $print_str ...)` を出す。しかし `$print_str` には**本体がない**。規律テスト `handwritten_wasm_runtime_does_not_grow`（render_wasm.rs、baseline 11）は `$print_str` を手書き preamble に加えることを禁じる（それは v0 の罠）。したがって print_str を **Almide でセルフホスト**し、v1 を通じて `(func $print_str ...)` へ compile する必要がある。それなら preamble の func 数は変わらず、規律テストは PASS する。

<a id="the-decision-the-thin-primitive-floor-candidate-a-scoped-to-print_str"></a>

## 決定：print_str に限定した薄い primitive 基盤（候補 A）

2候補を設計した。

- **A — 薄い基盤**：新 MIR primitive op `Load{width}` / `Store{width}` / `HostCall{cap}`（raw memory＋fd_write host import）を inline render し、その上で print_str（最終的には全 stdlib）を Almide で書く。§4 の理想（信頼する／証明不能な面積を最小にする「約20 op の証明可能な基盤」）。
- **B — 太い `host.write_line(s)` primitive**：byte copy＋iovec＋newline＋fd_write を inline に組み込んだ単一 op。1 brick で print が動くが、**手書き WAT 本体を増やす**（`(func $` の*数*をごまかすだけで、信頼する WAT の*面積*は増える）。stdlib を基盤へ混ぜる v0 の罠、「後で解消する段階案」である。却下：「今は罠を出荷して後で薄くする」は、プロジェクトが禁じる継ぎはぎそのもの。

**print_str で A を実行可能にする決定的洞察（「A には複数 brick の制御フローが要る」という懸念への反証）**：`print_str` に **copy loop、itoa、Div/Rem/比較、汎用制御フローは不要**。`fd_write` は **iovec 配列**を取るので、**2要素 iovec** で zero-copy print できる。

```
iovec[0] = { ptr = s + LIST_HEADER, len = load32(s, LIST_LEN_OFFSET) }  // the string bytes, in place
iovec[1] = { ptr = NEWLINE_ADDR,    len = 1 }                            // "\n"
fd_write(STDOUT_FD, IOVEC_ADDR, /*count*/ 2, NWRITTEN_ADDR)
```

保持した iovec 例のコメント：先頭要素は元の位置にある文字列 bytes、次の要素は改行で、count は2。

OS が gather し、こちらは loop しない。制御フロー＋itoa の懸念は `print_INT`（数値整形）に当てはまり、`print_STR` **ではない**。したがって print_str の薄い基盤に必要なのは、即値 offset 付き `load`/`store`＋`fd_write`＋単一 pointer 加算 `s + LIST_HEADER`、つまり **scalar 値フロー＋基盤 op 3個**で、loop は不要。

<a id="floor-design-the-trusted-proof-bounded-set"></a>

## 基盤の設計（信頼対象・証明範囲の閉じた集合）

新 MIR op は閉じた集合で、全域 match により inline render する。preamble の `(func $...)` は**ゼロ**追加なので、open-stdlib の baseline-11 ratchet は変更しない。`RC_PRIMITIVE_FNS` と同様、基盤として別計上する。小さく、全域で、判断を含まないため、wasm spec への忠実性を証明できる — §4.1 の目標である。

| MIR op | wasm | これ以上還元できない理由 |
|---|---|---|
| `PrimLoad{width, signed}` | `i32.load8_u`/`i32.load`/`i64.load` | 計算した offset の raw memory を読むことは、どの高水準 op よりも下にある |
| `PrimStore{width}` | `i32.store8`/`i32.store`/`i64.store` | その双対で、iovec を詰めるために必要 |
| `PrimHostCall{cap}` | `(call $fd_write ...)`（既存 WASI import） | wasm 自体に syscall はない。host import は唯一の sandbox 出口で、host 定義かつ言語内で表現できない。`Capability` を持つので網羅的 caps 計上に残る |

source の仕組み：コンパイラが認識する intrinsic の予約 **`prim` module** を MIR lowering 時に**名前で** dispatch する。これは `println`/`RtFn::PrintStr` が既に使う同じ継ぎ目であり、v0 の `@wasm_intrinsic`/`RuntimeCall` 経路ではない（それは v0 emitter 専用で、v1 lowering は `RuntimeCall` を `Opaque`/`Const` に defer する）。checker は固定 signature で `prim` を登録し、`prim.load32(ptr, off)` は `Op::PrimLoad{..}` に lowering される。`prim.*` は runtime source file だけに許す（AssemblyScript の `load<T>`/`store<T>` のような unsafe 部分言語）。

```almide
// print_str.almd (the runtime author writes this; compiled through v1 like any fn)
fn print_str(s: String) -> Unit = {
  let len = prim.load32(s, LIST_LEN_OFFSET)        // read header len
  prim.store32(IOVEC_ADDR, 0, prim.add(s, LIST_HEADER))  // iovec[0].ptr = s+12
  prim.store32(IOVEC_ADDR, 4, len)                  // iovec[0].len
  prim.store8(NEWLINE_ADDR, 0, 10)                  // '\n'
  prim.store32(IOVEC_ADDR, 8, NEWLINE_ADDR)         // iovec[1].ptr
  prim.store32(IOVEC_ADDR, 12, 1)                   // iovec[1].len
  prim.fd_write(STDOUT_FD, IOVEC_ADDR, 2, NWRITTEN_ADDR)
}
```

コード説明：runtime 作者が書く print_str.almd は他の関数同様に v1 で compile する。header 長を読み、文字列 bytes と改行の iovec を置き、fd_write を呼ぶ。コメントと sketch は原文どおり保持した。

<a id="settled-implementation-design-2026-06-14"></a>

## 確定した実装設計（2026-06-14）

**基礎は完了**（commits 74cc1fff `Op::ConstInt`、7d82f628 `IntBinOp` emission）：整数 literal が実体化し（`fn f() -> Int = 42` → `(i64.const 42)`）、整数演算が計算する（`a + b` → `i64.add`）。`lower_scalar_value`（lower/calls.rs）は Var/LitInt/Int-Add·Sub·Mul を再帰的に lowering し、部分集合の外では rollback→defer する。scalar は ownership を持たないため corpus-wall の cert は中立。これは print_str の `h + 4` / `h + 12` のアドレス計算と literal アドレスにまさに必要なものだ。

**SOURCE の仕組み＝PATH A**：bundled stdlib module `prim` を採用し、`@intrinsic` の再利用は選ばない。後者は v1 が Opaque に defer する `RuntimeCall` へ lowering されるためだ。Stdlib module は bundled `.almd` で、`bundled_sigs.rs` が signature を抽出する。`prim.X(..)` call は型検査され、`IrExprKind::Call{Module{"prim", X}}` に lowering されて、変わらず v1 lowering に届く。手順：(a) prim 関数を宣言する `stdlib/prim.almd` を加える（本体 `= _`）、(b) bundled-module list（almide-lang の `stdlib_info`）に `prim` を登録、(c) `lower/calls.rs` で `module == "prim"` を捕捉（`lower_pure_module_value_call` または兄弟関数の purity gate より前）し、各 `func` を MIR prim op に対応づける。

**MIR PRIM OPS**：新規で inline render → preamble の `(func $` 数は不変 → 規律 PASS。RC_PRIMITIVE_FNS と同様、閉じた集合を基盤として計上する。MIR は i64 統一で、i32 wasm memory 境界で op が wrap/extend する。

| prim fn（Almide） | MIR op | wasm render |
|---|---|---|
| `prim.handle(s: String) -> Int` | `PrimHandle{dst,src}` | `(i64.extend_i32_u (local.get $src))` — heap handle（i32）→ i64 アドレス |
| `prim.load32(addr: Int) -> Int` | `PrimLoad{dst,addr,width:4}` | `(i64.extend_i32_u (i32.load (i32.wrap_i64 (local.get $addr))))` |
| `prim.store32(addr: Int, val: Int)` | `PrimStore{addr,val,width:4}` | `(i32.store (i32.wrap_i64 …addr) (i32.wrap_i64 …val))` |
| `prim.store8(addr: Int, val: Int)` | `PrimStore{…,width:1}` | `(i32.store8 …)` |
| `prim.fd_write(fd,iov,count,nw: Int) -> Int` | `PrimFdWrite{dst,…}` | `(i64.extend_i32_u (call $fd_write (i32.wrap_i64 …)×4))` — `Capability::Stdout` を持つ |

Cert：PrimHandle/Load/Store は scalar なので ownership 上 no-op。name witness には `dst` を定義し、operand を use する。**PrimFdWrite は `Capability::Stdout` を持つ**ため、caps fold は `RtFn::PrintStr` 同様 Stdout に写し、print_str とその caller に正しく Stdout 宣言を要求する必要がある。op は一貫して i64 ValueId を使う（scalar model）。`prim.handle` は String→Int の橋で、すべてのアドレス演算を `Int` の `IntBinOp` にする（String+Int 型エラーなし）。

**print_str.almd**：基礎の `h + 4` / `h + 12`＋literal アドレスを使用。アドレスは既存 preamble layout、NWRITTEN=0、IOVEC=8、SCRATCH=512 に合わせる。
```almide
fn print_str(s: String) -> Unit = {
  let h = prim.handle(s)            // i64 address of the [rc][len][cap][bytes] block
  let len = prim.load32(h + 4)      // header len
  let data = h + 12                 // byte start
  prim.store32(8, data)             // iovec[0] = { data, len }
  prim.store32(12, len)
  prim.store8(512, 10)              // "\n" at scratch
  prim.store32(16, 512)             // iovec[1] = { 512, 1 }
  prim.store32(20, 1)
  prim.fd_write(1, 8, 2, 0)         // fd_write(stdout, iovec@8, count 2, nwritten@0)
}
```

コード説明：h は [rc][len][cap][bytes] block の i64 アドレス、h+4 は header 長、h+12 は byte 開始位置。iovec[0] は data と len、scratch の512に改行を置き、iovec[1] は512と1。stdout、iovec@8、count 2、nwritten@0 で fd_write する。

**LINKAGE**：この slice 内で確定する未解決詳細。render_program は単一 source の `ir.functions` を lowering する。bundled `prim`/print_str module は型付けには自動 import されても、本体は `ir.functions` に lowering されない可能性がある。したがって print_str 本体を含める必要がある。compile source に `stdlib/print_str.almd` を連結するか、`PrintStr` 到達時に linker が bundled print_str fn 本体を含める。PrintStr CallFn → `(call $print_str)` の配線は存在する。

**SUB-SLICES**：(1) MIR prim op＋render＋wasmtime で実際に "hi" を印字する手構成 MIR `print_str` test（frontend なしで op＋render を独立確認）、(2) `prim` module＋lower/calls.rs の捕捉（source→op）、(3) print_str.almd＋linkage → `println("hello")` が v0 と byte 一致。規律テスト PASS（セルフホスト、preamble 増加なし）、corpus-wall/gate/cargo test は緑。

**目標（2026-06-14 設定）：「v1 が v0 と同じように動くまで」** — この基盤＋print が要石。その後 scalar/制御フロー/データをすべて e2e で v0 検証可能にし、stdlib を同じ基盤上でセルフホストする。

<a id="decomposition-campaign-order-toward-all-v0-features"></a>

## 分解（「v0 全機能」へ向かう campaign 順序）

1. **Scalar 値計算**（基礎、前提条件、defer 中の (a') track）：`Op::Const` が literal 値を持つ（`Const{dst, value: Option<i64>}`、`None`=defer/0）。lowering は scalar 演算を `Const` に defer せず `IntBinOp` を出し、scalar 値を通す（call/prim 結果は `Const(0)` でなく実値として流れる）。今は function-export+call または PrintInt fixture で unit test 可能、print 着地後は e2e 観測可能。侵襲的：`Op::Const` は約12箇所（cert/V/render/lower）で match されるが、すべて ownership の no-op を維持（scalar は所有しない）するため ownership cert は byte 不変。
2. **`prim` module＋基盤 op 3個**（`PrimLoad`/`PrimStore`/`PrimHostCall`）：checker module＋名前による lowering＋inline render＋規律テストの基盤別計上（RC_PRIMITIVE_FNS と同様）。
3. 1+2 上の **print_str.almd** → v1 が `(func $print_str)` に compile → 既存 `PrintStr` CallFn が解決 → **`println("hello")` が動き、v0 と byte 一致**。規律テスト PASS（preamble 不変）、corpus-wall/gate/cargo test 緑。
4. **観測可能性を解放** → scalar 値、制御フローなどが e2e v0 検証可能に。その後 `print_int`（itoa → Div/Rem/比較＋制御フロー track）、さらに同じ基盤で `string`/`list`/`map`/`json` をセルフホスト — §4 の「手書き wasm runtime ゼロ」への収束。

<a id="invariants-守る系-must-hold-throughout"></a>

## 不変条件（守る系、全期間で維持）

- 基盤 op は inline render → preamble の `(func $` 数は不変 → 規律テスト PASS。基盤は閉じた証明範囲として別計上し、open-stdlib baseline に混ぜない。
- `PrimHostCall` は `Capability` を持つので sandbox/caps 計上は網羅的なまま（host effect は不可視にならない）。
- scalar 値に ownership はないので、scalar 値＋prim 作業によって ownership cert（i/a/d/m）は byte 不変。corpus-wall ownership/names/caps ACCEPT 数は検証済み回復以外で動かしてはならない。
- セルフホスト print_str は通常の `MirFunction`。新しい手書き WAT は加えない。

<a id="done-so-far-2026-06-14"></a>

## ここまで完了（2026-06-14）

要石を実現：普通の `println("…")`（v0 と同一 source）が、prim 基盤上の Almide 製 `print_str` の自動 link を介して v0 と byte 一致する（commits 915fe2ee → b2896c87）。scalar 値基礎は完了：`ConstInt`、`IntBinOp` Add/Sub/Mul/Div/Mod。prim 基盤は `Op::Prim{Handle/Load/Store/FdWrite}`。print_str は単一 iovec の fd_write を2回使い string+"\n" を書く（wasmtime は2要素 iovec を gather しない）。

<a id="next-control-flow-execution--print_int-the-campaigns-next-architectural-slice"></a>

## 次：制御フロー実行 → print_int（campaign の次のアーキテクチャ slice）

健全性上重要（verify_ownership/cert に触れる）で corpus はこれを実行しない（runtime だけが使う）ため、scalar-call slice と同様に専用の敵対的検証が必要。新たな集中時間で行うのがよい。

**理由**：`if/match` は現在線形化される（両 arm を flat op list に lowering し、値は `Const`/`Opaque` に defer）。cert には健全でも、実行としては成立しない（両 arm の effect が走る）。`print_int`（再帰 itoa）には実際の分岐、つまり選ばれた arm だけの実行が必要。

**設計**（scalar を先行、heap-result if は後段）：
- `Op::If { cond: ValueId, then_body: Vec<Op>, then_val: Option<ValueId>, else_body:
  Vec<Op>, else_val: Option<ValueId>, dst: Option<ValueId> }` — 入れ子の op 本体。
- cond の比較 `IntOp` は `Lt/Le/Gt/Ge/Eq/Ne`（`left.ty == Ty::Int` を gate にする）。render は `(i64.extend_i32_u (i64.lt_s …))`（i64 比較→i32 0/1→i64）。
- lowering：`lower_branch` を変更して、arm ごとに均衡する `Op::If`＋arm 値を出す。既存線形化の arm ごとの drop 規律を保持する — その不変条件が「片 arm だけ実行」の健全性を支える。
- cert：`verify_ownership` / `name_witness` / `cap_witness` は両 arm に順番に再帰。corpus が線形化について既に証明する同じロジックを、構造化 arm に使う（再利用による健全性。ただし lowering の arm ごとの均衡が不可欠な前提であり、敵対的に検証する）。
- render：`(local.set $dst (if (result i64) (i32.wrap_i64? — cond is i64 0/1, use
  (i32.eqz (i32.eqz (i32.wrap_i64 cond))) or (i64.ne cond 0)) (then <then_body>
  (local.get $then_val)) (else <else_body> (local.get $else_val))))`。
- `defined_value`/`value_reprs`：`Op::If` dst→その repr。V は再帰または skip。
- その後 `stdlib/print_int.almd`：再帰 `put_int(n, pos)` — `if n < 10 then {store8;
  pos+1} else { let p = put_int(n/10, pos); store8(p, '0'+n%10); p+1 }` — prim＋Div/Mod［完了］＋If［今回］＋再帰［完了］上で構築。`int.to_string`/PrintInt をそこへ配線する。

print_int 後：制御フロー＋数値が観測可能 → scalar/branch 表面全体を v0 と e2e 検証。その後、同じ基盤で string/list/map/json をセルフホストする。
