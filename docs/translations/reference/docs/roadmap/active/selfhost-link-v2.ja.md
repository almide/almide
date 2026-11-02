<!-- description: Self-host linking v2 — renderer がすでに生成する単相化 generic 本体をリンクし、shim registry の型別 twin を廃止する -->
<a id="self-host-linking-v2--link-the-mono-instances-retire-the-twin-matrix"></a>

# Self-host linking v2 — mono instance をリンクし twin matrix を廃止する

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](selfhost-link-v2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](selfhost-link-v2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](selfhost-link-v2.zh-CN.md)

> 非規範的な履歴参照資料。固定原本を正本として変更せず保持し、自動同期しない。引用コマンド・測定・コンパイラ検証は実行していない。契約構文・フラグ・lowering は提案で、出荷済み機能ではない。冒頭の事後条件への言及と後の対象外指定、lowering 前後の記載差は原文どおり併存する。LLM 正確性の主張は設計上の期待で、新たな測定結果ではない。linking v2 は提案のまま。旧件数・bug・host の記述は現行の全量調査や普遍的保証ではなく、embedded Wasm host 対応は原文冒頭の限定を保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/selfhost-link-v2.md)

<a id="proposal-scope-and-historical-measurements"></a>

## 提案範囲と過去の測定

これは linking 統一の提案であり、完了報告ではない。#1154 の観測と下記 callee／call 件数は動機となった snapshot であり、新しい全 corpus 調査を主張しない。host effect はすべて native-only ではない。embedded Wasm host は対応する fs/process/http 操作を提供し、operation ごとの wall は [target availability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml) にある。この事実は、提案に残る実際の registry／twin と単相化本体 linking の課題を解消するものではない。旧 machinery の説明は [v1-selfhost-machinery.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-machinery.md)。

[[v1-selfhost-machinery]] に記録された shim 時代の後継設計。その取り組みは prim floor 上に splice-context `.almd` shim を手書きし、`self_host_registry.rs` で NAME によりリンクして、stdlib を wasm 側へ self-host した。動作はした（約380関数が byte-for-byte に実行する）が、本書で解消するための負債構造も蓄積した。

<a id="the-evidence-that-the-successor-is-already-half-built"></a>

## 後継がすでに半分できている証拠

動的、構造的な二つの観測。

**動的 — #1154 の wall-escape trace。** `result.zip(ok(1.0), ok("abc"))` の出力 module には両方が存在した：

- `$almide_rt_result_zip__Float_String_String` — **実際の generic `stdlib/result.almd` 本体を正しい型で単相化した instance**。通常の user-fn pipeline が render し、byte-correct だが **dead**。
- `$result.zip` — `result_core.almd` の手書き scalar shim。registry が link し、両引数を len-as-tag として読み、heap-Ok 引数では**誤る**（ok 入力二つから `err("abc")` を返した）。

**構造的 — sibling-linking 経路はすでに存在し動いている。** `linkable_module_fns`（`pipeline.rs:99`）は **registry entry に担当されない** bundled pure-Almide fn を、すべて通常の user sibling として供給する。`resolve_user_module_calls`（`pipeline.rs:150`）が Module call を Named call に書き換え、`monomorphize_module_fns` が TYPE 引数 tuple ごとに一つの本体へ特殊化する（`mono/mod.rs:44`、型変数による suffix — `__fallible_map__Int_String_String`）。`list.split_at`、`list.iterate`、七つの `__fallible_*` carrier は当時すでにこの経路で wasm 側に到達している。registry の `registry_served_names()`（`pipeline.rs:79`）は、文字通り shim を生き残らせる除外 list である。dotted name に shim が使われるのは必須だからではなく、list に載っているから。未登録には正しい machinery が動き、登録済みは手書き shim が横取りする、という逆転が論拠のすべてである。

<a id="the-debt-shape-being-retired"></a>

## 廃止する負債の形

1. **consumer ごとに導出し直す二つの tag layout。** scalar-Ok Result は tag を `@4` に持つ（len-as-tag）。heap-Ok Result は 1-slot cap-as-tag block（payload の下位 `@12`、tag `@16` — `result_materialize.rs`）。どちらの layout も問題ではない。どちらが適用されるかを全 router arm が導出し直すことが問題。
2. **手動 routing の twin matrix。** `result_call_name`（`mod_p4_c.rs`）と関連関数は combinator × payload-kind × twin（`_h` / `_x` / `_s2h`）を手で対応づける。comment block は bug 台帳：silent `ok("")` class（fuzz index 904、C-139）、seed-20260718 の map misroute（C-151）、#1154 の zip。いずれも同じ matrix の不足または狭すぎる arm である。String-suffix twin は手作業の単相化であり、手作業の単相化はずれる。
3. **unlinkable class。** 29 dotted callee にまたがる74 call site が class-b（「unlinkable stdlib call」）の wall となる。該当 cell の shim を誰も手書きしなかったため：`zlib.*`、`process.*`、`fs.fold_lines*`、`string.split_once`、`map.upsert_skv_wall`、`list.split_at`、…。すべて v2 が削除する表の欠けた行である。

<a id="the-v2-claim"></a>

## v2 の主張

> callee に pure `.almd` 本体がある dotted stdlib call は、既存の sibling 経路（`linkable_module_fns` ＋型 tuple ごとの単相化）で link する。`registry_served_names()` は PRIM FLOOR、つまり host intrinsic と memory を直接読む splice-context 本体まで縮小する。v2 は新しい machinery ではなく、検証した family ごとに list から名前を消していく取り組みである。

価値の順に並べた帰結：

- twin matrix と combinator ごとの routing arm を削除。layout 選択は、mono instance 自身の lowering がすでに参照する単一 repr oracle（`repr_of`）へ収束する。
- #1154 の bug class を表現できなくなる。誤った本体を選ぶ name-level routing 段階がなくなるため。
- class-b unlinkable 集合は「誰もまだ cell を手書きしていない」から真の host 境界（fs/process/http effect、allocator、prim floor）へ縮む。
- `list.iterate` と `__fallible_*` carrier は特別でなくなる。他と同じ generic 本体として instantiation ごとに link する。これが #1134 umbrella の終着形。shape ごとの wall burn-down が本体を lowerable にし、v2 が linking を統一する。

<a id="what-stays-a-shim-on-purpose"></a>

## 意図的に shim のまま残すもの

- **prim floor**（`prim.load*/store*/alloc*`、handle arithmetic）と、その上に書かれたすべての splice-context 本体。それ自体が memory model である。
- **host-effect intrinsic**（fs、process、http、env、random の host seed）：callback を越え errno を分類する表面は、C-215/C-220 がすでに native-first として範囲を定めている。
- **証明された** performance 上の理由がある手書き shim は残せるが、各々 mono instance との parity gate が必要。parity gate のない shim は将来の #1154 と同じ形。

<a id="staged-plan-each-stage-falsifiable"></a>

## 各段階を反証可能にした計画

1. **先に parity harness（反証手段）。** dotted name に pure generic `.almd` 本体もある registry entry ごとに、corpus が使う instantiation で shim と mono instance の両方を render する。生成入力上の実行を byte 比較する（3-way oracle の能力を内部へ向ける）。相違は、その時点で生きた #1154 級 bug を意味し、移行前に知る価値がある。この harness は以降すべての切替の soak gate にもなる。
2. **Family 1：`result.*`** — 二度問題を起こした matrix。instantiation ごとに call 解決先を mono instance へ切り替え、combinator ごとに `_h`/`_x` arm を削除。wall のある `_x` cell（zip-mixed 等）を、wall でなく実行するコードへ変える。ratchet：routing arm 数と class-b callee 数をともに単調減少させ、commit ごとに台帳化（C1 の規律）。
3. **Family 2..n：`option.* / list.* / map.*`** — 同じ family 単位の切替。bug 履歴、次に unlinkable cell 数で順序づける。
4. **registry の終着点。** `self_host_registry.rs` は prim floor ＋ host intrinsic へ縮み、twin suffix namespace（`_h`、`_x`、`_s2h`）を削除。`v1-selfhost-machinery.md` は時代の記録として `done/` へ移す。

<a id="costs-and-open-decisions"></a>

## コストと未決定事項

- **module size。** 共有 shim 一つに対し、instantiation ごとに mono instance 一つ。推測せず測る。minigit/onebrc の size 統計を scoreboard にする。DCE は未使用 instance をすでに消す（dead zip instance は出力されることを示し、問いは LIVE になったときの限界的増加だけ）。実際に増えるなら instantiation-collapsing（同一 repr の instantiation が一つの instance を共有）が既知の手段。
- **performance。** shim は手調整の prim-floor code、mono instance は lowering された generic 本体。perf ratchet（`check-perf-ratio.sh`）が他の変更と同様に切替 commit を gate する。
- **layout 統一**（未決定。v2 の要件ではない）：scalar-Ok result を cap-as-tag にまとめれば二重 layout の区別を完全に消せるが、hot scalar path の boxing コストを伴う。v2 は意図的に依存しない。どちらでも repr oracle が二重性を吸収する。family 1 の測定後に判断する。

<a id="what-would-falsify-this"></a>

## 反証となるもの

- **parity harness が孤立 bug を越える体系的 shim↔mono 相違を見つける** — mono instance 側が信頼できず、修正順序を逆にする（generic 本体を先に直す）。
- **#1134 の shape burn-down 後も fallible/HOF family の mono instance に wall が残る** — linking 切替が lowerability に先行しており、family ごとに burn-down の後へ順序を直す。
- **instantiation-collapsing で回復できない許容不能な size／perf 後退** — hot family は twin matrix を残すが、parity harness の後ろに置き、gate なしには二度としない。

<a id="relations--three-adjacent-recreates-three-altitudes"></a>

## 関係 — 隣接する三つの再設計、三つの階層

- **[Closure Architecture v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)**（完了）は closure の表現層を担当する。一つの identity、一つの capture-set、`lift_lambda` の decline class。#1134 family-1 wall（fallible_hof/fallible_lambda baseline entry）はそこにある。bundled generic 本体は lambda 引数が lift できれば当時すでに link でき、lift できなければ wall が発火する。本書はその境界に触れず、dotted call の本体をどこから取るか決める層を削除する。
- 入力となる取り組み：#1134 の shape ごとの wall burn-down（本体と lambda を lowerable にする）、#1108 2b-iii（user HOF の2-mode単相化。v2 切替も同じ mono machinery を使う）。
- 直交するもの：identity 層の再設計（#908 / QualifiedRef）。そちらは NAME-keyed identity、こちらは NAME-keyed linking を廃止する。組み合わせられるが、互いを要件としない。
- 完了時に置き換えるもの：[[v1-selfhost-machinery]] の linking 部分。prim-floor 部分はそこで継続する。
