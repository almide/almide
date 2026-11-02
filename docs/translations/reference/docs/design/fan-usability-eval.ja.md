<a id="fan-usability-evaluation-can-a-writer-pick-the-right-fan-and-does-it-misread-cancellation"></a>

# Fan の使いやすさ評価：書き手は適切な fan を選べるか、cancel を誤解しないか

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-usability-eval.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](fan-usability-eval.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-usability-eval.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-usability-eval.md)
> これは評価設計であり、完了した結果ではない。冒頭は説明文を読まないとする一方、misconception score は phrase list でコメント／説明を明示的に調べる。両原文記述を保持する。過去の fan 名と条件を現行 API の推奨に書き換えない。

> 最終更新：2026-10-17 · 設計のみ。全 MSR 作業と同様、評価は [almide-dojo](https://github.com/almide/almide-dojo) で行う（CLAUDE.md「Repo Boundary」）。結果により [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) の反証条件を判定する。

<a id="what-is-measured"></a>

## 測ること

評価は二つの問いに答える。

1. **選択。** 平易な言葉でタスクを与えられた書き手（モデルまたは人）は、選択の種類が合う fan 形式を選ぶか。種類は [fan-cross-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md) の K1–K5。
2. **Cancellation。** 実際にはそうでないのに、何かが停止または取り消されると書き手が考えるか。例：`fan.map` が最初の Err で停止する、`fan.any` が送信済み要求を cancel する、`fan.race` が敗者を早期停止する、という理解。

両方を oracle が判定する*書かれたプログラムの挙動*として測り、書き手の生成した説明文を読むことで測ることはない。

<a id="task-bank"></a>

## タスク集合

各タスクは短い依頼文、starter file、hidden oracle からなる。依頼文には fan 形式の名前を一切出さない。6 family、各 6 タスク（合計 36）、半分が「記述」、半分が「変更」（MSR）。

| Family | 依頼文の形（例） | 正答 | 典型的な誤答 |
|---|---|---|---|
| F1 K1 | 「優先順に mirror を試し、最初に応答するものを使う。」 | `fan.any(mirrors, f)` | `fan.race`（時間的に最初と期待する） |
| F2 K4 | 「複数 solver は純粋である。仕事量が最小のものを選び、どの機械でも同じ答えにする。」 | `fan.race { … }` | `fan.any`（最小費用ではなく先頭を取る） |
| F3 K5 全件またはエラー | 「全ページを取得し、どれかが失敗すれば失敗とし、どれか報告する。」 | `fan.map(xs, f)!` | 全ページの試行を依頼しているのに早期停止する `for` loop |
| F4 K5 全結果 | 「全検査を実行し、それぞれの結果を載せた報告を出力する。」 | `fan.settle(xs, f)` | `fan.map`（要素ごとの結果を失う） |
| F5 表現不能 | 「最初に応答する server を使い、遅いものを無視する。」（K2）、または「どの job でも終了次第、失敗でも返す。」（K3） | 表現不能と伝え、trade-off を明示して `fan.any` を使う | I/O arm を持つ `fan.race`（E006）、または遅い call が cancel されるとの主張 |
| F6 cancellation | 「一つ失敗したら残る upload を止める。」 | `for … !` loop。`fan.map` は決して早期停止しないため | `fan.map` と、後続 upload が起きなかったという主張・コメント・テスト |

変更タスクは、その family の正しい program から始め、要件を一つ変える。例：「エラーは最初に失敗したページではなく、リスト内で最初のページにする」（すでに成り立つので変更不要）、または「失敗した結果も保持する」（`fan.map` → `fan.settle`）。

<a id="oracle"></a>

## Oracle

全タスクの oracle は、`tests/fan_acceptance_test.rs` 形式の決定的な program である。

- **全 leg で同じ出力。** 提出物を native、`ALMIDE_FAN_COST_OFF=1` 付き native、`ALMIDE_FAN_SEQUENTIAL=1` 付き native、wasm の 4 leg で実行し、stdout が一致しなければならない。
- **期待出力。** stdout は依頼文の期待出力と一致しなければならない。
- **遅い要素と速い要素。** oracle の helper fn は一部の要素を遅く、一部を速くし、完了順に依存する回答を失敗させる。期待出力は wall time に決して依存しない。
- **副作用を記録。** 各 effect は tag 付き `println` なので、oracle が実行された要素を判別できる（F1、F6）。

タスクごとに二つのスコアがある。

- **選択が正しい。** program が両 target で compile され、oracle の stdout が一致する。
- **誤解。** program、そのコメント、または書き手の説明が、次のいずれかを主張した場合の指摘：oracle 上は実行されたのに未開始または停止したという要素、cancel された要求、K2/K3 の結果。Dojo の既存 malicious-hint detector はすでにコメントを読み、これを固定 phrase list（`cancel`、`abort the others`、`whichever finishes first`、`stops the rest`）で再利用する。

<a id="conditions"></a>

## 条件

| 条件 | 書き手に与える context |
|---|---|
| C0 | 現在の CHEATSHEET fan 節をそのまま |
| C1 | C0 と ADR-0028 の D-a hint 文、D-c gloss |
| C2 | 受理された ADR-0028：順序内の最初の Ok に `??` / `list.find_map`、clock 必須の `fan.min(clock)`、D-a/D-c 付き |

書き手は当日の Dojo model roster とする。各（task、condition、model）で 5 sample を実行する。各結果行に model、sample 数、日付を記録する。

<a id="procedure"></a>

## 手順

1. Dojo の `tasks/fan/` 以下で、task bank と oracle を版付きで凍結する。
2. 条件ごとに全 task × 全 model × 5 sample を実行する。CI 外で、operator の機械にある API key を使って行う（Dojo rule）。
3. 各 run を selection と misconception で採点する。family ごと、condition ごとに報告し、各比率に総数（「何件中か」）を添える。
4. C0 → C1 を比較し、診断と gloss だけの効果を調べ、C1 → C2 で名前変更による上乗せ効果を調べる。
5. ADR-0028 の反証条件を適用する。C2 が F1 + F2 + F5 の誤読率を C1 比で少なくとも 3 分の 1 減らさない場合、removal epoch 前に名称維持（option A）を再検討する。

<a id="what-this-evaluation-does-not-claim"></a>

## この評価が主張しないこと

- 書かれた program を測り、理解そのものは測らない。
- latency は測らない。遅くても正しい F1 回答は合格する。
- condition ごとの採点済み run が 30 未満の family は報告するが、反証条件には使わない。
