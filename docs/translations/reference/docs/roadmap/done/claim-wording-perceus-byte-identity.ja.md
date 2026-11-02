# Claim wording: Perceus phrasing and the byte-identity guarantee scope

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](claim-wording-perceus-byte-identity.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](claim-wording-perceus-byte-identity.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](claim-wording-perceus-byte-identity.zh-CN.md)

> 2026-07-15 完了と記録された歴史的ロードマップの非規範的な読者用コピーです。測定・発信文の保証・削除 API の記述は当時の原文の主張であり、現在の保証を独立に検証したものではありません。特に後続文書は fan の時間 API の変更を記録しており、本記録を現在の API 案内に読み替えません。歴史の正本は原文であり、自動同期しません。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/claim-wording-perceus-byte-identity.md)

公開クレーム（ツイート/README claims block）を実測どおり 100 点で言い切るための
表現調整 2 点。GitHub issue は [#773](https://github.com/almide/almide/issues/773)。
両文言とも**確定済み**、README への反映も完了。

## 1. 「Perceus RC で自動管理」→「Perceus 方式の所有権推論で自動管理」【確定】

厳密には:
- **wasm レッグ**: Perceus MIR の判定を RC（rc_inc/rc_dec + free-list）で実行。
- **Rust レッグ**: 同じ Perceus MIR の判定（Alloc/Dup/Drop/Consume、
  `verify_ownership` 認証済み）を Rust の clone/drop で実現。RC という**実行機構**は
  wasm 側の実現。

確定文言: 「メモリ管理は Perceus 方式の所有権推論が自動決定（wasm は RC 実行、
Rust 出力は同判定を Rust の drop で実現）」。これなら両ターゲットに言い切れる。

README 反映: Memory Safety 節を「decided by Perceus-style ownership inference /
what differs per target is only the *execution mechanism*」構成に書き換え。
trust-spine ladder（#764）への参照も「shared scalar and list ops already render
on both targets from the same MIR」に更新（rung 4 出荷後の実態）。

## 2. 「出力バイト一致を保証」の範囲明示【確定】

確定文言: 「観測可能出力（stdout/stderr/exit）のバイト一致を、契約台帳
（C-001..C-133、flagged 0）＋クロスターゲット差分ゲート（spec/wasm_cross 246
fixtures）＋ org 実プロジェクト両ターゲット検証で**継続的に保証**」。

明示除外（台帳で管理）:
- 本質的非決定性は「決定的不変量」を契約として認証（例: C-112 `random.int` の
  range 不変量）。唯一の wall-clock 面（fan.timeout）は C-006 で削除済み。
- wasm 未実装 API（fs.glob 等）は**コンパイル/実行拒否**であり、間違ったバイトは
  出ない（honest-wall）。

README 反映: Equivalence Claim 節に「continuous, with an explicit scope」の
3 項目（in scope / nondeterministic sources / not-yet-implemented APIs）を追加。

## 確定版発信文（ツイート等）

> まさにその需要向けの言語を作ってまして、プログラマは所有権もライフタイムも
> 書かない。メモリ管理は Perceus 方式の所有権推論が自動決定（wasm は RC 実行、
> Rust 出力は同判定を drop で実現）。例外無し・網羅的 match の Rust 系型規律の
> まま Rust/WASM にコンパイルされ、観測可能出力（stdout/stderr/exit）のバイト
> 一致を契約台帳＋差分ゲートで継続的に保証してます。よければ覗いてみてください

短縮版（文字数が厳しい場合）:

> 所有権もライフタイムも書かない言語を作ってます。メモリ管理は Perceus 方式の
> 所有権推論が自動決定（wasm は RC、Rust は同判定を drop で実現）。例外無し・
> 網羅的 match で、Rust/WASM 両出力の観測可能出力バイト一致を契約台帳＋CI ゲート
> で継続保証。よければ覗いてみてください

## 背景（言い切りを支える事実）

- 最後の既知乖離（mutable module var の snapshot alias — `--no-verified` v0-wasm が
  alias、native/デフォルト v1 は COW）は e088c25d 時点で**デフォルト経路で解消**し、
  C-033 の fixture `spec/wasm_cross/module_var_alias_cow.almd` として pin 済み。
  乖離が残るのは明示的 opt-out（`--no-verified`）のみ。
- 網羅的 match は checker が E010 で強制（実測確認済み）。
