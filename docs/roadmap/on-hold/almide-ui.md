<!-- description: SolidJS-like reactive UI framework built as a pure Almide library -->
# Almide UI — Reactive Web Framework as Almide Library

## Thesis

Almide で書かれた SolidJS ライクなリアクティブ UI フレームワークを **Almide のライブラリとして** 構築する。コンパイラにフレームワーク固有の最適化パスを追加しない。コンパイラの汎用最適化（型特殊化、インライン展開、パイプライン融合）がフレームワークコードにも等しく適用され、結果として Svelte/SolidJS 級の出力を得る。

```
ユーザーが書くもの: Almide の builder 構文 + signal ライブラリ
コンパイラがやること: 普通の Almide コードとして最適化 → TS 出力
出てくるもの: 仮想 DOM なし、fine-grained 更新の素の JS
```

**フレームワークの知識はライブラリに閉じる。コンパイラは Almide を知っていれば十分。**

## Why This Is The Right Approach

### 他のアプローチとの比較

| アプローチ | 例 | 問題 |
|---|---|---|
| コンパイラにフレームワークを内蔵 | Svelte | コンパイラが複雑化。フレームワークの進化 = コンパイラの改修 |
| JS ランタイムライブラリ | React, Vue | 仮想 DOM diff のランタイムコスト。バンドルサイズ |
| JS ライブラリ + fine-grained | SolidJS | 最速級。だが JS の型情報がないので最適化の天井がある |
| **Almide ライブラリ** | **この提案** | SolidJS モデル + Almide の型情報による追加最適化 |

### Almide ライブラリである利点

1. **コンパイラを変更しない** — builder 機構と汎用最適化だけで成立する
2. **フレームワークが Almide で書かれている** — ユーザーコードと同じ言語、同じ最適化対象
3. **型情報がフレームワーク内部にも効く** — `Signal[Int]` の `.get()` は Int を返すと確定。型特殊化が末端まで貫通する
4. **フレームワークの進化がコンパイラと独立** — ライブラリのバージョンアップでコンパイラリリース不要
5. **マルチターゲットの恩恵** — フレームワーク自体のテストを `--target rust` で高速に回せる

## Architecture

### レイヤー構成

```
almide-ui/                    ← Almide パッケージ (ライブラリ)
├── reactive.almd             Signal, Effect, Derived
├── dom.almd                  DOM プリミティブ (@extern で bridge)
├── component.almd            コンポーネントライフサイクル
├── renderer.almd             Signal→DOM 更新の接続
└── html.almd                 builder Html + 要素関数 (div, p, ...)
```

### Reactive Core

```almide
// reactive.almd

type Signal[T] = {
  var value: T,
  var subs: List[fn() -> Unit],
}

fn signal[T](initial: T) -> Signal[T] =
  { value: initial, subs: [] }

fn get[T](s: Signal[T]) -> T = s.value

fn set[T](s: Signal[T], new_val: T) -> Unit = {
  s.value = new_val
  for sub in s.subs { sub() }
}

fn subscribe[T](s: Signal[T], f: fn() -> Unit) -> Unit =
  s.subs = s.subs ++ [f]

fn derived[T](compute: fn() -> T) -> Signal[T] = {
  // 依存する signal を自動追跡して computed value を返す
  // SolidJS の createMemo と同等
  ...
}
```

**コンパイラから見れば普通の Almide コード。** `get` は 1 行関数なのでインライン展開される。`Signal[Int]` は単相化される。

### DOM Bridge

```almide
// dom.almd — 最小限の @extern bridge

@extern(ts, "document.createElement(tag)")
fn create_element(tag: String) -> DomNode

@extern(ts, "node.textContent = text")
fn set_text(node: DomNode, text: String) -> Unit

@extern(ts, "parent.appendChild(child)")
fn append(parent: DomNode, child: DomNode) -> Unit

@extern(ts, "node.addEventListener(event, handler)")
fn on(node: DomNode, event: String, handler: fn() -> Unit) -> Unit

@extern(ts, "node.setAttribute(name, value)")
fn set_attr(node: DomNode, name: String, value: String) -> Unit
```

DOM API は 10-15 個の `@extern` で全部カバーできる。フレームワーク側の仕事はこれらを組み合わせて宣言的 API を提供すること。

### Renderer — Signal と DOM の接続

```almide
// renderer.almd

fn reactive_text(parent: DomNode, s: Signal[String]) -> Unit = {
  let node = create_element("span")
  set_text(node, s.get())
  append(parent, node)
  // signal が変わったら textContent だけ更新
  s.subscribe(fn() => set_text(node, s.get()))
}

fn reactive_attr(node: DomNode, name: String, s: Signal[String]) -> Unit = {
  set_attr(node, name, s.get())
  s.subscribe(fn() => set_attr(node, name, s.get()))
}

fn reactive_list[T](
  parent: DomNode,
  items: Signal[List[T]],
  render: fn(T) -> DomNode,
) -> Unit = {
  // リスト差分更新 (keyed reconciliation)
  ...
}
```

### ユーザーコード

```almide
import almide_ui exposing (signal, get, set, derived)
import almide_ui/html exposing (div, h1, p, button, ul, li)

var count = signal(0)
let doubled = derived(fn() => count.get() * 2)

template app() -> HtmlDoc = Html {
  div(class: "app") {
    h1 { "Counter" }
    p { "Count: ${count.get()}" }
    p { "Doubled: ${doubled.get()}" }
    button(onclick: fn() => count.set(count.get() + 1)) {
      "Increment"
    }
  }
}
```

ユーザーは Signal と builder 構文だけ知っていればいい。DOM 操作は見えない。

## Compiler Optimization — What Happens Automatically

フレームワークもユーザーコードも、コンパイラにとっては同じ Almide。以下が自動で適用される:

### 1. インライン展開

```almide
// signal.get() の定義
fn get[T](s: Signal[T]) -> T = s.value

// ユーザーコード
p { "Count: ${count.get()}" }

// インライン展開後 (コンパイラ内部)
p { "Count: ${count.value}" }

// TS 出力
p.textContent = `Count: ${count.value}`;
```

Signal の `.get()` / `.set()` は 1 行関数。コンパイラがインライン展開すれば、Signal のオーバーヘッドがゼロになる。

### 2. 型特殊化 (単相化)

```almide
// ジェネリック定義
fn get[T](s: Signal[T]) -> T = s.value

// Signal[Int] で使用 → Int 特殊化版が生成
// Signal[String] で使用 → String 特殊化版が生成
```

V8 は monomorphic call site を最速で処理する。型特殊化により、全ての Signal 操作が monomorphic になる。

### 3. パイプライン融合

```almide
let visible_names = items.get()
  |> list.filter((x) => x.active)
  |> list.map((x) => x.name)
```

→ 中間配列なしの単一ループに融合。リアクティブリストの更新が高速化される。

### 4. Dead code elimination

使われていない Signal メソッド、DOM ヘルパーは出力から除去。バンドルサイズが最小化される。

## Future Option: Compiler-Aware Signal Tracking

ライブラリだけで SolidJS 級の性能は出る。さらに Svelte 級を狙うなら、**オプションとして** コンパイラが Signal を認識するパスを追加できる:

```almide
// builder block 内の Signal.get() 呼び出しをコンパイラが追跡
Html {
  p { "Count: ${count.get()}" }     // ← count に依存
  p { "Name: ${name.get()}" }      // ← name に依存
  p { "Static text" }               // ← 依存なし
}
```

コンパイラが依存関係を見て、更新関数を自動生成:

```typescript
// create
const p0 = document.createElement('p');
p0.textContent = `Count: ${count.value}`;
const p1 = document.createElement('p');
p1.textContent = `Name: ${name.value}`;
const p2 = document.createElement('p');
p2.textContent = 'Static text';

// update — count が変わったとき p0 だけ更新
count.subscribe(() => { p0.textContent = `Count: ${count.value}`; });
// update — name が変わったとき p1 だけ更新
name.subscribe(() => { p1.textContent = `Name: ${name.value}`; });
// p2 は subscribe なし（静的なので更新不要）
```

**これはライブラリでも実行時に同等のことをやれる** (SolidJS がそうしている) ので、コンパイラ対応はあくまでオプショナルな最適化。後から足せる。

## Competitive Position

```
                    コンパイル時解析    ランタイムコスト    エコシステム
Svelte              ◎ (compiler)       ◎ (最小)          ○ (JS)
SolidJS             △ (JSX transform)  ◎ (signal)        ○ (JS)
React               × (runtime diff)   △ (vDOM)          ◎ (最大)
Leptos (Rust→WASM)  ○ (signal)         ○ (WASM overhead) △ (WASM FFI)
Almide UI           ○→◎ (後から追加可)  ◎ (inline signal) ◎ (native JS output)
```

Almide UI の差別化ポイント:
- **WASM じゃなく素の JS が出る** — エッジ最速、エコシステム完全互換
- **同じコードが `--target rust` でネイティブにもなる** — SSR を Rust バイナリで、クライアントを TS で、が 1 言語
- **型情報がフレームワーク内部まで貫通** — SolidJS にはない最適化余地
- **フレームワークがコンパイラと独立** — Svelte と違いライブラリとして進化可能

## Relationship to Other Roadmap Items

- **ts-edge-native.md**: Almide UI の出力先。emitter 最適化 (Phase 1) は Almide UI の性能に直結する
- **Result Builder (template.md)**: `builder Html` はまさにこのフレームワークの UI 記述基盤。builder 機構が完成すれば Almide UI の DOM 構築が自然に動く
- **cross-target-semantics.md**: SSR (Rust) + Client (TS) の同一言語ストーリーには cross-target の正確性が前提
- **emit-wasm-direct.md**: 独立。Almide UI は TS 出力を使うので WASM パスとは関係ない

## Prerequisites

1. **builder 機構 (template.md Phase 1)** — `builder Html` と trailing block がないと UI 記述が書けない
2. **emitter 最適化 (ts-edge-native.md Phase 1)** — 型特殊化・インライン展開がないとランタイムオーバーヘッドが大きい
3. **ジェネリクスの安定** — `Signal[T]` が正しく型チェック・codegen される必要がある
4. **クロージャの codegen** — `subscribe(fn() => ...)` が正しく TS に出力される必要がある

## Why ON HOLD

builder 機構と emitter 最適化が先。ただし:

- **設計上のブロッカーはない** — Signal はただの Almide の型、DOM bridge は `@extern`、builder は既に設計済み
- **コンパイラ変更は不要** — 汎用最適化が効けば、フレームワーク固有のパスはオプション
- **段階的に構築可能** — reactive core → DOM bridge → renderer → builder 統合、の順で独立して作れる
- **SolidJS が証明済み** — ライブラリアプローチ + fine-grained signals で Svelte 級の性能は出る

確度は高い。前提となる言語機能が揃えば、フレームワーク自体は Almide で書くだけ。
