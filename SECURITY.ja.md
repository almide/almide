# セキュリティ方針

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./SECURITY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](./SECURITY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](./SECURITY.zh-CN.md)

## セキュリティ責任者

Almide を開発する株式会社Aid-On（Aid-On Inc.）の代表取締役、Hiromi Motodera（本寺 広海）がセキュリティ責任者である。報告は個人のアドレスではなく、以下の非公開窓口を通じて行う。

## 脆弱性の報告

**GitHub Security Advisories** を通じて非公開で報告する：
https://github.com/almide/almide/security/advisories/new — 未修正の脆弱性について公開 Issue を作成しない。初回の応答は 7 日以内を目安とする。協調的な情報公開のため、90 日間の猶予をお願いしている（修正公開後、合意により短縮可能）。

対象範囲：コンパイラとその出力コード（メモリ安全性の主張を破る誤コンパイルはセキュリティ不具合に該当する。`docs/project/ISSUE-TAXONOMY.md` の重大度分類を参照：`I-unsound` / `I-miscompile` は CI 方針上のリリース阻害条件）、ランタイム（`runtime/rs`）、wasm の処理経路とホスト側 shim、およびリリースパイプライン。

## サポートするバージョン

最新のマイナーリリース系列に修正を提供する。古い系列には修正をバックポートしないため、アップグレードすること。リリース阻害方針：`I-unsound` / `I-miscompile` / `I-divergence` / `regression` の未解決 Issue がある状態では、正式版タグを公開できない（リリース workflow が実施、#1482）。

## 依存関係の方針

CI は RustSec アドバイザリデータベースに照らして依存関係ツリーを監査する（`cargo audit`、週次および lockfile の変更ごと。`.github/workflows/dependency-audit.yml` を参照）。修正版のないアドバイザリは警告を抑制するのではなく、Issue でトリアージする。恒久的な無視には workflow ファイルへの理由の明記が必要となる。

## リリースの完全性

すべてのリリースに `almide-checksums.sha256` を同梱し、リリース成果物には **Sigstore のアテステーション**（GitHub artifact attestation）を付ける。ダウンロードした各成果物は次のように検証できる。

```
gh attestation verify <asset> -R almide/almide
```

ツール自体もこの検証を行う（#4127）。

- `almide self-update` はコンパイラ独自の HTTPS クライアントでダウンロードする（rustls。PATH 上の `curl` は使わず、リダイレクト先は https のみ、#4181）。プロセス内でアーカイブのハッシュを計算し、完全一致するファイル名のチェックサム行と照合し、新しい非公開ディレクトリに配置して、`gh attestation verify` でアテステーションを検証する。`gh` がなければ拒否する。`--skip-attestation` を指定するとチェックサムだけでインストールし、その旨を表示する。
- `tools/install.sh` / `tools/install.ps1` はチェックサムの欠落・不一致を拒否し、`gh attestation verify` でアテステーションを検証する。`self-update` と同様、`gh` がなければ拒否する。`--skip-attestation`（`curl … | sh -s -- --skip-attestation`）を指定するとチェックサムだけでインストールし、その旨を表示する。`scripts/check-installer-attestation.sh` は CI 上でスタブ化したリリースに対して両インストーラーを実行する。

各リリースに添付する適格性資料（`almide-dossier-*.md`）には、そのタグの信頼チェーンの検証記録が含まれ、同じ方法でアテステーションを付ける。
