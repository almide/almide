<a id="fan-semantics-audit-what-the-seven-fan-forms-do-today"></a>

# Fan 意味論監査：7つの fan 形式が現在何をするか

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-semantics-audit.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](fan-semantics-audit.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-semantics-audit.zh-CN.md)

> 固定原文の非規範的な歴史読者スナップショット。現行実装の手引きでも独立した言語比較でもない。変更していない原本が正本。自動同期、新たなテスト・ベンチマーク・ソース検証は行っていない。監査自身の証拠固定点は0746eb689（10月17日）で、10月18日の状態追記があり、以下の読者版原文固定点とは別である。修正済み issue、残る知見、決定済みだが導入中の Duration キャンセルを各時点の文脈のまま保持する。原文の pure 分類でも println は許され、観測可能な出力がない保証ではない。他言語の主張、「唯一」の主張、select_ok／破棄の説明、絶対的な決定性の記述は歴史的原文主張であり、今回独立に再確立していない。コード・識別子・参照は原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-semantics-audit.md)

> 最終更新：2026-10-17 · 固定コミット **0746eb689**（develop）。各主張は、そのコミットの `file:line`、native と wasm で実行した [research/fan-audit/probes/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/fan-audit/probes) のプローブ、または `tests/fan_acceptance_test.rs` のテストを引用する。

この監査は固定コミットに存在した意味論を記録し、提案はしない。これに基づく決定は [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)（2026-10-18採択）、他言語との比較は [fan-cross-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md) にある。

> **2026-10-18時点の状態。** dialect epoch 14以降、以下の表面構文は変わった。`fan.any { }` / `fan.any(xs, f)`（§4）は削除され、同じ選択は `a() ?? b() ?? fallback` と `list.find_map(xs, (x) => f(x)?)` で行う。いずれも定義上逐次である。`fan.race`（§5）は `fan.min(clock)` となり、clock 引数は必須、全失敗時の文言は `fan.min: no branch
> succeeded within budget`。`fan.min(duration…)` は決定済みで #3788 に追跡される。§6–§10の知見とそこで挙げた issue は記録どおりに残る。#3778（メーター）と #3780（ordered な分岐）は2026-10-18に修正済み。native の既定ルートは共有 meter plan を読み、`+` のみ課金し、`@ordered` stdlib 関数に到達する fan は全ターゲットで分岐を順に実行する。#3779と#3784も同日に修正。parallel twins は配ったブロックごとに timeline を1つ保持するため、`fan { list.map }` 内でも最小インデックスの trap が選ばれ、軽い `fan.map` は要素ごとの登録コストを払わなくなった。

各形式を、構文 → 型検査 → lowering → ランタイム（native/wasm、pure/effect）の順に追跡した。経路は `almide run`（native）、同じものに `ALMIDE_FAN_SEQUENTIAL=1` / `ALMIDE_FAN_COST_OFF=1` を設定したもの、そして組み込みホスト上で動く `almide run --target wasm`（`src/cli/run.rs:511-550`）。インタープリタ（`crates/almide-interp`）には CLI 入口がないため、その動作はソースを読んだだけで、実行していない。

<a id="summary"></a>

## 概要

| 形式 | 選ぶもの | 失敗後も実行するか | 何かを止めるか | Native | Wasm |
|---|---|---|---|---|---|
| `fan { a, b }` | 値のタプル、最小インデックスの Err | はい、全分岐 | いいえ | スレッド（effect 分岐も） | 順序どおり |
| `fan.map(xs, f)` | 入力順のリスト、最小インデックスの Err | はい、全要素 | いいえ | pure lambda：pool、effect：順序どおり | 順序どおり |
| `fan.settle { }` / `(xs, f)` | 全 Result | はい | いいえ | 順序どおり（タプル / `list.map` に lowering） | 順序どおり |
| `fan.any { }` / `(xs, f)` | 入力順で最初の Ok | 最初の Ok で止まる | 後続要素は開始しない | 順序どおり | 順序どおり。ただし fs-read prefetch は全読み取りを開始 |
| `fan.race { }` / `(xs, f)` | compute が最小の Ok。同値なら入力順 | はい、全分岐 | 予算付き分岐を打ち切る | 順序どおり | 順序どおり |
| `fan.bounded(c) { }` | 消費 ≤ 予算なら Ok | – | 本体を打ち切る | インライン | インライン |
| `fan.timeout(d) { }` | deadline を検出する検査がなければ Ok | – | 本体を打ち切る | インライン | インライン |

effect に対応する形式は2種類、すなわち `fan`、`map`、`settle`、`any` のブロック形式と mapper 形式である。4形式は本体の pure を検査する（E006）。両 `race` 形式、`bounded`、`timeout` が該当する。全形式に effect fn 文脈が必要（E007）。

<a id="1-fan--a-b-"></a>

## 1. `fan { a, b }`

**経路**：
- **パーサー：** `crates/almide-syntax/src/parser/compounds.rs:272-310`。`let`/`var`（283）、ループ（286）、`;`（295-296）、空ブロック（304-306）を拒否する。
- **検査：** effect fn 外で E007、キャプチャした `var` に E008（`crates/almide-frontend/src/concurrent_reach.rs:1-21`）。
- **Lowering：** `!` を持つ分岐を thunk scope にする（`crates/almide-frontend/src/lower/expressions_fan_arms.rs:30-49`）。
- **Native：** `mark_inline_fans`（`crates/almide-codegen/src/pass_fan_lowering.rs:82-120`）と `render_fan`（`crates/almide-codegen/src/walker/expressions_fan.rs:4-38`）、テンプレート `fan_expr` / `fan_single` / `fan_inline`（`crates/almide-codegen/templates/rust.toml:541-559`）、分岐ごとの出力 timeline（`crates/almide-codegen/src/prelude_fan.rs`）。
- **Wasm：** 逐次 desugar（`crates/almide-mir/src/lower/desugar_fan.rs:555-680`）。

| 項目 | 動作 | 証拠 |
|---|---|---|
| 成功 | 分岐順の値のタプル | テスト `results_and_output_follow_input_order…` |
| エラー | 後ろの分岐が時間上先に失敗しても、**最小インデックス**の Err | テスト `the_lowest_index_error…`（`block err A err`）；`desugar_fan.rs:636-640` |
| 順序 | native：分岐は同時に開始し、任意順で完了。結果と出力は分岐順（timeline を分岐順で flush、`prelude_fan.rs:9-15`）。Wasm：すべて分岐順。 | テスト `results_and_output…`（Cが先に完了しても `end A, end B, end C`） |
| 失敗後 | 全分岐が実行される | テスト（`ran C`） |
| キャンセル | なし。非停止分岐の隣に Err があればハングする。trap はそれより前の分岐を待ち、後ろは待たない（ADR-0024 D6）。後続分岐の開始も妨げない（`prelude_fan.rs:92-107` はマークして待つだけ）。 | 監査プローブ P7–P9（ブロック形式、受け入れノート内） |
| 副作用 | Err では全分岐の出力を保持。trap では前の分岐の出力と trap 分岐の部分出力を保持し、後ろの出力は捨てる。プロセス状態（`env.set`）は共有され、**native と wasm は一致しない（#3780）**。 | プローブ `A/p19_env_race.almd` |
| 空／全失敗／非停止 | 空は parse error、全失敗は最小インデックスを返し、非停止はハング | parser 304-306、テスト |
| 対象範囲 | native は2分岐以上の全ブロックをスレッド化。ただし Rc 値をキャプチャする分岐（`fan_inline`、逐次）や1分岐ブロック（`fan_single`）を除く。`ALMIDE_FAN_SEQUENTIAL` はブロックに**効果なし**。Wasm は順序どおりだが、pure scalar の `list.map` 分岐はインスタンス並列（`crates/almide-wasm/src/fan_par.rs:1-38`）。 | `expressions_fan.rs:15-20` |

`fan { list.map(xs, f) }` はブロック形式のデータ並列ケースである。
- **Native：** `almide_rt_list_par_map`（`runtime/rs/src/list.rs:1125-1154`）。**要素別の trap 順序がない**。時間上最初の trap が選ばれる（#3779、プローブ `A/p18_trap_order.almd`、無視されたテスト `a_trap_in_a_parallel_list_map…`）。
- **Wasm：** ホスト worker インスタンス（`crates/almide-wasm-run/src/host_fan.rs`）。trap 時は逐次で再実行するため、最小インデックスが選ばれる。

<a id="2-fanmapxs-f"></a>

## 2. `fan.map(xs, f)`

**経路**：
- **検査：** `crates/almide-frontend/src/check/static_dispatch.rs:202-283`。コールバックは `Result[B, String]` を返す必要がある。
- **Native ルーティング：** `crates/almide-codegen/src/pass_rust_lowering_fan.rs:496-507, 577-597`。Send-safe 型上の pure lambda リテラルは、コストモデル（`list.rs:303-343`）付きの `almide_rt_fan_map_par`（`runtime/rs/src/fan.rs:60-106`）へ行く。effect コールバックは、その唯一の effect が入れ子 fan の場合のみ並列化（`Purity::for_fan`、`tests/fan_nested_effect_parallel_test.rs`）。それ以外は逐次 `almide_rt_fan_map`（`fan.rs:18-38`）を使う。
- **Wasm：** `stdlib/fan_map.almd:1-27`、逐次。

| 項目 | 動作 | 証拠 |
|---|---|---|
| 成功 | 入力順の `ok(list)` | テスト `results_and_output…`（`map [36, 1050, …]`） |
| エラー | 最小インデックスの Err（`fan.rs:24-35, 110-121`） | テスト（インデックス6が先に失敗しても `err e2`、`err bad3`） |
| 順序 | 並列：ブロックを配られた worker がインデックス別 slot に書く。逐次：開始 = 完了 = 戻り = 入力順。 | テスト |
| 失敗後 | 全要素が実行（ADR-0024 D1） | テスト（`e2` 後の `run 3, run 4, run 5`） |
| キャンセル | なし。trap 時も並列経路は前の要素を待つ（`fan.rs:97`）。worker は trap 後もブロックの取得を止めない（`list.rs:643-680`）。 | — |
| 副作用 | effect コールバックは全経路で順序どおり動き、全 effect を保持 | テスト `effects_already_performed…` |
| 空／全失敗 | `ok([])`；最小インデックスの Err | テスト `empty_input…` |
| 対象範囲 | native pure：並列、native effect：入れ子 fan だけのコールバックを除き逐次、wasm：逐次 | 受け入れノートのプローブ計時 |

<a id="3-fansettle--a-b--and-fansettlexs-f"></a>

## 3. `fan.settle { a, b }` と `fan.settle(xs, f)`

- **ブロック：** `lower_fan_settle`（`expressions_fan_arms.rs:5-28`）が通常の**タプルリテラル**に lowering する。
- **Mapper：** `crates/almide-frontend/src/lower/calls.rs:65-71` が `list.map(xs, f)` に lowering する。

両方とも全ターゲットで逐次実行する。`almide_rt_fan_settle`（`runtime/rs/src/fan.rs:184-203`）には生成元がない。

| 項目 | 動作 | 証拠 |
|---|---|---|
| 結果 | 全 Result のタプル（ブロック）またはリスト（mapper）。通常の分岐は `ok` で包む | テスト `the_lowest_index_error…`（`settle 5`） |
| 順序 | 全箇所で入力順 | — |
| 失敗後 | 全要素が実行。**trap** はそれより後のすべてを止める | プローブ P11/P12（受け入れノート） |
| 空 | ブロック：parse error、mapper：`[]` | テスト `empty_input…` |
| 拒否 | thunk リスト `fan.settle([...])` は E027、effect fn 外は E007 | — |

<a id="4-fanany--a-b--and-fananyxs-f"></a>

## 4. `fan.any { a, b }` と `fan.any(xs, f)`

**経路**：
- **Parse：** ブロックは `fan.__any_block([() => a, …])` となる（`crates/almide-syntax/src/parser/fan.rs:296-321`）。
- **Lowering：** ブロックを `any`、mapper を `any_map` に改名（`lower/calls.rs:55-64`）。
- **Native：** `almide_rt_fan_any` / `almide_rt_fan_any_map`（`runtime/rs/src/fan.rs:148-179`）。
- **Wasm：** `stdlib/fan_any.almd` または `crates/almide-wasm/src/fan.rs:86-90` の accumulator。fs-read コールバックは prefetch lowering（`crates/almide-wasm/src/fan.rs:74-85`）を通る。

| 項目 | 動作 | 証拠 |
|---|---|---|
| 成功 | **入力順**で最初の Ok（時間上最初ではない） | テスト `any_runs_nothing_after_the_winner…` |
| 全失敗／空 | 定数 `err("fan.any: all candidates failed")`。各要素自身のエラーは捨てる | テスト `empty_input…` |
| 順序 | 開始 = 完了 = 戻り = 入力順 | テスト（`probe -1, probe -2, probe 5`） |
| 勝者の後 | 勝者より後は開始しない（`probe 7` は表示されない） | テスト |
| キャンセル | 未開始要素は単に開始しない。例外：wasm ではファイル読み取りコールバックを**prefetch**するため、全読み取りを先に発行し、敗者を破棄する（op 42 / subtask.cancel）。読み取りには可視の副作用がないため、ADR-0024修正（`docs/adr/0024-…md:443-445`）がこれを許す。 | プローブ `B/any_fs_import.almd` |
| 副作用 | 勝者より前の全要素の effect を順序どおり保持 | テスト（`ran a, ran b`） |
| 対象範囲 | effect 分岐とコールバックは全ターゲットで許可。全箇所逐次 | `static_dispatch.rs:322-359` |

<a id="5-fanrace--a-b--fanracexs-f-and-budgeted-forms"></a>

## 5. `fan.race { a, b }`、`fan.race(xs, f)` と予算付き形式

**経路**：
- **Parse：** `parser/fan.rs:206-273`。分岐を `,` または改行で区切り、`;` は拒否（プローブ `B/semi.almd`）。
- **検査：** `crates/almide-frontend/src/check/infer_calls_closures.rs:363-462`。計測領域内の分岐と mapper は **pure** と検査し、結果は `Result[T, String]`。
- **Lowering：** `crates/almide-frontend/src/lower/expressions_fan.rs:273-685`。各分岐を `__almd_bounded_N(budget, …)` に切り出して、最小の `(spend, index)` を保持する逐次 fold。比較は厳密な `<`（`:435`、`:623`）なので同値なら前の分岐。
- **死んだランタイム：** `almide_rt_fan_race`（`runtime/rs/src/fan.rs:123-142`）と `rewrite_race_head`（`desugar_fan.rs:305-340`）には生成元がない。thunk リスト呼び出しは E027（`static_dispatch.rs:284-307`）。

| 項目 | 動作 | 証拠 |
|---|---|---|
| 成功 | 消費が最小の Ok。同値なら入力順 | テスト（`race 205 105 105`、`race map 15`） |
| Err 分岐 | より安くても選択対象から外れる | テスト（`race err-skip 5000`） |
| 全失敗／空／全予算超過 | `err("fan.race: no branch completed within budget")`。予算なしでも同じ | テスト `empty_input…` |
| 順序 | 全ターゲットで入力順に分岐を1つずつ実行 | `expressions_fan.rs:316` |
| 勝者の後 | 全分岐を実行し、後ろの分岐が trap すればプログラムを中断 | プローブ `B/race_trap.almd` |
| キャンセル | 予算付き分岐は次の課金箇所で打ち切り、ゼロ値を返す（`render_native_op_families.rs:39-48`、`crates/almide-wasm/src/fuel.rs:114-127`）。並行するものがないので待機もない。 | プローブ `B/race_diverge_budget.almd` |
| 予算範囲 | ブロック形式：一度評価した同じ予算を**各分岐**へ（`expressions_fan.rs:302-309`）。mapper 形式：要素ごと（`:541-575`）。予算なしでは上限は `i64::MAX` ns。 | プローブ `B/race_map.almd` |
| 副作用 | 分岐は pure（E006）。観測可能なのは `println`、trap、非停止だけ。 | テスト `effects_inside_race…` |
| 対象範囲 | Int と String の勝者。**Float/List の勝者は両ターゲットでコンパイル不可（#3782）。** | プローブ `B/rt_*.almd` |
| ターゲット間 | **勝者が異なり得る（#3778）** | プローブ `B/race_flip.almd`、無視されたテスト |

予算なしの非停止分岐（プローブ `B/race_diverge_nobudget.almd`）は native では即座に終わり、wasm では何年も動く。両方とも同じ `i64::MAX` ns 上限で打ち切られるが、native では LLVM が副作用のない fuel countdown を閉形式へ畳み込む。答えは同じで壁時計時間だけ異なるため、これは動作の差異ではない。長いが有限の race は両ターゲットで一致する（プローブ `B/race_long_nobudget.almd`）。

<a id="6-fanboundedcomputexn--body-"></a>

## 6. `fan.bounded(compute.X(n)) { body }`

**経路**：
- **Parse：** `parser/fan.rs:127-159`。本体は完全なブロック。
- **検査：** `infer_calls_closures.rs:331-357`。本体は pure、予算は `Compute` 必須、Result 型本体はエラー。
- **Lowering：** `lower/expressions.rs:188-226` と `lower/expressions_fan.rs:142-260` が `budget_enter → body → budget_exit` を作る。
- **Native：** 既定は v1 MIR render。`ALMIDE_FORCE_METER_SPLICE=1` と `ALMIDE_FORCE_METER_CODEGEN=1` は他の2経路を選ぶ（`src/cli/mod.rs:240-341`）。shim は `render_native_shims.rs:262-292`。
- **Wasm：** `crates/almide-wasm/src/fuel.rs:16-61, 119-195`。

| 項目 | 動作 | 証拠 |
|---|---|---|
| 成功 | 消費 ≤ `floor(ns / 3)` 単位のとき、かつそのときだけ `ok(v)` | テスト（3006 nsで `bounded at ok 496551`、3005 nsで `under`） |
| エラー | `err("fan.bounded: budget exhausted")` | テスト |
| 順序 | 呼び出し元スレッド上で1本体をインライン実行 | — |
| 超過時 | 次の課金箇所で打ち切り、領域までの各包含関数がゼロ値を返す | `fuel.rs:114-118` |
| 副作用 | effect は E006。`println` は許可（ADR-0022）され、打ち切り前の出力を保持。 | テスト `effects_already_performed…`（全経路で9本の `iter` 行） |
| 端のケース | 空本体は native で `ok`、**wasm で E082（#3783）**。課金箇所がない本体は予算ゼロでも必ず完了する。ソースでは領域を入れ子にできない（E007） | プローブ `C/e4*.almd`、`C/n1_nocharge.almd`、`C/n3_nest.almd` |
| 単一呼び出しの引数 | `{ f(g(x)) }` では `g(x)` はメーター開始**前**に実行される（`walker/expressions_fan.rs:167-168`）。未文書化で ADR-0028 Q6 の対象。 | プローブ `C/m1_meter_shape.almd` |
| ターゲット間 | **判定が異なり得る（#3778）**：ループなし wrapper の入口と文字列連結 | プローブ `C/m2_wrap_diverge.almd`、`C/concat_cut.almd`、無視されたテスト |

<a id="7-fantimeoutdurationxn--body-"></a>

## 7. `fan.timeout(duration.X(n)) { body }`

**経路**：
- **Parse：** `parser/fan.rs:163-200`。旧 `fan.timeout(100, f)` は E027（プローブ `C/l1_legacy.almd`）。
- **検査：** `infer_calls_closures.rs:470-496`。型は `Duration` で、他は `bounded` と同じ規則。
- **Native：** `render_native_shims.rs:164-216`、単調な `Instant`。
- **Wasm：** `fuel.rs:65-99` が host op 34、`SystemTime` を読む（#3781）。

| 項目 | 動作 | 証拠 |
|---|---|---|
| 成功／エラー | 課金箇所の検査が `now ≥ deadline` を検出しないとき、かつそのときだけ Ok。それ以外は `err("fan.timeout: deadline exceeded")` | プローブ `C/t1_basic.almd` |
| 検査時点 | 課金箇所（関数入口、ループ先頭、連結）だけ。単一操作の途中では検査しない | `render_native_op_families.rs:50-53` |
| 副作用 | effect は E006（`C/e1_timeout_effect.almd`）。print は保持 | テスト `effects_inside_race…` |
| 再現性 | 約束しない。打ち切り位置はマシンに依存（`C/t2_vary.almd`）。`ALMIDE_OMEGA` replay は **native のみ（#3781）**。 | プローブ `C/t3_replay.almd` |
| 端のケース | `bounded` と同じ：空本体は wasm で E082、課金のない本体は止まらない | プローブ `C/e4b_empty_timeout.almd`、`C/n1_nocharge.almd` |

<a id="8-logical-cost-the-compute-meter"></a>

## 8. 論理コスト（`Compute` メーター）

- **単位。** 1課金単位は3 ns。`CM1_NS_PER_CHARGE`、2026-08-02に再較正、版 v0.3（`crates/almide-types/src/time_units.rs:9-18`）。予算は `units = ns / 3` で変換する。
- **課金箇所：**
  - 関数入口ごとに1；
  - ループ先頭評価ごとに1（n反復のコストは n + 1）；
  - 文字列連結ごとに `1 + len/16`（native 既定ルートのみ、#3778）；
  - codegen メーターで closure hop ごとに1（`crates/almide-mir/src/charge_probe.rs:3-4, 67-103`、`crates/almide-codegen/src/pass_meter_charge.rs:10-19`）。
  - stdlib 本体は課金しない。
  - Wasm と codegen メーターは、ループなし・非再帰のユーザー関数の入口を免除する（`crates/almide-ir/src/meter_plan.rs:5-9, 126-130`）。native 既定ルートは免除しない（#3778）。
- **予算範囲。** `bounded`：本体全体。race ブロック：各分岐に全予算。race mapper：各要素に全予算。領域は包含領域と min-cap するが、ソースでは入れ子にできない。
- **値はコンパイラに依存する。** 消費は inliner と lowering が決める IR/MIR 課金箇所上で定義される。したがってコンパイラ更新で、ソース変更なしに race の勝者や bounded の判定が変わり得る。計測プログラムは、カウントを固定するため、プログラム全体で LICM（`pass_licm.rs:38-43`）、unrolling（`unroll.rs:8-10`）、accumulator TRE（`emit.rs:125-133`）を無効化する。現在、同じビルドの2つの native ルートですでに一致していない（#3778）。

<a id="9-effect-admission"></a>

## 9. Effect の受け入れ

| 形式 | effect 分岐／コールバック | 診断 |
|---|---|---|
| `fan { }`、`fan.settle { }` | 許可 | — |
| `fan.map`、`fan.settle(xs, f)`、`fan.any`、`fan.any { }` | 許可（lambda または裸の effect fn） | — |
| `fan.race { }`、`fan.race(xs, f)` | 拒否 | E006 `cannot call effect function '…' inside a fan.race region` |
| `fan.bounded`、`fan.timeout` | 拒否 | E006、ヒント「fan.bounded は決定的計算を計測するため、本体は PURE」 |
| effect fn 外の全形式 | 拒否 | E007（ブロック）。`fan.map` は**コードなし**のエラー（`static_dispatch.rs:176-180`） |
| キャプチャした `var` | 拒否 | E008 |

証拠：テスト `effects_inside_race_bounded_and_timeout_are_refused`、テスト `fan_outside_an_effect_fn_and_a_captured_var_are_refused`、プローブ `C/a*.almd`。

<a id="10-discrepancies"></a>

## 10. 不一致

項目は3種類ある。

- **ここで修正**：コメントや文書が誤ったことを述べており、この PR が動作変更なしで訂正する。
- **Issue**：実装が約束に違反している。issue は登録済みで、ここでは動作を変えない。
- **ADR**：まだ約束がなく、問いは ADR-0028 にある。

| # | 場所 | 以前の説明 | 実際の実行 | 扱い |
|---|---|---|---|---|
| 1 | `runtime/rs/src/fan.rs:8-10, 123-147` | race は最初に settle した thunk を返す。「並列 + 壁時計非決定的」。race/settle は `thread::scope` を保持 | frontend の辞書式最小 `(spend, index)` fold。`almide_rt_fan_race` / `almide_rt_fan_settle` は到達不能 | ここで修正（コメント） |
| 2 | `crates/almide-mir/src/lower/desugar_fan.rs:1-6` | 「`fan.race([…]) ≡ t0`」。any は最後の thunk の結果へ fallback | race head は到達不能、any は定数 Err を返す | ここで修正 |
| 3 | `static_dispatch.rs:146-147, 189-197, 205-209, 331-333` | 「削除された race」、「auto-`?` を伝播」、any mapper「未実装」 | tombstone なのは race *呼び出し*だけ。auto-? は E041、any mapper は稼働中 | ここで修正 |
| 4 | `runtime/rs/src/fan.rs:41-45`、`pass_rust_lowering_fan.rs:164-165` | 「String は意図的に拒否」、Rc 型は逐次に留まる | String/List/Set/Map は Send-safe で並列（`:185-186`） | ここで修正 |
| 5 | `docs/SPEC.md` §fan、`docs/specs/effect-system.md:313-317`、C-004 | `fan.race` は0.42.0で削除、thunk[0]だけ実行；`fan.any(thunks)` | race head と mapper は稼働、thunk リストは E027 | ここで修正（SPEC、effect-system）。C-004文言 → 先に als（下記） |
| 6 | SPEC、ADR-0001:111 | race 分岐間に `;` | `;` は parse error | ここで修正（SPEC。ADR-0001は決定記録なのでそのまま） |
| 7 | SPEC §fan.bounded | 「単一関数呼び出し必須」、「入れ子予算は min-cap」、`fan.timeout` は「将来」 | 完全なブロック本体、ソースで入れ子不可、timeout は存在して pure | ここで修正 |
| 8 | ADR-0024 Status | 「まだ何も実装していない」 | D1、D5、D6は実装済み。D3/D4/step 4は未実装 | ここで修正（状態行） |
| 9 | ADR-0024 D7 | `ALMIDE_FAN_SEQUENTIAL=1` が ablation スイッチ | par twins のみ、ブロックには無効 | ここで修正（D7の下の注記） |
| 10 | `stdlib/fan_any.almd:3-4` | 後続要素は評価しない | wasm fs-read prefetch は全読み取りを開始 | ここで修正 |
| 11 | `crates/almide-interp/src/eval.rs:146-153` | Err 分岐で短絡 | 全分岐を評価してから最初の Err（読んだだけ、未実行） | ここで修正 |
| 12 | `src/cli_args.rs:41-44` | `--target wasm` は `wasmtime` CLI で実行 | 組み込みホストで実行 | ここで修正 |
| 13 | CHEATSHEET:691、696 | `fan.settle` に「scalar 上」、「分岐は並列の兄弟」 | String/Listも対応、settleは逐次 | ここで修正 |
| 14 | `lower/expressions.rs:270-271`、`expressions_fan.rs:479-480`、`ast.rs:263-268`、`infer_calls_closures.rs:326-330, 361` | 「while-scan」、「全分岐は単一 pure 呼び出し」、bounded 本体は「単一呼び出し」 | `ForIn`、式の分岐とブロック本体を受け入れる | ここで修正 |
| 15 | CHEATSHEET:728、C-204、C-205、C-294 | 全ターゲットで同じ勝者／判定 | native 既定メーター ≠ wasm メーター | Issue #3778 |
| 16 | C-200、`list.rs:1141-1142` | trap は前の要素を待つ | native `fan { list.map }` は時間上最初の trap を選ぶ | Issue #3779 |
| 17 | ADR-0024 D3、C-004 | ordered op に到達する fan は逐次実行 | ordered-op 解析なし、native の `env.set` は競合 | Issue #3780 |
| 18 | C-208、cli.md「どこでも replay」 | 全ターゲットで ω replay | native のみ、テストは空虚 | Issue #3781 |
| 19 | CHEATSHEET:705-708、SPEC §13.5 | race は `T` に対してジェネリック | Float/List の勝者はコンパイル不可 | Issue #3782 |
| 20 | — | （約束なし） | 空の bounded/timeout 本体：native ok、wasm E082 | Issue #3783 |
| 21 | — | （約束なし） | 単一呼び出し引数は meter / deadline の外で実行 | ADR-0028 E、#3790で修正 |
| 22 | ADR-0024 D4 | fan が逐次実行時に注記 | 注記を出さない | ADR-0028テスト計画（診断） |
| 23 | C-200 / ADR-0024 D6 | trap 後、基盤は「要素開始を止める」 | 開始は止まらず、出力だけを制限 | ADR-0028テスト計画 |
| 24 | 診断 | — | clock 型不一致と「通常の値」エラーにコードなし。1つの E006 が E001/E041 に連鎖。`fan.settle(xs, f)` の非 Result E001 は文言が崩れている | ADR-0028 §Diagnostics |
| 25 | `docs/specs/cli.md`、C-006、C-214 | 「fan.timeout は存在しない／削除されたまま」 | `fan.timeout(d) { }` head は存在 | C-006 / C-214文言 → 先に als |

行5、25などの契約文修正は既存契約の*記述*を変更する。`scripts/check-als-pin.sh` は judge ledger と記述を比較するため、編集は先に almide/als を通す。ここでは列挙するだけで、この PR ではいずれも変更しない。
