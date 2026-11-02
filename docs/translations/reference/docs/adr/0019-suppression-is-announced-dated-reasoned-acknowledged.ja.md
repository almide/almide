<a id="adr-0019-suppression-is-announced-dated-reasoned-and-acknowledged--its-shape-is-fixed-before-the-construct-exists"></a>

# ADR-0019：抑制は告知し、期限・理由・確認を伴う — 構文の導入前に形を固定する

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0019-suppression-is-announced-dated-reasoned-acknowledged.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0019-suppression-is-announced-dated-reasoned-acknowledged.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0019-suppression-is-announced-dated-reasoned-acknowledged.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md)
> Accepted は将来の構文案を固定する意味であり、原文は何も実装されておらず列挙ゲートも未存在と明記している。現状の不在や将来の導入は固定原文時点の記述で、新たな監査ではない。署名のない確認トレーラは主張であり、本人性や署名を検証するものではない。


- **状態**：Accepted — 2026-09-22、個別の ○× 判定ラウンドなしで決定。メンテナの常設指示「○× が必要なところは推奨を採る」に従い、以下の各選択（綴り、範囲、期間、トレーラ名、終了時の挙動）は推奨案をそのまま採用した。ここに書かれたものは何も実装されていない。この記録は、それを導入する PR が出たときの構文を拘束する（#2152 項目 4、arena-breakthroughs T8）。
- **日付**：2026-09-22
- **背景**：Almide に抑制構文はない。`@allow`、コメント pragma、manifest の免除、コマンドラインフラグのどれもない。コンパイルが検出した警告は毎回すべて出力される（警告を隠す唯一の切り替えは REPL 内部の `SUPPRESS_WARNINGS` で、ソーステキストからは到達できない）。導入圧力は、すでにロードマップにある三方向から来る。dialect epoch による警告からエラーへの昇格（E053 自身のモジュール文書がまさに計画している）、lint の偽陽性、bounded profile の追加規則である。この言語が最適化対象とする書き手はモデルであり、モデルは `#[allow(dead_code)]` と `// eslint-disable-next-line` を反射的に再現する。黙ったまま永久に免除する形に手を伸ばすだろう。`attr_vocab.rs` はすでに「何でも受理し、何も報告しない構文」を、そのような書き手にとって最悪の表面と呼ぶ。現地調査（arena-breakthroughs §2.3）では、告知され、期限があり、コミット履歴で確認される抑制を持つ近隣例が一つ見つかった。aver の `[[check.suppress]]` は理由を必須とし、警告だけを対象にし、各ファイルの免除が取り除いた警告数と、何にも一致しなかった規則を報告する。
- **決定**：Almide が抑制構文を得るときは、次の形とし、これより緩いものにしない。
  1. **綴りと範囲。** 一つの宣言（`fn`、`effect fn`、`type`、トップレベル `let`、`test`）に `@allow(code = "E053", until = "2027-01-31",
     reason = "…")` を付け、その宣言の span 内のその一つのコードを免除する。一属性が免除するのは一コード。二コードなら属性も二つで、それぞれ独自の日付と理由を持つ。モジュール・ファイル単位、コメント、manifest、コマンドラインの形式は設けない。
  2. **警告のみ。** コンパイラが warning レベルで出すコードだけを免除できる。error レベルのコードを名指しすると、それ自体がエラーになる。dialect epoch が警告をエラーへ昇格すると、その全免除もその epoch でエラーになる。免除で epoch を延期できない。
  3. **非沈黙 — 毎回のコンパイルで再告知。** 有効な免除はそれぞれ stderr に一行、`note[E053]: waived at <file>:<line> until
     <date> (<n> occurrence(s)): <reason>` を出す。そのコンパイルで何も抑制しなかった免除は未使用と報告する。`check`、`build`、`run`、`test` のすべてが出力し、`--json` では `waived` レコードとなる。フラグ・環境変数・profile のどれでも消せない。免除済み警告は `--deny-warnings` に数えない。告知は警告ではなく、それ自体を免除できない。
  4. **期限付き — 期限切れはエラー。** `until` は必須で、コンパイル日から最長 **180 日**後までの ISO-8601 暦日とする。`until` 後の最初のコンパイルで、免除は ERROR となり、その免除、理由、過ぎた日付を名指しする。日付の欠落・不正・上限超過もエラー。コンパイル日は通常システム時計だが、`SOURCE_DATE_EPOCH` が設定されていればそれを使い、古い tag の再ビルドを当時の日付に固定できる。日付が決めるのは判定だけで、出力バイト列には決して入らない。
  5. **理由付き。** `reason` は必須で、前後の空白を除いて空であってはならない。
  6. **コミットトレーラで確認。** 差分で `@allow(` を追加するか、既存の `code`、`until`、`reason` を変更するコミットは、**`Waiver-Acked-by: Name <email>`** トレーラを持つ。免除の削除には不要。
- **根拠**：(1) *非沈黙*：`#[allow]` は一度書くと二度と読まれないので失敗する。全ビルドで各免除を再告知し、負債を支払える人の前に置き続ける。モデルなら後続ビルドのたびにコンテキストウィンドウへ入る。aver の削除警告数と未使用規則の報告も同じ発想であり、免除ごとの出力はさらに一歩進めたもの。(2) *期限付き*：理由があっても終わりのない免除は永久である。日付がそれを借金にする。180 日は実際の upstream 修正に十分長く、`until = "9999-12-31"` で借金を贈与に変えることができないほど短い。(3) *期限切れはエラー*：「期限切れの免除」を WARNING にすると、それ自体が免除・無視できる。この構文が止めるべき後退そのものになる。(4) *理由付き*：誰も説明しなかった免除は、誰も解消できない免除である（同規則に対する aver の表現）。(5) *確認*：エージェントは黙って借金できず、履歴に誰が同意したかを記す。(6) *`@allow` の綴り*：Rust を知るモデルは、日付も理由もない `@allow(x)` を書く。実際の属性を `@allow` にすれば、この試みは不足フィールドを fix-it が名指しする正確なエラーに着地し、事前分布を捕らえて直せる。別名なら `@allow(x)` は未知の属性、つまり E053 警告になり、また黙る経路になる。(7) *宣言スコープ*：編集の局所性。免除は対象コードと共に移動し、そのコードを編集する人の差分に現れる。(8) *警告のみ*：エラーは言語規則であり、免除した規則は私的方言である。
- **代替案**：
  (a) *抑制を決して追加しない* — 現在の状態ではあるが、常設方針としては却下。締め切り下で最初の偽陽性 lint や epoch 昇格に遭うと、その場しのぎの脱出口ができる（検査を無効化、正しいコードを劣化させて書き換え、古いコンパイラに固定）。今形を決めておくことで、その圧力の下で急造の無言の `@allow` が入るのを防ぐ。
  (b) *Rust の `#[allow(lint)]`* — 無言、永久、理由なし。RFC 2383 の `reason =` は任意で、`#[expect]` は期待が満たされなくなったときだけ発言する。四特性のうち三つに失敗。
  (c) *manifest の表（aver の `[[check.suppress]]`）* — 理由付き、報告あり、集中管理。しかし局所性により却下。関数を編集する人が読まないファイルから glob 照合され、改名で黙って範囲が広がるか孤立し、関数の差分にもない。未使用規則の報告は採用する（決定 3）。
  (d) *コマンドラインフラグ（`--allow E053`）* — ソースでなくビルドスクリプトにあり、CI とローカルで不一致となる。理由も日付もない。
  (e) *コメント pragma（`// almide: allow E053`）* — コメントは AST ではなく、formatter、LSP、checker から見えない。pragma の誤記は無反応で、E053 の失敗を再び招く。
  (f) *期限切れを警告にする* — 根拠 (3) により却下。
  (g) *理由の代わりに追跡 issue URL を必須化* — URL はオフラインでは役立たず、モデルは読めない。issue があれば理由に番号を含められる。
  (h) *署名付き確認*（署名コミットまたは署名トレーラ）— 署名方針はリポジトリ設定であり言語規則ではない。全貢献者が署名するわけでもない。トレーラは記録された主張である（帰結）。
- **構文導入後の強制**（導入 PR が全行を含める。現在はどれも存在しない）：

  | 特性 | ゲート |
  |---|---|
  | 非沈黙 | 告知を確認する `tests/diagnostics/` の broken/fixed ペア。同じ免除済みファイルで `check`、`build`、`run`、`test`、`--json`、`--deny-warnings` を実行し、毎回 note と未使用免除報告を確認する CLI テスト |
  | 期限付き | `until` の欠落・不正・上限超過・期限切れそれぞれの E-code と `tests/diagnostics/` ペア。期限切れ fixture は `SOURCE_DATE_EPOCH` で固定して決定的にする。コードは導入 PR が割り当て、ここでは予約しない |
  | 理由付き | `reason` が欠落または空の場合の E-code とそのペア |
  | 警告のみ | `@allow` が error レベルのコードを指す場合の E-code と、epoch 昇格で既存免除がそのエラーに変わることを確認するテスト |
  | 確認 | `scripts/check-waiver-acks.sh` を CI の `checks` job が PR のコミット範囲に対して実行し、lefthook の `commit-msg` hook でも実行する。`.almd` 内の `@allow(` を追加・変更する差分のコミットには `Waiver-Acked-by:` が必須（`git interpret-trailers --parse` で読む）。`proofs/gate-verification.toml` の行とその負例テスト |
  | 棚卸し | 診断 fixture を除くリポジトリ自身の有効免除数を、刻印付き・縮小専用上限とし、導入時は 0 |

- **帰結**：すべての抑制が毎ビルドで見え、期限を持ち、理由を説明し、確認者の名前が履歴に残る。馴染みの綴りを書くモデルは完全な形へ修正される。コストは、日付に依存するビルド判定（`SOURCE_DATE_EPOCH` と日付を出力バイト列へ含めない制約で限定）、免除を持つコードの stderr ノイズ（意図的）、貢献者が覚えるべきトレーラ（hook が教える）。確認は主張であって証明ではない。git トレーラは署名のないテキストなので、ゲートは存在と形を強制するだけで本人性は保証しない。偽造の確認は永続履歴に残る虚偽記載でレビューから見える。それだけの強さを主張する。現時点では何も変わらない。これは抑制構文を導入する PR のレビュー基準となる形である。
- **反証条件**：(1) 導入後、二つの連続リリースで `until` を先延ばしするだけのコミットが免除を削除するコミットを上回ったら、借金モデルは失敗しており決定を再検討する（更新回数制限または追跡 issue 必須化）。(2) Dojo の計測で、モデルのパッチがコードの修正より `@allow` 挿入を多く行うなら、馴染みの綴りという議論は誤りで、名前を再検討する。(3) upstream 修正がより長い周期（依存先のリリース周期）で動くため 180 日期限が更新を強いるなら、期間を再検討する。ただし期限切れそのものは再検討しない。
- **参照**：#2152（項目 4）；docs/roadmap/active/arena-breakthroughs.md（T8 項目 4、§2.3）；crates/almide-frontend/src/attr_vocab.rs（E053）；`../almide-references/aver/docs/cli.md`、`[[check.suppress]]`；Rust RFC 2383（lint の理由）、https://rust-lang.github.io/rfcs/2383-lint-reasons.html ；`git interpret-trailers`、https://git-scm.com/docs/git-interpret-trailers ；Linux kernel のパッチ過程における `Acked-by:`、https://docs.kernel.org/process/submitting-patches.html ；`SOURCE_DATE_EPOCH`、https://reproducible-builds.org/docs/source-date-epoch/ 。番号付け：0018 は進行中ブランチ（value-strings 記録）が保持しているため、この記録は衝突を避けて 0019 とする。
