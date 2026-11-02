<a id="c-006--fantimeout-was-removed-0290"></a>

# C-006 — fan.timeout の削除（0.29.0）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](C-006-fan-timeout-removed.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](C-006-fan-timeout-removed.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](C-006-fan-timeout-removed.zh-CN.md)

> 非規範の原文固定読者スナップショット。変更していない原本が正本。自動同期・独立実行・再測定・契約保証の新たな検証はしていない。現行／履歴の記述は固定原文時点に属する。C-006 は旧 timeout 形式の削除と後の協調的ブロック形式を区別する。C-033 は修正前診断と解決指示を履歴として保持し、旧削除・ゲート変更指示を実行しない。repr ヘルパー／export の詳細は過去の emitter を説明するもので現行公開 ABI ではない。コードとコメントは原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-006-fan-timeout-removed.md)

> [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) の契約 C-006 を補足する説明文。
> 状態：**active、0.47.0 で一部置き換え済み** — C-006 は0.29.0での旧 `fan.timeout(ms, thunk)` 形式の削除を記録する。現在の `fan.timeout(duration) { body }` 形式は C-208 の協調的な oracle-tier deadline である。[現行 fan 仕様](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#136-fantimeout--the-oracle-tier-wall-clock-deadline)を参照。
> 以下の節は過去の削除を説明するもので、現在の言語に `fan.timeout` がないという意味ではない。

<a id="history-why-it-was-the-sole-exemption-0240--028x"></a>

## 履歴：唯一の例外だった理由（0.24.0 – 0.28.x）

`fan.timeout(ms, thunk)` は**壁時計時間の effect** だった。2つのターゲットでは時間の概念が根本的に異なっていた。

| ターゲット | 仕組み | 結果 |
|--------|-----------|--------|
| native | 実際のスレッド + `recv_timeout(ms)` | timeout が発火し得て、遅い thunk は放棄された |
| wasm | 時計、スケジューラ、スレッドがない | thunk が完了まで走り、timeout は一度も経過しなかった |

wasm サンドボックスで「壁時計時間の経過」に移植可能な意味はないため、`fan.timeout` はターゲット間等価性保証から除外され、wasm 出力時には目立つビルド時警告が出た。除外された `fan.*` 操作はこれだけだった。`fan.race` / `fan.any` / `fan.map` / `fan.settle` はすべてリスト順序により決定的にされ（C-004 / C-005を参照）、等価性の約束を満たす。

<a id="why-removal-not-portable-semantics-0290"></a>

## 移植可能な意味論ではなく削除を選んだ理由（0.29.0）

ターゲット間の差より深刻なのは、deadline が発火するかがマシン負荷に依存し、**ネイティブの2回の実行同士でさえ**、結果がプログラムと入力の関数ではなかったことだ。この性質に反する唯一の標準ライブラリ API だった。「移植可能な」意味論（協調的 deadline 検査）を新設しても、1関数を救うために新たな意味論を加えながら非決定性は残る。組織全体の調査で利用箇所がゼロと分かったため、操作そのものを削除した。

- 型チェッカーは `fan.timeout` を、一般的な未知メンバーエラーではなく、移行ヒント付きの専用 tombstone（E027）で拒否する。
- deadline はプログラムを呼び出す**ホスト境界**に属する（`timeout 5 ./app`、wasmtime ホストの deadline、supervisor）。
- ネイティブランタイム（`almide_rt_fan_timeout`）、wasm emit 分岐、v1 MIR desugar、CLI のビルド時警告はすべて同じ変更で削除された。

<a id="old-form-diagnostic-evidence"></a>

## 旧形式の診断証拠

- `tests/diagnostics/e027-fan-timeout-removed/` — broken/fixed の組。旧形式の壊れた呼び出しは、移行ヒント `fan.timeout(duration.ms(5000)) { work(x) }` を伴う `error[E027]`（`fan.timeout changed signature`）を出す。fixed ファイルはこの現行ブロック形式を使い、問題なくコンパイルされなければならない。
- `tests/diagnostic_harness_test.rs::broken_files_produce_expected_diagnostics` — 全診断ケースを実行するハーネス。

型検査はターゲット分岐に先行するため、旧形式の tombstone はターゲットに依存しない。現在のブロック形式 timeout の動作とターゲット間の限界は、この削除記録ではなく C-208 が別に扱う。
