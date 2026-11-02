# 型の Where 制約

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](type-where-constraints.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](type-where-constraints.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](type-where-constraints.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットであり、自動同期されない。状態・実装・計測の記述は原文時点の記録で、翻訳時の独立検証や現行保証ではない。計画を実装済みとは扱わない。

説明: 既存のインラインジェネリック境界を超える、where 節構文と型定義制約の提案。

> **状態: 設計。独立した `where` 構文は提案であり、現在の構文ではない。**

## すでにサポートされているもの

ジェネリック関数では、プロトコル境界と構造的境界をすでにインラインで表現できる。

```almide
fn display[T: Showable](item: T) -> String = item.show()
fn describe[T: { name: String, .. }](item: T) -> String = item.name
```

最初の断片の `Showable` はユーザーが宣言したプロトコルである。複数のプロトコル境界には `+` を使う。現在の規則と完全な例は [type-system.md](../../../../../specs/type-system.md#protocol-bounds-on-generics) にあり、[protocol_generics_test.almd](../../../../../../spec/lang/protocol_generics_test.almd) と [open_record_test.almd](../../../../../../spec/lang/open_record_test.almd) で検証されている。したがって、ジェネリック制約がまったく存在しないと説明するのは誤りである。

## 残る提案

関数定義と型定義で独立した `where` 節を使うことを検討する。ジェネリック型定義の制約を使用箇所でどう検査するかも含む。以下の構文は設計の概略であり、コンパイル可能な例でもリリースの約束でもない。

```almide
type Set[T]
  where T: Hash + Eq
= { items: Map[T, Unit] }

fn max[T](items: List[T]) -> Option[T]
  where T: Ord
= list.fold(items, none, (best, x) => ...)
```

作業は既存のインライン境界との関係を規定し、採用する拡張についてパーサーとチェッカーのサポートを実装し、受理・拒否テストを追加することである。すでに提供されている関数ジェネリックのインライン境界を再実装することではない。ここではリリース目標を割り当てていない。

当初の設計ではテスト文脈の `where` 形式も検討された。同じキーワードを共有していることは、どちらかの提案が実装済みである証拠にはならない。現在のジェネリック境界構文とは分けて扱う。
