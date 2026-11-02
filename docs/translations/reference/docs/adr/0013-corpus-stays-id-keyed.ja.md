
> 読者向けの参考訳です。上記コミットに固定した原文が正本であり、この訳は規範文書の改訂ではありません。状態・数値・実装状況は決定当時の記録です。Accepted は設計の採択であり、実装完了や現在の検証結果を意味しません。自動同期は行いません。原文中の外部調査 ../almide-references や QUALIFICATION.md 等の記述は原文の参照であり、翻訳作業で別途取得・再検証したものではありません。

<a id="adr-0013-the-conformance-corpus-stays-contract-id-keyed-semantic-reorganization-is-rejected"></a>

# ADR-0013: 適合性コーパスは契約 ID をキーとしたままにする。意味別の再編成は採用しない

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0013-corpus-stays-id-keyed.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0013-corpus-stays-id-keyed.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0013-corpus-stays-id-keyed.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-corpus-stays-id-keyed.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。



- **状態**: Accepted（採択済み）
- **日付**: 2026-08-20
- **背景**: 2026-08 の参照調査（../almide-references）では、rustc がフラットで機械的なテスト配置を禁止し（`deny_new_top_level_ui_tests`、compiler-team#902）、機械的なキーを使ったファイル名（`issue-123456.rs`）にも注意を促していることを記録した。一見すると、このリポジトリのパスを固定したコーパス配置に反対する根拠に読める。一方、すべての利用側はこのリポジトリをコミットで固定している。ゴールデンマニフェストはオラクルがハッシュ化する出力にフィクスチャのパスを埋め込み、契約台帳の証拠項目と 591 個の `// @contract:` ヘッダーはパスを参照し、既存の実装も差分比較できるコピーを保持している。
- **決定**: コーパスは現在の配置と、パスを固定した識別を維持する。フィクスチャのキーは場所ではなく契約 ID（`C-NNN`）とする。案内には生成した索引（ALS の節索引、適合性レポート）を使い、ファイルの移動は行わない。新しいフィクスチャには対象を先頭に置いた snake_case 名（`map_upsert_str.almd`）を付けなければならない。数字だけの名前や `issue` / `bug` で始まる名前はスタイルゲートが拒否する。
- **理由**: rustc 自身の例外が認めているのは、対応を逆にたどれる方向、つまり安定した不透明な ID から導かれるパス（`tests/ui/error-codes/EXXXX.rs`）である。これはまさにこのコーパスの形である。調査の注意はパスだけをキーにするコーパスに向けたものであり、こちらは `C-NNN ⇄ fixture` の双方向の対応をゲートで検査している。再編成をしても生成索引以上の利点は得られず、コミットを固定しているすべての利用側でゴールデンマニフェストを再生成し、全引用を書き換え、Stage B の切り替えが依存するコピー同士の差分比較可能性を失うことになる。
- **代替案**: (a) rustc 型の意味別ディレクトリと README⇄ファイルシステムのゲート — 不採用。利点はすでに生成索引で提供でき、費用は上記の通り。(b) 何もしない — 不採用。調査が警告した命名規律の問題は実在し、今回ゲートで検査するようになった。
- **帰結**: コーパスのパスは、実質的に追記専用の識別子になる。フィクスチャ名の規律はスタイルゲートが担当する。コーパスに新しい整理軸が必要になっても、移動ではなく生成索引として追加する。
- **反証条件**: 生成索引では対応できないがディレクトリ移動なら対応できる案内上またはレビュー上の失敗が実証されること。当時までの 1,749 コーパスファイルでは観測されていない。
- **参照**: ../almide-references 調査レポート（D1）、rustc の `src/tools/tidy/src/ui_tests.rs`、`tests/best-practices.md`。
