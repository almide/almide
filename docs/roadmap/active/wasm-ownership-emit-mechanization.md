<!-- description: Mechanizing the WASM ownership emit layer to contain Perceus drift structurally -->
# WASM 所有権 emit 層の機械化 (Perceus drift の構造的封じ込め)

Status: active — 提案 + #643 の精密分析
Owner: compiler
Related: #643 (wasm rc double-free), #668/#643 history, project_wasm_frees_activation,
project_completeness_roadmap, contract C-066

## 問題の本質: Perceus は「解析層」だけ機械化されている

Almide の WASM 向け Perceus は2層に分かれている。

1. **解析層 (`pass_perceus.rs`) — 機械化済み。** IR 上で `RcInc`/`RcDec`
   ノードをどこに置くかを所有権解析で決める。`PerceusVerify` パスが
   IR レベルの inc/dec バランスを機械検証する (warn ではなく hard-error)。

2. **emit 層 (`emit_wasm/*.rs`) — 手書き。** `UnwrapOr` / `list.get` /
   record 構築 / `ToOption` … 各操作の実メモリアクセスを wasm で手書きする。
   **各 emit は「解析層が前提とする所有権契約」を自力で守る義務がある** が、
   その契約は型でも検証器でも表現されておらず、コメントと規律だけで保たれている。

native ターゲットは所有権を Rust (= 外部オラクル) に肩代わりさせるので
`unwrap_or` 等が move/clone され自動で正しい。WASM はそれを手で再実装して
いるので、**再実装に穴があると drift する**。これは rt-oracle レジストリが
存在する理由そのもの (クロスターゲットバグの約72%が「wasm ランタイムが
native オラクルから drift」)。

### なぜ検証ゲートが防げないか

`PerceusVerify` が見るのは **(1) の IR レベルの inc/dec バランスだけ**。
emit 層が「+1 付きの owned ポインタ」を返したか「+1 なしの borrow (alias)」を
返したかは IR の抽象より下なので**見えない**。だから「IR 上はバランスして
いるのに実行時に二重解放/leak」が成立する。`list.get` の some-box が要素を
`rc_inc` するのは emit 層の詳細 (`calls_list.rs`) で、検証器には不可視。

## #643 の精密分析 (still OPEN)

再現 (some-arm が真因。issue タイトルの none-arm 仮説は誤り):

```almide
let cs = ["a", "b"]
var out: List[String] = []
var i = 0
while i < 2 {
  let nx = list.get(cs, i) ?? ""     // in-bounds = some-arm
  list.push(out, list.slice(cs, i, i + 1) |> list.join(""))  // 後続 alloc
  i = i + 1
}
```

- native: 正しい。wasm: `__rc_dec` で double-free sentinel (rc==0 再 dec) → `unreachable` trap。
- 切り分け (`ALMIDE_WASM_FREES=1` 既定):
  - **some-arm (in-bounds) + 後続 alloc + ループ → TRAP**
  - none-arm (常に OOB) のみ → OK
  - `?? ""` を `"z"` に置換 → OK
  - 後続 alloc を `list.push(out, "lit")` に置換 → OK (latent)
  - ループを外す → OK

post-Perceus IR (`ALMIDE_DUMP_IR=Perceus`) のループ本体:

```
bind nx = unwrap_or(runtime_call list_get(cs, idx), "")
rc_inc nx                       // VDecl alias-inc (yields_borrowed_alias(UnwrapOr)=true)
  bind t657 = { bind t656=list_slice(cs..); bind t659=list_join(t656); rc_dec t656; t659 }
  runtime_call list_push(out, t657)
  rc_dec t657
assign i = i + 1
rc_dec nx                       // scope-end
...teardown: rc_dec cs; rc_dec out
```

確定した事実:

- `nx` (alias-inc + scope-dec) は IR 上バランス。
- `list.get` の some-box は要素を `rc_inc` する (`calls_list.rs` の SHARE)
  が、**その box を dec する IR ノードが存在しない** (UnwrapOr の inner は
  中間式で、どの var にも束縛されないため temp-dec が付かない)。
- `pass_perceus.rs:743` のコメントは「UnwrapOr は leak 方向に倒す
  (double-free より安全)」と明言 — **つまり double-free は設計違反**で、
  どこかで余分な dec が混入している。

### 確定 — 非割り当てトレーサで根本機構を特定 (2026-06-13)

最初の計装 (`__println_int`) は **それ自体が `__alloc` する** ため free→reuse
パターンを変え corruption をマスクした (HEISENBUG)。これを **固定低位メモリ
[16..36) に生バイトを `fd_write(stderr)` で書く非割り当てトレーサ** に置換し、
`__alloc` の返却 ptr+size と `__rc_dec` の free/double-free を観測したところ、
r643 の全タイムラインが取れた:

```
ALLOC 4184/16(cs list) 4208/8(out 空) 4224/4(Some box) 4240/12(slice) ...
FREE 4240(slice) ALLOC 4264/40 ALLOC 4224/12  ← 4224 は #2 で size4・未 free
FREE 4224 ... FREE 4264 FREE 4184 DOUBLE-FREE 4264 → trap
```

**根本機構**: `out` の要素スロットは `ptr + LIST_DATA_OFFSET(8)` から始まる。
容量 0 の `out`(4208, size8) の data[0] は **隣接する Some box(4224)の
ヘッダ(4216)と重なる**。`list.get(cs,i) ?? d` の Some box は **dec されず
leak**(UnwrapOr の inner 中間式に temp-dec が付かない)し、そのヘッダ(size=4)
が **size≥12 に上書き**される。すると `__alloc` の free-list の size チェック
(`cur.size >= request`)をすり抜け、**生きた undersize ブロック(4224)を払い
出す** → バッファオーバーフロー + 後続の rc_dec で二重解放(rc==0 sentinel)。
= 「ループ内の反復ヒープ temp(Some box / slice / join)の leak が隣接ブロック
のヘッダを破損し、`__alloc` が生きた小さいブロックを再配布する」クラス。

### スケール依存 — 試した get_or 融合は不十分 + 回帰

`list.get(xs,i) ?? d` を `list.get_or(xs,i,d)`(Some box を作らない)へ融合
すると **len2 の報告 repro は両ターゲット byte 一致**になるが:

- **len≥3 で再分岐 / len6 で trap** — slice/join temp 経由の同クラス破損が残る
  (Some box は1インスタンスに過ぎない)。
- `spec/wasm_cross/alias_combinator_rc` が **byte gate で divergence/trap** —
  get_or の rc 経路は get→unwrap と異なり、既存の alias RC を壊す。

→ よって **融合は #643 の fix ではない**(撤回済み)。真の修正は emit 層の
所有権機械化: (1) UnwrapOr/getter が確保した Option box を確実に dec する
(または box を作らない経路に統一)、(2) 反復ヒープ temp(slice/join 結果)の
rc を per-iteration で正しく解放、(3) `__alloc` の free-list reuse に
`cur.rc == 0` 不変条件を足し「生きたブロックの再配布」を防御(silent 破損を
clean trap に変える hardening)。+ `spec/churn/` に「ループ×反復ヒープ temp」
churn fixture を追加。推測パッチは新たな leak/regression を生むため**未マージ**、
#643 は本 roadmap §1 として OPEN 継続。

## 関連: #591 error.context の OK パスが main の戻り値を汚染 (still OPEN)

同じ「emit 層が IR の下で所有権/レイアウトを手書きし drift する」クラス。
`error.context(ok(3), msg)` は native 正(`3`)、wasm は **exit=1 + stderr に
巨大空白の `Error:` 行**を出し、`m ?? -1` は `0`(`match` 経由だと値は `3` で
正しい)。切り分け済み:

- err パス（C-098 系の inner=err）は native==wasm で正常。
- OK パスのみ破綻。`error.context` は IR では `runtime_call almide_rt_error_context`
  で、wasm では `dispatch_runtime_fallback("error","context")` →
  `emit_call` の inline arm（`calls.rs` の `("error","context")`、手書きの
  `if_i32` で OK は入力をパススルー、err は `": "` を挟んで再 wrap）に落ちる。
- inline arm は OK 結果(Result[Int,String], payload i64@4, 12B)と err 結果
  (手書きで 8B `[tag][str@4]`)で**レイアウトが非対称**。`??` が `0` を読み、
  match が `3` を読む不一致＋ main が Err を返す事実は、typed-if の結果が
  消費されきらず main の tail/return に漏れている疑いを示す。

→ 正攻法は inline 手書きをやめ、native オラクル
`almide_rt_error_context`(`r.clone().map_err(|e| format!("{ctx}: {e}"))`)を
**実 wasm runtime routine 化**して rt-oracle 登録すること(上の
`emit_extract_owned` 集約と同じ「手書き emit をオラクル準拠の単一実装に
寄せる」方針)。固定用 fixture `spec/wasm_cross/error_context.almd` は未追加(要作成 — 現状この主張を pin する fixture は存在しない)。

## 提案: emit 層の所有権を単一ヘルパーに集約する

散発的な手書き `rc_inc`/payload-load をやめ、「コンテナ/box から owned 値を
取り出す」を**単一の emit ヘルパー**に集約する:

```
fn emit_extract_owned(&mut self, payload_ty: &Ty, offset: u32)
//   load payload at offset; if is_heap_type(payload_ty) { rc_inc }   (ptr 透過)
```

- `UnwrapOr` (Option some / Result ok)、`Unwrap`、`Try`、`ToOption`、
  `OptionalChain`、map/list の Option-返却アクセサが**全て同じ規律**を通る。
- box が dec される経路 (consume-then-drop) では、payload を新しい owned
  reference として取り出す = +1 が必須、という契約が**1箇所**に集約され、
  #668 で「pass-through combinator は直したが UnwrapOr の some-arm が漏れた」
  ような部分対応の再発を構造的に防ぐ。

## 提案: 静的検証では見えない層を fuzz / churn ゲートで塞ぐ

IR レベルの `PerceusVerify` は emit 層の借用/所有権を見られない。よって:

- `spec/churn/` に **「owned-extract → drop」形 (OOB-default を含む
  `list.get(...) ?? d` を heap 要素のコンテナに対しループで回す)** の
  churn fixture を追加し、wasmtime で wall-clock 制限下に native 出力と
  比較する。単一 byte fixture では足りない (latent corruption は出力が
  正しくても起きる)。
- 可能なら **rc_inc/rc_dec の総数を runtime カウンタで native↔wasm
  比較**する差分ハーネス (所有権の drift を IR の下で直接観測する)。

## 段階

1. **#643 を runtime トレースで確定 → 単発修正** (emit 層 or pass_perceus、
   設計意図「leak 優先」に合わせる)。+ `spec/wasm_cross/option_fallback_rc.almd`(未追加 — 要作成)
   回帰固定 + contract。
2. `emit_extract_owned` ヘルパー抽出 + 全 payload-取り出しサイトを移行。
3. `spec/churn/` owned-extract fixture + (将来) rc-count 差分ハーネス。
4. 文書化: emit 層が守るべき所有権契約を `emit_wasm/CLAUDE.md` に明文化。

## 却下した案

- **解析層だけ強化**: emit 層の borrow-vs-owned は IR に上がらないので、
  解析をいくら厳しくしても emit 層の drift は検出できない。
- **native 同様 Rust に丸投げ**: wasm は Rust を経由しないので不可。
  だからこそ所有権規律を「機械化された単一ヘルパー」に寄せる必要がある。

## 2026-09-21 再測定: 段階 2〜4 は未実行のまま、同じ層から blocker が出続けている

上の診断(2026-07-30)から 7 週間。段階 1(#643 の単発修正)だけが実行され、
段階 2 の `emit_extract_owned` は grep 0 件、段階 3 の churn fixture・段階 4 の
契約明文化も無い。その間に何が起きたかを数える。

**blocker の出所(2026-08-22〜09-21 の I-unsound / I-divergence / I-miscompile /
regression 51 件、機構別)**:

| 機構 | 件数 | 例 |
|---|---:|---|
| **wasm emit 層の RC 規律**(消費サイトで inc / guard が抜ける) | **~12** | #1786 #1820 #2260 #2308 #2317 #2395 #2397 #2398 |
| wasm の i32/u32 添字・幅の切り詰め | ~7 | #1907 #1909 #2385 #2396 |
| wasm の固定長バッファ / park span | 5 | #1826 #2116 #2117 #2120 |
| 名前キー衝突(一つの識別子空間に二種類) | 4 | #1726 #1828 #2369 |
| native 借用推論のスペル漏れ | 3 | #1713 #2315 #2377 |
| 再帰深さ | 3 | #2291 #2306 #2307 |

最大バケットは本 roadmap が名指しした層で、**0.63.0-rc3 の soak が拾った 4 件のうち
3 件**(#2395 / #2397 / #2398)がここ。3 件とも「同じ操作の別コピーに規則が写っていない」
形だった:

- #2398: `list.update` — 兄弟 `list.set` は `ArgMode::Retain` で自動的に正しく、
  `update` は closure body を `self.lower(...)` で降ろすので mode が無い。
- #2397: `list.fold` — staged コピー(`list.rs`)は 10fed0494 で規則を得たが、
  `map`/`filter` 連鎖でだけ通る fused コピー(`list_fuse.rs`)はその**前に書かれ**、
  受け取っていない。
- #2395: `value.keys` — wasm 側と `value_core.almd` 側の**両方**にタグ検査が無く、
  interp だけが(偶然の 0 で)棄権していた。

### 診断の訂正: 穴は「ヘルパーの欠如」ではなく「一操作に実装が何個あるか」

07-30 の提案は「payload 取り出しを単一ヘルパーに寄せる」だった。それは正しいが、
今回の 3 件はヘルパーで防げる形ではない — **同じ操作が複数回実装されていて、
規則が一つに入っても残りに写らない**ことが原因。数える:

| 操作(list.almd の `= _` 穴) | native | structural emitter の arm | self-host の prim-floor コピー |
|---|---:|---:|---:|
| `fold` | 1 intrinsic | **3**(staged / fused / enumerate) | **6**(`list_fold*.almd`、shape 別) |
| `sort` | 1 | 1 | 6 |
| `get` | 1 | 2 | 4 |
| `filter` | 1 | 2 | 4 |
| `map` | 1 | 2 | 3 |

`list.fold` は **10 個の実装**を持つ。self-host 側のコピーは shape ごとの単相化
(`hrec` = heap acc × heap elem、`hsca`、`ols`、`str_hacc` …)で、prim floor に直接
書かれた手書きメモリコードである — Koka/Lean/Roc のように「stdlib は言語内の一つの
本体、Perceus が一様に処理」ではない。stdlib 3,877 関数のうち `= _` は **694**。

**掃引が見逃す理由(#2397 で実証)**: ee の閉包性掃引は述語を「closure body を lower して
heap slot に格納」とし、閉包性を **guard 綴り 4 種の件数**で立てた。accumulator の
rebind は local への格納なので母集団に入らず、`list_fuse.rs` は guard 語彙を一つも
持たないので「数えるものが無い」= 空として登録された。
**マーカーの有無で数える掃引はマーカー以前のコピーを見ない。「このファイルに 0 一致」は
clean の印ではなく未変換コピーの印である。**

構文形で取り直した母集団: `self.lower(<body>, …)` 44 サイト。うち未ガードで残るのは
`set.fold` / `map.fold`(`collections_set.rs:577` / `collections.rs:464`)だが、
両者は accumulator を `Lowered::view` で返すので呼び手が解放せず、freed read ではなく
leak 級(#2408)。view 返しが load-bearing であることは emitter のどこにも記録されていない。

### 段階の書き直し(0.63.0 final の後)

段階 2〜4 は据え置き、その前に **計数ゲート**を置く。順序は「測る → ratchet → 畳む」:

1. **実装計数ゲート** — `= _` 穴ごとに (native intrinsic, emitter arm, self-host コピー)
   の三つ組を数える台帳 `proofs/impl-count-ledger.toml` と shrink-only の ratchet。
   新しいコピーは台帳に行を足さないと CI が落ちる。#2397 は「fold の emitter arm が
   2→3 に増えた瞬間」に見えていたはずの事象。
2. **`list.fold` を畳む** — 10 実装を、target ごとに **1 本体**へ。fused/staged の 2 arm
   は 1 つの lowering + 融合を前段の pass で行う形に、self-host の 6 shape コピーは
   type-erased 1 本体(`list_fold_hrec.almd` が既にその形)へ。畳むたびに台帳の数字が
   下がり、alloc 台帳 / size ratchet / run parity が挙動不変を証する。
3. `update` / `filter_map` / `sort` と続け、**list.almd の 62 穴**を同じ手順で回す。
4. 07-30 の段階 2〜4 はこの後。畳んだ後なら「単一ヘルパー」が寄せる先は 1 本体だけになる。

却下: 「fixture を増やして次のコピー漏れを捕まえる」— #2397 の fixture は連鎖が
fuse する形でしか書けず、fuse しない fixture は全部緑だった。捕まえるのは計数であって
挙動ではない。
