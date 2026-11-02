# Fan の意味論監査プローブ

[English](./README.md) | [日本語](./README.ja.md) | [简体中文](./README.zh-CN.md)

[docs/design/fan-semantics-audit.md](../../../docs/design/fan-semantics-audit.md) の各「probe」引用の根拠となるプログラム。0746eb689 に固定している。リポジトリのルートから両ターゲットで実行する。

```bash
almide run research/fan-audit/probes/B/race_order.almd
almide run research/fan-audit/probes/B/race_order.almd --target wasm
```

- `A/`：ブロック、`fan.map`、`fan.settle` の形式。不一致を発見した二つのプローブだけを残している。その他の block/map/settle の観測は `tests/fan_acceptance_test.rs` で固定している。
- `B/`：`fan.any` と `fan.race`。
- `C/`：`fan.bounded`、`fan.timeout`、時間のコンストラクター、およびエフェクトの許可判定。

これらは証拠であり、テストではない。`test` ブロックを持たないため、`almide test` はスキップする。いくつかはコンパイル失敗を想定している。それらは、例えば E006 によってプログラムが拒否されることを確認するプローブである。
