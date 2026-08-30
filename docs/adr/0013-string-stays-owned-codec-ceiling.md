# ADR-0013: `String` stays owned — the codec ceiling is recorded, not chased
- **Status**: Proposed（○× 待ち。○ = 本 ADR を Accepted にして上限を台帳へ、× = Alternative A を採用する ADR に書き直す）
- **Date**: 2026-08-30
- **決定範囲**: Almide の `String` 値の**表現**（所有 `String` のまま / 共有 `Rc<str>` 系へ）と、
  それが決める Codec デコードの性能上限。`Value` の表現は含まない（#1679 で実測済み、後述）。
- **関連**: #1673 / #1678 / #1679（着地 6 PR: #1680, #1681, #1682, #1683, #1686, #1687）、
  `docs/design/MEMORY-SAFETY.md`（RcCow の設計）、`docs/STABILITY.md`（凍結面）、
  ADR-0004（stdlib の String 終着）。

## Context

「上流のスキーマ検証ライブラリ（`../almide-references` 配下、v4.5.2）に負けない」を目標に、
同一ワークロード（8 フィールド + ネスト record + `List[String]` の record を 100 万回、arm64、ns/op）で
詰められる機構を全部詰めた結果:

| | v0.59.2 | develop (2026-08-30) | 上流 |
|---|---:|---:|---:|
| `for v in data { User.decode(v) }` | 1693 | **205** | `safeParse` ~100 |
| `json.parse`（同一文書） | 948 | **515** | `JSON.parse` ~500 |
| parse + decode（end to end） | 1576 | **735** | ~600 |

parse は V8 と並び、decode は 8.3 倍縮んだが、**decode 単体で 2 倍、e2e で 1.2 倍**の差が残る。

残差の内訳は一つに絞れている。`User.decode` の 205 ns のうち約 160 ns は record の
`String` フィールド 8 本（`name`, `email`, `tags[3]`, `address.{city,zip,country}`）の**確保**で、
上流はこれを払わない — JS の文字列は共有され、`safeParse` は入力オブジェクトの文字列を
そのまま指す。同じ形を Rust で手書きして測ると、フィールドを `&str` で借りる record は 53 ns、
所有 `String` の record は 168 ns（#1679 本文の表）。

`Value` 側の共有化（`Str(Arc<str>)` + キー intern の HashSet）は試作して**遅くなることを実測**
（parse +6%、decode +20%）— 原子カウントとハッシュ探索が、この allocator の ~20 ns の短文字列確保より高い。
効いたのはキーだけの `&'static str` intern（#1687、parse −18%）で、値の共有化は効かない。
したがって残差は runtime ではなく **`String` 型の表現**の問題である。

## Decision

**`String` は所有のまま。** Codec デコードの上限（所有 `String` の確保コスト ≒ フィールド 1 本 ~20 ns）を
`docs/project/BENCHMARKS.md` に「設計上の上限」として記録し、上流との差を追わない。

## Rationale

1. **値意味論と局所性は使命側の選択。** `String` が共有参照になると、`var s = t; s = s + "x"` の
   RcCow 分岐、`mut` 引数の書き戻し規約（C-132）、alias_safety の COW 判定が **String にも**適用対象に
   なる。今それらは List/Map/Record に閉じている。LLM が書くコードの予測可能性（MSR）に効く面積を、
   1.2 倍の e2e 差のために広げる理由が今の証拠には無い。
2. **wasm レッグは独立に String を持つ。** 表現を変えると byte-identity の証明対象が増え、
   `proven-vs-trusted` の境界が動く。現状の 320+ 契約は表現に依存していない。
3. **差は上限として説明可能。** 「decode ≒ 100 ns + 20 ns × String フィールド数」は文書化できる線形則で、
   隠れた劣化ではない。上流の 100 ns は「文字列をコピーしない検証」の値であり、
   **型付き record を構築する**デコードと同じ仕事ではない。

## Alternatives

- **A. 共有 `String`（`Rc<str>` / RcCow 文字列）。** decode を ~50 ns に落とせる（手書き計測 53 ns）。
  却下理由: 上記 1–2。採るなら、fan 越境のため `Arc` が必須（#1679 試作の教訓）で、
  原子カウントが**全ての String 操作**に乗る — 短文字列 `+` を多用する既存ベンチ（strchurn）が
  悪化する可能性が高い。○ の場合はこの ADR を書き直し、strchurn / fasta の ratchet を先に測る。
- **B. デコード側の借用 record（`UserRef<'a>`）。** 言語に参照型が無いので表現不能。却下。
- **C. `Value` の共有化。** 実測で遅い（#1679）。却下。
- **D. SSO（短文字列の inline 化）。** 確保は消えるがコピーは残り、`String` の ABI が変わる。
  runtime/rs と生成 Rust の両方に波及し、wasm レッグと表現が乖離する。本 ADR では検討のみ。

## Consequences

- 良くなる: 「どこで負けているか」が線形則として台帳に載り、以後の性能議論が表現論に戻らない。
- 払う: 上流比 decode 2 倍・e2e 1.2 倍を**上限として受け入れる**。
- 変わらない: 表現・契約・凍結面。追加作業は BENCHMARKS.md の 1 段落のみ。

## Falsifier

次のどれかが起きたら撤回し、Alternative A の ADR を起こす:

- `String` フィールド 1 本あたりの確保コストが 20 ns を**大きく超える**環境（allocator）が主戦場になる。
- MSR 測定（Dojo）で「文字列の共有/コピー」が書き手の誤りの原因として上位に現れる — 表現を変える方が
  使命に資する証拠になる。
- wasm レッグが String を共有表現で持つようになり、native だけ所有のままでは byte-identity が
  むしろ難しくなる。

## References

- #1673, #1678, #1679（実測と反証の記録）、PR #1680–#1687
- `../almide-references/RESEARCH.md` §5（JSON 文字列走査）、§6（Koka の借用/所有環境）
- `research/benchmark/perf/string-gap-1004.md`（strchurn: String 確保が支配するベンチ）
