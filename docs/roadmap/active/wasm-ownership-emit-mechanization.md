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

未確定 (要 NON-allocating runtime トレース): どのブロックが rc==0 まで
落ちて再 dec されるかは静的に確定できなかった。box の +1 が leak するなら
double-free ではなく leak になるはずで、ledger と実挙動が一致しない。

**決定的な観測 — HEISENBUG**: `__rc_dec` の double-free sentinel を
`unreachable` から「ポインタ出力 + return (再 push をスキップ)」に置換すると、
r643 / f 両方が **正しい出力で exit 0**、二重解放マーカーが**一度も出ない**。
原因は計装に使った `__println_int` 自体が `__alloc` する (i64→文字列 +
iovec) ため、**print がヒープの再利用パターンを変えて corruption をマスク**
すること。つまりこの二重解放は **アロケータの free→reuse タイミングに依存**
し、観測 (割り当てを挟む) が挙動を変える種類のバグ。具体的には list.get の
some-box (要素を `rc_inc`) が free され、直後の `list.slice|>list.join` の
`__alloc` がその block を再利用する経路と、box / element の dec タイミングが
噛み合った時にのみ発火する。

→ 安全な単発修正には **(a) 割り当てを伴わない runtime トレーサ** (違反
ポインタを固定 global に書く、または rc-count を native↔wasm で比較する
差分ハーネス) で「余分な dec」を1個に特定する、もしくは **(b) 上記
`emit_extract_owned` への集約 + `list.get` の some-box +1 と UnwrapOr の
alias-inc の二重カウント関係を設計レベルで一本化する** 必要がある。推測
パッチは新たな leak/regression を生むリスクが高いため**未マージ**。#643 は
本 roadmap の §1 として OPEN 継続。

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
   設計意図「leak 優先」に合わせる)。+ `spec/wasm_cross/option_fallback_rc.almd`
   回帰固定 + contract。
2. `emit_extract_owned` ヘルパー抽出 + 全 payload-取り出しサイトを移行。
3. `spec/churn/` owned-extract fixture + (将来) rc-count 差分ハーネス。
4. 文書化: emit 層が守るべき所有権契約を `emit_wasm/CLAUDE.md` に明文化。

## 却下した案

- **解析層だけ強化**: emit 層の borrow-vs-owned は IR に上がらないので、
  解析をいくら厳しくしても emit 層の drift は検出できない。
- **native 同様 Rust に丸投げ**: wasm は Rust を経由しないので不可。
  だからこそ所有権規律を「機械化された単一ヘルパー」に寄せる必要がある。
