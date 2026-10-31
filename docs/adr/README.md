# Architecture Decision Records

言語設計・アーキテクチャの**決定**と、その**根拠**と、**却下した代替案**を時系列で残す。

## なぜ ADR か

`docs/roadmap/` は「これから何をするか」、`docs/specs/` は「いま何であるか」を書く。
どちらも**なぜそう決めたか**を保存しない。結果として同じ議論が半年後に再演され、
一度却下した案が根拠を失ったまま復活する。ADR はその 3 つ目の軸を持つ。

[REJECTED_PATTERNS.md](../design/REJECTED_PATTERNS.md) は「採用しない機能」の一覧であり、
ADR はその**決定に至る過程**（調査した証拠、比較した代替案、反証条件）を保存する。
却下が REJECTED_PATTERNS に載るときは、根拠として ADR を指す。

## 形式

`NNNN-kebab-case-title.md`。番号は連番、リネームしない（リンクが腐る）。

```markdown
# ADR-NNNN: Title in English

- **Status**: Proposed | Accepted | Superseded by [ADR-MMMM](./MMMM-....md)
- **Date**: YYYY-MM-DD
- **Context**: 何が問題だったか（決定の前に読者が知る必要のある事実）
- **Decision**: 何を決めたか（一文で言い切る）
- **Rationale**: なぜそれか（証拠つき）
- **Alternatives**: 検討して却下した案と、その理由
- **Consequences**: 何が良くなり、何を払うか
- **Falsifier**: 何が起きたらこの決定を撤回するか
- **References**: 一次情報の URL
```

## 規則

1. **Status は嘘をつかない。** 実装が決定と食い違ったら、ADR を改訂するか
   Superseded にする。放置された ADR は SPEC.md §13 の再演になる。
2. **却下した案は消さない。** 却下理由こそが再演を防ぐ資産である。
   後の証拠で却下が誤りと分かったら、新しい ADR で supersede し、
   **なぜ前回の判断が誤ったか**を書く。
3. **Falsifier は必須。** 撤回条件を書けない決定は、決定ではなく好みである。
4. **一次情報を引く。** 「〜と言われている」ではなく、仕様書・公式ドキュメント・
   ソースコードの URL と逐語引用で書く。

## 読み方と後続決定への案内

ADR 本文は決定時点の証拠・却下理由・改訂履歴です。`Accepted` は設計の採択を表し、全項目の実装完了を表しません。本文に残る「未実装」や古い API は、後日の追記・後続 ADR と合わせて読んでください。現行の言語仕様は [specs](../specs/)、公開関数は [標準ライブラリ](../stdlib/README.md) を参照します。

以下は読み違いを防ぐ案内です。ADR 本文を改変せず、未決の設計を新たに採択・撤回するものではありません。

| 記録 | 後続決定・実装状況の読み方 |
|---|---|
| 0001 | `fan.race` の名前・時計引数・計測コストの範囲は [0028](./0028-fan-selection-names-and-cost-contract.md) の採択済み決定を参照。時間単位自体の決定は保持します。 |
| 0002・0003・0012 | 明示的な伝搬は [0008](./0008-explicit-propagation-only.md)、型付きエラーと lambda の合流は [0012](./0012-typed-error-refinement-in-the-marker.md)・[0021](./0021-lambda-failure-channel-is-the-join-of-its-bangs.md)。0003 の厳格な `CustomE → String` 拒否と実装の差は 0021 D5・Falsifier 3 に記録されたままで、この索引では解決しません。 |
| 0004 | `result.context` の記述は D2 の却下と Alternatives 7 を併読してください。現行の公開関数は [result](../stdlib/result.md) で確認できます。 |
| 0012・0021 | 冒頭の実装前の説明だけで未着手と判断しないでください。型付きマーカーと callback の伝搬は [typed_error_callback_channel_test.almd](../../spec/lang/typed_error_callback_channel_test.almd) などで検証されています。 |
| 0020・0023 | 0020 §5.3 の export 形は [0023](./0023-wasm-http-over-wasi-http.md) が置き換えます。本文の後日の実装注記と [Wasm build 経路](../../src/cli/build_wasm.rs) を併読してください。serve export の実装を、HTTP client の全経路や host-op 廃止の完了と同一視しないでください。 |
| 0025・0026 | 0025 §2.4 の `process` / `Env` 分類は 0026 D2 の後日の実装注記と [effect category 定義](../../crates/almide-ir/src/effect.rs) を参照。0026 の冒頭の「fixpoint 以外未実装」は後日の追記より前の状態です。 |
| 0026 の外部関数境界への参照 | 最終追記の「ADR-0027 §5」は参照先未解決です。[0027](./0027-effect-functions-are-inspected-in-a-fixed-host-world.md) は検査ハーネスの決定であり、外部関数境界の根拠には使えません。代わりの規範をこの索引で推定しません。 |
| 0029 | Status と Open の 2026-10-18 の裁定を参照。冒頭に残る「questions … still need a ruling」は裁定前の説明です。却下された test-level 属性案は、別の現行 ADR ではなく本文の却下案として残っています。 |

番号 0013 は異なる 2 記録に使われています。既存リンクを保つためリネームせず、下の索引ではタイトルで区別します。

## Index

| # | Title | Status | Date |
|---|---|---|---|
| [0001](./0001-deterministic-time-units.md) | Deterministic budgets are written in time units | Accepted | 2026-08-01 |
| [0002](./0002-fallibility-effect-orthogonal.md) | Fallibility and effect are orthogonal axes; `-> T!` marks pure-fallible | Accepted (amended 2026-09-24: effect implies fallible, §D6) | 2026-08-05 |
| [0003](./0003-error-type-conversion-at-propagation.md) | Error-type conversion at propagation points — lossy conversion must be spelled | Accepted | 2026-08-05 |
| [0004](./0004-error-branchability-doctrine.md) | Error branchability — String stays terminal; doctrine, lint, and context instead of tags | Accepted | 2026-08-05 |
| [0005](./0005-operators-desugar-to-stdlib.md) | Value-level operators are desugarings of named stdlib functions | Accepted | 2026-08-05 |
| [0006](./0006-fallibility-polymorphic-hofs.md) | One-bit fallibility polymorphism for HOFs; the try_* family is dissolved | Accepted (amended 2026-09-27: E flows from the callback, D4 withdrawn — ADR-0021) | 2026-08-05 |
| [0007](./0007-partition-replaces-collect.md) | result.collect is removed; partition is the substance for all-errors collection | Accepted | 2026-08-05 |
| [0008](./0008-explicit-propagation-only.md) | Propagation is always explicit — auto-? abolished, `!` only, Result is must-use | Accepted | 2026-08-05 |
| [0009](./0009-fn-type-quadrant-transparency.md) | Function types complete the capability quadrant; fn params are transparent by default | Accepted | 2026-08-06 |
| [0010](./0010-option-type-shorthand.md) | T? is general Option shorthand — every type position, atom-tight binding, fmt-normalized | Accepted | 2026-08-06 |
| [0011](./0011-execution-substrate-is-a-free-variable.md) | The execution substrate is a free variable — and one arm's output is the last hole in the determinism claim | Proposed (amended 2026-10-04: wasm compute parallelism is per instance, memory never shared, §D2a) | 2026-08-07 |
| [0012](./0012-typed-error-refinement-in-the-marker.md) | Error-surface end state — refinement stays in the marker (`T!E`), erasure stays the default | Accepted (D4-4 replaced by ADR-0021) | 2026-08-11 |
| [0013](./0013-corpus-stays-id-keyed.md) | The conformance corpus stays contract-ID-keyed; semantic reorganization rejected, naming discipline gated | Accepted | 2026-08-20 |
| [0013 (String)](./0013-string-shared-immutable-buffer.md) | String becomes a shared immutable buffer; mutating operations use copy-on-write | Accepted | 2026-09-04 |
| [0014](./0014-expectations-are-derived.md) | Pinned expectations are machine-derived or absent — never hand-maintained | Accepted | 2026-08-20 |
| [0015](./0015-reference-evaluator-is-fresh-source-level-python.md) | The reference evaluator is a fresh, source-level, judge-owned evaluator behind a black-box protocol — seeded by λ_almd, ratcheted by an abstain ledger (amended: Rust) | Accepted | 2026-08-21 |
| [0016](./0016-numeric-determinism-is-normative.md) | Numeric determinism is one normative family, written once — not a property the targets happen to share | Accepted | 2026-08-21 |
| [0017](./0017-flight-profile-is-a-normative-subset.md) | The bounded profile (née flight profile) is a normative subset of the language, specified here before any checker enforces it (amended at adjudication: `@bounded`, chapter `bounded.md`, prefix `B`) | Accepted | 2026-08-21 |
| [0019](./0019-suppression-is-announced-dated-reasoned-acknowledged.md) | Suppression is announced, dated, reasoned and acknowledged — its shape is fixed before the construct exists | Accepted | 2026-09-22 |
| [0020](./0020-http-serving-and-handler-state.md) | A served handler cannot reach a `var`; state lives in a host-independent KV store; the app is a value and every host is a thin shell | Accepted (§5.3 export shape superseded by ADR-0023) | 2026-09-27 |
| [0021](./0021-lambda-failure-channel-is-the-join-of-its-bangs.md) | A lambda's failure channel carries the error type its `!`s agree on — contextual type first, then the join, String as the top | Accepted | 2026-09-27 |
| [0022](./0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) | The output and abort builtins are admissible in a pure fn — writes and aborts, never reads, and they do not make the caller effectful | Accepted | 2026-09-27 |
| [0023](./0023-wasm-http-over-wasi-http.md) | Wasm HTTP is `wasi:http@0.3` — a program that uses HTTP builds as a p3 component, the embedded host serves the same imports, and no host serves a custom HTTP op | Accepted | 2026-09-28 |
| [0024](./0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) | `fan` runs effect callbacks concurrently on every target, as if sequential — `fan.map` runs every element, the compiler picks the substrate, and no concurrency concept reaches the writer | Accepted | 2026-09-30 |
| [0025](./0025-wasm-process-is-a-private-host-capability.md) | Wasm `process` is a private host capability — `almide:process/spawn`, shipped only when the op set names it, refused at load by a stock host, bounded by `[permissions] proc` | Accepted | 2026-10-03 |
| [0026](./0026-effect-categories-ride-the-transparent-fn-type.md) | Effect categories ride the transparent fn type — sets flow through callbacks unannotated, a two-level hierarchy, and inferred ⊆ manifest ⊆ host is one chain | Accepted (D1–D4) | 2026-10-03 |
| [0027](./0027-effect-functions-are-inspected-in-a-fixed-host-world.md) | Effect functions are inspected in a fixed host world — a seeded sandbox, property verdicts for what the host decides, and loopback fixtures for the network | Accepted (D1–D8, 2026-10-17) | 2026-10-17 |
| [0028](./0028-fan-selection-names-and-cost-contract.md) | Replace `fan.any` with `??` / `list.find_map`; rename `fan.race` to `fan.min` with a required clock; scope cost to a meter version | Accepted | 2026-10-18 |
| [0029](./0029-assert-aborts-runs-its-closure-in-a-child.md) | `testing.assert_aborts(expected, closure)` runs only its closure, in a child process or a fresh instance — the closure captures nothing, every abort counts, and `testing.assert_throws` leaves at dialect epoch 14 | Accepted (2026-10-18) | 2026-10-17 |
| [0030](./0030-compile-cost-is-a-contract.md) | Compile cost is a contract — every phase has a counted budget, the source cannot raise it, and exceeding it is a defined diagnostic | Proposed | 2026-10-24 |
| [0031](./0031-raw-pointers-carry-provenance.md) | A pointer the program can read through carries its provenance — `RawPtr` splits into an opaque `Handle` and a bounds-checked `Region`, and a foreign pointer is a `Region` only where its `@extern` states the length | Accepted | 2026-10-27 |
