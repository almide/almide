<a id="c-033--copy-on-write-for-aliased-mutables"></a>

# C-033 — 別名を持つ可変値の copy-on-write

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](C-033-cow-truth-table.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](C-033-cow-truth-table.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](C-033-cow-truth-table.zh-CN.md)

> 非規範の原文固定読者スナップショット。変更していない原本が正本。自動同期・独立実行・再測定・契約保証の新たな検証はしていない。現行／履歴の記述は固定原文時点に属する。C-006 は旧 timeout 形式の削除と後の協調的ブロック形式を区別する。C-033 は修正前診断と解決指示を履歴として保持し、旧削除・ゲート変更指示を実行しない。repr ヘルパー／export の詳細は過去の emitter を説明するもので現行公開 ABI ではない。コードとコメントは原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-033-cow-truth-table.md)

> [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) の契約 C-033 を補足する説明文。
> 状態：**active** — AliasCowPass + native pass_clone で修正済み（PR #394、2026-06-06）。以下の真理値表は現在両ターゲットでバイト同一であり、`spec/wasm_cross/alias_cow.almd` が固定する。以下の履歴は、`crates/almide-codegen/rt-oracle-registry.toml` の旧来の例外扱いだった `cow_check` 注記から移したもの（v0 emitter とともに #782 で廃止。git 履歴を参照。現行ゲートは spec/wasm_cross と interp oracle）。
> 修正前は、差異を記した文書のみの記録だった。以下の節は修正前の診断と解決条件を保持するもので、現在の実装状態ではない。

<a id="the-promise"></a>

## 約束

```almide
var a = [1, 2, 3, 4, 5]
var b = a        // b is a value copy, not an alias
a[0] = 999       // mutate a in place
// a == [999, 2, 3, 4, 5], b == [1, 2, 3, 4, 5]   (b UNCHANGED)
```

値意味論：`var b = a` はコピーするため、その後の `a` の破壊的変更が `b` を通じて観測されてはならない。仕組みは copy-on-write、すなわち共有値への最初の破壊的変更時に裏側のバッファをコピーすることだ。

<a id="historical-divergence-before-the-fix"></a>

## 過去の差異（修正前）

| 手順 | native（Rust） | wasm |
|------|---------------|------|
| `var a = [1,2,3,4,5]` | `a` -> バッファ A | `a` -> ヒープバッファ A |
| `var b = a` | `Rc::make_mut` / clone-on-write であるべき | `a` と `b` を単一 local にまとめ、別名をコピー伝播する |
| `a[0] = 999` | 分類の欠落：最小再現例で **E0382** move-after-use | バッファ A を**その場で**変更（COW なし） |
| `a`、`b` を読む | （最小再現例ではコンパイルできない） | `a = 999,2,3,4,5` **かつ** `b = 999,2,3,4,5` |

観測されたプローブ：native `999,2,3,4,5 | 1,2,3,4,5` に対し wasm は `999,2,3,4,5 | 999,2,3,4,5`。wasm では `b` が壊れる。

<a id="why-the-historical-runtime-only-fix-was-insufficient"></a>

## 当時のランタイムだけの修正では不十分だった理由

これを防ぐはずの wasm ルーチン `__cow_check` は、wasm emitter に**呼び出し元がない**。`IndexAssign` / `FieldAssign` / `ListSwap` / `ListReverse` はすべてその場で変更する。`IndexAssign` で `__cow_check` を接続することは**必要だが十分ではない**。wasm パイプラインはさらに、**別名を持つ可変 `var` を1つの local にまとめ**、別名をコピー伝播するため、ヒープ COW が分離できるものがない。native 自身にも対応する VarStorage 分類の欠落があり、同じ最小再現例は Rust ターゲットで E0382 move-after-use になる。

正しい修正は、**両ターゲットの VarStorage／別名統合モデル**にまたがる。すべての破壊的変更箇所で、別々の local に書き戻す COW を行い、**さらに**可変変数の別名コピー伝播を抑制する必要がある。これは runtime-routine drain の範囲外なので、偽の「verified」への切り替えを強制せず、このルーチンを**正直に旧来の例外扱いとして残す**。

<a id="historical-closure-criteria-fulfilled-by-the-fix-above"></a>

## 当時の解決条件（上記修正により達成済み）

VarStorage／別名の作業が入る際は、`var b = a; a[i] = v; read both` fixture を `spec/wasm_cross/` に追加し、それを `class = "fixture"` 証拠として `C-033` を `status = "active"` に変え、この文書を削除し、`scripts/check-rt-oracle-registry.sh` の `MAX_GRANDFATHERED` と `scripts/check-contracts.sh` の `MAX_FLAGGED` を減らす。**すべて同じ PR で行う**。flagged 件数はラチェットであり、減らすことしかできない。
