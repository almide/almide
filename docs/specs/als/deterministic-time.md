# ALS — 決定的時間（deterministic time）

正規化元: [ADR-0001](../../adr/0001-deterministic-time-units.md)（S1–S8）、
[SPEC.md §13](../../SPEC.md)。決定的時間は「charge unit × CM-1（versioned 校正定数）」
で測る論理時計であり、壁時計ではない。本章の全促約は native == wasm の観測一致
（stdout / stderr / exit code）を含む。

## ALS-D1 時間構築子と代数

時間量は閉じた単位集合（`ns / us / ms / s / min / h`）のモジュール修飾構築子
（`compute.*` = 決定的計算時間、`duration.*` = 壁時計）でのみ作られ、裸 `Int` は
型エラー。負の構築子引数・負のスケール係数は §13 終了規約の決定的 abort
（`Error: negative time...` + exit 1）、オーバーフロー構築は最大値へ飽和する。
代数は `T + T`（飽和）/ `T - T`（0 飽和）/ `T × Int`（飽和・負係数 abort）/
同型比較のみ。`T × T`・時計混合・`/` は型エラー。
Fixtures: `spec/wasm_cross/time_negative_trap.almd`, `time_negative_scale.almd`,
`time_saturate.almd`, `time_ops_algebra.almd`。

## ALS-D2 決定的予算（fan.bounded）

`fan.bounded(c) { body }` の判定はプログラムと入力のみの関数である: 消費 charge
unit が予算 unit（`ns / CM-1`、切り捨て）を超えたときのみ Err（台帳定数メッセージ）。
同じプログラムはどのターゲット・どのホストでも同じ宣言ナノ秒で Ok ⇄ Err が反転する
（unit 境界厳密）。入れ子は min-cap（EIP-150 式）。bind 文は charge 0。
Fixtures: `spec/wasm_cross/fuel_bounded_boundary.almd`, `fuel_block_body.almd`,
`fuel_bare_result.almd`。

## ALS-D3 決定的 race（fan.race）

勝者は「予算内で完走し、かつ自身が Err を返さなかった arm」のうち
`(spend, index)` 辞書式最小 — 最安 arm、同点は原文順。Result arm の Err は
その arm を候補から外す（伝播しない）。全滅は台帳定数 Err。
Fixtures: `spec/wasm_cross/fuel_race_boundary.almd`, `fuel_race_err_skip.almd`,
`fuel_bare_result.almd`。

## ALS-D4 settle の tuple 契約

`fan.settle { a; b; … }` の値は arm 順の tuple `(Result[A, String], …)` である:
異型 arm 可、素の arm は Ok に包まれ、効果 arm の Err はその slot に捕捉される
（伝播しない）。評価は arm 順で決定的。
Fixtures: `spec/wasm_cross/fan_settle_tuple.almd`。

## ALS-D5 壁時計期限（fan.timeout、oracle 層）

`fan.timeout(duration.ms(n)) { body }` は壁時計期限を **charge site で協調
チェック**する（中断点統一原理 — 操作の途中では決して切らない、Go の context と
同じ協調モデル）。判定は ω 相対（R_Ω）: どのチェックで期限が切れたかは host の
入力であり、**ω を固定した replay は観測を byte 一致で再現する**（native で採録
した ω を wasm で replay しても定義から成立）。決定的に主張できるのは両端のみ —
十分大きい期限は常に完走し、発散 body + 微小期限は常に charge site で切れる。
Fixtures: `spec/wasm_cross/fuel_timeout_ends.almd`（両端）+ record/replay gate
（`tests/charge_probe_test.rs::timeout_deterministic_ends_and_replay`）。
