<a id="region-based-memory-management"></a>

# リージョンに基づくメモリ管理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](region-inference.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](region-inference.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](region-inference.zh-CN.md)

> 固定原文の非規範的な読者用コピーであり、自動同期しません。歴史的な実装と未実装の保留提案を区別します。現在の実装についての記述も原文時点のもので、ここで独立に検証していません。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/region-inference.md)

> **状態**：完全なリージョン推論は保留中の提案のままである。以下の Phase 1+2 は v0.23.4 の実装を記録したもので、現在の Wasm アロケータではない。構造的バックエンドは現在、参照カウントとサイズクラス別フリーリスト（`crates/almide-wasm/src/alloc_inline.rs`）を使う。古い bump-restore 設計の制限から、現在のリークを推定してはならない。

<a id="historical-implementation-v0234"></a>

## 歴史的な実装（v0.23.4）

<a id="phase-1-function-level-region-v0234"></a>

### Phase 1：関数単位のリージョン（v0.23.4）

Unit を返す関数とラムダは、入口と出口でヒープポインタを自動的に保存・復元する。内部の一時的な確保はすべて、関数から戻る際に回収される。

**対象**：ベンチマークのコールバック、イベントハンドラ、リクエスト処理。

<a id="phase-2a-loop-iteration-region-v0234"></a>

### Phase 2a：ループ反復のリージョン（v0.23.4）

本体が外側スコープの変数へヒープ値を代入しない while ループでは、各反復を自動的にスコープ化する。一時的な確保は反復ごとに回収される。

**対象**：`while i < N { let _r = list.map(...); i = i + 1 }`

<a id="phase-2b-block-liveness-region-v0234"></a>

### Phase 2b：ブロックの生存性リージョン（v0.23.4）

ブロック中のヒープ型 let 束縛ごとに、コンパイラが最後の参照を探し、その後へヒープ復元を挿入する。確保した領域は使われなくなり次第回収される。

**対象**：`let step1 = map(...); let step2 = filter(step1, ...); // step1 freed here`

<a id="phase-3-full-region-inference-future"></a>

## Phase 3：完全なリージョン推論（将来）

<a id="cases-motivating-the-original-proposal"></a>

### 元の提案の動機となったケース

以下は bump のみのリージョン復元の制約であり、現在の欠陥一覧ではない。入れ子の値の drop と可変値の置換は、新しいアロケータを提案する前に、現在の所有権・drop 実装に照らして評価する必要がある。

1. **入れ子のヒープ構造**：`List[List[Int]]` — bump 復元では内部リストを個別に解放できない（復元すると印より上をすべて解放する）。
2. **可変変数の置換**：`var s = "hello"; s = "world"` — 古い "hello" の確保は、その bump 復元だけでは回収できない（復元すると "world" が無効になる）。現在の RC の挙動についての主張ではない。
3. **クロージャによるキャプチャ**：キャプチャした値は、そのスコープより長く生存し得る。リージョン推論はキャプチャの生存期間を追跡しなければならない。
4. **関数をまたぐリージョン多相性**：`fn make_list() -> List[Int]` — 返すリストのリージョンは呼び出し元の文脈に依存する。

<a id="design-approach-mlkit-style-region-inference"></a>

### 設計方針：MLKit 方式のリージョン推論

- **リージョン型注釈**：すべてのヒープ型に暗黙のリージョン変数を持たせる。`List[Int]@r1` は「リージョン r1 に確保されたリスト」を意味する。
- **リージョン推論**：Hindley-Milner の単一化をリージョン制約で拡張する。リージョン変数は推論し、ユーザーは書かない。
- **リージョン多相性**：`fn map[A, B, r1, r2](xs: List[A]@r1, f: A -> B) -> List[B]@r2`。出力のリージョン r2 は呼び出し位置で決まる。
- **リージョン効果**：関数が確保するリージョンを宣言する。`fn f() -r1-> T` は「f はリージョン r1 に確保する」を意味する。
- **Letregion**：`letregion r in { ... }` はブロック終了時に解放されるリージョンを作る。コンパイラが推論により自動挿入する。

<a id="implementation-scope"></a>

### 実装範囲

- **フロントエンド**：almide-frontend の型検査器でリージョン変数を推論
- **IR**：IrExpr の型にリージョン注釈
- **コード生成**：リージョンを考慮した確保（有効なリージョンごとに一つの bump ポインタ）
- **ランタイム**：リージョンスタック（単一の heap_ptr ではなくリージョンを push/pop）

<a id="references"></a>

### 参考資料

- Tofte & Talpin (1997): "Region-Based Memory Management"
- MLKit compiler: https://github.com/melsman/mlkit
- Cyclone language: Region-based safe C

<a id="original-motivation"></a>

### 当初の動機

v0.23.4 の計画では、Phase 1+2 で次のパターンを優先した：
- 短命なプロセス（Phase 1）
- 処理ループ（Phase 2a）
- パイプライン変換（Phase 2b）

Phase 3 の当初の動機は、複雑な状態を持つ長時間稼働サーバーだった。この提案を再開するには、現在の RC とフリーリストによる回収を超える実測された必要性が要る。歴史的なリージョン計画だけでは、その必要性は示されない。
