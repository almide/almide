# 解放の churn ゲート (ALMIDE_WASM_FREES=1)

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> 原文: [spec/churn/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/churn/README.md)。非規範的な読者向けスナップショット翻訳です。自動同期されません。計測値・主張の独立した再検証は行っていません。原文を正本とします。

実 RC の wasm モード用、長いループでの回収フィクスチャです。デフォルトのコーパスには含みません（数百万回の反復）。次で実行します。

```
scripts/check-frees-churn.sh
```

スクリプトは 600 秒の watchdog 内で正常終了することを要求し、取得した native 出力と Wasm の取得出力（stderr を統合）を比較します。RSS は測定しません。シェルのコマンド置換で末尾の改行が除かれるため、stdout/stderr/終了状態の完全なバイト単位比較ではありません。コーパスだけでは、実際の二重解放が残るビルドを二度通過させました。これらの形状は、コーパスには担えないゲートです。

- record_churn.almd — 新しい文字列フィールドを持つ値の構築＋破棄を 2M 回反復（free-list 再利用、sentinel/resurrection の保護帯）。
- tco_loop_churn.almd — ループで変化する TCO 呼出しを 200k 回（M2 の managed-param OOB を露呈した形状。反復ごとの TCO 回収の受入テスト）。

複合的な再帰 drop フィクスチャは、v1 stage-2 移行の基準として始まり、当時の `emit_typed_rc_dec` 分岐と提案されたフラットな MIR Drop ノードを対象としていました。この移行の根拠は歴史的なものですが、フィクスチャは現在の構造的 Wasm 経路における回収の回帰テストとして残ります。各例は、未解決の #643 some-box リークと別の形状にするため、意図的に `?? ` (UnwrapOr) を避けています。

- map_entry_churn.almd — Map[String, String] の構築＋破棄を 200k 回（ヒープ上のキーと値を別々に破棄）。
- closure_env_churn.almd — ヒープ上の String を捕捉するクロージャの構築＋破棄を 200k 回（環境が捕捉値を再帰的に drop する必要があります）。
- nested_named_churn.almd — List[String] を入れ子に持つレコードの構築＋破棄を 200k 回（record → list → string の再帰 drop）。
