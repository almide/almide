# ALS — 決定的時間（deterministic time）

> Last updated: 2026-08-27

正規化元: [ADR-0001](../../adr/0001-deterministic-time-units.md)（S1–S8）、
[SPEC.md §13](../../SPEC.md)。決定的時間は「charge unit × CM-1（versioned 校正定数）」
で測る論理時計であり、壁時計ではない。本章の全促約は native == wasm の観測一致
（stdout / stderr / exit code）を含む。

## ALS-DT1 時間構築子と代数

時間量は閉じた単位集合（`ns / us / ms / s / min / h`）のモジュール修飾構築子
（`compute.*` = 決定的計算時間、`duration.*` = 壁時計）でのみ作られ、裸 `Int` は
型エラー。負の構築子引数・負のスケール係数は §13 終了規約の決定的 abort
（`Error: negative time...` + exit 1）、オーバーフロー構築は最大値へ飽和する。
代数は `T + T`（飽和）/ `T - T`（0 飽和）/ `T × Int`（飽和・負係数 abort）/
同型比較のみ。`T × T`・時計混合・`/` は型エラー。
Fixtures: `spec/wasm_cross/time_negative_trap.almd`, `time_negative_scale.almd`,
`time_saturate.almd`, `time_ops_algebra.almd`。

## ALS-DT2 決定的予算（fan.bounded）

`fan.bounded(c) { body }` の判定はプログラムと入力のみの関数である: 消費 charge
unit が予算 unit（`ns / CM-1`、切り捨て）を超えたときのみ Err（台帳定数メッセージ
`fan.bounded: budget exhausted`）。同じプログラムはどのターゲット・どのホストでも
同じ宣言ナノ秒で Ok ⇄ Err が反転する（unit 境界厳密）。入れ子は min-cap
（EIP-150 式 — 現行リリースは region の入れ子自体を E007 で拒否するため、
この規則の executable 証拠は参照評価器の実装のみである）。bind 文は
charge 0。

**charge 表（CM-1 v0.3 = 3ns/unit、C-207）**: charge site は (a) fn entry —
1 unit、ただしループ無し・非再帰の callee は共有 MIR インライナで消える
（entry charge ごと 0 unit、C-294）、(b) ループ head 判定 — 1 評価につき
1 unit（`while` は最終の偽判定も数える: n 周 = n+1 unit）、(c) バルク文字列
連結 — `1 + 結果バイト長/16` unit（結果キー、C-204 の T3-5）。bind 文・
直線式・分岐は 0。判定は **check-then-charge**: site ごとに
`spent + cost > budget` なら cut（最初の charge を踏む前に完了する式は
0 予算でも成功する）。`heavy(1000)`（entry 1 + loop head 1001）= 1002 unit
= 3006ns が校正例で、`spec/wasm_cross/fuel_bounded_boundary.almd` と
`fuel_dyn_charge.almd`（252 unit、750/760ns で反転）が unit 厳密に固定する。

**cut の簿記（2026-08-25 裁定、C-320）**: 予算超過による cut は、通常の脱出と
同じメーター簿記（region exit）を実行してから脱出する。ゆえに —

1. 充填（exhausted）した region の判定は**必ず Err**。途中値やゼロ値の Ok
   （stale verdict）は不適合。
2. region の判定と値は、同一プログラム内の**他の region に依存しない**
   （メーター状態は region 間に漏れない — 先行 region の cut が後続 region の
   判定を変えることは観測されない）。
3. cut が **arm 直下のループ**で発火するか **callee 内**で発火するかは観測
   不能（同じ宣言ナノ秒で同じ判定）。arm の値は body ブロックの値であり、
   ループ形（`for` / `while`）に依存しない。
4. メーターが課金するのは予算対象 body の計算のみ。観測を実体化するランタイム
   機構（文字列整形・`??` fallback 経路）は課金も cut もされず、Err 後の
   fallback と後続出力は常に健全。

Fixtures: `spec/wasm_cross/fuel_bounded_boundary.almd`, `fuel_block_body.almd`,
`fuel_bare_result.almd`; cut 簿記は `spec/wasm_cross/fuel_cut_in_arm_loop.almd`
（0.58.0 は規則 1〜2 に両ターゲット同一に違反する — 直下ループの cut が stale Ok(0) になり、以後の全 region が汚染される。合意判定には映らない種で、
参照レグが値を固定するまで agreement で運ぶ。almide/almide#1572）。

## ALS-DT3 決定的 race（fan.race）

勝者は「予算内で完走し、かつ自身が Err を返さなかった arm」のうち
`(spend, index)` 辞書式最小 — 最安 arm、同点は原文順。Result arm の Err は
その arm を候補から外す（伝播しない）。全滅は台帳定数 Err。
Fixtures: `spec/wasm_cross/fuel_race_boundary.almd`, `fuel_race_err_skip.almd`,
`fuel_bare_result.almd`。

## ALS-DT4 settle の tuple 契約

`fan.settle { a; b; … }` の値は arm 順の tuple `(Result[A, String], …)` である:
異型 arm 可、素の arm は Ok に包まれ、効果 arm の Err はその slot に捕捉される
（伝播しない）。評価は arm 順で決定的。
Fixtures: `spec/wasm_cross/fan_settle_tuple.almd`。

## ALS-DT5 壁時計期限（fan.timeout、oracle 層）

`fan.timeout(duration.ms(n)) { body }` は壁時計期限を **charge site で協調
チェック**する（中断点統一原理 — 操作の途中では決して切らない、Go の context と
同じ協調モデル）。判定は ω 相対（R_Ω）: どのチェックで期限が切れたかは host の
入力であり、**ω を固定した replay は観測を byte 一致で再現する**（native で採録
した ω を wasm で replay しても定義から成立）。決定的に主張できるのは両端のみ —
十分大きい期限は常に完走し、発散 body + 微小期限は常に charge site で切れる。
Fixtures: `spec/wasm_cross/fuel_timeout_ends.almd`（両端）+ record/replay gate
（`tests/charge_probe_test.rs::timeout_deterministic_ends_and_replay`）。
