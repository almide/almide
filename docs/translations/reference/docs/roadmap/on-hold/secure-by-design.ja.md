<a id="secure-by-design"></a>

# 設計による安全性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](secure-by-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](secure-by-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](secure-by-design.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/secure-by-design.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した非規範的な読者向け翻訳です。自動同期されず、原本が引き続き権威を持ちます。安全性・セキュリティ・比較・実装・測定の主張は設計時点の原本の記述として保持し、独立した実行、時間測定、監査、認証は行っていません。現行リリースについての新しい保証ではありません。コードブロックとコメントは原文のまま、メタデータは無効化した読者向け注記です。

> 原本の説明（無効化したメタデータ）: 提案中の多層セキュリティモデル、実装済みの基盤、残る信頼境界。

> **状態: 保留 — 設計提案であり、セキュリティ認証ではありません。**
> 2026-10-31 にリポジトリと照合してレビューしました。不透明型、SafeHtml/SafePath のヘルパー、effect/capability 解析は基盤を提供します。これらは普遍的な Web の安全性や、以下で提案するパッケージごとのポリシーインターフェースを確立するものではありません。

<a id="goal-and-limits"></a>

## 目標と限界

危険な出力と外部アクセスを明示的かつ監査可能にします。型付きラッパーが役立つのは、構築子が意図したポリシーを強制し、関連するすべての sink がそのラッパーを要求する場合だけです。ラッパーだけでは、注入、パストラバーサル、データ持ち出し、悪意ある依存関係を防げません。

この文書は 5 層の設計と残る統合作業を保持します。現在の保証とパッケージの挙動は、[構成によるセキュリティ](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/security-by-construction.md)と[パッケージ仕様](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)を参照してください。

<a id="layer-1-effect-isolation--checked-calls-not-a-sandbox"></a>

## 第 1 層: effect の分離 — 呼び出しの検査であり、sandbox ではない

checker は通常の pure 呼び出しと effect 呼び出しを区別します。これは effect を伴う操作の特定に役立ちますが、組み込み出力の例外があります。pure API は、観測可能な挙動がないこと、停止、資源使用の上限、パッケージの無害性を証明しません。effect 検査は、信頼できないネイティブコードの隔離境界ではありません。

**既存の基盤:** effect 検査と capability 解析。現在の適用範囲を、pure 関数は I/O を行えないという全面的な主張に広げてはなりません。

<a id="layer-2-explicit-external-bridges"></a>

## 第 2 層: 明示的な外部への橋渡し

`@extern` は一部の外部宣言を可視化します。標準ライブラリの intrinsic、host import、ネイティブ依存関係も信頼境界を越えます。したがって `@extern` の検索は完全なセキュリティ監査ではなく、Rust の `unsafe` 境界と同等でもありません。

**残る提案:** platform tag とそのポリシー意味論。以前の `@extern(platform: web, ...)` 例は提案構文であり、プラットフォームを制限した実行が現在強制されている証拠ではありません。capability 推論は既に存在し、この提案の tag 構文を先に出荷することには依存しません。

<a id="layer-3-opaque-types-and-safe-sinks"></a>

## 第 3 層: 不透明型と安全な sink

言語は `mod type T = U` による nominal なモジュール非公開ラッパーをサポートします。既存のヘルパーは [`html.SafeHtml`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/html.almd) と [`path.SafePath`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/path.almd) を参照してください。

- `html.escape` はテキストをエスケープし、`html.raw` は**信頼済みの、未エスケープ**入力を意図的に包みます。したがって SafeHtml はすべての値がエスケープされたことを証明しません。
- SafePath の構築は一部の安全でない文字列を拒否します。ファイルシステム内の閉じ込めは sink、symlink、競合、プラットフォームの意味論にも依存します。
- 提案中の SafeSql ラッパーとパラメーター化されたデータベース sink には、データベース実装と、定義された検証・符号化契約が必要です。
- 検証済みラッパーを要求するフレームワークの sink は別の統合作業として残っています。すべての標準ライブラリ I/O が不透明ラッパーだけを受け付けるわけではありません。

**残る提案:** 安全な sink のシグネチャを定義し、構築子と文脈別エスケープを検証し、その後、安全でない構築と sink の迂回が拒否されることをテストします。以前の `Response.html` / HTML-builder のスケッチは設計例であり、出荷済みフレームワーク API ではありません。

<a id="layer-4-package-capability-policy"></a>

## 第 4 層: パッケージの capability ポリシー

capability 推論は、関数から到達する effect の記述に役立ちます。追加の提案は、利用者が付与する、推移的な**依存関係ごとのポリシー**です。例えば:

```toml
# Proposed interface, not a current manifest contract.
[dependencies.example]
version = "1.0"
capabilities = ["fs"]
```

> コードコメントの訳: この TOML 例は提案中のインターフェースであり、現在の manifest 契約ではありません。

このインターフェースの実装前に、以下を規定します。

- 推論された effect と native/host 実装を capability にどう対応づけるか。
- 未知の呼び出しも含め、強制が静的、実行時、または両方のいずれか。
- 推移的依存関係への許可と診断。
- 更新で要求 capability が変わる場合のポリシー。
- 付与済み capability の範囲内での挙動（ファイルシステムの許可は、無害に使うことの証明ではありません）。

既存の capability 推論を未実装と記述してはならず、この提案のパッケージポリシーが強制されている証拠と混同してもなりません。

<a id="layer-5-supply-chain-identity-and-provenance"></a>

## 第 5 層: サプライチェーンの同一性と来歴

この提案は、固定された同一性、信頼された来歴、capability 変更のレビューを組み合わせます。実装済みの lock/trust の挙動は現在のパッケージ仕様を参照してください。ホスト型 registry は引き続き[別の提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md)です。

コンテンツハッシュは、信頼された期待する同一性との不一致を検出します。無害な挙動を証明せず、悪意ある初回公開を防がず、単独ではタイポスクワッティングを解決しません。ハッシュ検証はネイティブのビルド手順を sandbox 化しません。registry は考えられる配布手段の 1 つであり、ハッシュ計算の前提条件ではありません。

**残る提案:** 信頼された来歴、同一性の固定、更新レビュー、ネイティブ依存関係の信頼との相互作用を定義します。capability の変更やハッシュの不一致には、強制されるという想定ではなく、明示的なポリシーと診断が必要です。

<a id="implementation-sequence-and-why-this-remains-on-hold"></a>

## 実装順序と保留が続く理由

1. **既存の基盤:** 不透明型、HTML/path ヘルパー、effect/capability 解析。普遍的に安全な sink を主張せずに維持します。
2. **sink の統合:** 信頼済みの抜け道や検証失敗時の挙動も含め、安全な HTML、SQL、コマンド、ファイルシステムの境界を規定しテストします。
3. **パッケージポリシー:** 許可、推移的な強制、外部コードの信頼を定義します。platform-tag の設計が役立つ可能性はありますが、それは既存の解析器ではありません。
4. **配布・来歴:** 信頼された同一性と capability 変更レビューを、パッケージツールと将来の registry に統合します。

残る作業には単なる日程調整ではなく、ポリシーや研究上の問題が含まれます。どの性質を強制するか、どの構成要素を信頼するか、付与された権限がどう悪用され得るか、各保証をどの証拠で確立するかです。言語に JavaScript の prototype や動的 eval がないことだけで、外部ホストやネイティブ依存関係を制約できるわけではありません。

<a id="intended-protections-and-residual-risks"></a>

## 意図する防御と残るリスク

網羅率のパーセンテージは主張しません。以前の 100%/95% という推定は目標であり、測定でも証明でもなく、セキュリティ保証として用いるべきではありません。

| 境界 | 可能な防御 | 残る責務 |
|---|---|---|
| HTML/SQL/コマンド出力 | 型付き sink で不適合な値を拒否 | 正しいエスケープ・パラメーター化、構築子、信頼済みの raw 入力 |
| ファイルシステムのパス | 一部の無効なパス文字列を拒否 | 閉じ込め、symlink、競合、プラットフォーム挙動、権限 |
| 依存関係の effect | 選択した capability を特定し制限 | 完全な強制と、付与済み capability 内での悪用 |
| パッケージの同一性 | 信頼された期待する同一性からの変更を検出 | 初期の信頼、来歴、悪意ある更新、ネイティブビルドの挙動 |

認可ロジック、検証の正しさ、デプロイ設定、サイドチャネル、SSRF ポリシー、資源枯渇には、なお明示的な設計とレビューが必要です。これらの層は多層防御を支えますが、OWASP の各カテゴリの排除を確立したり、アプリケーションのセキュリティレビューを不要にしたりするものではありません。

## 原本に関する注意

保留状態、リポジトリとの照合日、実装済み基盤、残る提案を原本の報告どおり保持しています。提案中のポリシー構文は現在の manifest 契約ではありません。退けられた網羅率を保証として復活させず、認証・普遍的安全性・アプリケーションレビュー不要の主張を加えません。
