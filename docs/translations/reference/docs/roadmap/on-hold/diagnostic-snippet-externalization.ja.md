<a id="externalize-try-snippets-from-rust-literals"></a>

# `try:` スニペットを Rust リテラルから外部化する

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](diagnostic-snippet-externalization.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](diagnostic-snippet-externalization.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](diagnostic-snippet-externalization.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/diagnostic-snippet-externalization.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットであり、自動同期されない。状態・実装・計測の記述は原文時点の記録で、翻訳時の独立検証や現行保証ではない。計画を実装済みとは扱わない。

説明: Rust リテラルから検査済みソーステンプレートへ診断修正スニペットを移行する、残る作業。

<a id="current-state"></a>

## 現状

前提となる [同梱 Almide ディスパッチ](../../../../../roadmap/done/bundled-almide-dispatch.md) は 2026-04-16 に提供済みであり、もはや障害ではない。スニペット移行自体は提案のままで、`stdlib/diagnostics/` テンプレートディレクトリは存在しない。

修正テキストは引き続き Rust で構築されている。例:

- [`check/calls.rs`](../../../../../../crates/almide-frontend/src/check/calls.rs): `with_try` が引数個数と名前変更の修正を構築する。
- [`check/solving.rs`](../../../../../../crates/almide-frontend/src/check/solving.rs): `unit_leak_snippet` が結果式の修正を提供する。
- [`stdlib.rs`](../../../../../../crates/almide-frontend/src/stdlib.rs): `try_snippet_for_alias` がより豊かな別名修正を提供する。

これらは例であり、固定したファイル／件数一覧ではない。言語のパーサーと切り離されたテキストが古い修正方法を教えてしまう、という乖離のリスクは残る。

<a id="remaining-design-and-work"></a>

## 残る設計と作業

1. 現在の修正生成箇所を棚卸しし、動的な Rust フォーマット処理ではなく検査済みソーステンプレートにできる、完全なスニペットを選ぶ。
2. テンプレートディレクトリ（提案: `stdlib/diagnostics/*.almd`）、明示的なプレースホルダー置換、コンパイラの読み込み経路を導入する。同梱ディスパッチが利用可能であるだけでは、この読み込み経路は実装されない。
3. 診断の意味を変えず、スニペットをユーザー拡張可能にしたりローカライズしたりせずに、選択した修正を移行する。
4. 具体値を代入し、すべてのテンプレートを解析し、完全な修正と説明するものをそれぞれ型検査するテストハーネスを追加する。意図的な断片は明示し、型検査済みプログラムとして数えない。

例えば、末尾の値の欠落をテンプレートで説明できる。

```almide
// placeholders: {fn_name}, {binding_name}, {ret_ty}
// Add {binding_name} as the trailing expression so the fn returns {ret_ty}:
//
//   let {binding_name} = <computation>
//   {binding_name}
```

これは提案されたテンプレート表記であり、既存のローダーや合格済みテストではない。移行はテンプレートと検証ハーネスが一緒に導入されて初めて完了する。前提のディスパッチが完了しただけでは、移行完了の証拠にならない。
