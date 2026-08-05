# Architecture Decision Records

言語設計・アーキテクチャの**決定**と、その**根拠**と、**却下した代替案**を時系列で残す。

## なぜ ADR か

`docs/roadmap/` は「これから何をするか」、`docs/specs/` は「いま何であるか」を書く。
どちらも**なぜそう決めたか**を保存しない。結果として同じ議論が半年後に再演され、
一度却下した案が根拠を失ったまま復活する。ADR はその 3 つ目の軸を持つ。

[REJECTED_PATTERNS.md](../REJECTED_PATTERNS.md) は「採用しない機能」の一覧であり、
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

## Index

| # | Title | Status | Date |
|---|---|---|---|
| [0001](./0001-deterministic-time-units.md) | Deterministic budgets are written in time units | Accepted | 2026-08-01 |
| [0002](./0002-fallibility-effect-orthogonal.md) | Fallibility and effect are orthogonal axes; `-> T!` marks pure-fallible | Accepted | 2026-08-05 |
| [0003](./0003-error-type-conversion-at-propagation.md) | Error-type conversion at propagation points — lossy conversion must be spelled | Accepted | 2026-08-05 |
