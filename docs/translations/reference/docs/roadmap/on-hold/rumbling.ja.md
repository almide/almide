<a id="the-rumbling--almide-oss-rewrite-campaign"></a>

# The Rumbling — Almide OSS 書き直しキャンペーン

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](rumbling.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](rumbling.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](rumbling.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rumbling.md)
> このキャンペーンは保留中。他言語比較、小サイズの目標、提案上の強みは仮説または当時の位置づけであり、現在の優位性を確立するものではない。4.5KB の観察は手順未記録で、配布候補は利用可能成果物でも公開許可でもない。


> 原文の説明：WASM サイズと LLM 精度を示すため、OSS ツールを Almide で書き直すキャンペーン

**状態**：保留中（言語機能が成熟した後の Block 0）
**優先度**：戦略的 — 言語普及の主要な推進力
**前提条件**：WASM 直接 emit 完了、stdlib 安定、セルフホスティング Phase 0-1

<a id="why"></a>

## 理由

言語の普及は「それで何を作れるか」で決まる。Go は Docker と Kubernetes で勝ち、Rust は ripgrep と fd で「高速 CLI ツール」の地位を取った。

このキャンペーンは、既存の優位を前提にせず、三つの仮説を検証する：
1. **Wasm バイナリサイズ** — 当初の 4.5KB Hello World という観察には、ここに版やビルド手順の記録がない。再利用前に現行成果物を測定する。
2. **Native と Wasm 出力** — TS/JS コード生成ターゲットは廃止済み。
3. **LLM modification survival rate** — Almide Dojo で範囲を定めたタスク／モデルを評価する。普遍的な言語ランキングを推論しない。

The Rumbling は、ライセンス互換な OSS を Almide で書き直し、これらの武器を示す計画である。

<a id="execution-principles"></a>

## 実行原則

- **Block 0 が通るまで先へ進まない** — 自分のツールを書けない言語で他人のツールを書き直すのは矛盾
- **各 block 内の項目は独立** — どの項目からでも開始可能
- **各完了後に発信する** — 単独で記事として成立するものだけを出荷
- **劣化コピーは作らない** — 機能を減らしても、何らかの軸で元のものより明確に優れる場合だけ出荷
- **巨大なものや Almide の強みが当てはまらないものは飛ばす** — データベース、暗号ライブラリ、OS レベルのツールは対象外

---

<a id="block-0-dogfood-our-own-tools"></a>

## Block 0：Dogfood（自分たちのツール）

Almide 自身のツールチェーンを Almide で書く。セルフホスティングへの入口。

| ツール | 示すこと | 規模 |
|---|---|---|
| `almide fmt` | 文字列処理が本番レベルで動く | 小 |
| `almide test` ランナー | CLI ツールを書ける | 小 |
| テストフレームワーク | assert/matcher DSL を自然に書ける | 小 |

**ゲート条件**：Block 0 の全 3 項目が動くまで Block 1 は開始しない。

---

<a id="block-1-wasm-showcase-measure-size-for-the-actual-workload"></a>

## Block 1：Wasm ショーケース（実際の負荷に対するサイズを測る）

極めて小さい WASM バイナリサイズを示す。すべてブラウザで動くデモを付ける。

| ツール | 既存 | Almide の優位 |
|---|---|---|
| markdown → HTML | marked.js（元の比較、版未記録） | 同等動作の runtime＋host＋成果物サイズを測る |
| JSON formatter / validator | jq（元の比較、版未記録） | 同等の負荷／ビルドを比較する。ここではサイズ優位は確立していない |
| TOML parser | toml-rs（crate） | ブラウザ配備の比較はこの記録で検証されていない |
| base64 encode / decode | btoa/atob | 自己完結した WASM |

**成功条件**：「このページで動く WASM は X KB」と言えること。

---

<a id="block-2-nativewasm-showcase-distribution-adapters-are-separate-work"></a>

## Block 2：Native/Wasm ショーケース（配布アダプタは別作業）

native/Wasm の再利用を示す。npm や crates.io 向けパッケージ化には、ラッパ、API、公開の明示的な作業が必要となる。npm での配布を廃止済みの TS/JS コンパイラターゲットと混同してはならない。以下の表は配布候補であって、利用可能な出力やパッケージ公開の許可ではない。

| ライブラリ | 用途 | 配布 |
|---|---|---|
| Slug 生成 | URL slug | npm + crate + WASM |
| Semver parser | バージョン比較 | npm + crate + WASM |
| 色変換 | hex / rgb / hsl | npm + crate + WASM |
| uuid v4 | ID 生成 | npm + crate + WASM |

**成功条件**：「知らずに Almide 製ライブラリを使っている」状態を作る。

---

<a id="block-3-llm-modification-showcase-proving-survival-rate"></a>

## Block 3：LLM 変更ショーケース（Survival Rate を示す）

LLM がコードを正確に変更できることを示す。各ツールに「LLM に X を頼んだら壊さずに動いた」というデモを付ける。

| CLI | 既存 | Almide の強み |
|---|---|---|
| HTTP client | httpie, curlie | LLM がコード全体を把握し、壊さずにヘッダを追加 |
| File watcher | watchexec | LLM が全コードを把握できるほど単純 |
| Env 管理 | direnv lite | 設定ファイルの読み書き |
| Task runner | just lite | TOML 解析＋プロセス実行 |

**成功条件**：実測した modification survival rate データを出せること。

---

<a id="block-4-platform-ecosystem"></a>

## Block 4：プラットフォーム（エコシステム）

Block 0-3 で信頼を築いた後。

| プロジェクト | 既存 | 意義 |
|---|---|---|
| パッケージレジストリ | crates.io / npm | Almide エコシステムの基盤 |
| Playground | Rust Playground | WASM コンパイラがブラウザで動く（セルフホスティングの結果） |
| LSP server | — | 本格的な開発体験 |

---

<a id="not-doing"></a>

## やらないこと

| 対象 | 理由 |
|---|---|
| DB エンジン | Almide の強みが当てはまらない。低レベル I/O が必要 |
| 暗号ライブラリ | 安全性の証明が言語の成熟度を超える |
| OS レベルのツール | 直接 syscall が必要。Almide の抽象度に合わない |
| 巨大フレームワーク | 労力に見合わない。代わりに小さく速いものを多く出す |
