[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> リポジトリのエージェント指示を読むための、非規範・原文固定の写し。正本 CLAUDE.md は変更しない。この CLAUDE.reader.md 系列は、指示が自動読込されるファイル名を意図的に使わない。歴史的原文の指示であり、新たな指示階層でも、現在の実装や安全性を独立検証した保証でもない。自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-tools/CLAUDE.md)

# almide-tools

開発者向けツール：formatter、モジュール interface 抽出、`.almdi` artifact。

<a id="formatter-fmtrs"></a>

## Formatter（`fmt.rs`）

- `format_program(program) -> String` — AST → 整形済みソース。
- コメントを保持し、式の複雑さに基づき短い／長い整形を判断する。
- `auto_imports()` — 不足する stdlib import を追加し、未使用のものを除く。

<a id="module-interface-interfacers"></a>

## モジュール Interface（`interface.rs`）

- `extract_module_interface(ir_program) -> ModuleInterface` — JSON API 記述を生成する。
- 型シグネチャ、ドキュメント、非推奨フラグ、C FFI の ABI 配置を含む。
- export ツールが pip/npm/gem パッケージを生成する際に使う。

<a id="module-artifacts-almdirs"></a>

## モジュール Artifact（`almdi.rs`）

- `.almdi` のシリアライズ：公開 interface と型検査済み IR。
- language server はルート CLI の `src/cli/lsp*.rs` に実装される。

<a id="rules"></a>

## 規則

- **formatter は冪等でなければならない。** 常に `format(format(x)) == format(x)`。
- **interface JSON が契約。** 外部ツールがこれを解析するため、破壊的変更にはバージョンを上げる必要がある。
- **ABI 配置は codegen と一致させる。** `AbiLayout` のフィールド offset は `almide-codegen` が実際に出力するものと一致する必要がある。codegen のレコード配置を変えたら interface 抽出も更新する。
