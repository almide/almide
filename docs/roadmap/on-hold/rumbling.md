<!-- description: Campaign to rewrite OSS tools in Almide to prove WASM size and LLM accuracy -->
# The Rumbling — Almide OSS Rewrite Campaign

**Status**: On Hold (Block 0 は言語機能の成熟後)
**Priority**: Strategic — 言語普及の主軸
**Prerequisite**: WASM direct emit 完成, stdlib 安定, セルフホスティング Phase 0-1

## なぜやるのか

言語の普及は「何が書けるか」で決まる。Go は Docker と Kubernetes で勝った。Rust は ripgrep と fd で「速い CLI」のポジションを取った。

Almide には3つの武器がある:
1. **WASM バイナリサイズ** — Hello World 4.5KB (自己完結型)
2. **4ターゲット出力** — 同じソースから Rust / TypeScript / JavaScript / WASM
3. **LLM 修正生存率** — LLM が最も正確に読み書きできる言語

The Rumbling は、ライセンス的に問題のない OSS を Almide で書き直し、これらの武器を実証する計画。

## 実行原則

- **Block 0 が通らなければ Block 1 以降に進まない** — 自分のツールが書けない言語で他を書き直すのは矛盾
- **各ブロック内は独立** — どの項目からでも着手可能
- **1つ完了するたびにポストする** — 単体で記事になるものだけ出す
- **劣化コピーは出さない** — 機能を絞ってでも、本家より明確に優れている点がある状態で出す
- **巨大なもの・Almide の強みが活きないものはやらない** — DB、暗号ライブラリ、OS レベルのツールは対象外

---

## Block 0: Dogfood (自分の道具)

Almide 自身のツールチェーンを Almide で書く。セルフホスティングの入口。

| ツール | 証明すること | サイズ感 |
|---|---|---|
| `almide fmt` | 文字列処理が実用レベルで書ける | 小 |
| `almide test` runner | CLI ツールが書ける | 小 |
| テストフレームワーク | assert/matcher DSL が自然に書ける | 小 |

**ゲート条件**: Block 0 の 3 つが動くまで Block 1 に進まない。

---

## Block 1: WASM Showcase (4.5KB の証明)

WASM バイナリサイズの異常な小ささを見せる。全てブラウザで動くデモ付き。

| ツール | 既存 | Almide の優位点 |
|---|---|---|
| markdown → HTML | marked.js (50KB min) | WASM 数 KB でブラウザ動作 |
| JSON formatter / validator | jq (WASM 800KB+) | 桁違いに小さい |
| TOML parser | toml-rs (crate) | ブラウザで動く WASM 版が存在しない |
| base64 encode / decode | btoa/atob | 自己完結 WASM |

**成功基準**: 「このページで動いている WASM は X KB です」と言えること。

---

## Block 2: Multi-Target Showcase (1ソース4ターゲット)

同じコードから Rust crate + npm package + WASM module が出ることを見せる。npm と crates.io に実際に publish する。

| ライブラリ | 用途 | 配布形態 |
|---|---|---|
| slug 生成 | URL スラッグ | npm + crate + WASM |
| semver パーサー | バージョン比較 | npm + crate + WASM |
| color 変換 | hex / rgb / hsl | npm + crate + WASM |
| uuid v4 | ID 生成 | npm + crate + WASM |

**成功基準**: 「Almide で書かれたライブラリを知らずに使っている」状態を作る。

---

## Block 3: LLM Modification Showcase (修正生存率の証明)

LLM が正確に修正できることを示す。各ツールに「LLM に X を頼んだら壊れずに直った」デモを付ける。

| CLI | 既存 | Almide の強み |
|---|---|---|
| http クライアント | httpie, curlie | LLM が全容を把握してヘッダー追加等を壊れずに修正 |
| file watcher | watchexec | シンプルで LLM が全容を把握できるサイズ |
| env 管理 | direnv 簡易版 | 設定ファイルの読み書き |
| task runner | just 簡易版 | TOML パース + プロセス実行 |

**成功基準**: 修正生存率の計測データが出せること。

---

## Block 4: Platform (エコシステム)

Block 0-3 で信用が溜まった後。

| プロジェクト | 既存 | 意味 |
|---|---|---|
| パッケージレジストリ | crates.io / npm | Almide エコシステムの基盤 |
| Playground | Rust Playground | ブラウザで WASM コンパイラが動く (セルフホスティングの帰結) |
| LSP server | — | 開発体験の本格化 |

---

## やらないもの

| 対象 | 理由 |
|---|---|
| DB エンジン | Almide の強みが活きない。低レベル I/O が必要 |
| 暗号ライブラリ | 安全性の証明が言語の成熟度を超えている |
| OS レベルのツール | syscall 直叩きが必要。Almide の抽象レベルと合わない |
| 巨大フレームワーク | 工数に見合わない。小さくて速いものを大量に |
