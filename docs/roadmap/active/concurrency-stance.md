<!-- description: The concurrency model decision and the fan-family fixes it settles -->
# Concurrency stance: deterministic data-parallelism

> issue [#1000](https://github.com/almide/almide/issues/1000) の回答。ラダー行 0.44。
> この決定は `fan` 関連の未解決 4 件（#1023 / #1024 / #1025 / #1026）すべての正解を導く。

## 決定

**Almide の並行性モデルは「決定的データ並列」である。**

`fan` は**スケジューリングの構文であって、意味論の構文ではない**。実行は並列でよいが、
プログラムの観測可能な振る舞い（stdout / stderr / 終了コード / 返り値）は、
**リスト順に逐次評価した場合と厳密に同一**であると定義する。

リスト順の意味を与えられないものは、言語に入れない。

structured concurrency（goroutine / channel 相当）は採らない。

## なぜこれ以外にないのか

決め手は、新しい方針を選んだことではない。**すでにある約束のうち、両立しない 2 つの
どちらを捨てるかを決めた**ことである。

`docs/SPEC.md` は同じ機能について正反対のことを言っている。

- §13.1「real threads on native, sequential in declaration order on wasm — **results are
  identical either way**」
- §13.2「`fan.race(thunks)` — **first to complete wins**」

「どちらで実行しても結果が同じ」と「速い方が勝つ」は同時に成立しない。前者は
タイミングからの独立を要求し、後者はタイミングそのものを結果にする。

そして前者は捨てられない。native ⇄ wasm のバイト一致は契約台帳の存在理由そのもので、
`docs/contracts/contracts.toml` の 196 契約はすべてこの等価性の上に積まれている。
wasm32 にスレッドはないので、タイミング依存の構文は wasm 側で**原理的に実装できない**。
`fan.timeout` が 0.29.0 で撤去されたのも同じ理由だった —
「プログラムと入力の関数になっていない唯一の stdlib 表面」だったからである。

さらに、この言語の任務は LLM が最も正確に書ける言語であること、すなわち
modification survival rate である。同じ入力で走らせるたび出力が変わるコードは、
その計測自体を壊す。**決定性は Almide にとって性能特性ではなく、正しさの定義の一部**
である。

だから捨てるのは「速い方が勝つ」の側だ。

## 4 件への帰結

### #1024 `fan.race` — **撤去済（2026-07-31、develop にマージ）**

済んでいること: checker の E027 トンボストーン、C-004 の題と statement から
`fan.race` 節を削除、C-005 から head-Err 節と `spec/wasm_cross/fan_race_err.almd` を削除、
SPEC.md §9.8/§13.2・README・DESIGN の更新、診断 fixture `e027-fan-race-removed` 追加、
6 つの呼び出しサイトの移行。契約ゲート緑（198 契約 / flagged 0）、
spec 324 + examples 11 ファイル緑。

**移行は機械置換では済まなかった。** `fan.race(ts)` ≡ `ts[0]()` に対し `fan.any` は
thunk[0] が失敗したとき挙動が違う（any は次の Ok を探す）ので、head が Err のケースを
機械的に置換すると**通るが誤ったことを主張するテスト**になる。各ファイルが何を pin
しているかを読んで移した:

- `fan_race_test.almd` — race 専用なので削除（E027 fixture が置き換える）
- `fan_race_any_wasm.almd` → `fan_any_wasm.almd` — race ケースを落とし any を残す
- `fan_value_regression_test.almd` — 「Result 束縛が `??`/`==` を通る」は any へ、
  「capturing thunk 2 本が 1 リストを共有する（E0308）」は **2 本という形が本体**
  なので `fan.settle` へ（下の any テストと重複させない）
- `fan_pure_thunks` / `fan_var_thunk_list` / `fan_deterministic` — race 行を落とす、
  または同じ lowering 経路を通る `fan.any` へ

**トンボストーンの連鎖**も見つかった: `e027-fan-timeout-removed/fixed.almd` は
0.29.0 で `fan.timeout` の移行先として `fan.race` を指しており、今回の撤去で
コンパイルしなくなった。**トンボストーンの移行先は生きた表面でなければならない** —
死んだ表面へ誘導するヒントは、次の撤去で静かに壊れる。

### #1024（元の分析）

実装は既にリスト順で正しい。C-004 も正しい。嘘をついているのは SPEC.md だけ。

ただし「ドキュメントを直す」で終わらせない。IR を読むと
`desugar_fan.rs::rewrite_race_head` は `fan.race([t0, t1, …])` を **`t0` にそのまま
置換する**。他の thunk は実行すらされない。つまり `fan.race(ts)` は `ts[0]()` と
**完全に等価な空ラッパー**である。

決定的モデルの下では `race` という名前に与えられる意味が存在せず、意味を与えられた
としても既存表面の重複にしかならない。`fan.timeout` と同じ扱いにする —
**E027 系の check 時トンボストーン + 移行ヒント**（`ts[0]()` を直接呼ぶ、あるいは
失敗をスキップしたいなら `fan.any`）。共存させない。

**実施内容**: checker の `race` アームを E027 トンボストーンに置換。C-004 の題と
statement から `fan.race` 節を削除（`fan.any`/`map`/`settle` は不変）、C-005 から
head-Err 節と `spec/wasm_cross/fan_race_err.almd` を削除、SPEC.md §9.8/§13.2、
README、DESIGN を更新、診断 fixture `e027-fan-race-removed` を追加。
契約ゲート緑（198 契約 / flagged 0）。

撤去中に**トンボストーンの連鎖**が 1 つ見つかった: `e027-fan-timeout-removed` の
`fixed.almd`（0.29.0 で `fan.timeout` の移行先として書かれたもの）が `fan.race` を
使っていて、今回の撤去でコンパイルしなくなった。**トンボストーンの移行先は生きた
表面でなければならない** — 死んだ表面へ誘導するヒントは、次の撤去で静かに壊れる。
`fan.any` に更新し、`fan.timeout` のヒント文からも `fan.race` を削除した。

`fan.any`（リスト順で最初の Ok）は残す。失敗をスキップするという、重複でない意味がある。

### #1023 Err は兄弟をキャンセルしない — 仕様が誤り、実装が正しい

SPEC.md §9.9 / §13.1 の「siblings are cancelled」を削除する。

決定的モデルの下で**キャンセルは実装してはならない**。キャンセルされた兄弟の副作用が
どこまで実行済みかはタイミング依存になり、決定性が壊れる。`fan` は常に全兄弟を join し、
ブロックの値は**リスト順で最初の `Err`** とする。

「停止しない兄弟がいると永久にハングする」は、この立場では**バグではなく仕様**である。
逐次評価で停止しないプログラムは並列評価でも停止しない。そう明記する。

### #1026 trap した兄弟がプロセスを落とす — 契約を新設し、出力を決定化する

これは本物の穴である。C-004 は結果の順序だけを固定し、**副作用の interleaving は
native では wall-clock、wasm では逐次**という例外を statement の中に抱えている。
台帳のラチェットはこの手の「statement 内例外」を数えないので、等価性の主張に
数えられていない綻びが残っている。

理想側に倒す。**native の各 arm の stdout / stderr をバッファし、fan 決着後に
リスト順でフラッシュする**。wasm は逐次実行なので既にリスト順であり、これは
native を wasm に合わせる方向の変更である。結果:

- C-004 の EXCEPTION 節が退役でき、台帳の等価性主張が一段強くなる
- trap の意味が定義できる — arm k が trap したら arm 0..k-1 の出力がフラッシュされ、
  k 以降はされない。両ターゲットで同一
- 新契約 `fan` × trap を、この収束した振る舞いで書ける

代償はストリーミング性（長時間走る arm の出力が最後まで見えない）。決定性を上に置く。
台帳の観測範囲は stdout / stderr / exit code なので、ファイル書き込みや
ネットワークはこの主張の外にある — それは構成上の境界であって、隠した例外ではない。

### #1025 兄弟が同じ可変コレクションを alias できる — **#1027 の修正で閉じた**

これだけは並行性モデルと無関係に壊れている。frontend が通し、native は rustc E0382 で
ICE、wasm は黙って変更を捨てる。E008 が `var` キャプチャを正しく拒否している一方、
**呼び出し引数経由の同じ危険が完全に無検査**である。

決定的モデルでは 2 つの arm が同じ可変束縛を触ることに定義可能な意味がない
（リスト順の意味を与えるなら逐次実行と同じで、並列の意味がない）。

**2026-07-31 の調査で、根は fan より 1 段深いことが判明した。** #1025 は fan の
alias 問題として報告されているが、fan は必要条件ですらない:

```almide
effect fn pusher(xs: List[Int], n: Int) -> Result[Int, String] = { xs.push(n); ok(n) }
let shared: List[Int] = []
let a = pusher(shared, 1)!
let b = pusher(shared, 2)!
// native: len=2 / wasm: len=0
```

UFCS 形式 `xs.push(n)` は mut パラメータ検査を素通りする（同じ本体を
`list.push(xs, n)` と書けば E007 で正しく拒否される）。原因は
`check/calls_ufcs.rs` が `validate_mut_args` にレシーバを含めない引数リストを
渡していること — UFCS 脱糖後、レシーバは引数 0 であり、`list.push(mut xs, x)` の
mut パラメータはまさに index 0 である。

これは wall でも trap でもなく、**checker が受理したプログラムでの
クロスターゲット出力乖離＝誤ったバイト**であり、契約台帳が存在する理由そのものの
クラスに当たる。**[#1027](https://github.com/almide/almide/issues/1027) を #1025 より
先に直す**。**2026-07-31 修正済** — `check_call_target_builtin_ufcs` が解決キーを
知っている位置で、レシーバを引数 0 に置いた `validate_ufcs_mut_args` を呼ぶ。
上の repro は両ターゲットで E007 になり、正当な `mut` レシーバは動作継続。
324 spec ファイル緑 + 診断 fixture `e007-ufcs-mut-receiver`。

そして **#1025 は #1027 の修正だけで閉じた**。fan の兄弟 alias に到達する経路は
2 本しかなく、両方が既に塞がっている:

- 引数に `mut` 宣言が無いまま UFCS で破壊的変更 → **E007**（#1027 の修正）
- 引数に `mut` 宣言がある → 呼び出し側は `var` 束縛でなければならない →
  fan 内の `var` キャプチャは **E008**（従来から存在）

issue の repro をそのまま実行すると両ターゲットで E007 になり、`mut` 宣言版は
両ターゲットで E008 になる。fan 専用の新しい規則は不要だった — 必要だったのは
mut 性がどの構文でも等しく強制されることだけで、それが無かったことが唯一の穴だった。
立場（決定的データ並列）はこの結論と整合する: 2 つの arm が同じ可変束縛を触る
プログラムは、そもそも書けない。mut 性が強制されれば #1025 は「2 つの arm が同じ `mut` 引数を渡す」に
還元され、宣言だけで判定できるようになる。

## 実装順序（0.44 Unit）

1. **#1027 — UFCS の mut パラメータ検査漏れ（誤バイト、最優先）**
2. #1025 — fan の兄弟間 alias（#1027 の後なら宣言だけで判定できる）
3. #1024 — `fan.race` トンボストーン + SPEC.md 修正
4. #1023 — SPEC.md からキャンセル記述を削除、リスト順 Err を fixture で固定
5. #1026 — arm 出力のバッファリング + リスト順フラッシュ、C-004 の EXCEPTION 退役、
   trap 契約の新設
6. interp の `fan.race` / `fan.any` abstain を畳めるなら畳む（3 番目の審級を広げる）

## この決定が閉じないもの

将来 structured concurrency が必要になった場合、この決定は**再訪可能**である。
ただしそのときは wasm 側の答え（逐次フォールバックか、wall か、フラグ付き divergence か）
を構文ごとに contract で先に決めることが条件になる。今それを払う理由がない、
というのがこの決定の内容であって、「不可能」と言っているのではない。
