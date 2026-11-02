# 等価性の主張 — ターゲット間でバイト単位に一致すること

[English](./EQUIVALENCE.md) | [日本語](./EQUIVALENCE.ja.md) | [简体中文](./EQUIVALENCE.zh-CN.md)

**ターゲット間の等価性は、契約ごとに検証します。** 対応するプログラムについて、入力とホストの前提を揃え、決定的な観測結果（stdout、stderr、終了コード）を比較します。ホストの観測、乱数、リソース制限、オラクルに対して相対的な時刻については、明記された不変条件や比較条件が必要です。有限のコーパスを通過しても、両ターゲット向けにコンパイルできるすべてのプログラムについての証明にはなりません。

「バイト単位に一致する」とは、コンパイルされた成果物ではなく、*実行時の出力*を指します。ネイティブバイナリと `.wasm` モジュールは、その構造上、異なるバイト列です。比較するのは契約が対象とする観測結果です。

## 範囲

この保証は、**範囲を明示して継続的に維持するもの**です。一度限りのスナップショットではなく、変更のたびに再実行するゲートによって維持します。

- **決定的な観測結果**：宣言されたクロスターゲットのフィクスチャと差分テストでバイト列を比較します。ホストが異なっても無条件に等しいという意味ではありません。
- **非決定的な入力**：契約はサンプルの一致ではなく、不変条件（乱数値が範囲内に収まる、など）を要求する場合があります。以前のタイムアウト削除（C-006）は過去の記録です。現在の `fan.timeout` はオラクルに対して相対的です（C-208）。
- **利用できない操作**：選択した経路がプログラムや操作を拒否する場合があります。`proofs/target-availability.toml` を参照してください。拒否は、そのプログラムについて一致を観測できたことを意味しません。
- **明示されたホスト・表現の違い**：プラットフォーム情報の報告や圧縮出力には、範囲を限定した不変条件があります。普遍的なバイト列の一致ではありません。たとえば zlib の契約は、圧縮戦略の違いを記述したうえで、復号とラウンドトリップを比較します。以前の2関数の除外リストですべてを網羅できると考えず、該当する契約と実行可能なフィクスチャを確認してください。

## 契約台帳

この主張は文章だけではありません。観測可能な約束はすべて、[振る舞い契約台帳](../contracts)の名前付き契約であり、それぞれが `fixture` 以上のクラスの実行可能な証拠に結び付いています。例外条項は `scripts/gen-claims.sh` が台帳から[計測ページの主張ブロック](../benchmarks/README.md)へ自動生成します。その横の契約数は、計測日とともに台帳から記録されます（`proofs/ledger-counts.toml`、`scripts/gen-ledger-counts.sh`。`scripts/check-ledger-counts.sh` が毎晩再計測）。ブロックがいずれかの入力と食い違うと、`scripts/check-contracts.sh` が CI を失敗させます。生成ブロックは宣言された入力と同期されますが、それだけで、すべての文章や証拠へのリンクが主張する振る舞いを証明することにはなりません。

台帳の仕組み：

- 新しい観測可能な振る舞いには、新しい `C-NNN` 契約**と**、`// @contract: C-NNN` ヘッダーでその契約を宣言するフィクスチャが少なくとも1つ必要です。リンクは双方向で、CI が検証します。
- 相違を解消したら、同じ PR で契約の `status` を `active` に変更します。`flagged-for-revision` の件数は**減少だけ**が許されます（ラチェット方式、現在の上限は 0）。

## 証拠の層

| 証拠の層 | 検証・維持するもの |
|---|---|
| [契約台帳](../contracts) | すべての約束に `C-NNN` の名前を付ける。`active` な契約には `fixture` 以上のクラスの証拠が必要 |
| [クロスターゲットのフィクスチャゲート](../../tests/wasm_runtime_cross_target.rs) | すべての `spec/wasm_cross/*.almd` フィクスチャを両ターゲットで実行し、出力をバイト単位で比較（`wasm_cross_target_spec`） |
| [差分ファズテスト](../../tests/regex_fuzz_test.rs) | ランダム化されたプログラムと入力について、ネイティブと wasm の出力を比較 |
| 出力生成時の Σ-probe | wasm の Unicode・大文字小文字変換テーブルを、出力生成時にスカラー値の全域で Rust `std` と網羅的に照合 |
| [Lean 4 belt](https://github.com/almide/almide/tree/develop/crates/almide-perceus-belt) | RC 挿入の正しさを Lean カーネルで機械検証 |
| [組織全体の byte-verify 検査](https://github.com/almide/almide/blob/develop/scripts/org-byte-verify.sh) | almide 組織の実行可能な全リポジトリを両ターゲットで実行し、stdout と終了結果をバイト単位で比較 |

## 自分で検証する

```bash
almide test                        # every fixture, both targets where declared
bash scripts/check-contracts.sh    # ledger integrity + claims-block freshness
bash scripts/org-byte-verify.sh    # org-wide two-target byte comparison
```

証明書が確立するのは、対象範囲と信頼の前提のもとでモデル化された証拠の性質です。ネイティブと Wasm の意味論的な等価性を証明するものではありません。[証明済みの部分と信頼に依存する部分](../contracts/proven-vs-trusted.md)、[TRUST-SPINE.md](../TRUST-SPINE.md) を参照してください。
